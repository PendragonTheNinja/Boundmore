//! Planet kinds: data in `assets/planets.toml`. A kind is a template: when a
//! planet is created its generation settings are copied out of the kind and
//! saved with the planet, so later edits to the file never change terrain
//! that already exists (only new planets see them).
//!
//! Colours work the same way: a kind lists the palette colours each natural
//! block may take (`[kind.colours]`), and a new planet picks its own few
//! from them by seed ([`crate::pick_colours`]), saved with its settings.

use crate::biomes::{BiomeCatalogue, BiomeChoice, BiomeSpec, pick_biomes};
use crate::{ConfigError, PlanetGenConfig, palette, pick_colours};
use bm_core::atmosphere::AirLine;
use bm_core::{BlockRegistry, PlanetLook, RegistryError};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct PlanetKind {
    /// Unique, referred to by the server's planet list.
    pub name: String,
    /// What players see ("Temperate").
    pub display: String,
    /// 1 is the gentle start; higher tiers hold rarer ore.
    pub tier: u8,
    /// The line of its air (milestone 18): hazardous from tier 4.
    pub air: AirLine,
    /// Its element (milestone 18 part 3): some creatures there carry it.
    pub element: Option<bm_core::creatures::Element>,
    pub look: PlanetLook,
    /// Block name to the palette colour names a planet of this kind may give
    /// it (`[kind.colours]`).
    pub colours: BTreeMap<String, Vec<String>>,
    /// The biomes a planet of this kind may draw (milestone 19); none: the
    /// single landscape of its `generation` settings.
    pub biome_choice: Option<BiomeChoice>,
    /// The catalogue's biomes this kind allows, to draw from.
    biome_pool: Vec<crate::biomes::BiomeEntry>,
    generation: PlanetGenConfig,
}

