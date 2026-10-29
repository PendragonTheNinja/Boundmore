//! Reclaim: what comes back to an account when one of its claims lapses
//! (milestone 6). The contents of the claim's stations and storage blocks
//! wait here, without limit, until the owner takes them at any warp shrine;
//! notices about lapses that happened while they were away wait with them.
//!
//! Stacks merge by item and colour (never a worn tool), so a lapse of a
//! hundred chests is a short list. Saved by item name in the same write batch
//! as the chunks the lapse turned to ash, so a crash can neither lose the
//! items nor leave them in both places.

use crate::store::{SavedReclaim, SavedStack};
use bm_core::{Inventory, ItemRegistry, ItemStack};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reclaim {
    pub items: Vec<ItemStack>,
    pub notices: Vec<String>,
}

impl Reclaim {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.notices.is_empty()
    }
}

/// Adds a stack to a list, onto a matching stack while it has room (a count
/// is a `u16`), else as a new entry.
pub fn merge(list: &mut Vec<ItemStack>, s: ItemStack) {
    if s.count == 0 {
        return;
    }
    let mut left = s.count;
    for e in list.iter_mut().filter(|e| e.stacks_with(&s)) {
        let n = left.min(u16::MAX - e.count);
        e.count += n;
        left -= n;
        if left == 0 {
            return;
        }
    }
    list.push(ItemStack { count: left, ..s });
}

/// Every account's reclaim.
#[derive(Default)]
pub struct Reclaims {
    map: HashMap<String, Reclaim>,
    /// Changed since the last save.
    dirty: BTreeSet<String>,
}

impl Reclaims {
    /// Rebuilds from the save. Items no longer in the catalogue are dropped
    /// with a warning.
    pub fn load(saved: Vec<(String, SavedReclaim)>, items: &ItemRegistry) -> Self {
        let mut r = Self::default();
        for (account, saved) in saved {
            let mut e = Reclaim {
                notices: saved.notices,
                ..Default::default()
            };
            for s in &saved.items {
                match s.resolve(items) {
                    Some(st) => merge(&mut e.items, st),
                    None => tracing::warn!(
                        "reclaim of {account}: {} × `{}` is no longer an item; dropped",
                        s.count,
                        s.item
                    ),
                }
            }
            if e.is_empty() {
                r.dirty.insert(account);
            } else {
                r.map.insert(account, e);
            }
        }
        r
    }

    /// The items waiting for an account (empty if none).
    pub fn items(&self, account: &str) -> &[ItemStack] {
        self.map.get(account).map_or(&[], |e| e.items.as_slice())
    }

    pub fn add(&mut self, account: &str, stacks: impl IntoIterator<Item = ItemStack>) {
        let e = self.map.entry(account.to_string()).or_default();
        for s in stacks {
            merge(&mut e.items, s);
        }
        self.tidy(account);
    }

    /// Keeps a notice for the account's next visit.
    pub fn add_notice(&mut self, account: &str, text: String) {
        self.map.entry(account.to_string()).or_default().notices.push(text);
        self.dirty.insert(account.to_string());
    }

    /// Takes the account's unread notices.
    pub fn take_notices(&mut self, account: &str) -> Vec<String> {
        let Some(e) = self.map.get_mut(account) else {
            return Vec::new();
        };
        let n = std::mem::take(&mut e.notices);
        if !n.is_empty() {
            self.tidy(account);
        }
        n
    }

    /// Moves as much as fits into `inv`, in list order. Returns whether
    /// anything moved.
    pub fn collect(&mut self, account: &str, inv: &mut Inventory, items: &ItemRegistry) -> bool {
        let Some(e) = self.map.get_mut(account) else {
            return false;
        };
        let mut moved = false;
        e.items.retain_mut(|s| {
            let left = if s.wear > 0 {
                // A worn tool keeps its wear, in a slot of its own.
                match inv.slots.iter_mut().find(|slot| slot.is_none()) {
                    Some(slot) => {
                        *slot = Some(*s);
                        0
                    }
                    None => s.count,
                }
            } else {
                inv.add_coloured(items, s.item, s.colour, s.count)
            };
            moved |= left != s.count;
            s.count = left;
            left > 0
        });
        if moved {
            self.tidy(account);
        }
        moved
    }

    fn tidy(&mut self, account: &str) {
        if self.map.get(account).is_some_and(Reclaim::is_empty) {
            self.map.remove(account);
        }
        self.dirty.insert(account.to_string());
    }

