//! Crops (milestone 16 part 2, `docs/milestones/16-food.md`, ADR-0010).
//!
//! A crop's growth is a pure function of time: sown at a universe tick, it
//! ripens after its kind's time, shortened or lengthened by the conditions
//! it was sown in. Nothing is rolled while it grows; only its yield is
//! rolled, once, at harvest. The server keeps one [`Crop`] record per crop
//! and swaps the crop's block as its stage changes; what players see is
//! always a block (`<crop>` stage blocks in `assets/blocks.toml`).
//!
//! Kinds are data (`assets/crops.toml`), resolved against the block and item
//! catalogues once at startup: a missing name is a startup error.

use crate::block::{BlockId, BlockRegistry};
use crate::items::{ItemId, ItemRegistry};
use crate::movement::TICK_HZ;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Visible stages: sown, young, grown, ripe.
pub const STAGES: u8 = 4;
/// Tilled soil takes a quarter off the time to ripen (a **guess**).
pub const TILLED_TIME: f64 = 0.75;
/// A crop that wants water, sown with none beside it, takes this many times
/// as long (Boundless's rice rule; the factor is a **guess**).
pub const DRY_TIME: f64 = 4.0;
/// Fertiliser doubles growth speed from when it is applied; the Farming
/// Epic triples it (Boundless: the epic doubles fertiliser's effect).
pub const FERTILISED_SPEED: f64 = 2.0;
pub const FERTILISED_SPEED_EPIC: f64 = 3.0;
/// Tilled soil gives half again of the crop's yield chance (a **guess**).
pub const TILLED_YIELD: f64 = 1.5;
/// The Farming Epic: 20% more yield (tuning 5: "+20% crop yield").
pub const EPIC_YIELD: f64 = 1.2;
/// XP for harvesting a ripe crop, per hour it takes to ripen (a **guess**).
pub const XP_PER_HOUR: f64 = 4.0;
/// XP for sowing one seed.
pub const SOW_XP: u64 = 1;

const HOUR: f64 = 3600.0 * TICK_HZ as f64;

#[derive(Debug, thiserror::Error)]
pub enum CropError {
    #[error("crops.toml: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("crop `{0}`: no block or item named `{1}`")]
    Unknown(String, String),
    #[error("crop `{0}`: needs {STAGES} stage blocks, a time above 0 and ground to grow on")]
    Bad(String),
}

#[derive(Deserialize)]
struct CropFile {
    #[serde(default)]
    crop: Vec<CropEntry>,
}

#[derive(Deserialize)]
struct CropEntry {
    name: String,
    display: String,
    /// The stage blocks, sown to ripe; the first is also the seed's block.
    stages: Vec<String>,
    /// The item a ripe harvest yields.
    #[serde(rename = "yield")]
    yields: String,
    hours: f64,
    #[serde(default)]
    farming: u8,
    ground: Vec<String>,
    #[serde(default)]
    water: bool,
    #[serde(default = "seed_extra")]
    seed_extra: f64,
    crop_base: f64,
    #[serde(default)]
    crop_extra: f64,
}

fn seed_extra() -> f64 {
    0.35
}

/// One crop kind, its names resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct CropDef {
    pub name: String,
    /// What players see ("Oats").
    pub display: String,
    /// Stage blocks, sown first, ripe last.
    pub stages: Vec<BlockId>,
    /// The seed: the item that places the first stage.
    pub seed: ItemId,
    pub yields: ItemId,
    /// Hours to ripen with nothing helping or hindering.
    pub hours: f64,
    /// The Farming level sowing it needs.
    pub farming: u8,
    /// Blocks it may be sown on.
    pub ground: Vec<BlockId>,
    /// Wants water beside it (rice).
    pub water: bool,
    /// Chance of a second seed at a ripe harvest (one is certain).
    pub seed_extra: f64,
    /// Expected yield at a ripe harvest: whole part certain, the rest a
    /// chance; then `crop_extra` a chance of one more.
    pub crop_base: f64,
    pub crop_extra: f64,
}

impl CropDef {
    pub fn ripe(&self) -> BlockId {
        *self.stages.last().expect("checked")
    }

