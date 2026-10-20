//! Swinging at blocks, on the client (milestone 9): when the next swing may
//! land, what it does to every block it hits, and the damage each block has
//! taken so far. Pure logic, so it is tested without a window.
//!
//! The server rules (ADR-0004); this predicts it with the same `bm-core`
//! functions, the same inputs and the same order:
//! - a swing at input `at` is allowed when `at >= last_at + last_ticks`, the
//!   input and interval of the swing before it, whatever it hit;
//! - it hits `mining::aoe_blocks(tool.aoe, hit, look at that input, player,
//!   at)`, each wrapped to its canonical position, skipping air and blocks
//!   that cannot be broken;
//! - a critical roll for each block, `mining::is_crit(chance, player, at,
//!   pos)` with that block's canonical position;
//! - each block takes `mining::swing_damage(tool, def, tier, crit, area)`,
//!   `area` the shape's multiplier when the tool has one (1 otherwise), and
//!   breaks when its damage reaches `mining::block_health(def, tier)`.
//!
//! The damage map holds the server's word for each block (`BlockHealth`)
//! plus our own swings it has not answered yet, so the bar never steps back
//! while answers are in flight. A block left alone for
//! `mining::HEAL_TICKS` heals (its entry goes), on our input clock.

use bm_core::items::{Aoe, ToolStats};
use bm_core::mining::{HEAL_TICKS, aoe_blocks, block_health, is_crit, swing_damage};
use bm_core::{BlockDef, BlockId, BlockPos, BlockRegistry, Mount, PlanetShape};
use glam::DVec3;
use std::collections::{HashMap, VecDeque};

/// The player's swing cooldown: the input and interval of the last swing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Swinger {
    last: Option<(u32, u32)>,
}

impl Swinger {
    /// The first input a swing may follow.
    pub fn ready_at(&self) -> u32 {
        self.last.map_or(0, |(at, ticks)| at.saturating_add(ticks))
    }

    /// The input a swing lands on while break is held through inputs
    /// `first..=latest` (this frame's): as early as the cooldown allows, but
    /// never on an input from before this frame. None while cooling down.
    pub fn next(&self, first: u32, latest: u32) -> Option<u32> {
        let ready = self.ready_at();
        (ready <= latest).then(|| ready.max(first.min(latest)))
    }

    /// A swing was made at input `at` with an item swinging every `ticks`.
    pub fn swung(&mut self, at: u32, ticks: u32) {
        self.last = Some((at, ticks));
    }
}

/// What one swing does to one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHit {
    /// Canonical position.
    pub pos: BlockPos,
    pub block: BlockId,
    pub damage: u32,
    /// Its damage reaches its health: it breaks.
    pub breaks: bool,
}

/// Everything a swing's prediction needs.
pub struct SwingInput<'a> {
    pub tool: &'a ToolStats,
    /// The block aimed at.
    pub hit: BlockPos,
    /// Look direction at input `at` (`movement::look_dir`).
    pub look: DVec3,
    pub player: u32,
    pub at: u32,
    pub tier: u8,
    pub shape: PlanetShape,
    pub registry: &'a BlockRegistry,
}

/// The blocks a swing hits and what it does to each, given the damage they
/// have taken (`map`) and what each block is (`block_at`, None if unknown:
/// skipped, as an unloaded block is).
pub fn predict_swing(s: &SwingInput, map: &DamageMap, block_at: impl Fn(BlockPos) -> Option<BlockId>) -> Vec<BlockHit> {
    let hit = s.shape.wrap_block(s.hit);
    let area = s.tool.aoe.map_or(1.0, Aoe::multiplier);
    let mut out: Vec<BlockHit> = Vec::new();
    for p in aoe_blocks(s.tool.aoe, hit, s.look, s.player, s.at) {
        if !s.shape.contains_y(p.y) {
            continue;
        }
        let pos = s.shape.wrap_block(p);
        if out.iter().any(|h| h.pos == pos) {
            continue;
        }
        let Some(block) = block_at(pos) else { continue };
        let def = s.registry.get(block);
        if block.is_air() || !def.breakable {
            continue;
        }
        let crit = is_crit(s.tool.crit_chance, s.player, s.at, pos);
        let damage = swing_damage(s.tool, def, s.tier, crit, area);
        let breaks = damage > 0 && map.damage(pos) + damage >= block_health(def, s.tier);
        out.push(BlockHit {
            pos,
            block,
            damage,
            breaks,
        });
    }
    out
}