    /// Changed reclaims for the save; None for one now empty.
    pub fn take_dirty(&mut self, items: &ItemRegistry) -> Vec<(String, Option<SavedReclaim>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|account| {
                let saved = self.map.get(&account).map(|e| SavedReclaim {
                    items: e.items.iter().filter_map(|s| SavedStack::new(items, s)).collect(),
                    notices: e.notices.clone(),
                });
                (account, saved)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{BlockRegistry, Colour};

    fn items() -> ItemRegistry {
        ItemRegistry::builtin(&BlockRegistry::builtin())
    }

    #[test]
    fn stacks_merge_by_item_and_colour_but_never_worn_tools() {
        let items = items();
        let stone = items.id("stone").unwrap();
        let hammer = items.id("stone_hammer").unwrap();
        let mut list = Vec::new();
        merge(&mut list, ItemStack::new(stone, 300));
        merge(&mut list, ItemStack::new(stone, 200));
        merge(&mut list, ItemStack::coloured(stone, Colour(9), 5));
        let worn = ItemStack {
            wear: 4,
            ..ItemStack::new(hammer, 1)
        };
        merge(&mut list, worn);
        merge(&mut list, worn);
        merge(&mut list, ItemStack::new(stone, 0));
        assert_eq!(
            list,
            vec![
                ItemStack::new(stone, 500),
                ItemStack::coloured(stone, Colour(9), 5),
                worn,
                worn
            ]
        );
        // A count past u16 starts another entry.
        merge(&mut list, ItemStack::new(stone, u16::MAX));
        assert_eq!(list[0].count, u16::MAX);
        assert_eq!(list[4], ItemStack::new(stone, 500));
    }

    #[test]
    fn collecting_moves_what_fits_and_keeps_the_rest() {
        let items = items();
        let stone = items.id("stone").unwrap();
        let sand = items.id("sand").unwrap();
        let hammer = items.id("stone_hammer").unwrap();
        let worn = ItemStack {
            wear: 4,
            ..ItemStack::new(hammer, 1)
        };
        let mut r = Reclaims::default();
        r.add(
            "alice",
            [
                ItemStack::coloured(stone, Colour(3), 250),
                worn,
                ItemStack::new(sand, 10),
            ],
        );
        assert_eq!(r.items("alice").len(), 3);
        assert!(r.items("bob").is_empty());

        // Three slots: the stone, first in the list, fills them all and
        // nothing else fits.
        let mut inv = Inventory::row(vec![None; 3]);
        assert!(r.collect("alice", &mut inv, &items));
        assert_eq!(
            inv.slots,
            vec![
                Some(ItemStack::coloured(stone, Colour(3), 100)),
                Some(ItemStack::coloured(stone, Colour(3), 100)),
                Some(ItemStack::coloured(stone, Colour(3), 50)),
            ]
        );
        assert_eq!(r.items("alice"), &[worn, ItemStack::new(sand, 10)]);
        assert!(!r.collect("alice", &mut inv, &items), "nothing fits now");

        let mut big = Inventory::default();
        assert!(r.collect("alice", &mut big, &items));
        assert_eq!(big.slots[0], Some(worn), "wear kept");
        assert_eq!(big.count(sand), 10);
        assert!(r.items("alice").is_empty());
        assert_eq!(r.take_dirty(&items), vec![("alice".to_string(), None)]);
    }

    #[test]
    fn reclaims_and_notices_survive_a_save_round_trip() {
        let items = items();
        let stone = items.id("stone").unwrap();
        let mut r = Reclaims::default();
        r.add("alice", [ItemStack::coloured(stone, Colour(77), 7)]);
        r.add_notice("alice", "Gone".into());
        r.add_notice("bob", "Also gone".into());
        let saved: Vec<_> = r.take_dirty(&items).into_iter().map(|(a, s)| (a, s.unwrap())).collect();
        assert_eq!(saved[0].1.items[0].item, "stone#77");
        let mut t = Reclaims::load(saved, &items);
        assert_eq!(t.items("alice"), &[ItemStack::coloured(stone, Colour(77), 7)]);
        assert_eq!(t.take_notices("alice"), vec!["Gone".to_string()]);
        assert!(t.take_notices("alice").is_empty());
        assert_eq!(t.take_notices("bob"), vec!["Also gone".to_string()]);
        let dirty = t.take_dirty(&items);
        assert_eq!(dirty.len(), 2);
        assert!(
            dirty.iter().any(|(a, s)| a == "bob" && s.is_none()),
            "an emptied reclaim is deleted"
        );
        assert!(
            dirty
                .iter()
                .any(|(a, s)| a == "alice" && s.as_ref().is_some_and(|s| s.notices.is_empty()))
        );
    }
}
