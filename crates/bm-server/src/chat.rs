//! Chat: cleaning what players say, and how fast they may say it.
//!
//! Routing (who hears which channel) lives in the game loop, which knows
//! where everyone is; this module is the pure part.

use bm_core::TICK_HZ;
use bm_protocol::MAX_CHAT_LEN;

/// Lines a player may say at once before the limit bites.
pub const CHAT_BURST: f64 = 5.0;
/// Lines per second after the burst.
pub const CHAT_RATE: f64 = 1.0;
/// Universe lines kept for players joining.
pub const HISTORY: usize = 50;

/// Invisible characters that reorder or hide text (zero-width spaces and
/// joiners, direction overrides, the byte-order mark). Not control
/// characters, but as unwelcome in a chat line.
fn invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// A chat line as it will be shown: control and invisible characters gone,
/// runs of whitespace one space, trimmed, at most `MAX_CHAT_LEN` characters.
/// None if nothing is left.
pub fn clean_text(text: &str) -> Option<String> {
    let mut out = String::new();
    for word in text.split_whitespace() {
        let word: String = word.chars().filter(|c| !c.is_control() && !invisible(*c)).collect();
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word);
        if out.chars().count() >= MAX_CHAT_LEN {
            break;
        }
    }
    let cut: String = out.chars().take(MAX_CHAT_LEN).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_string())
}

/// A token bucket in game ticks: `CHAT_BURST` lines at once, then
/// `CHAT_RATE` a second.
#[derive(Clone, Copy, Debug)]
pub struct ChatLimit {
    tokens: f64,
    at: u64,
    /// When the player was last told they are too fast.
    warned: Option<u64>,
}

/// What the limit says about one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Say,
    /// Dropped; tell the player (not more than once a second).
    TooFast {
        tell: bool,
    },
}

impl ChatLimit {
    pub fn new(tick: u64) -> Self {
        Self {
            tokens: CHAT_BURST,
            at: tick,
            warned: None,
        }
    }

    pub fn check(&mut self, tick: u64) -> Verdict {
        let elapsed = tick.saturating_sub(self.at) as f64 / TICK_HZ as f64;
        self.tokens = (self.tokens + elapsed * CHAT_RATE).min(CHAT_BURST);
        self.at = tick;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Verdict::Say;
        }
        let tell = self.warned.is_none_or(|w| tick >= w + TICK_HZ as u64);
        if tell {
            self.warned = Some(tick);
        }
        Verdict::TooFast { tell }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cleaned() {
        assert_eq!(clean_text("  hello   there \n"), Some("hello there".into()));
        assert_eq!(clean_text("a\tb\r\nc"), Some("a b c".into()), "breaks are spaces");
        assert_eq!(clean_text("bell\u{7}ring\u{1b}[31m"), Some("bellring[31m".into()));
        assert_eq!(clean_text("evil\u{202E}txt.exe\u{200B}"), Some("eviltxt.exe".into()));
        assert_eq!(clean_text("   "), None);
        assert_eq!(clean_text("\u{7}\u{0} \u{200B}"), None);
        assert_eq!(
            clean_text("héllo wörld ✨"),
            Some("héllo wörld ✨".into()),
            "letters of every kind stay"
        );
        let long = "ab ".repeat(500);
        let cut = clean_text(&long).unwrap();
        assert!(cut.chars().count() <= MAX_CHAT_LEN);
        assert!(!cut.ends_with(' '));
        let wide = "é".repeat(1000);
        assert_eq!(
            clean_text(&wide).unwrap().chars().count(),
            MAX_CHAT_LEN,
            "cut by characters"
        );
    }

    #[test]
    fn a_burst_then_one_a_second() {
        let hz = TICK_HZ as u64;
        let mut l = ChatLimit::new(1000);
        for _ in 0..5 {
            assert_eq!(l.check(1000), Verdict::Say);
        }
        assert_eq!(l.check(1000), Verdict::TooFast { tell: true });
        assert_eq!(l.check(1001), Verdict::TooFast { tell: false }, "told once a second");
        assert_eq!(l.check(1000 + hz / 2), Verdict::TooFast { tell: false });
        // A second on: one more line, and only one.
        assert_eq!(l.check(1000 + hz), Verdict::Say);
        assert_eq!(l.check(1000 + hz), Verdict::TooFast { tell: true });
        // A long quiet refills the burst, no more.
        let later = 1000 + 100 * hz;
        for _ in 0..5 {
            assert_eq!(l.check(later), Verdict::Say);
        }
        assert!(matches!(l.check(later), Verdict::TooFast { .. }));
    }
}
