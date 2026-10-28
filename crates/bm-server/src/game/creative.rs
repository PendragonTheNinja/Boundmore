//! Creative mode and the testing commands (a testing aid, Nathan
//! 2026-10-26): only on a server started with `--creative`
//! (`ServerConfig::creative`). The body's `creative` switch is set at an
//! agreed input like a skill change, so flying is predicted exactly; the
//! rest (one-swing breaking, `/give`, `/xp`) is the server's alone.

use super::*;

/// What a player sees when the server does not allow creative mode.
pub(super) const CREATIVE_OFF: &str = "Creative mode is off on this server: start it with --creative";

/// The commands `/help` lists.
const COMMANDS: &str = "Testing commands: /creative (or F8) flies and breaks anything in one swing; \
                        /give item count puts items in your pack; /xp amount adds XP.";

impl Game {
    /// Creative mode on or off for player `id`, right after input `at`;
    /// refused (and said so) where the server does not allow it.
    pub(super) fn set_creative(&mut self, id: u32, at: u32, on: bool) {
        let allowed = self.cfg.creative;
        let Some(p) = self.players.get_mut(&id) else { return };
        let on = on && allowed;
        p.state.body.creative = on;
        p.out.send(&ServerMsg::Creative { at, on });
        let text = match (on, allowed) {
            (true, _) => "Creative mode: fly where you look (Space rises, Shift is fast); anything breaks in one swing",
            (false, true) => "Creative mode off",
            (false, false) => CREATIVE_OFF,
        };
        p.out.send(&ServerMsg::Notice { text: text.into() });
        tracing::info!("{} creative {}", p.name, if on { "on" } else { "off" });
    }

    /// A typed command (`ClientMsg::Command`).
    pub(super) fn command(&mut self, id: u32, text: &str) {
        // Commands for everyone.
        let mut words = text.trim().trim_start_matches('/').split_whitespace();
        if words.next().is_some_and(|w| w.eq_ignore_ascii_case("settlement")) {
            let rest: Vec<&str> = words.collect();
            return self.name_settlement(id, &rest);
        }
        if !self.cfg.creative {
            return self.notice(id, CREATIVE_OFF);
        }
        let mut words = text.trim().trim_start_matches('/').split_whitespace();
        let word = words.next().unwrap_or("").to_lowercase();
        let rest: Vec<&str> = words.collect();
        match word.as_str() {
            "give" => self.give(id, &rest),
            "xp" => match rest.first().and_then(|n| n.parse::<u64>().ok()) {
                Some(n) => {
                    self.add_xp(id, n.min(10_000_000));
                    self.notice(id, &format!("Added {n} XP"));
                }
                None => self.notice(id, "Add XP with /xp amount"),
            },
            _ => self.notice(id, COMMANDS),
        }
    }

    /// `/give item [count]`: the item by its name (spaces or underscores,
    /// any case) or shown name, a stack's worth unless a count is given.
    fn give(&mut self, id: u32, args: &[&str]) {
        let (name, count) = match args.last().and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => (args[..args.len() - 1].join("_"), Some(n)),
            None => (args.join("_"), None),
        };
        let key = name.to_lowercase();
        if key.is_empty() {
            return self.notice(id, "Give with /give item count, for example /give iron_ingot 50");
        }
        let found = self.items.id(&key).ok().or_else(|| {
            self.items
                .defs()
                .iter()
                .position(|d| d.display.to_lowercase().replace(' ', "_") == key)
                .map(|i| bm_core::ItemId(i as u16))
        });
        let Some(item) = found else {
            return self.notice(id, &format!("There is no item called {name}"));
        };
        let def = self.items.get(item).expect("found");
        let n = count.unwrap_or(def.stack as u32).clamp(1, 9_999) as u16;
        let shown = def.display.clone();
        let Some(p) = self.players.get_mut(&id) else { return };
        let before = p.inventory.clone();
        let left = p.inventory.add(&self.items, item, n);
        send_slots(p, &before);
        let given = n - left;
        self.notice(id, &format!("Given {given} {shown}"));
    }
}