    /// XP for a ripe harvest.
    pub fn harvest_xp(&self) -> u64 {
        (self.hours * XP_PER_HOUR).round() as u64
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CropKinds {
    defs: Vec<CropDef>,
    by_block: HashMap<BlockId, (u16, u8)>,
}

impl CropKinds {
    pub fn build(toml_text: &str, blocks: &BlockRegistry, items: &ItemRegistry) -> Result<Self, CropError> {
        let file: CropFile = toml::from_str(toml_text)?;
        let mut defs = Vec::new();
        let mut by_block = HashMap::new();
        for e in file.crop {
            let block = |n: &str| {
                blocks
                    .id(n)
                    .map_err(|_| CropError::Unknown(e.name.clone(), n.to_string()))
            };
            let item = |n: &str| {
                items
                    .id(n)
                    .map_err(|_| CropError::Unknown(e.name.clone(), n.to_string()))
            };
            if e.stages.len() != STAGES as usize || e.hours <= 0.0 || e.ground.is_empty() {
                return Err(CropError::Bad(e.name));
            }
            let stages = e.stages.iter().map(|n| block(n)).collect::<Result<Vec<_>, _>>()?;
            let seed = items
                .item_for_block(stages[0])
                .ok_or_else(|| CropError::Unknown(e.name.clone(), e.stages[0].clone()))?;
            let def = CropDef {
                display: e.display,
                stages: stages.clone(),
                seed,
                yields: item(&e.yields)?,
                hours: e.hours,
                farming: e.farming,
                ground: e.ground.iter().map(|n| block(n)).collect::<Result<_, _>>()?,
                water: e.water,
                seed_extra: e.seed_extra,
                crop_base: e.crop_base,
                crop_extra: e.crop_extra,
                name: e.name,
            };
            for (s, b) in stages.iter().enumerate() {
                by_block.insert(*b, (defs.len() as u16, s as u8 + 1));
            }
            defs.push(def);
        }
        Ok(Self { defs, by_block })
    }

    pub fn builtin(blocks: &BlockRegistry, items: &ItemRegistry) -> Self {
        Self::build(include_str!("../../../assets/crops.toml"), blocks, items).expect("assets/crops.toml is valid")
    }

    pub fn get(&self, kind: u16) -> Option<&CropDef> {
        self.defs.get(kind as usize)
    }

    pub fn by_name(&self, name: &str) -> Option<u16> {
        self.defs.iter().position(|d| d.name == name).map(|i| i as u16)
    }

    /// The crop and stage (1 sown to [`STAGES`] ripe) a block shows, if it
    /// is a crop's.
    pub fn of_block(&self, block: BlockId) -> Option<(u16, u8)> {
        self.by_block.get(&block).copied()
    }

    /// The crop whose seed `block` is (its first stage).
    pub fn sown_as(&self, block: BlockId) -> Option<u16> {
        self.of_block(block).filter(|&(_, s)| s == 1).map(|(k, _)| k)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, &CropDef)> {
        self.defs.iter().enumerate().map(|(i, d)| (i as u16, d))
    }
}

/// One growing crop, as the server keeps and saves it: when it was sown and
/// what it was sown with. Its stage follows (ADR-0010).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub kind: u16,
    /// Universe tick it was sown (or replanted).
    pub sown_at: u64,
    /// Universe tick fertiliser was applied, if it was.
    pub fertilised_at: Option<u64>,
    /// Sown on tilled soil.
    pub tilled: bool,
    /// Water beside it when sown.
    pub wet: bool,
    /// The sower had the Farming Epic (fertiliser works harder).
    pub epic: bool,
}

impl Crop {
    /// Ticks to ripen with no fertiliser, from its kind and conditions.
    pub fn ripen_ticks(&self, def: &CropDef) -> f64 {
        let mut t = def.hours * HOUR;
        if self.tilled {
            t *= TILLED_TIME;
        }
        if def.water && !self.wet {
            t *= DRY_TIME;
        }
        t.max(1.0)
    }

