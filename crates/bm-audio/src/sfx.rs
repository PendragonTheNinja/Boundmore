//! Every sound the game names, and what a block sounds like.

use bm_core::{BlockDef, Material};

macro_rules! sfx {
    ($($v:ident = $name:literal),* $(,)?) => {
        /// A sound the game plays. Its files are `<name>_<n>.wav` (or `.ogg`)
        /// in `assets/sounds/`, n from 1.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Sfx { $($v),* }

        impl Sfx {
            pub const ALL: &'static [Sfx] = &[$(Sfx::$v),*];

            /// The file stem.
            pub fn name(self) -> &'static str {
                match self { $(Sfx::$v => $name),* }
            }
        }
    };
}

sfx! {
    SwingHammer = "swing_hammer",
    SwingAxe = "swing_axe",
    SwingShovel = "swing_shovel",
    SwingLight = "swing_light",
    HitStone = "hit_stone",
    HitWood = "hit_wood",
    HitSoil = "hit_soil",
    HitPlant = "hit_plant",
    HitMetal = "hit_metal",
    HitGlass = "hit_glass",
    HitCrystal = "hit_crystal",
    BreakStone = "break_stone",
    BreakWood = "break_wood",
    BreakSoil = "break_soil",
    BreakPlant = "break_plant",
    BreakMetal = "break_metal",
    BreakGlass = "break_glass",
    BreakCrystal = "break_crystal",
    Place = "place",
    PlaceHard = "place_hard",
    Chisel = "chisel",
    Pickup = "pickup",
    StepStone = "step_stone",
    StepSoil = "step_soil",
    StepWood = "step_wood",
    StepSand = "step_sand",
    StepSnow = "step_snow",
    StepWater = "step_water",
    Jump = "jump",
    Land = "land",
    Splash = "splash",
    Hurt = "hurt",
    Defeat = "defeat",
    CreatureHit = "creature_hit",
    CreatureWindup = "creature_windup",
    PopperWhistle = "popper_whistle",
    Sling = "sling",
    Throw = "throw",
    Blast = "blast",
    GrappleFire = "grapple_fire",
    GrappleStick = "grapple_stick",
    GrappleRelease = "grapple_release",
    Reel = "reel",
    DoorOpen = "door_open",
    DoorClose = "door_close",
    Click = "click",
    Craft = "craft",
    Coin = "coin",
    Notice = "notice",
    Warp = "warp",
    MeteorLand = "meteor_land",
}

/// What a block sounds like when struck, broken or walked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mat {
    Stone,
    Wood,
    Soil,
    Plant,
    Metal,
    Glass,
    Crystal,
    Sand,
    Snow,
    Water,
}

impl Mat {
    /// From the catalogue's `material`, with the block's name for what the
    /// catalogue does not say (metal, glass, sand, snow, water).
    pub fn of(def: &BlockDef) -> Mat {
        let n = def.name.as_str();
        let has = |w: &[&str]| w.iter().any(|k| n.contains(k));
        if def.liquid || n == "water" {
            return Mat::Water;
        }
        if has(&["glass", "ice"]) {
            return Mat::Glass;
        }
        if has(&["snow"]) {
            return Mat::Snow;
        }
        if has(&["sand", "gravel", "ash"]) && !has(&["sandstone"]) {
            return Mat::Sand;
        }
        if has(&[
            "metal", "iron", "copper", "silver", "gold", "titanium", "lock", "lantern", "coil", "spark", "conduit",
        ]) {
            return Mat::Metal;
        }
        match def.material {
            Material::Wood => Mat::Wood,
            Material::Soil => Mat::Soil,
            Material::Plant => Mat::Plant,
            Material::Crystal => Mat::Crystal,
            Material::Rock | Material::None => {
                if has(&[
                    "wood",
                    "timber",
                    "trunk",
                    "door",
                    "chest",
                    "table",
                    "chair",
                    "workbench",
                ]) {
                    Mat::Wood
                } else if has(&[
                    "crystal", "gem", "amethyst", "emerald", "sapphire", "topaz", "diamond", "ruby",
                ]) {
                    Mat::Crystal
                } else {
                    Mat::Stone
                }
            }
        }
    }

    /// A tool landing on it.
    pub fn hit(self) -> Sfx {
        match self {
            Mat::Stone => Sfx::HitStone,
            Mat::Wood => Sfx::HitWood,
            Mat::Soil | Mat::Sand | Mat::Snow | Mat::Water => Sfx::HitSoil,
            Mat::Plant => Sfx::HitPlant,
            Mat::Metal => Sfx::HitMetal,
            Mat::Glass => Sfx::HitGlass,
            Mat::Crystal => Sfx::HitCrystal,
        }
    }

    /// It breaking.
    pub fn broken(self) -> Sfx {
        match self {
            Mat::Stone => Sfx::BreakStone,
            Mat::Wood => Sfx::BreakWood,
            Mat::Soil | Mat::Sand | Mat::Snow | Mat::Water => Sfx::BreakSoil,
            Mat::Plant => Sfx::BreakPlant,
            Mat::Metal => Sfx::BreakMetal,
            Mat::Glass => Sfx::BreakGlass,
            Mat::Crystal => Sfx::BreakCrystal,
        }
    }

    /// A footstep on it.
    pub fn step(self) -> Sfx {
        match self {
            Mat::Stone | Mat::Metal | Mat::Glass | Mat::Crystal => Sfx::StepStone,
            Mat::Wood => Sfx::StepWood,
            Mat::Soil | Mat::Plant => Sfx::StepSoil,
            Mat::Sand => Sfx::StepSand,
            Mat::Snow => Sfx::StepSnow,
            Mat::Water => Sfx::StepWater,
        }
    }

    /// Placing it: hard things knock.
    pub fn placed(self) -> Sfx {
        match self {
            Mat::Stone | Mat::Wood | Mat::Metal | Mat::Glass | Mat::Crystal => Sfx::PlaceHard,
            _ => Sfx::Place,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = Sfx::ALL.iter().map(|s| s.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Sfx::ALL.len());
    }

    #[test]
    fn every_catalogue_block_has_a_sensible_sound() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/blocks.toml")).unwrap();
        let reg = bm_core::BlockRegistry::from_toml(&text).unwrap();
        let mat = |name: &str| Mat::of(reg.get(reg.id(name).unwrap()));
        assert_eq!(mat("stone"), Mat::Stone);
        assert_eq!(mat("sandstone"), Mat::Stone);
        assert_eq!(mat("trunk"), Mat::Wood);
        assert_eq!(mat("timber"), Mat::Wood);
        assert_eq!(mat("turf"), Mat::Soil);
        assert_eq!(mat("sand"), Mat::Sand);
        assert_eq!(mat("snow"), Mat::Snow);
        assert_eq!(mat("water"), Mat::Water);
        assert_eq!(mat("clear_glass"), Mat::Glass);
        assert_eq!(mat("refined_iron"), Mat::Metal);
        assert_eq!(mat("glow_crystal"), Mat::Crystal);
        assert_eq!(mat("plain_wood_door"), Mat::Wood);
        assert_eq!(mat("plain_metal_door"), Mat::Metal);
        assert_eq!(mat("foliage"), Mat::Plant);
    }
}