/// One of our swings the server has not answered yet.
#[derive(Clone, Copy, Debug)]
struct Pending {
    seq: u32,
    at: u32,
    damage: u32,
    breaks: bool,
}

#[derive(Clone, Debug, Default)]
struct Entry {
    /// The server's last word (`BlockHealth`).
    server: u32,
    pending: VecDeque<Pending>,
    /// Input tick of the last hit we know of, for healing.
    last_hit: u32,
}

/// Damage blocks have taken, by canonical position.
#[derive(Clone, Debug, Default)]
pub struct DamageMap {
    map: HashMap<BlockPos, Entry>,
}

impl DamageMap {
    /// Damage `pos` has taken, our unanswered swings included.
    pub fn damage(&self, pos: BlockPos) -> u32 {
        self.map
            .get(&pos)
            .map_or(0, |e| e.server + e.pending.iter().map(|p| p.damage).sum::<u32>())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Our swing `seq` at input `at` dealt `hit.damage` to `hit.pos`. A swing
    /// that deals nothing changes nothing (the server sends nothing for it).
    pub fn hit(&mut self, seq: u32, at: u32, hit: &BlockHit) {
        if hit.damage == 0 {
            return;
        }
        let e = self.map.entry(hit.pos).or_default();
        e.pending.push_back(Pending {
            seq,
            at,
            damage: hit.damage,
            breaks: hit.breaks,
        });
        e.last_hit = e.last_hit.max(at);
    }

    /// The server says `pos` has taken `damage` (`BlockHealth`), at our input
    /// tick `now`. It answers our oldest unanswered swing there, if any; if
    /// that swing was predicted to break the block, the server did not break
    /// it, and its edit seq is returned so the prediction can be undone.
    pub fn server_health(&mut self, pos: BlockPos, damage: u32, now: u32) -> Option<u32> {
        let e = self.map.entry(pos).or_default();
        e.server = damage;
        let answered = e.pending.pop_front();
        match answered {
            // Ours: the hit was when we swung.
            Some(p) => e.last_hit = e.last_hit.max(p.at),
            // Someone else's (or a reset): it happened about now.
            None => e.last_hit = now,
        }
        if damage == 0 && e.pending.is_empty() {
            self.map.remove(&pos);
        }
        answered.filter(|p| p.breaks).map(|p| p.seq)
    }

    /// The block at `pos` changed (`BlockChanged`, or a settled edit): it
    /// starts again at full health.
    pub fn clear(&mut self, pos: BlockPos) {
        self.map.remove(&pos);
    }

    /// Drops blocks with no hit for `HEAL_TICKS` (they heal to full) at input
    /// tick `now`. Our swings still unanswered after that long are given up
    /// on too: the server's word stands.
    pub fn heal(&mut self, now: u32) {
        let heal = HEAL_TICKS as u32;
        self.map.retain(|_, e| {
            e.pending.retain(|p| now < p.at.saturating_add(heal));
            !e.pending.is_empty() || now < e.last_hit.saturating_add(heal)
        });
    }

    pub fn reset(&mut self) {
        self.map.clear();
    }
}

/// What is left of a block, 0 to 1, for the health bar.
pub fn health_left(def: &BlockDef, tier: u8, damage: u32) -> f32 {
    let full = block_health(def, tier).max(1);
    1.0 - (damage.min(full) as f32 / full as f32)
}

/// Where a prop goes when the face with outward `normal` of a block is
/// clicked: on top (`Floor`), or against a side (the wall mount whose
/// support points back at the clicked block). None for a bottom face, or a
/// ray that started inside the block.
pub fn mount_for_face(normal: [i64; 3]) -> Option<Mount> {
    if normal == [0, 0, 0] {
        return None;
    }
    Mount::from_support((-normal[0], -normal[1], -normal[2]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::ItemRegistry;
    use bm_core::mining::swing_ticks;

    fn reg() -> (BlockRegistry, ItemRegistry) {
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        (b, i)
    }

    fn tool(i: &ItemRegistry, name: &str) -> ToolStats {
        i.swing_tool(Some(i.id(name).unwrap())).unwrap()
    }

    #[test]
    fn swings_follow_the_cooldown_of_the_last_item() {
        let mut s = Swinger::default();
        // At once on the first press.
        assert_eq!(s.next(1, 3), Some(1));
        s.swung(1, swing_ticks(30.0));
        assert_eq!(s.ready_at(), 36);
        assert_eq!(s.next(4, 20), None, "cooling down");
        // Held through the ready input: the swing lands on it exactly, so
        // frames do not stretch the cadence.
        assert_eq!(s.next(34, 37), Some(36));
        // Released and pressed again long after: this frame's first input.
        assert_eq!(s.next(90, 92), Some(90));
        // A frame with no new input swings on the latest.
        assert_eq!(s.next(51, 50), Some(50));
        // Switching to a slower item: the last swing's interval still rules.
        s.swung(36, 35);
        s.swung(71, swing_ticks(10.0));
        assert_eq!(s.ready_at(), 71 + swing_ticks(10.0));
    }

    #[test]
    fn copper_breaks_stone_in_three_predicted_swings() {
        let (b, i) = reg();
        let stone = b.id("stone").unwrap();
        let copper = tool(&i, "copper_hammer");
        let shape = PlanetShape::new(8, 4);
        let mut map = DamageMap::default();
        let pos = BlockPos::new(10, 20, 30);
        let mut broke = Vec::new();
        for (n, at) in [1u32, 36, 71].into_iter().enumerate() {
            let input = SwingInput {
                tool: &copper,
                hit: pos,
                look: DVec3::X,
                player: 1,
                at,
                tier: 1,
                shape,
                registry: &b,
            };
            let hits = predict_swing(&input, &map, |_| Some(stone));
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].damage, 400);
            map.hit(n as u32 + 1, at, &hits[0]);
            broke.push(hits[0].breaks);
        }
        assert_eq!(broke, [false, false, true]);
        assert_eq!(map.damage(pos), 1200);
        // On tier 3 (armour 100) it takes six.
        let mut map = DamageMap::default();
        let mut n = 0;
        loop {
            n += 1;
            let input = SwingInput {
                tool: &copper,
                hit: pos,
                look: DVec3::X,
                player: 1,
                at: n * 35,
                tier: 3,
                shape,
                registry: &b,
            };
            let h = predict_swing(&input, &map, |_| Some(stone))[0];
            map.hit(n, n * 35, &h);
            if h.breaks {
                break;
            }
        }
        assert_eq!(n, 6);
    }

    #[test]
    fn swings_skip_air_and_the_unbreakable_and_wrap() {
        let (b, i) = reg();
        let stone = b.id("stone").unwrap();
        let bedrock = b.id("bedrock").unwrap();
        let shape = PlanetShape::new(8, 4);
        let w = shape.width_blocks();
        let wide = ToolStats {
            aoe: Some(Aoe::ThreeByOne),
            crit_chance: 0.0,
            ..tool(&i, "iron_hammer")
        };
        // Aimed across the seam: the row runs along z at x = width.
        let input = SwingInput {
            tool: &wide,
            hit: BlockPos::new(w, 20, 0),
            look: DVec3::X,
            player: 1,
            at: 5,
            tier: 1,
            shape,
            registry: &b,
        };
        let map = DamageMap::default();
        let hits = predict_swing(&input, &map, |p| {
            Some(match p.z {
                0 => stone,
                1 => bedrock,
                _ => BlockId::AIR,
            })
        });
        assert_eq!(hits.len(), 1, "air and bedrock are skipped: {hits:?}");
        assert_eq!(hits[0].pos, BlockPos::new(0, 20, 0), "canonical");
        // The row's multiplier applies even when only one block is hit.
        assert_eq!(hits[0].damage, (700.0 * 0.4) as u32);
    }

    #[test]
    fn the_server_answers_our_swings_in_order() {
        let (b, _) = reg();
        let stone = b.id("stone").unwrap();
        let pos = BlockPos::new(1, 2, 3);
        let hit = |damage, breaks| BlockHit {
            pos,
            block: stone,
            damage,
            breaks,
        };
        let mut map = DamageMap::default();
        map.hit(1, 10, &hit(400, false));
        map.hit(2, 45, &hit(400, false));
        assert_eq!(map.damage(pos), 800);
        // The answer to the first does not step the bar back.
        assert_eq!(map.server_health(pos, 400, 50), None);
        assert_eq!(map.damage(pos), 800);
        assert_eq!(map.server_health(pos, 800, 60), None);
        assert_eq!(map.damage(pos), 800);
        // A predicted break the server answers with health was not a break.
        map.hit(3, 80, &hit(400, true));
        assert_eq!(map.server_health(pos, 800, 90), Some(3));
        assert_eq!(map.damage(pos), 800);
        // Someone else's hit, then the block changes.
        map.server_health(pos, 1000, 95);
        assert_eq!(map.damage(pos), 1000);
        map.clear(pos);
        assert_eq!(map.damage(pos), 0);
        // A reset to 0 with nothing of ours in flight forgets the block.
        map.server_health(pos, 0, 99);
        assert!(map.is_empty());
        // Zero-damage swings are not waited for.
        map.hit(4, 100, &hit(0, false));
        assert!(map.is_empty());
    }

    #[test]
    fn damage_heals_after_five_seconds_alone() {
        let (b, _) = reg();
        let stone = b.id("stone").unwrap();
        let pos = BlockPos::new(1, 2, 3);
        let mut map = DamageMap::default();
        map.hit(
            1,
            100,
            &BlockHit {
                pos,
                block: stone,
                damage: 400,
                breaks: false,
            },
        );
        map.server_health(pos, 400, 110);
        let heal = HEAL_TICKS as u32;
        map.heal(100 + heal - 1);
        assert_eq!(map.damage(pos), 400, "not yet");
        map.heal(100 + heal);
        assert_eq!(map.damage(pos), 0, "healed from our hit, not from the answer");
        // Someone else's hit heals from when we heard of it.
        map.server_health(pos, 400, 1000);
        map.heal(1000 + heal - 1);
        assert_eq!(map.damage(pos), 400);
        map.heal(1000 + heal);
        assert!(map.is_empty());
        // A swing never answered is given up on after as long.
        map.hit(
            2,
            2000,
            &BlockHit {
                pos,
                block: stone,
                damage: 400,
                breaks: false,
            },
        );
        map.heal(2000 + heal);
        assert!(map.is_empty());
    }

    #[test]
    fn props_go_where_the_face_points() {
        assert_eq!(mount_for_face([0, 1, 0]), Some(Mount::Floor), "on top");
        assert_eq!(mount_for_face([0, -1, 0]), None, "never under");
        assert_eq!(mount_for_face([0, 0, 0]), None);
        // The east face of a block (+x): the torch is east of it and hangs
        // on the block to its west.
        assert_eq!(mount_for_face([1, 0, 0]), Some(Mount::West));
        assert_eq!(mount_for_face([-1, 0, 0]), Some(Mount::East));
        assert_eq!(mount_for_face([0, 0, 1]), Some(Mount::North));
        assert_eq!(mount_for_face([0, 0, -1]), Some(Mount::South));
        for n in [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, 1, 0]] {
            let m = mount_for_face(n).unwrap();
            let (x, y, z) = m.support();
            assert_eq!([x, y, z], [-n[0], -n[1], -n[2]], "{m:?} hangs on the clicked block");
        }
    }

    #[test]
    fn health_left_reads_as_a_fraction() {
        let (b, _) = reg();
        let stone = b.get(b.id("stone").unwrap());
        assert_eq!(health_left(stone, 1, 0), 1.0);
        assert_eq!(health_left(stone, 1, 600), 0.5);
        assert_eq!(health_left(stone, 3, 900), 0.5);
        assert_eq!(health_left(stone, 1, 5000), 0.0);
    }
}