    /// Growth so far, 0 (sown) to 1 (ripe) and beyond.
    pub fn progress(&self, def: &CropDef, now: u64) -> f64 {
        let t = self.ripen_ticks(def);
        let elapsed = now.saturating_sub(self.sown_at) as f64;
        match self.fertilised_at {
            Some(f) if f < now => {
                let before = f.saturating_sub(self.sown_at) as f64;
                let speed = if self.epic {
                    FERTILISED_SPEED_EPIC
                } else {
                    FERTILISED_SPEED
                };
                (before + (elapsed - before) * speed) / t
            }
            _ => elapsed / t,
        }
    }

    /// The stage it shows at `now`: 1 to [`STAGES`] (ripe at progress 1).
    pub fn stage(&self, def: &CropDef, now: u64) -> u8 {
        stage_of(self.progress(def, now))
    }

    pub fn ripe(&self, def: &CropDef, now: u64) -> bool {
        self.stage(def, now) == STAGES
    }

    /// The first tick after `now` its stage changes, if it is not ripe yet.
    pub fn next_change(&self, def: &CropDef, now: u64) -> Option<u64> {
        let stage = self.stage(def, now);
        if stage >= STAGES {
            return None;
        }
        // Progress where the next stage begins, then the tick that reaches
        // it (inverting `progress`).
        let want = stage as f64 / (STAGES - 1) as f64;
        let t = self.ripen_ticks(def);
        let target = want * t;
        let at = match self.fertilised_at {
            Some(f) if f <= now => {
                let before = f.saturating_sub(self.sown_at) as f64;
                let speed = if self.epic {
                    FERTILISED_SPEED_EPIC
                } else {
                    FERTILISED_SPEED
                };
                if target <= before {
                    self.sown_at as f64 + target
                } else {
                    f as f64 + (target - before) / speed
                }
            }
            _ => self.sown_at as f64 + target,
        };
        Some((at.ceil() as u64).max(now + 1))
    }

