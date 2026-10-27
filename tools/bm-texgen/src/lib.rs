//! Boundmore's procedural texture painter.
//!
//! Paints every block texture the catalogue names (`assets/blocks.toml`),
//! their variants, the crack overlays, an icon for every item that is not a
//! block (`assets/items.toml`), a review contact sheet, an icon sheet and an
//! in-context preview. Deterministic: the same build writes the same bytes.
//! The look is set by `docs/art-direction.md`.

pub mod building;
pub mod canvas;
pub mod color;
pub mod creatures;
pub mod doors;
pub mod farming;
pub mod flora;
pub mod food;
pub mod forge;
pub mod furnishings;
pub mod gathered;
pub mod gems;
pub mod icons;
pub mod living;
pub mod machines;
pub mod materials;
pub mod noise;
pub mod overlay;
pub mod preview;
pub mod sheet;
pub mod signs;
pub mod spark;
pub mod stations;
pub mod tex;
pub mod worlds;

use tex::Tex;

/// Textures on natural, frequently tiled faces get this many extra variants.
pub const VARIANTS: u32 = 3;

/// Which textures get variants.
pub const VARIED: [&str; 17] = [
    "stone",
    "deep_stone",
    "soil",
    "turf_top",
    "sand",
    "gravel",
    "snow",
    "foliage",
    "basalt",
    "limestone",
    "clay",
    "peat",
    "mud",
    "meadow_turf_top",
    "moss_turf_top",
    "fronds",
    "glossy_foliage",
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

/// The catalogue's textures, then any painted ahead of the catalogue
/// naming them (`worlds::BLOCKS`, `gems::BLOCKS`), so their files are ready when it does.
fn painted_names() -> Vec<String> {
    let mut names = catalogue_names();
    for n in worlds::BLOCKS.into_iter().chain(gems::BLOCKS) {
        if !names.iter().any(|m| m == n) {
            names.push(n.to_string());
        }
    }
    names
}

/// Paints every catalogue texture with its variants. An error names any
/// texture the catalogue wants that no painter knows.
pub fn families() -> Result<Vec<Family>, String> {
    painted_names()
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

/// Paints the named catalogue textures, with their variants, at `n` pixels
/// across (the texture trial: `docs/proposals/textures-128.md`).
pub fn families_at(n: usize, names: &[&str]) -> Result<Vec<Family>, String> {
    tex::at_res(n, || {
        names
            .iter()
            .map(|&name| {
                let count = if VARIED.contains(&name) { 1 + VARIANTS } else { 1 };
                let faces = (0..count)
                    .map(|v| materials::paint_named(name, v))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| format!("bm-texgen has no painter for `{name}`"))?;
                Ok(Family {
                    name: name.to_string(),
                    faces,
                })
            })
            .collect()
    })
}

/// The six textures of the 128 trial.
pub const TRIAL: [&str; 6] = ["stone", "turf_top", "soil", "trunk_side", "copper_vein", "foliage"];

/// Whether an item shows a painted icon rather than its block as a cube:
/// every item that is not a block, and props (a torch is no cube).
pub fn has_icon(def: &bm_core::ItemDef, blocks: &bm_core::BlockRegistry) -> bool {
    def.block().is_none_or(|b| blocks.get(b).is_prop())
}

/// The icon every item with one ([`has_icon`]) in the built-in catalogue
/// shows, in catalogue order, without repeats (`icon` in `assets/items.toml`,
/// else the item's name).
pub fn icon_names() -> Vec<String> {
    let blocks = bm_core::BlockRegistry::builtin();
    let items = bm_core::ItemRegistry::builtin(&blocks);
    let mut names: Vec<String> = Vec::new();
    for def in items.defs().iter().filter(|d| has_icon(d, &blocks)) {
        let name = def.icon.clone().unwrap_or_else(|| def.name.clone());
        if !names.contains(&name) {
            names.push(name);
        }
    }
    // Icons painted ahead of the catalogue naming them.
    for n in worlds::ICONS.iter().map(|n| n.to_string()).chain(gems::icon_names()) {
        if !names.contains(&n) {
            names.push(n);
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
    let mut img = image::RgbaImage::new(tex.n as u32, tex.n as u32);
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
        // Every item that is not a block, and every prop, gets an icon.
        let icons = item_icons().expect("every icon has a painter");
        let items = bm_core::ItemRegistry::builtin(&registry);
        assert!(icons.iter().any(|(n, _)| n == "torch"), "the torch has an icon");
        for def in items.defs().iter().filter(|d| has_icon(d, &registry)) {
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
    fn the_trial_paints_at_128_seamlessly_and_leaves_32_alone() {
        let before = families().unwrap();
        let big = families_at(128, &TRIAL).unwrap();
        for fam in &big {
            for face in &fam.faces {
                assert_eq!(face.n, 128, "{}", fam.name);
                assert_eq!(face.px.len(), 128 * 128, "{}", fam.name);
                assert!(face.px.iter().all(|p| p.a >= 1.0), "{} is opaque", fam.name);
            }
            // Seamless: the step across each wrap is like a step inside.
            let f = fam.base();
            let step = |a: color::Rgba, b: color::Rgba| (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs();
            let mean = |pairs: &dyn Fn(usize) -> (color::Rgba, color::Rgba)| {
                (0..128).map(|i| step(pairs(i).0, pairs(i).1)).sum::<f32>() / 128.0
            };
            let across = mean(&|i| (f.get(127, i), f.get(0, i))) + mean(&|i| (f.get(i, 127), f.get(i, 0)));
            let inside = mean(&|i| (f.get(63, i), f.get(64, i))) + mean(&|i| (f.get(i, 63), f.get(i, 64)));
            assert!(across < inside * 2.0 + 0.05, "{}: seam {across} vs {inside}", fam.name);
        }
        // Painting at 128 does not leak into the default size.
        let after = families().unwrap();
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.faces, b.faces, "{} changed", a.name);
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
