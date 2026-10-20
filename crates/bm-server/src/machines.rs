//! Machines made of several blocks (milestone 11, as in Boundless).
//!
//! - **Four-block machines** (`Assembly::Four`: the workbench, the
//!   extractor) work only when exactly 4 blocks of the same machine touch by
//!   faces, in any shape. A fifth touching block is refused when placed
//!   ([`exceeds_four`]), so a working group never changes while it stands; a
//!   cluster of more than 4 left over from before milestone 11 does not work
//!   and says so.
//! - **The furnace** is a crucible (`Assembly::FurnaceCrucible`) directly on
//!   a base (`Assembly::FurnaceBase`); either alone does nothing.
//!
//! A machine's station record (`crafting::Stations`) is kept at its
//! [`Machine::key`]: a group's first block (lowest y, then z, then x, of the
//! canonical positions), a furnace's base, or the block itself. Any block of
//! a machine addresses it. These are pure functions over a block lookup, so
//! the rules are tested without a world.

use bm_core::block::Assembly;
use bm_core::{BlockId, BlockPos, BlockRegistry, PlanetShape, Station};

/// Said by a furnace that is not a crucible on a base.
pub const FURNACE_IDLE: &str = "A furnace needs its crucible on top";

/// The six face neighbours.
const FACES: [(i64, i64, i64); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];

/// A machine as the blocks around one of its blocks make it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    /// Where its station record is kept.
    pub key: BlockPos,
    pub kind: Station,
    /// Its blocks as they stand (the one asked about among them), in key
    /// order: the key first when it works.
    pub blocks: Vec<BlockPos>,
    /// Why it does not work, in plain words; None when it does.
    pub idle: Option<String>,
}

/// The order that picks a group's first block.
fn order(p: &BlockPos) -> (i64, i64, i64) {
    (p.y, p.z, p.x)
}

/// A block's name in a sentence: `workbench`, `furnace crucible`.
fn plain_name(reg: &BlockRegistry, id: BlockId) -> String {
    reg.get(id).name.replace('_', " ")
}

fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "An",
        _ => "A",
    }
}

/// Blocks of type `id` connected by faces to `start` (itself included, read
/// as `id` whatever is there), at most `limit` of them.
fn touching(
    shape: PlanetShape,
    block: &impl Fn(BlockPos) -> Option<BlockId>,
    start: BlockPos,
    id: BlockId,
    limit: usize,
) -> Vec<BlockPos> {
    let mut found = vec![start];
    let mut next = 0;
    while next < found.len() && found.len() < limit {
        let at = found[next];
        next += 1;
        for (dx, dy, dz) in FACES {
            let n = at.offset(dx, dy, dz);
            if !shape.contains_y(n.y) {
                continue;
            }
            let n = shape.wrap_block(n);
            if !found.contains(&n) && block(n) == Some(id) {
                found.push(n);
                if found.len() >= limit {
                    break;
                }
            }
        }
    }
    found
}

/// Whether placing a block of four-block machine `id` at `canon` would make
/// more than 4 of them touch.
pub fn exceeds_four(
    shape: PlanetShape,
    block: impl Fn(BlockPos) -> Option<BlockId>,
    canon: BlockPos,
    id: BlockId,
) -> bool {
    let with = |p: BlockPos| if p == canon { Some(id) } else { block(p) };
    touching(shape, &with, canon, id, 5).len() > 4
}