    /// Ticks until ripe, from `now` (0 when ripe).
    pub fn ticks_to_ripe(&self, def: &CropDef, now: u64) -> u64 {
        let mut at = now;
        while let Some(next) = self.next_change(def, at) {
            at = next;
        }
        at - now
    }
}

/// Stage from progress: thirds of the way give stages 1 to 3, ripe at 1.
pub fn stage_of(progress: f64) -> u8 {
    if progress >= 1.0 {
        STAGES
    } else {
        1 + ((progress.max(0.0) * (STAGES - 1) as f64) as u8).min(STAGES - 2)
    }
}

/// What a ripe harvest gives: seeds (one certain, a chance of a second) and
/// the crop's yield. `roll` gives numbers in 0..1 (the server's rng).
pub fn harvest(def: &CropDef, tilled: bool, epic: bool, mut roll: impl FnMut() -> f64) -> (u16, u16) {
    let seeds = 1 + (roll() < def.seed_extra) as u16;
    let base = def.crop_base * if tilled { TILLED_YIELD } else { 1.0 } * if epic { EPIC_YIELD } else { 1.0 };
    let mut crop = base.floor() as u16;
    if roll() < base.fract() {
        crop += 1;
    }
    if roll() < def.crop_extra {
        crop += 1;
    }
    (seeds, crop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds() -> (BlockRegistry, ItemRegistry, CropKinds) {
        let b = BlockRegistry::builtin();
        let i = ItemRegistry::builtin(&b);
        let k = CropKinds::builtin(&b, &i);
        (b, i, k)
    }

    fn sown(kind: u16, at: u64) -> Crop {
        Crop {
            kind,
            sown_at: at,
            fertilised_at: None,
            tilled: false,
            wet: false,
            epic: false,
        }
    }

    #[test]
    fn crops_load_and_know_their_blocks() {
        let (b, i, k) = kinds();
        let tuber = k.by_name("tuber").unwrap();
        let def = k.get(tuber).unwrap();
        assert_eq!(def.yields, i.id("tuber").unwrap());
        assert_eq!(k.of_block(def.ripe()), Some((tuber, STAGES)));
        assert_eq!(k.sown_as(def.stages[0]), Some(tuber));
        assert_eq!(k.sown_as(def.stages[1]), None);
        assert!(def.ground.contains(&b.id("tilled_soil").unwrap()));
        assert!(k.get(k.by_name("rice").unwrap()).unwrap().water);
        assert_eq!(k.iter().count(), 7);
    }

    #[test]
    fn growth_is_the_clock_alone() {
        let (_, _, k) = kinds();
        let kind = k.by_name("tuber").unwrap();
        let def = k.get(kind).unwrap();
        let hour = 3600 * TICK_HZ as u64;
        let c = sown(kind, 1000);
        assert_eq!(c.stage(def, 1000), 1);
        assert_eq!(c.stage(def, 1000 + hour), 2);
        assert_eq!(c.stage(def, 1000 + 2 * hour), 3);
        assert_eq!(c.stage(def, 1000 + 3 * hour - 1), 3);
        assert!(c.ripe(def, 1000 + 3 * hour));
        assert_eq!(c.next_change(def, 1000), Some(1000 + hour));
        assert_eq!(c.next_change(def, 1000 + 3 * hour), None);
        assert_eq!(c.ticks_to_ripe(def, 1000), 3 * hour);
        // Sown together, ripe together; tilled a quarter sooner.
        let tilled = Crop { tilled: true, ..c };
        assert_eq!(tilled.ticks_to_ripe(def, 1000), 3 * hour * 3 / 4);
        assert!(tilled.ripe(def, 1000 + 9 * hour / 4));
    }

    #[test]
    fn rice_wants_water_and_fertiliser_speeds_what_is_left() {
        let (_, _, k) = kinds();
        let rice = k.by_name("rice").unwrap();
        let def = k.get(rice).unwrap();
        let hour = 3600 * TICK_HZ as u64;
        let wet = Crop {
            wet: true,
            ..sown(rice, 0)
        };
        let dry = sown(rice, 0);
        assert_eq!(wet.ticks_to_ripe(def, 0), 6 * hour);
        assert_eq!(dry.ticks_to_ripe(def, 0), 24 * hour);
        // Fertilised halfway: the other half takes half as long.
        let fed = Crop {
            fertilised_at: Some(3 * hour),
            ..wet
        };
        assert_eq!(fed.ticks_to_ripe(def, 3 * hour), 3 * hour / 2);
        assert!(fed.ripe(def, 3 * hour + 3 * hour / 2));
        let epic = Crop { epic: true, ..fed };
        assert_eq!(epic.ticks_to_ripe(def, 3 * hour), hour);
    }

    #[test]
    fn harvests_roll_the_drop_data() {
        let (_, _, k) = kinds();
        let tuber = k.get(k.by_name("tuber").unwrap()).unwrap();
        // Rolls all high: one seed, one tuber.
        assert_eq!(harvest(tuber, false, false, || 0.99), (1, 1));
        // Rolls all low: two seeds, two tubers.
        assert_eq!(harvest(tuber, false, false, || 0.0), (2, 2));
        // Tilled: 1.5 expected, so a coin for the second.
        assert_eq!(harvest(tuber, true, false, || 0.49), (1, 2));
        let oats = k.get(k.by_name("oats").unwrap()).unwrap();
        assert_eq!(harvest(oats, false, false, || 0.99), (1, 0));
        assert_eq!(harvest(oats, false, false, || 0.15), (2, 1));
        assert_eq!(harvest(oats, false, false, || 0.05), (2, 2));
        assert_eq!(oats.harvest_xp(), 16);
        // The epic: 1.2 tubers expected.
        assert_eq!(harvest(tuber, false, true, || 0.19), (2, 2));
        assert_eq!(harvest(tuber, false, true, || 0.21), (2, 1));
    }

    #[test]
    fn stages_split_progress_in_thirds() {
        assert_eq!(stage_of(0.0), 1);
        assert_eq!(stage_of(0.34), 2);
        assert_eq!(stage_of(0.67), 3);
        assert_eq!(stage_of(0.999), 3);
        assert_eq!(stage_of(1.0), 4);
        assert_eq!(stage_of(7.0), 4);
    }
}
