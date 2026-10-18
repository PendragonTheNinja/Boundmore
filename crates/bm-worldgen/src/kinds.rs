//! Planet kinds: data in `assets/planets.toml`. A kind is a template: when a
//! planet is created its generation settings are copied out of the kind and
//! saved with the planet, so later edits to the file never change terrain
//! that already exists (only new planets see them).

use crate::PlanetGenConfig;
use bm_core::{BlockRegistry, PlanetLook, RegistryError};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct PlanetKind {
    /// Unique, referred to by the server's planet list.
    pub name: String,
    /// What players see ("Temperate").
    pub display: String,
    /// 1 is the gentle start; higher tiers hold rarer ore.
    pub tier: u8,
    pub look: PlanetLook,
    generation: PlanetGenConfig,
}

impl PlanetKind {
    /// The settings a new planet of this kind is generated with.
    pub fn generation(&self, seed: u64) -> PlanetGenConfig {
        PlanetGenConfig {
            seed,
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
    look: PlanetLook,
    #[serde(default)]
    generation: PlanetGenConfig,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanetKinds {
    pub kinds: Vec<PlanetKind>,
}

impl PlanetKinds {
    pub fn from_toml(source: &str, blocks: &BlockRegistry) -> Result<Self, KindError> {
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
                .map_err(|err| KindError::Block(e.name.clone(), err))?;
            kinds.push(PlanetKind {
                display: e.display.unwrap_or_else(|| title_case(&e.name)),
                name: e.name,
                tier: e.tier.max(1),
                look: e.look,
                generation: e.generation,
            });
        }
        if kinds.is_empty() {
            return Err(KindError::Empty);
        }
        Ok(Self { kinds })
    }

    pub fn builtin(blocks: &BlockRegistry) -> Self {
        Self::from_toml(include_str!("../../../assets/planets.toml"), blocks).expect("assets/planets.toml is valid")
    }

    pub fn get(&self, name: &str) -> Option<&PlanetKind> {
        self.kinds.iter().find(|k| k.name == name)
    }
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
