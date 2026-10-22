//! Breaking blocks the way Boundless did (milestone 9,
//! `docs/research/boundless-tuning.md` section 2).
//!
//! A tool swings once per [`swing_ticks`]. Each swing deals [`swing_damage`]
//! to every block it hits; a block breaks when the damage it has taken
//! reaches its [`block_health`], and heals back to full after
//! [`HEAL_TICKS`] without a hit. Health and armour come from the block's
//! [`Health`] category and the planet's tier.
//!
//! Everything here is a pure function of its inputs, and uses only plain
//! `f64` arithmetic, so the client's prediction and the server's ruling agree
//! exactly (critical hits included: [`is_crit`] is a hash, not a dice roll).

use crate::block::{BlockDef, Health};
use crate::coords::BlockPos;
use crate::items::{Aoe, ToolStats};
use crate::movement::TICK_HZ;
use crate::skills::Effects;
use glam::DVec3;

/// The highest world tier with numbers (tier 8 exists in Boundless's data
/// although no public world used it).
pub const MAX_TIER: u8 = 8;

/// Health multiplier for each tier, 1 to 8 [2.2]. Tier 2 is x1.0 (Nathan,
/// 2026-10-20).
const TIER_HEALTH: [f64; 8] = [1.0, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0];
/// Armour for each tier, 1 to 8: taken off every hit [2.2].
const TIER_ARMOUR: [u32; 8] = [0, 0, 100, 200, 400, 600, 800, 1000];

/// Swings a prop (a torch) takes, whatever hits it (wiki: "two hits").
pub const PROP_HITS: u32 = 2;

/// A damaged block heals to full after this long without a hit. Not
/// documented for Boundless: a guess (5 s), listed in `docs/feel.md`.
pub const HEAL_TICKS: u64 = 5 * TICK_HZ as u64;

/// Damage multiplier for a hit with the wrong kind of tool [2.4].
pub const WRONG_TOOL: f64 = 0.25;

fn tier_index(tier: u8) -> usize {
    (tier.clamp(1, MAX_TIER) - 1) as usize
}

/// The health multiplier of a planet of `tier` [2.2] (milestone 12 also
/// scales a gathering object's XP by it).
pub fn tier_health(tier: u8) -> f64 {
    TIER_HEALTH[tier_index(tier)]
}

/// Armour a block on a planet of `tier` takes off every hit.
pub fn armour(tier: u8) -> u32 {
    TIER_ARMOUR[tier_index(tier)]
}

/// The base health of a category, before the tier [2.2].
pub fn base_health(health: Health) -> u32 {
    match health {
        Health::Soft => 800,
        Health::Hard => 1200,
        Health::Seam => 2400,
        Health::Prop => PROP_HITS,
        Health::Plant => 800,
    }
}

/// Health of `def` on a planet of `tier`. A prop's health is counted in
/// swings ([`PROP_HITS`]) and ignores the tier.
pub fn block_health(def: &BlockDef, tier: u8) -> u32 {
    match def.health {
        Health::Prop => PROP_HITS,
        h => (base_health(h) as f64 * TIER_HEALTH[tier_index(tier)]) as u32,
    }
}

/// Seconds between swings at action speed `speed` (a13o's curve, [2.5a]):
/// 1 s at 5 down to 0.05 s at 150.
pub fn seconds_per_hit(speed: f32) -> f64 {
    let s = (speed as f64).clamp(5.0, 150.0);
    let t = 1.0 - (s - 5.0) / 145.0;
    0.05 + 0.95 * t * t * t
}

/// Ticks (60 Hz) between swings: [`seconds_per_hit`] rounded to whole ticks,
/// at least one.
pub fn swing_ticks(speed: f32) -> u32 {
    ((seconds_per_hit(speed) * TICK_HZ as f64).round() as u32).max(1)
}

/// Whether `tool` is the right kind for `def` (a tool with no class, the
/// hand stone, suits everything).
pub fn right_tool(tool: &ToolStats, def: &BlockDef) -> bool {
    tool.class.is_none_or(|c| c.suits(def.material))
}