/// The machine the block at `canon` (canonical) belongs to; None if it is
/// not part of one.
pub fn machine_at(
    reg: &BlockRegistry,
    shape: PlanetShape,
    block: impl Fn(BlockPos) -> Option<BlockId>,
    canon: BlockPos,
) -> Option<Machine> {
    let id = block(canon)?;
    let def = reg.get(id);
    let is = |p: BlockPos, a: Assembly| {
        shape.contains_y(p.y) && block(shape.wrap_block(p)).is_some_and(|b| reg.get(b).assembly == Some(a))
    };
    let alone = |kind| Machine {
        key: canon,
        kind,
        blocks: vec![canon],
        idle: None,
    };
    match def.assembly {
        Some(Assembly::FurnaceCrucible) => {
            let below = canon.offset(0, -1, 0);
            Some(if is(below, Assembly::FurnaceBase) {
                let below = shape.wrap_block(below);
                Machine {
                    key: below,
                    kind: Station::Furnace,
                    blocks: vec![below, canon],
                    idle: None,
                }
            } else {
                Machine {
                    idle: Some(FURNACE_IDLE.into()),
                    ..alone(Station::Furnace)
                }
            })
        }
        Some(Assembly::FurnaceBase) => {
            let kind = def.station.unwrap_or(Station::Furnace);
            let above = canon.offset(0, 1, 0);
            Some(if is(above, Assembly::FurnaceCrucible) {
                Machine {
                    blocks: vec![canon, shape.wrap_block(above)],
                    ..alone(kind)
                }
            } else {
                Machine {
                    idle: Some(FURNACE_IDLE.into()),
                    ..alone(kind)
                }
            })
        }
        Some(Assembly::Four) => {
            let kind = def.station?;
            let mut group = touching(shape, &block, canon, id, 5);
            group.sort_by_key(order);
            let name = plain_name(reg, id);
            Some(match group.len() {
                4 => Machine {
                    key: group[0],
                    kind,
                    blocks: group,
                    idle: None,
                },
                n => Machine {
                    key: canon,
                    kind,
                    blocks: group,
                    idle: Some(if n < 4 {
                        format!("{} {name} needs 4 blocks touching", article(&name))
                    } else {
                        format!(
                            "{} {name} is exactly 4 blocks touching; break the others",
                            article(&name)
                        )
                    }),
                },
            })
        }
        None => def.station.map(alone),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Patch {
        reg: BlockRegistry,
        shape: PlanetShape,
        blocks: HashMap<BlockPos, BlockId>,
    }

    impl Patch {
        fn new() -> Self {
            Self {
                reg: BlockRegistry::builtin(),
                shape: PlanetShape::new(16, 4),
                blocks: HashMap::new(),
            }
        }
        fn put(&mut self, name: &str, p: BlockPos) {
            let id = self.reg.id(name).unwrap();
            self.blocks.insert(self.shape.wrap_block(p), id);
        }
        fn at(&self, p: BlockPos) -> Option<Machine> {
            let canon = self.shape.wrap_block(p);
            machine_at(&self.reg, self.shape, |q| self.blocks.get(&q).copied(), canon)
        }
        fn exceeds(&self, name: &str, p: BlockPos) -> bool {
            let id = self.reg.id(name).unwrap();
            exceeds_four(
                self.shape,
                |q| self.blocks.get(&q).copied(),
                self.shape.wrap_block(p),
                id,
            )
        }
    }

    #[test]
    fn four_touching_blocks_make_one_machine_keyed_by_its_first_block() {
        let mut w = Patch::new();
        // An L tetromino, standing up.
        let l = [
            BlockPos::new(5, 21, 3),
            BlockPos::new(5, 20, 3),
            BlockPos::new(6, 20, 3),
            BlockPos::new(7, 20, 3),
        ];
        for (i, p) in l.iter().enumerate().take(3) {
            w.put("workbench", *p);
            let m = w.at(*p).unwrap();
            assert_eq!(m.idle.as_deref(), Some("A workbench needs 4 blocks touching"), "{i}");
            assert_eq!(m.key, *p, "an incomplete machine keeps its record where it is");
        }
        w.put("workbench", l[3]);
        for p in l {
            let m = w.at(p).unwrap();
            assert_eq!(m.idle, None);
            assert_eq!(m.kind, Station::Workbench);
            assert_eq!(m.key, BlockPos::new(5, 20, 3), "lowest y, then z, then x");
            assert_eq!(m.blocks.len(), 4);
        }
        // A fifth touching is refused; one apart is not.
        assert!(w.exceeds("workbench", BlockPos::new(8, 20, 3)));
        assert!(w.exceeds("workbench", BlockPos::new(5, 20, 4)));
        assert!(!w.exceeds("workbench", BlockPos::new(9, 20, 3)));
        // Another machine's blocks do not count.
        assert!(!w.exceeds("extractor", BlockPos::new(8, 20, 3)));
        w.put("extractor", BlockPos::new(8, 20, 3));
        assert_eq!(
            w.at(BlockPos::new(8, 20, 3)).unwrap().idle.as_deref(),
            Some("An extractor needs 4 blocks touching")
        );
        assert_eq!(
            w.at(l[0]).unwrap().idle,
            None,
            "a neighbour of another kind changes nothing"
        );
        // Two groups of two joined by one more make five: refused.
        let mut w = Patch::new();
        for x in [0, 1, 3, 4] {
            w.put("workbench", BlockPos::new(x, 20, 0));
        }
        assert!(w.exceeds("workbench", BlockPos::new(2, 20, 0)));
        assert!(
            !w.exceeds("workbench", BlockPos::new(0, 21, 0)),
            "two and one more is three"
        );
    }

    #[test]
    fn groups_join_across_the_seam_and_too_many_do_not_work() {
        let mut w = Patch::new();
        // x = 511 and x = 0 touch on a planet 512 wide.
        for p in [(510, 0), (511, 0), (0, 0), (1, 0)] {
            w.put("workbench", BlockPos::new(p.0, 20, p.1));
        }
        let m = w.at(BlockPos::new(-1, 20, 0)).unwrap();
        assert_eq!(m.idle, None);
        assert_eq!(m.key, BlockPos::new(0, 20, 0));
        // A cluster of five, from before machines had four blocks.
        w.put("workbench", BlockPos::new(2, 20, 0));
        let m = w.at(BlockPos::new(0, 20, 0)).unwrap();
        assert_eq!(
            m.idle.as_deref(),
            Some("A workbench is exactly 4 blocks touching; break the others")
        );
        assert_eq!(m.key, BlockPos::new(0, 20, 0));
    }

    #[test]
    fn a_furnace_is_a_crucible_on_a_base() {
        let mut w = Patch::new();
        let base = BlockPos::new(3, 20, 3);
        w.put("furnace", base);
        let m = w.at(base).unwrap();
        assert_eq!((m.kind, m.idle.as_deref()), (Station::Furnace, Some(FURNACE_IDLE)));
        w.put("furnace_crucible", base.offset(1, 0, 0));
        assert_eq!(
            w.at(base).unwrap().idle.as_deref(),
            Some(FURNACE_IDLE),
            "beside is not on top"
        );
        let m = w.at(base.offset(1, 0, 0)).unwrap();
        assert_eq!((m.key, m.idle.as_deref()), (base.offset(1, 0, 0), Some(FURNACE_IDLE)));
        w.put("furnace_crucible", base.offset(0, 1, 0));
        for p in [base, base.offset(0, 1, 0)] {
            let m = w.at(p).unwrap();
            assert_eq!((m.key, m.kind, m.idle.clone()), (base, Station::Furnace, None));
            assert_eq!(m.blocks, vec![base, base.offset(0, 1, 0)]);
        }
    }

    #[test]
    fn single_block_stations_and_other_blocks() {
        let mut w = Patch::new();
        let p = BlockPos::new(1, 20, 1);
        w.put("crafting_table", p);
        let m = w.at(p).unwrap();
        assert_eq!((m.key, m.kind, m.idle), (p, Station::CraftingTable, None));
        w.put("stone", p);
        assert_eq!(w.at(p), None);
        assert_eq!(w.at(BlockPos::new(9, 20, 9)), None, "nothing there");
    }
}
