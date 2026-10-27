//! Coin and feats (milestone 24, `docs/milestones/24-coin-and-trade.md`):
//! the server counts each feat counter at its event, pays the tiers reached
//! (coin in the ledger, XP through `add_xp`) and tells the player.

use super::*;

impl Game {
    /// Adds `n` to player `id`'s counter `key` and pays any feat tier it
    /// reaches.
    pub(super) fn count(&mut self, id: u32, key: &str, n: u64) {
        self.count_with(id, |l, book| l.add(book, key, n));
    }

    /// Raises player `id`'s counter `key` to at least `value` (a level, a
    /// number of worlds) and pays what that reaches.
    pub(super) fn count_at_least(&mut self, id: u32, key: &str, value: u64) {
        self.count_with(id, |l, book| l.raise(book, key, value));
    }

    fn count_with(
        &mut self,
        id: u32,
        f: impl FnOnce(&mut bm_core::feats::Ledger, &bm_core::feats::FeatBook) -> Vec<bm_core::feats::Reached>,
    ) {
        let Some(p) = self.players.get(&id) else { return };
        let account = p.account.clone();
        let Some(ledger) = self.ledgers.get_mut(&account) else {
            return;
        };
        let reached = f(ledger, &self.feat_book);
        self.ledgers_dirty.insert(account);
        self.ledgers_unsent.insert(id);
        for r in reached {
            let name = self.feat_book.get(&r.feat).map_or(r.feat.clone(), |d| d.name.clone());
            self.notice(id, &format!("Feat: {name} {}, {} coin", roman(r.tier), r.coin));
            self.add_xp(id, r.xp);
        }
    }

    /// Crafter feats (milestone 24): `count` crafts of recipe `index` at
    /// `station` (counted when queued), and each recipe the first time.
    pub(super) fn crafted(&mut self, id: u32, station: Station, index: u16, count: u16) {
        let key = match station {
            Station::Hand => "craft:hand",
            Station::CraftingTable => "craft:crafting_table",
            Station::Furnace => "craft:furnace",
            Station::Workbench => "craft:workbench",
            Station::Extractor => "craft:extractor",
            Station::Mixer => "craft:mixer",
            Station::Refinery => "craft:refinery",
            Station::Compactor => "craft:compactor",
            _ => return,
        };
        self.count(id, key, count as u64);
        let first = format!("recipe:{index}");
        if self.ledger_of(id).is_some_and(|l| l.count(&first) == 0) {
            self.count(id, &first, 1);
            self.count(id, "recipes", 1);
        }
    }

    /// Counts the whole blocks each player has moved since last counted.
    pub(super) fn count_moves(&mut self) {
        const KEYS: [&str; 4] = ["sprinted", "swum", "climbed", "travelled"];
        let mut whole = Vec::new();
        for (id, p) in &mut self.players {
            for (i, m) in p.moved.iter_mut().enumerate() {
                if *m >= 1.0 {
                    let n = m.floor();
                    *m -= n;
                    whole.push((*id, KEYS[i], n as u64));
                }
            }
        }
        for (id, key, n) in whole {
            self.count(id, key, n);
        }
    }

    /// Player `id`'s coin and feats.
    pub(super) fn ledger_of(&self, id: u32) -> Option<&bm_core::feats::Ledger> {
        self.players.get(&id).and_then(|p| self.ledgers.get(&p.account))
    }

    /// Sends each player whose coin or feats changed since last sent.
    pub(super) fn send_ledgers(&mut self) {
        for id in std::mem::take(&mut self.ledgers_unsent) {
            if let Some(p) = self.players.get(&id)
                && let Some(l) = self.ledgers.get(&p.account)
            {
                p.out.send(&ServerMsg::Ledger(l.clone()));
            }
        }
    }
}

/// "I" to "V".
pub(super) fn roman(n: u8) -> &'static str {
    ["", "I", "II", "III", "IV", "V"][n.min(5) as usize]
}