/// Damage one swing of `tool` deals to `def` on a planet of `tier` [2.3]:
/// critical first, then armour, then a quarter for the wrong tool, then the
/// area multiplier `area` (1 for a single block, [`Aoe::multiplier`]
/// otherwise). Props take one point per swing, whatever the tool.
pub fn swing_damage(tool: &ToolStats, def: &BlockDef, tier: u8, crit: bool, area: f64) -> u32 {
    if def.health == Health::Prop {
        return 1;
    }
    let mut d = tool.damage as f64;
    if crit {
        d *= 1.0 + tool.crit_effect as f64;
    }
    // Gathering objects are never armoured [tuning 2.2].
    let armour = if def.health == Health::Plant { 0 } else { armour(tier) };
    d = (d - armour as f64).max(0.0);
    if !right_tool(tool, def) {
        d *= WRONG_TOOL;
    }
    (d * area).floor() as u32
}

/// `tool` as a character with skill `effects` swings it (milestone 12,
/// tuning 2.3): damage scaled by `Effects::damage_multiplier` and rounded
/// up (`dmg = ceil(e x skill_mult)`, before crit and armour), action speed
/// plus `Effects::action_speed_bonus`, crit chance plus
/// `Effects::crit_bonus` (at most 1). Client and server both pass the result
/// to [`swing_ticks`], [`is_crit`] and [`swing_damage`].
pub fn skilled_tool(tool: &ToolStats, effects: &Effects) -> ToolStats {
    skilled_tool_keen(tool, effects, 1.0)
}

/// [`skilled_tool`] with Keen food (milestone 16): it multiplies the crit
/// chance skills add (Boundless's criticaling food multiplies the agility
/// crit, tuning 3.3), `keen` from `PlayerState::crit_multiplier`.
pub fn skilled_tool_keen(tool: &ToolStats, effects: &Effects, keen: f32) -> ToolStats {
    // A hair under, so 400 x 2.85 is 1140, not 1141 from rounding error.
    let damage = (tool.damage as f64 * effects.damage_multiplier(tool.class) - 1e-9).ceil();
    ToolStats {
        damage: damage.max(0.0) as u32,
        action_speed: tool.action_speed + effects.action_speed_bonus(tool.class) as f32,
        crit_chance: (tool.crit_chance + effects.crit_bonus() as f32 * keen).min(1.0),
        ..*tool
    }
}

/// Swings `tool` needs to break a fresh `def` with no critical hits.
pub fn hits_to_break(tool: &ToolStats, def: &BlockDef, tier: u8) -> Option<u32> {
    let d = swing_damage(tool, def, tier, false, 1.0);
    (d > 0).then(|| block_health(def, tier).div_ceil(d))
}

