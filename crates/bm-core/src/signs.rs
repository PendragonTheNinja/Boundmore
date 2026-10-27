//! Signs (milestone 22 part 4, `docs/milestones/22-building-blocks.md`).
//!
//! What a sign says, the limits on it, Boundless's colour codes, and where
//! on its block the text goes. A standalone sign is one block; sign panels
//! join into a rectangle of up to [`MAX_PANELS`] by [`MAX_PANELS`] when first
//! written. The server keeps the texts; the client draws them.

use crate::block::Shape;
use crate::forms::{Form, FormKind};
use serde::{Deserialize, Serialize};

/// The most characters of display text (Boundless's; colour codes count).
pub const DISPLAY_CHARS: usize = 128;
/// The most lines of display text.
pub const DISPLAY_LINES: usize = 8;
/// The most characters of further text (a guess: Measure 7).
pub const MORE_CHARS: usize = 1000;
/// The most panels a sign spans each way.
pub const MAX_PANELS: u8 = 10;

/// How the display text is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Align {
    Left,
    #[default]
    Centre,
    Right,
}

/// What a sign says.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignText {
    /// Shown on the sign, sized to fit.
    pub text: String,
    /// Read by opening the sign.
    pub more: String,
    pub align: Align,
    pub italic: bool,
}

impl SignText {
    /// Whether there is nothing on it (writing this unwrites the sign).
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty() && self.more.trim().is_empty()
    }

    /// This text if it keeps to the limits: no control characters but line
    /// breaks, at most [`DISPLAY_CHARS`] and [`DISPLAY_LINES`] shown and
    /// [`MORE_CHARS`] further. Line ends are normalised to `\n`.
    pub fn checked(mut self) -> Result<SignText, &'static str> {
        let clean = |s: &str| s.replace("\r\n", "\n").replace('\r', "\n");
        self.text = clean(&self.text);
        self.more = clean(&self.more);
        let bad = |s: &str| s.chars().any(|c| c.is_control() && c != '\n');
        if bad(&self.text) || bad(&self.more) {
            return Err("A sign takes letters, not control characters");
        }
        if self.text.chars().count() > DISPLAY_CHARS {
            return Err("A sign shows at most 128 characters");
        }
        if self.text.lines().count() > DISPLAY_LINES {
            return Err("A sign shows at most 8 lines");
        }
        if self.more.chars().count() > MORE_CHARS {
            return Err("A sign's further text holds at most 1000 characters");
        }
        Ok(self)
    }
}

/// A written sign as players are sent it: its text and, for panels, how
/// many panels it spans across and up from its anchor (1 by 1 alone).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignView {
    pub text: SignText,
    pub across: u8,
    pub up: u8,
}

/// A colour a code asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Rgb([u8; 3]),
    /// A palette colour by id (ADR-0007).
    Palette(u8),
}

/// The display text split at its colour codes: `:#f13:` and `:#ff1133:`
/// (hex colours) and `:#nn:` (a palette colour id in hex), as Boundless's.
/// Each run is the colour in force (None: the sign's own) and its text.
/// Anything that is not a whole code is text.
pub fn runs(text: &str) -> Vec<(Option<Ink>, String)> {
    let mut out: Vec<(Option<Ink>, String)> = vec![(None, String::new())];
    let mut rest = text;
    while let Some(i) = rest.find(":#") {
        let after = &rest[i + 2..];
        let code = after.find(':').map(|j| &after[..j]).and_then(parse_code);
        match code {
            Some((ink, len)) => {
                out.last_mut().expect("a run").1.push_str(&rest[..i]);
                out.push((Some(ink), String::new()));
                rest = &rest[i + 2 + len + 1..];
            }
            None => {
                out.last_mut().expect("a run").1.push_str(&rest[..i + 2]);
                rest = after;
            }
        }
    }
    out.last_mut().expect("a run").1.push_str(rest);
    out.retain(|(_, t)| !t.is_empty());
    out
}

fn parse_code(code: &str) -> Option<(Ink, usize)> {
    if !code.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = |s: &str| u8::from_str_radix(s, 16).ok();
    let ink = match code.len() {
        2 => Ink::Palette(hex(code)?),
        3 => {
            let d: Vec<u8> = code.chars().map(|c| c.to_digit(16).unwrap_or(0) as u8 * 17).collect();
            Ink::Rgb([d[0], d[1], d[2]])
        }
        6 => Ink::Rgb([hex(&code[0..2])?, hex(&code[2..4])?, hex(&code[4..6])?]),
        _ => return None,
    };
    Some((ink, code.len()))
}

/// Where a sign's text goes on its block, in block units: `origin` the
/// lower left corner as read, `right` and `up` unit directions, the size,
/// and `normal` the way the text faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFace {
    pub origin: [f64; 3],
    pub right: [f64; 3],
    pub up: [f64; 3],
    pub normal: [f64; 3],
    pub width: f64,
    pub height: f64,
}

impl TextFace {
    /// The point `(u, v)` block units across and up from the origin.
    pub fn at(&self, u: f64, v: f64) -> [f64; 3] {
        std::array::from_fn(|a| self.origin[a] + self.right[a] * u + self.up[a] * v)
    }
}

/// Whether a shape is a sign's (standalone or panel).
pub fn is_sign(shape: Shape) -> bool {
    Form::of(shape).is_some_and(|f| matches!(f.kind, FormKind::Sign | FormKind::Panel))
}

