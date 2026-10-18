//! Boundmore's procedural texture painter.
//!
//! Paints every block texture the catalogue names (`assets/blocks.toml`),
//! their variants, the crack overlays, an icon for every item that is not a
//! block (`assets/items.toml`), a review contact sheet, an icon sheet and an
//! in-context preview. Deterministic: the same build writes the same bytes.
//! The look is set by `docs/art-direction.md`.

pub mod canvas;
pub mod color;
pub mod icons;
pub mod materials;
pub mod noise;
pub mod overlay;
pub mod preview;
pub mod sheet;
pub mod stations;
pub mod tex;

use tex::Tex;

/// Textures on natural, frequently tiled faces get this many extra variants.
pub const VARIANTS: u32 = 3;

/// Which textures get variants.
pub const VARIED: [&str; 8] = [
    "stone",
    "deep_stone",
    "soil",
    "turf_top",
    "sand",
    "gravel",
    "snow",
    "foliage",
];

/// A texture and its variants (index 0 is the base).
#[derive(Clone, Debug)]
pub struct Family {
    pub name: String,
    pub faces: Vec<Tex>,
}

impl Family {
    /// File stem of face `i`: `stone`, `stone_1`, ...
    pub fn file_stem(&self, i: usize) -> String {
        if i == 0 {
            self.name.clone()
        } else {
            format!("{}_{i}", self.name)
        }
    }

    pub fn base(&self) -> &Tex {
        &self.faces[0]
    }
}

/// Every texture name the built-in catalogue refers to, in catalogue order.
pub fn catalogue_names() -> Vec<String> {
    let registry = bm_core::BlockRegistry::builtin();
    let mut names: Vec<String> = Vec::new();
    for def in registry.defs() {
        let t = &def.textures;
        for n in [t.top(), t.side(), t.bottom()].into_iter().flatten() {
            if !names.iter().any(|m| m == n) {
                names.push(n.to_string());
            }
        }
    }
    names
}

/// Paints every catalogue texture with its variants. An error names any
/// texture the catalogue wants that no painter knows.
pub fn families() -> Result<Vec<Family>, String> {
    catalogue_names()
        .into_iter()
        .map(|name| {
            let count = if VARIED.contains(&name.as_str()) {
                1 + VARIANTS
            } else {
                1
            };
            let faces = (0..count)
                .map(|v| materials::paint_named(&name, v))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| format!("the catalogue names texture `{name}` but bm-texgen has no painter for it"))?;
            Ok(Family { name, faces })
        })
        .collect()
}

/// The icon every non-block item in the built-in catalogue shows, in
/// catalogue order, without repeats (`icon` in `assets/items.toml`, else the
/// item's name).
pub fn icon_names() -> Vec<String> {
    let blocks = bm_core::BlockRegistry::builtin();
    let items = bm_core::ItemRegistry::builtin(&blocks);
    let mut names: Vec<String> = Vec::new();
    for def in items.defs().iter().filter(|d| d.block().is_none()) {
        let name = def.icon.clone().unwrap_or_else(|| def.name.clone());
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Paints every item icon. An error names any icon the catalogue wants that
/// no painter knows.
pub fn item_icons() -> Result<Vec<(String, Tex)>, String> {
    icon_names()
        .into_iter()
        .map(|name| {
            let tex = icons::paint_icon(&name)
                .ok_or_else(|| format!("the item catalogue names icon `{name}` but bm-texgen has no painter for it"))?;
            Ok((name, tex))
        })
        .collect()
}

/// Looks a family up by name.
pub fn find<'a>(families: &'a [Family], name: &str) -> &'a Family {
    families
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no texture family `{name}`"))
}

/// Converts a face to an 8-bit image.
pub fn to_image(tex: &Tex) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(tex::SIZE as u32, tex::SIZE as u32);
    for (i, p) in img.pixels_mut().enumerate() {
        *p = image::Rgba(tex.px[i].to_u8());
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_texture_is_painted() {
        let fams = families().expect("every texture has a painter");
        let registry = bm_core::BlockRegistry::builtin();
        for def in registry.defs().iter().filter(|d| d.renders()) {
            for face in def.textures.faces().expect("validated catalogue") {
                let fam = fams.iter().find(|f| f.name == face);
                assert!(fam.is_some(), "block `{}` texture `{face}` is not painted", def.name);
            }
        }
        for name in VARIED {
            assert_eq!(find(&fams, name).faces.len(), 1 + VARIANTS as usize, "{name} variants");
        }
        // Every item that is not a block gets an icon.
        let icons = item_icons().expect("every icon has a painter");
        let items = bm_core::ItemRegistry::builtin(&registry);
        for def in items.defs().iter().filter(|d| d.block().is_none()) {
            let want = def.icon.as_deref().unwrap_or(&def.name);
            assert!(
                icons.iter().any(|(n, _)| n == want),
                "item `{}` icon `{want}` is not painted",
                def.name
            );
        }
        assert!(!icons.is_empty(), "the catalogue has non-block items");
    }

    #[test]
    fn icons_are_32_by_32_on_clear_ground_and_deterministic() {
        let a = item_icons().unwrap();
        let b = item_icons().unwrap();
        for ((name, icon), (_, again)) in a.iter().zip(&b) {
            assert_eq!(icon, again, "icon {name} differs between runs");
            assert_eq!(to_image(icon).dimensions(), (32, 32), "{name}");
            // The corners are background, and the sprite is a solid shape.
            for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
                assert_eq!(icon.get(x, y).a, 0.0, "icon {name} corner ({x}, {y}) is not clear");
            }
            assert!(
                icon.px.iter().all(|p| p.a == 0.0 || p.a == 1.0),
                "icon {name} has partial alpha"
            );
            let filled = icon.px.iter().filter(|p| p.a > 0.0).count();
            assert!(filled > 150, "icon {name} covers only {filled} pixels");
        }
    }

    #[test]
    fn outputs_are_32_by_32() {
        for fam in families().unwrap() {
            for face in &fam.faces {
                let img = to_image(face);
                assert_eq!((img.width(), img.height()), (32, 32), "{}", fam.name);
            }
        }
        for crack in overlay::cracks() {
            assert_eq!(to_image(&crack).dimensions(), (32, 32));
        }
    }

    #[test]
    fn painting_is_deterministic() {
        let a = families().unwrap();
        let b = families().unwrap();
        for (fa, fb) in a.iter().zip(&b) {
            assert_eq!(fa.faces, fb.faces, "{} differs between runs", fa.name);
        }
    }

    #[test]
    fn only_translucent_blocks_carry_alpha() {
        let registry = bm_core::BlockRegistry::builtin();
        let fams = families().unwrap();
        for def in registry.defs().iter().filter(|d| d.opaque) {
            for face in def.textures.faces().unwrap() {
                for tex in &find(&fams, face).faces {
                    assert!(
                        tex.px.iter().all(|p| p.a >= 1.0),
                        "opaque `{face}` has transparent pixels"
                    );
                }
            }
        }
    }
}