fn mix(mut z: u64) -> u64 {
    // splitmix64's finaliser.
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A number in 0..1 fixed by the swing: who swung, at which input, at which
/// block. The same on client and server.
pub fn swing_roll(player: u32, at: u32, pos: BlockPos, salt: u64) -> f64 {
    let mut h = mix(salt ^ 0x6d69_6e65);
    h = mix(h ^ player as u64);
    h = mix(h ^ at as u64);
    h = mix(h ^ pos.x as u64);
    h = mix(h ^ pos.y as u64);
    h = mix(h ^ pos.z as u64);
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Whether this swing is a critical hit for a tool with `chance` (0..1).
pub fn is_crit(chance: f32, player: u32, at: u32, pos: BlockPos) -> bool {
    chance > 0.0 && swing_roll(player, at, pos, 1) < chance as f64
}

/// The axis (0 x, 1 y, 2 z) a look direction mostly points along.
fn main_axis(v: DVec3) -> usize {
    let a = v.abs();
    if a.y >= a.x && a.y >= a.z {
        1
    } else if a.x >= a.z {
        0
    } else {
        2
    }
}

fn unit(axis: usize) -> BlockPos {
    match axis {
        0 => BlockPos::new(1, 0, 0),
        1 => BlockPos::new(0, 1, 0),
        _ => BlockPos::new(0, 0, 1),
    }
}

fn add(p: BlockPos, d: BlockPos, k: i64) -> BlockPos {
    BlockPos::new(p.x + d.x * k, p.y + d.y * k, p.z + d.z * k)
}

/// The blocks one swing hits, the hit block first, for a tool with `aoe`
/// (None: just the hit block) looking along `look`. Shapes lie in the plane
/// facing the player (across the axis the look mostly points along) [2.6]:
/// a row of three across the view, the hit block and the one below, a plus
/// of five, a square of nine, or the hit block and 1 to 4 neighbours in that
/// plane picked by the swing (`player`, `at`). Positions are not wrapped.
pub fn aoe_blocks(aoe: Option<Aoe>, hit: BlockPos, look: DVec3, player: u32, at: u32) -> Vec<BlockPos> {
    let Some(aoe) = aoe else { return vec![hit] };
    let normal = main_axis(look);
    // The plane's two axes: `across` is horizontal and across the view,
    // `other` is the remaining one (vertical, unless looking up or down).
    let (across, other) = match normal {
        0 => (2, 1),
        2 => (0, 1),
        _ => {
            if look.x.abs() > look.z.abs() {
                (2, 0)
            } else {
                (0, 2)
            }
        }
    };
    let (a, o) = (unit(across), unit(other));
    let ring = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
    let at_ = |i: i64, j: i64| add(add(hit, a, i), o, j);
    match aoe {
        Aoe::OneByTwo => vec![hit, BlockPos::new(hit.x, hit.y - 1, hit.z)],
        Aoe::ThreeByOne => vec![hit, at_(-1, 0), at_(1, 0)],
        Aoe::Cross => vec![hit, at_(-1, 0), at_(1, 0), at_(0, -1), at_(0, 1)],
        Aoe::ThreeByThree => {
            let mut v = vec![hit];
            v.extend(ring.iter().map(|&(i, j)| at_(i, j)));
            v
        }
        Aoe::Random => {
            let extra = 1 + (swing_roll(player, at, hit, 2) * 4.0) as usize;
            let mut pool: Vec<BlockPos> = ring.iter().map(|&(i, j)| at_(i, j)).collect();
            let mut v = vec![hit];
            for k in 0..extra.min(4) {
                let r = (swing_roll(player, at, hit, 3 + k as u64) * pool.len() as f64) as usize;
                v.push(pool.swap_remove(r.min(pool.len() - 1)));
            }
            v
        }
    }
}

/// Extra items a block yields beyond its own drop, on a tier: `expected` is
/// the mean count (the whole part always, the rest as a chance), `roll` a
/// number in 0..1.
pub fn bonus_count(expected: f32, roll: f64) -> u16 {
    let e = expected.max(0.0) as f64;
    let whole = e.floor();
    whole as u16 + u16::from(roll < e - whole)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockRegistry;
    use crate::items::ItemRegistry;

    fn tool(items: &ItemRegistry, name: &str) -> ToolStats {
        items.get(items.id(name).unwrap()).unwrap().tool().unwrap()
    }

    #[test]
    fn swing_curve_matches_a13o() {
        // The Action Speed tab's samples, to its two decimals [2.5a].
        for (speed, secs) in [
            (5.0, 1.00),
            (10.0, 0.91),
            (15.0, 0.82),
            (20.0, 0.73),
            (30.0, 0.59),
            (38.0, 0.49),
            (60.0, 0.28),
            (99.0, 0.09),
            (125.0, 0.05),
            (150.0, 0.05),
        ] {
            let got = seconds_per_hit(speed);
            assert!((got - secs).abs() < 0.006, "speed {speed}: {got}");
        }
        assert_eq!(seconds_per_hit(1.0), 1.0);
        assert_eq!(swing_ticks(30.0), 35, "0.589 s");
        assert_eq!(swing_ticks(150.0), 3);
    }

    #[test]
    fn health_and_armour_by_tier() {
        let b = BlockRegistry::builtin();
        let stone = b.get(b.id("stone").unwrap());
        let copper = b.get(b.id("copper_vein").unwrap());
        let soil = b.get(b.id("soil").unwrap());
        assert_eq!(block_health(stone, 1), 1200);
        assert_eq!(block_health(stone, 2), 1200);
        assert_eq!(block_health(stone, 3), 1800);
        assert_eq!(block_health(stone, 6), 4800);
        assert_eq!(block_health(copper, 1), 2400);
        assert_eq!(block_health(soil, 4), 1600);
        assert_eq!(armour(1), 0);
        assert_eq!(armour(3), 100);
        assert_eq!(armour(7), 800);
        // Out-of-range tiers clamp.
        assert_eq!(armour(0), 0);
        assert_eq!(armour(99), 1000);
    }

    #[test]
    fn hits_match_boundless() {
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        let stone = b.get(b.id("stone").unwrap());
        let vein = b.get(b.id("copper_vein").unwrap());
        let soil = b.get(b.id("soil").unwrap());
        let copper = tool(&i, "copper_hammer");
        let iron = tool(&i, "iron_hammer");
        let stone_hammer = tool(&i, "stone_hammer");
        // Spec acceptance 4 and tuning 2.3's table.
        assert_eq!(hits_to_break(&copper, stone, 1), Some(3));
        assert_eq!(hits_to_break(&iron, stone, 1), Some(2));
        assert_eq!(hits_to_break(&stone_hammer, stone, 1), Some(3));
        assert_eq!(hits_to_break(&copper, vein, 1), Some(6));
        assert_eq!(hits_to_break(&copper, stone, 3), Some(6));
        assert_eq!(hits_to_break(&iron, stone, 4), Some(5));
        assert_eq!(hits_to_break(&iron, stone, 6), Some(48));
        assert_eq!(hits_to_break(&copper, stone, 6), None, "armour 600 stops a 400 hit");
        // The wrong tool deals a quarter: a copper shovel on stone.
        let shovel = tool(&i, "copper_shovel");
        assert_eq!(swing_damage(&shovel, stone, 1, false, 1.0), 100);
        assert_eq!(hits_to_break(&shovel, soil, 1), Some(2));
        // The hand stone suits everything.
        let hand_stone = tool(&i, "hand_stone");
        assert_eq!(hits_to_break(&hand_stone, stone, 1), Some(6));
        assert_eq!(hits_to_break(&hand_stone, soil, 1), Some(4));
    }

    #[test]
    fn skills_scale_damage_cadence_and_crits() {
        use crate::skills::{Character, SkillTree, xp_for_level};
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        let t = SkillTree::builtin();
        let stone = b.get(b.id("stone").unwrap());
        let copper = tool(&i, "copper_hammer");
        let hand_stone = tool(&i, "hand_stone");
        // Untrained: the tool as it is.
        let none = Character::default().effects(&t);
        assert_eq!(skilled_tool(&copper, &none), copper);
        // The miner's build (tuning 2.7): copper hits 1140, the hand stone
        // ignores mastery and the hammer epic (470), and the tables' hits.
        let mut c = Character {
            xp: xp_for_level(50),
            ..Default::default()
        };
        for (id, n) in [
            ("power", 8),
            ("all_attributes", 5),
            ("hammer_mastery", 5),
            ("dexterity", 8),
            ("agility", 8),
        ] {
            for _ in 0..n {
                c.learn(&t, id).unwrap();
            }
        }
        c.learn(&t, "hammer_epic").unwrap();
        c.learn(&t, "damage_epic").unwrap();
        let e = c.effects(&t);
        let skilled = skilled_tool(&copper, &e);
        assert_eq!(skilled.damage, 1140);
        assert_eq!(skilled_tool(&hand_stone, &e).damage, 470);
        assert_eq!(hits_to_break(&skilled, stone, 1), Some(2));
        assert_eq!(hits_to_break(&skilled, stone, 6), Some(9));
        // 9 dexterity, 5 mastery and the epic: +18 + 10 + 10 action speed.
        assert!((skilled.action_speed - copper.action_speed - 38.0).abs() < 1e-4);
        assert!(swing_ticks(skilled.action_speed) < swing_ticks(copper.action_speed));
        // 9 agility: +20% crit chance.
        assert!((skilled.crit_chance - copper.crit_chance - 0.2).abs() < 1e-6);
    }

    #[test]
    fn crit_and_area() {
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        let stone = b.get(b.id("stone").unwrap());
        let iron = tool(&i, "iron_hammer");
        // Crit before armour: 700 x 1.2 = 840, less 100 on tier 3.
        assert_eq!(swing_damage(&iron, stone, 3, true, 1.0), 740);
        // Area after armour [2.5a]: T3 copper 1x2 = (400 - 100) x 0.6.
        let copper = tool(&i, "copper_hammer");
        assert_eq!(swing_damage(&copper, stone, 3, false, Aoe::OneByTwo.multiplier()), 180);
        // About a fifth of iron swings crit, and the roll is repeatable.
        let n = (0..10_000)
            .filter(|&at| is_crit(0.2, 7, at, BlockPos::new(3, 4, 5)))
            .count();
        assert!((1800..2200).contains(&n), "{n}");
        assert_eq!(
            is_crit(0.2, 7, 99, BlockPos::new(1, 2, 3)),
            is_crit(0.2, 7, 99, BlockPos::new(1, 2, 3))
        );
        assert!(!is_crit(0.0, 7, 99, BlockPos::new(1, 2, 3)));
    }

    #[test]
    fn props_take_two_swings() {
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        let torch = b.get(b.id("torch").unwrap());
        assert_eq!(hits_to_break(&tool(&i, "iron_hammer"), torch, 7), Some(2));
        assert_eq!(hits_to_break(&tool(&i, "hand_stone"), torch, 1), Some(2));
    }

    #[test]
    fn area_shapes() {
        let hit = BlockPos::new(10, 20, 30);
        let east = DVec3::new(1.0, 0.1, 0.2);
        assert_eq!(aoe_blocks(None, hit, east, 1, 1), vec![hit]);
        let row = aoe_blocks(Some(Aoe::ThreeByOne), hit, east, 1, 1);
        assert_eq!(
            row,
            vec![hit, BlockPos::new(10, 20, 29), BlockPos::new(10, 20, 31)],
            "across the view, horizontal"
        );
        let two = aoe_blocks(Some(Aoe::OneByTwo), hit, east, 1, 1);
        assert_eq!(two, vec![hit, BlockPos::new(10, 19, 30)]);
        let square = aoe_blocks(Some(Aoe::ThreeByThree), hit, east, 1, 1);
        assert_eq!(square.len(), 9);
        assert!(square.iter().all(|p| p.x == 10), "in the plane facing the player");
        let down = DVec3::new(0.1, -1.0, 0.3);
        let floor = aoe_blocks(Some(Aoe::Cross), hit, down, 1, 1);
        assert_eq!(floor.len(), 5);
        assert!(floor.iter().all(|p| p.y == 20), "looking down, the shape lies flat");
        for at in 0..50 {
            let r = aoe_blocks(Some(Aoe::Random), hit, east, 1, at);
            assert!((2..=5).contains(&r.len()));
            assert_eq!(r[0], hit);
            let mut s = r.clone();
            s.sort_by_key(|p| (p.x, p.y, p.z));
            s.dedup();
            assert_eq!(s.len(), r.len(), "no block twice");
            assert!(r.iter().all(|p| p.x == 10));
        }
    }

    #[test]
    fn bonus_counts() {
        assert_eq!(bonus_count(0.5, 0.4), 1);
        assert_eq!(bonus_count(0.5, 0.6), 0);
        assert_eq!(bonus_count(1.05, 0.9), 1);
        assert_eq!(bonus_count(1.05, 0.01), 2);
        assert_eq!(bonus_count(0.0, 0.0), 0);
    }
}