impl PlanetKind {
    /// The settings a new planet of this kind is generated with, its natural
    /// colours picked from the kind's choices by `seed`. Call it once, when
    /// the planet is created, and save the result: the planet then keeps
    /// these colours whatever later happens to `planets.toml`.
    pub fn generation(&self, seed: u64) -> PlanetGenConfig {
        let mut colours = self.generation.colours.clone();
        colours.extend(pick_colours(&self.colours, seed));
        let biomes: Vec<BiomeSpec> = match &self.biome_choice {
            Some(choice) => pick_biomes(
                &BiomeCatalogue {
                    biomes: self.biome_pool.clone(),
                },
                choice,
                seed,
            ),
            None => Vec::new(),
        };
        PlanetGenConfig {
            seed,
            colours,
            biomes,
            ..self.generation.clone()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KindError {
    #[error("planet kinds are not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("planet kind `{0}`: {1}")]
    Block(String, RegistryError),
    #[error("planet kind `{0}`: {1}")]
    Generation(String, ConfigError),
    #[error("planet kind `{0}` is defined twice")]
    Duplicate(String),
    #[error("no planet kinds defined")]
    Empty,
}

#[derive(Deserialize)]
struct KindFile {
    #[serde(default)]
    kind: Vec<KindEntry>,
}

#[derive(Deserialize)]
struct KindEntry {
    name: String,
    display: Option<String>,
    tier: u8,
    #[serde(default)]
    air: AirLine,
    #[serde(default)]
    element: Option<bm_core::creatures::Element>,
    #[serde(default)]
    look: PlanetLook,
    #[serde(default)]
    colours: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    generation: PlanetGenConfig,
    #[serde(default)]
    biomes: Option<BiomeChoice>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanetKinds {
    pub kinds: Vec<PlanetKind>,
}

impl PlanetKinds {
    /// Kinds from `source`, with no biome catalogue (every kind a single
    /// landscape).
    pub fn from_toml(source: &str, blocks: &BlockRegistry) -> Result<Self, KindError> {
        Self::with_biomes(source, &BiomeCatalogue::default(), blocks)
    }

    /// Kinds from `source`, drawing biomes from `catalogue`.
    pub fn with_biomes(source: &str, catalogue: &BiomeCatalogue, blocks: &BlockRegistry) -> Result<Self, KindError> {
        let file: KindFile = toml::from_str(source)?;
        let mut kinds: Vec<PlanetKind> = Vec::new();
        for e in file.kind {
            if kinds.iter().any(|k| k.name == e.name) {
                return Err(KindError::Duplicate(e.name));
            }
            e.look
                .resolve_tints(blocks)
                .map_err(|err| KindError::Block(e.name.clone(), err))?;
            e.generation
                .check(blocks)
                .map_err(|err| KindError::Generation(e.name.clone(), err))?;
            check_colour_choices(&e.colours, blocks).map_err(|err| KindError::Generation(e.name.clone(), err))?;
            let biome_pool: Vec<_> = match &e.biomes {
                Some(_) => catalogue.biomes.clone(),
                None => Vec::new(),
            };
            for b in &biome_pool {
                for name in b.spec.blocks() {
                    blocks.id(name).map_err(|err| KindError::Block(e.name.clone(), err))?;
                }
            }
            kinds.push(PlanetKind {
                biome_choice: e.biomes.filter(|_| !catalogue.biomes.is_empty()),
                biome_pool,
                display: e.display.unwrap_or_else(|| title_case(&e.name)),
                name: e.name,
                tier: e.tier.max(1),
                air: e.air,
                element: e.element,
                look: e.look,
                colours: e.colours,
                generation: e.generation,
            });
        }
        if kinds.is_empty() {
            return Err(KindError::Empty);
        }
        Ok(Self { kinds })
    }

    pub fn builtin(blocks: &BlockRegistry) -> Self {
        Self::with_biomes(
            include_str!("../../../assets/planets.toml"),
            &BiomeCatalogue::builtin(),
            blocks,
        )
        .expect("assets/planets.toml is valid")
    }

    pub fn get(&self, name: &str) -> Option<&PlanetKind> {
        self.kinds.iter().find(|k| k.name == name)
    }
}

/// Every block named takes colours and every colour is in the palette. A
/// list may be any length (a planet picks up to four from it) but not empty.
fn check_colour_choices(choices: &BTreeMap<String, Vec<String>>, blocks: &BlockRegistry) -> Result<(), ConfigError> {
    let palette = palette();
    for (block, names) in choices {
        if !blocks.get(blocks.id(block)?).colourable {
            return Err(ConfigError::NotColourable(block.clone()));
        }
        if names.is_empty() {
            return Err(ConfigError::ColourCount(block.clone(), 0));
        }
        if let Some(bad) = names.iter().find(|n| palette.id(n).is_none()) {
            return Err(ConfigError::UnknownColour {
                block: block.clone(),
                colour: bad.clone(),
            });
        }
    }
    Ok(())
}

fn title_case(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_kinds_load_with_a_tier_one_start() {
        let blocks = BlockRegistry::builtin();
        let kinds = PlanetKinds::builtin(&blocks);
        assert!(kinds.kinds.len() >= 4);
        assert!(kinds.kinds.iter().any(|k| k.tier == 1));
        let k = kinds.get("temperate").expect("temperate kind");
        assert_eq!(k.generation(7).seed, 7);
        assert_eq!(k.display, "Temperate");
    }

    #[test]
    fn unknown_blocks_and_duplicates_are_refused() {
        let blocks = BlockRegistry::builtin();
        let bad = "[[kind]]\nname = \"x\"\ntier = 1\n[kind.look]\nday_zenith = [0,0,0]\nday_horizon = [0,0,0]\ncolor = [0,0,0]\ntints = [[\"nope\", [1,2,3]]]\n";
        assert!(matches!(
            PlanetKinds::from_toml(bad, &blocks),
            Err(KindError::Block(..))
        ));
        let dup = "[[kind]]\nname = \"x\"\ntier = 1\n[[kind]]\nname = \"x\"\ntier = 2\n";
        assert!(matches!(
            PlanetKinds::from_toml(dup, &blocks),
            Err(KindError::Duplicate(..))
        ));
    }
}
