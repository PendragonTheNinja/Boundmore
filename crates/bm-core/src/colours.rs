//! The colour palette: data in `assets/colours.toml` (ADR-0007), sent to
//! clients in the handshake. Ids are positions: 1 is the first entry, 0 is
//! natural (no colour: a block as its texture is painted).

use crate::block::Colour;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColourDef {
    pub name: String,
    /// sRGB 0-255.
    pub rgb: [u8; 3],
}

#[derive(Debug, thiserror::Error)]
pub enum ColourError {
    #[error("colour palette is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("the colour palette holds {0} colours; it may hold 1 to 255")]
    Size(usize),
    #[error("colour `{0}` is listed twice")]
    Duplicate(String),
}

#[derive(Deserialize)]
struct ColourFile {
    #[serde(default)]
    colour: Vec<ColourDef>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColourPalette {
    /// Index `i` is colour id `i + 1`.
    pub colours: Vec<ColourDef>,
}

impl ColourPalette {
    pub fn from_toml(source: &str) -> Result<Self, ColourError> {
        let file: ColourFile = toml::from_str(source)?;
        Self::from_defs(file.colour)
    }

    pub fn from_defs(colours: Vec<ColourDef>) -> Result<Self, ColourError> {
        if colours.is_empty() || colours.len() > 255 {
            return Err(ColourError::Size(colours.len()));
        }
        for (i, c) in colours.iter().enumerate() {
            if colours[..i].iter().any(|o| o.name == c.name) {
                return Err(ColourError::Duplicate(c.name.clone()));
            }
        }
        Ok(Self { colours })
    }

    pub fn builtin() -> Self {
        Self::from_toml(include_str!("../../../assets/colours.toml")).expect("assets/colours.toml is valid")
    }

    pub fn get(&self, c: Colour) -> Option<&ColourDef> {
        (c.0 as usize).checked_sub(1).and_then(|i| self.colours.get(i))
    }

    /// Whether `c` is natural or a colour in this palette.
    pub fn contains(&self, c: Colour) -> bool {
        c == Colour::NATURAL || self.get(c).is_some()
    }

    pub fn id(&self, name: &str) -> Option<Colour> {
        self.colours
            .iter()
            .position(|c| c.name == name)
            .map(|i| Colour(i as u8 + 1))
    }

    /// The palette colour closest to `rgb` (compared in linear light, with
    /// green weighted as the eye does). Never natural.
    pub fn nearest(&self, rgb: [u8; 3]) -> Colour {
        fn lin(v: u8) -> f32 {
            let x = v as f32 / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        }
        let target = rgb.map(lin);
        let dist = |c: &ColourDef| {
            let p = c.rgb.map(lin);
            let d = [p[0] - target[0], p[1] - target[1], p[2] - target[2]];
            0.3 * d[0] * d[0] + 0.59 * d[1] * d[1] + 0.11 * d[2] * d[2]
        };
        let best = self
            .colours
            .iter()
            .enumerate()
            .min_by(|a, b| dist(a.1).total_cmp(&dist(b.1)))
            .map_or(0, |(i, _)| i);
        Colour(best as u8 + 1)
    }

    /// The name players see ("Natural" for 0).
    pub fn name(&self, c: Colour) -> &str {
        self.get(c).map_or("Natural", |d| d.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_palette_has_255_named_colours() {
        let p = ColourPalette::builtin();
        assert_eq!(p.colours.len(), 255);
        assert_eq!(p.name(Colour::NATURAL), "Natural");
        assert!(p.contains(Colour(255)) && p.contains(Colour(0)));
        let first = p.id(&p.colours[0].name).unwrap();
        assert_eq!(first, Colour(1));
        assert_eq!(p.get(first).unwrap().rgb, p.colours[0].rgb);
    }

    #[test]
    fn nearest_finds_an_exact_match_and_never_natural() {
        let p = ColourPalette::builtin();
        for c in [Colour(1), Colour(77), Colour(200), Colour(255)] {
            let rgb = p.get(c).unwrap().rgb;
            assert_eq!(p.get(p.nearest(rgb)).unwrap().rgb, rgb);
        }
        assert_ne!(p.nearest([0, 0, 0]), Colour::NATURAL);
    }

    #[test]
    fn bad_palettes_are_refused() {
        assert!(matches!(ColourPalette::from_defs(vec![]), Err(ColourError::Size(0))));
        let twice = vec![
            ColourDef {
                name: "A".into(),
                rgb: [0; 3],
            },
            ColourDef {
                name: "A".into(),
                rgb: [1; 3],
            },
        ];
        assert!(matches!(
            ColourPalette::from_defs(twice),
            Err(ColourError::Duplicate(_))
        ));
    }
}
