//! Written signs on one planet (milestone 22 part 4): each by its anchor
//! (the lower left panel as read), with every block it covers, so a change
//! to any of them finds it. Saved in the `planet_signs` table by anchor.

use bm_core::signs::SignView;
use bm_core::{BlockPos, ChunkPos};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// The layout of a saved sign. Bump with any change to [`SavedSign`].
pub const SIGN_VERSION: u8 = 1;

/// A written sign as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSign {
    pub version: u8,
    pub view: SignView,
    /// Every block the sign covers (the anchor first), canonical.
    pub members: Vec<BlockPos>,
}

#[derive(Default)]
pub struct Signs {
    by_anchor: HashMap<BlockPos, SavedSign>,
    anchor_of: HashMap<BlockPos, BlockPos>,
    by_chunk: HashMap<ChunkPos, BTreeSet<BlockPos>>,
    dirty: BTreeSet<BlockPos>,
}

impl Signs {
    /// From saved rows; a row of another layout, or overlapping one already
    /// read, is reported and skipped.
    pub fn load(rows: Vec<(BlockPos, SavedSign)>) -> Self {
        let mut s = Signs::default();
        for (anchor, sign) in rows {
            if sign.version != SIGN_VERSION || sign.members.iter().any(|m| s.anchor_of.contains_key(m)) {
                tracing::error!("a saved sign at {anchor:?} is unreadable or overlaps another; skipped");
                continue;
            }
            s.insert(anchor, sign);
        }
        s.dirty.clear();
        s
    }

    fn insert(&mut self, anchor: BlockPos, sign: SavedSign) {
        for m in &sign.members {
            self.anchor_of.insert(*m, anchor);
        }
        self.by_chunk.entry(anchor.chunk()).or_default().insert(anchor);
        self.by_anchor.insert(anchor, sign);
        self.dirty.insert(anchor);
    }

    /// The anchor of the written sign covering `p`, if any.
    pub fn anchor_of(&self, p: BlockPos) -> Option<BlockPos> {
        self.anchor_of.get(&p).copied()
    }

    pub fn get(&self, anchor: BlockPos) -> Option<&SavedSign> {
        self.by_anchor.get(&anchor)
    }

    /// Writes the sign at `anchor` over `members` (replacing any there).
    pub fn write(&mut self, anchor: BlockPos, view: SignView, members: Vec<BlockPos>) {
        self.remove(anchor);
        self.insert(
            anchor,
            SavedSign {
                version: SIGN_VERSION,
                view,
                members,
            },
        );
    }

    /// Unwrites the sign at `anchor`.
    pub fn remove(&mut self, anchor: BlockPos) -> Option<SavedSign> {
        let sign = self.by_anchor.remove(&anchor)?;
        for m in &sign.members {
            self.anchor_of.remove(m);
        }
        if let Some(set) = self.by_chunk.get_mut(&anchor.chunk()) {
            set.remove(&anchor);
            if set.is_empty() {
                self.by_chunk.remove(&anchor.chunk());
            }
        }
        self.dirty.insert(anchor);
        Some(sign)
    }

    /// The signs anchored in chunk `c`.
    pub fn in_chunk(&self, c: ChunkPos) -> impl Iterator<Item = (BlockPos, &SignView)> {
        self.by_chunk
            .get(&c)
            .into_iter()
            .flatten()
            .filter_map(|a| self.by_anchor.get(a).map(|s| (*a, &s.view)))
    }

    pub fn len(&self) -> usize {
        self.by_anchor.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_anchor.is_empty()
    }

    /// What changed since the last call: each anchor's sign, or None if it
    /// was unwritten.
    pub fn take_dirty(&mut self) -> Vec<(BlockPos, Option<SavedSign>)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|a| (a, self.by_anchor.get(&a).cloned()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(t: &str) -> SignView {
        SignView {
            text: bm_core::signs::SignText {
                text: t.into(),
                ..Default::default()
            },
            across: 2,
            up: 1,
        }
    }

    #[test]
    fn signs_are_found_by_any_block_and_their_chunk_and_saved_once() {
        let (a, b) = (BlockPos::new(40, 10, 5), BlockPos::new(39, 10, 5));
        let mut s = Signs::default();
        s.write(a, view("Hello"), vec![a, b]);
        assert_eq!(s.anchor_of(b), Some(a));
        assert_eq!(s.in_chunk(a.chunk()).count(), 1);
        let saved = s.take_dirty();
        assert_eq!(saved.len(), 1);
        assert!(s.take_dirty().is_empty());
        let back = Signs::load(vec![(a, saved[0].1.clone().unwrap())]);
        assert_eq!(back.get(a).unwrap().view, view("Hello"));
        assert!(back.dirty.is_empty());
        s.remove(a);
        assert_eq!(s.anchor_of(b), None);
        assert_eq!(s.in_chunk(a.chunk()).count(), 0);
        assert_eq!(s.take_dirty(), vec![(a, None)]);
    }
}