/// The step from one panel to the next to its right as read, for a panel
/// of shape `shape` (None for anything else).
pub fn panel_right(shape: Shape) -> Option<(i64, i64, i64)> {
    let f = Form::of(shape).filter(|f| f.kind == FormKind::Panel)?;
    let r = turn_vec([-1.0, 0.0, 0.0], f.turn & 3);
    Some((r[0].round() as i64, 0, r[2].round() as i64))
}

/// Where the text of a sign whose anchor block has `shape` goes, spanning
/// `across` by `up` panels (1 by 1 for a standalone sign). None if the
/// shape is not a sign's.
pub fn text_face(shape: Shape, across: u8, up: u8) -> Option<TextFace> {
    let f = Form::of(shape)?;
    // Facing north (read from -z), in sixteenths: the board's x and y span
    // and its front z, less a sixteenth's margin all round.
    let (x0, x1, y0, y1, z) = match f.kind {
        FormKind::Sign if f.turn & 4 != 0 => (1.0, 15.0, 3.0, 13.0, 14.0),
        FormKind::Sign => (1.0, 15.0, 6.0, 16.0, 7.0),
        FormKind::Panel => (0.0, 16.0, 0.0, 16.0, 14.0),
        _ => return None,
    };
    let s = 1.0 / 16.0;
    let margin = s;
    // Read from the north, right is -x.
    let origin = [(x1 * s) - margin, y0 * s + margin, z * s - 0.002];
    let right = [-1.0, 0.0, 0.0];
    let turn = f.turn & 3;
    let (across, up) = (across.max(1) as f64, up.max(1) as f64);
    Some(TextFace {
        origin: turn_point(origin, turn),
        right: turn_vec(right, turn),
        up: [0.0, 1.0, 0.0],
        normal: turn_vec([0.0, 0.0, -1.0], turn),
        width: (x1 - x0) * s - 2.0 * margin + (across - 1.0),
        height: (y1 - y0) * s - 2.0 * margin + (up - 1.0),
    })
}

/// Quarter turns clockwise seen from above about the block's centre, as
/// `forms` turns boxes: (x, z) to (1 - z, x).
fn turn_point(p: [f64; 3], turns: u8) -> [f64; 3] {
    let mut p = p;
    for _ in 0..turns {
        p = [1.0 - p[2], p[1], p[0]];
    }
    p
}

fn turn_vec(v: [f64; 3], turns: u8) -> [f64; 3] {
    let mut v = v;
    for _ in 0..turns {
        v = [-v[2], v[1], v[0]];
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::Style;

    #[test]
    fn colour_codes_split_the_text_and_anything_else_is_text() {
        let r = runs("Hi :#f13:red:#00ff00: green :#0a:p :#zz: :#12345:");
        assert_eq!(
            r,
            vec![
                (None, "Hi ".to_string()),
                (Some(Ink::Rgb([0xff, 0x11, 0x33])), "red".to_string()),
                (Some(Ink::Rgb([0, 0xff, 0])), " green ".to_string()),
                (Some(Ink::Palette(10)), "p :#zz: :#12345:".to_string()),
            ]
        );
        assert_eq!(runs(""), vec![]);
    }

    #[test]
    fn texts_keep_to_their_limits() {
        let t = |text: &str, more: &str| {
            SignText {
                text: text.into(),
                more: more.into(),
                ..Default::default()
            }
            .checked()
        };
        assert!(t(&"é".repeat(128), "").is_ok(), "characters, not bytes");
        assert!(t(&"a".repeat(129), "").is_err());
        assert!(t("1\n2\n3\n4\n5\n6\n7\n8", "").is_ok());
        assert!(t("1\n2\n3\n4\n5\n6\n7\n8\n9", "").is_err());
        assert!(t("bell\u{7}", "").is_err());
        assert!(t("", &"m".repeat(1001)).is_err());
        assert_eq!(t("a\r\nb", "").unwrap().text, "a\nb");
        assert!(t(" ", "\n").unwrap().is_blank());
    }

    #[test]
    fn text_faces_sit_on_the_front_of_the_board_and_turn_with_it() {
        let shape = |kind, turn| Form::new(kind, Style::Plain, turn).unwrap().shape();
        // A wall sign read from the north: the text faces north, on the
        // board's front, with right towards -x.
        let f = text_face(shape(FormKind::Sign, 4), 1, 1).unwrap();
        assert_eq!(f.normal, [0.0, 0.0, -1.0]);
        assert!((f.origin[2] - 14.0 / 16.0).abs() < 0.01);
        assert!(f.origin[0] > 0.8 && f.right == [-1.0, 0.0, 0.0]);
        // Facing east (turn 1): read from +x, right towards -z.
        let e = text_face(shape(FormKind::Sign, 5), 1, 1).unwrap();
        assert_eq!(e.normal, [1.0, 0.0, 0.0]);
        assert_eq!(e.right, [0.0, 0.0, -1.0]);
        assert!((e.origin[0] - 2.0 / 16.0).abs() < 0.01, "against the west wall");
        // A 3 by 2 panel sign spans three blocks across and two up.
        let p = text_face(shape(FormKind::Panel, 0), 3, 2).unwrap();
        assert!((p.width - (3.0 - 2.0 / 16.0)).abs() < 1e-9);
        assert!((p.height - (2.0 - 2.0 / 16.0)).abs() < 1e-9);
        assert_eq!(panel_right(shape(FormKind::Panel, 0)), Some((-1, 0, 0)));
        assert_eq!(panel_right(shape(FormKind::Panel, 1)), Some((0, 0, -1)));
        assert_eq!(panel_right(shape(FormKind::Sign, 0)), None);
        assert!(is_sign(shape(FormKind::Panel, 2)) && !is_sign(shape(FormKind::Pole, 1)));
    }
}
