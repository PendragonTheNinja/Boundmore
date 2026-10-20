//! Damage blocks have taken from swings (milestone 9), per planet, in memory
//! only: it is never saved, and a block left alone heals to full after
//! `mining::HEAL_TICKS` (the entry is simply dropped; clients time healing
//! themselves).
//!
//! Each entry remembers the block it was dealt to, so a block that changed by
//! any path (an edit, a lapse, a portal opening, a chunk reloaded) never
//! inherits another block's damage, even where a caller forgot to clear it.

use bm_core::mining::HEAL_TICKS;
use bm_core::{BlockPos, BlockState};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    state: BlockState,
    damage: u32,
    /// Server tick of the last hit.
    last: u64,
}

impl Entry {
    fn live(&self, state: BlockState, now: u64) -> bool {
        self.state == state && now.saturating_sub(self.last) < HEAL_TICKS
    }
}

/// One planet's damaged blocks, by canonical position.
#[derive(Debug, Default)]
pub struct DamageMap {
    blocks: HashMap<BlockPos, Entry>,
}

impl DamageMap {
    /// Damage `state` at `pos` has taken and not yet healed by `now`.
    pub fn get(&self, pos: BlockPos, state: BlockState, now: u64) -> u32 {
        self.blocks
            .get(&pos)
            .filter(|e| e.live(state, now))
            .map_or(0, |e| e.damage)
    }

    /// Records a hit: `state` at `pos` has now taken `damage` in all.
    pub fn set(&mut self, pos: BlockPos, state: BlockState, damage: u32, now: u64) {
        if damage == 0 {
            self.blocks.remove(&pos);
        } else {
            self.blocks.insert(
                pos,
                Entry {
                    state,
                    damage,
                    last: now,
                },
            );
        }
    }

    /// Forgets `pos` (its block changed). Whether it had damage that had not
    /// healed by `now`.
    pub fn clear(&mut self, pos: BlockPos, now: u64) -> bool {
        self.blocks
            .remove(&pos)
            .is_some_and(|e| now.saturating_sub(e.last) < HEAL_TICKS)
    }

    /// Drops every entry healed by `now`.
    pub fn heal(&mut self, now: u64) {
        self.blocks.retain(|_, e| now.saturating_sub(e.last) < HEAL_TICKS);
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::BlockId;

    #[test]
    fn damage_heals_after_the_delay_and_never_moves_to_another_block() {
        let mut m = DamageMap::default();
        let p = BlockPos::new(1, 2, 3);
        let stone = BlockState::plain(BlockId(3));
        let soil = BlockState::plain(BlockId(4));
        m.set(p, stone, 400, 100);
        assert_eq!(m.get(p, stone, 100), 400);
        assert_eq!(m.get(p, stone, 100 + HEAL_TICKS - 1), 400);
        assert_eq!(m.get(p, stone, 100 + HEAL_TICKS), 0, "healed");
        assert_eq!(m.get(p, soil, 101), 0, "a different block starts fresh");
        // Another hit restarts the delay.
        m.set(p, stone, 800, 100 + HEAL_TICKS - 1);
        assert_eq!(m.get(p, stone, 100 + HEAL_TICKS + 10), 800);
        m.heal(100 + 2 * HEAL_TICKS);
        assert!(m.is_empty());
        m.set(p, stone, 5, 0);
        assert!(m.clear(p, 1));
        assert!(!m.clear(p, 1));
        m.set(p, stone, 0, 0);
        assert!(m.is_empty(), "no damage, no entry");
    }
}
