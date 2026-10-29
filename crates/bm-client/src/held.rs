//! Items held in first person (milestone 31).
//!
//! - **Models.** A block item is a small cube wearing its own top, side and
//!   bottom textures, painted in the planet's tint or the stack's colour as
//!   its inventory icon is (`icons::block_icon`). Every other item is its
//!   64 px painted icon extruded into a slab [`SLAB_PX`] icon pixels thick:
//!   each opaque pixel is a column, the front and back show the icon, and
//!   each side face samples the edge pixel it closes. Faces are greedy-merged
//!   (rectangles of opaque pixels front and back, runs of exposed edges on
//!   the sides), so a model is a few hundred quads whatever its painting.
//!   Built once per item, colour and tint (`model_key`), and cached by the
//!   renderer.
//! - **Poses.** Model space is the icon's plane (x right, y up, the icon one
//!   unit across) or the unit cube. [`pose`] places it in view space (x
//!   right, y up, looking down -z) for the right hand; the left hand is its
//!   mirror image (`mirror`), drawn with the mirrored mesh, so both hands
//!   hold their items the same way on their own side.
//! - **Motion.** [`HeldMotion`] turns what the player did into each hand's
//!   use, on the use's real cadence (a tool's swing time from the client's
//!   `Swinger`, the eating time, a grapple's shot), and adds the walk's bob,
//!   the camera's sway and the lowered pose of sprinting and swimming.
//!
//! No GPU here: the renderer uploads [`HeldMesh`]es and draws [`HeldDraw`]s.

use crate::icons::ICON_SIZE;
use crate::textures::{self, NO_TEXTURE};
use bm_core::{BlockDef, BlockRegistry, Colour, ItemDef, ItemKind, NO_TINT};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::path::Path;

/// Thickness of an item's slab in icon pixels: for its 64 pixels, about as
/// thick as a 16 pixel item one pixel deep.
pub const SLAB_PX: f32 = 4.0;
/// A vertex with no texture: its colour alone (boxes such as a lit bomb).
pub const NO_LAYER: u32 = u32::MAX;
/// The held items' own vertical field of view: the option and the sprint
/// widening never stretch them.
pub const HELD_FOV_DEG: f32 = 70.0;
/// Ticks a place or a chisel's jab takes.
const JAB_TICKS: f64 = 14.0;
/// Ticks a grapple's kick takes.
const RECOIL_TICKS: f64 = 16.0;
/// The longest a sling's draw or a bomb's throw is drawn over (a slow
/// cadence waits at rest after it).
const SHOT_TICKS_MAX: f64 = 36.0;

/// One vertex of a held model. Must match `HeldIn` in shaders/held.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HeldVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// The model's own texture layer (0 first), or [`NO_LAYER`].
    pub layer: u32,
    /// Linear rgb multiplying the texture; a: glow (0 lit, 1 shines alone).
    pub color: [f32; 4],
}

/// A held model: triangles and its texture layers (`ICON_SIZE` square,
/// straight sRGB RGBA).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeldMesh {
    pub vertices: Vec<HeldVertex>,
    pub indices: Vec<u32>,
    pub layers: Vec<Vec<u8>>,
}

impl HeldMesh {
    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// One quad `o, o + e1, o + e1 + e2, o + e2` facing `n` (wound so its
    /// front faces `n` whichever order the edges come in), with texture
    /// coordinates `uv0`, `uv0 + du`, ...
    #[allow(clippy::too_many_arguments)]
    fn quad(&mut self, o: Vec3, e1: Vec3, e2: Vec3, n: Vec3, uv0: Vec2, du: Vec2, dv: Vec2, layer: u32) {
        let (e1, e2, du, dv) = if e1.cross(e2).dot(n) >= 0.0 {
            (e1, e2, du, dv)
        } else {
            (e2, e1, dv, du)
        };
        let base = self.vertices.len() as u32;
        for (p, uv) in [
            (o, uv0),
            (o + e1, uv0 + du),
            (o + e1 + e2, uv0 + du + dv),
            (o + e2, uv0 + dv),
        ] {
            self.vertices.push(HeldVertex {
                pos: p.to_array(),
                normal: n.to_array(),
                uv: uv.to_array(),
                layer,
                color: [1.0, 1.0, 1.0, 0.0],
            });
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// The mirror image across x = 0 (the left hand's), wound to face out.
    pub fn mirrored(&self) -> HeldMesh {
        let flip = |v: &HeldVertex| HeldVertex {
            pos: [-v.pos[0], v.pos[1], v.pos[2]],
            normal: [-v.normal[0], v.normal[1], v.normal[2]],
            ..*v
        };
        HeldMesh {
            vertices: self.vertices.iter().map(flip).collect(),
            indices: self.indices.chunks_exact(3).flat_map(|t| [t[0], t[2], t[1]]).collect(),
            layers: self.layers.clone(),
        }
    }
}

/// An icon's opaque pixels: alpha at least half.
fn opaque(rgba: &[u8], n: usize, x: usize, y: usize) -> bool {
    rgba[(y * n + x) * 4 + 3] >= 128
}

/// Extrudes an `n` pixel square icon (RGBA) into a slab `depth` pixels
/// thick, centred on the origin, one unit across: front (+z) and back
/// faces as greedy rectangles of opaque pixels, side faces as runs of
/// pixels whose neighbour that way is transparent or outside, each sampling
/// the icon on texture layer 0. Transparent pixels give nothing.
pub fn extrude(rgba: &[u8], n: usize, depth: f32) -> HeldMesh {
    let mut mesh = HeldMesh::default();
    let k = 1.0 / n as f32;
    let d = depth * k;
    let at = |x: usize, y: usize, z: f32| Vec3::new(x as f32 * k - 0.5, 0.5 - y as f32 * k, z);
    let solid = |x: isize, y: isize| {
        x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n && opaque(rgba, n, x as usize, y as usize)
    };
    // Front and back: greedy rectangles.
    let mut used = vec![false; n * n];
    for y in 0..n {
        let mut x = 0;
        while x < n {
            if used[y * n + x] || !opaque(rgba, n, x, y) {
                x += 1;
                continue;
            }
            let mut w = 1;
            while x + w < n && !used[y * n + x + w] && opaque(rgba, n, x + w, y) {
                w += 1;
            }
            let mut h = 1;
            'grow: while y + h < n {
                for i in x..x + w {
                    if used[(y + h) * n + i] || !opaque(rgba, n, i, y + h) {
                        break 'grow;
                    }
                }
                h += 1;
            }
            for yy in y..y + h {
                for xx in x..x + w {
                    used[yy * n + xx] = true;
                }
            }
            let e1 = Vec3::new(w as f32 * k, 0.0, 0.0);
            let e2 = Vec3::new(0.0, h as f32 * k, 0.0);
            let uv0 = Vec2::new(x as f32 * k, (y + h) as f32 * k);
            let (du, dv) = (Vec2::new(w as f32 * k, 0.0), Vec2::new(0.0, -(h as f32) * k));
            for (z, nz) in [(d / 2.0, 1.0), (-d / 2.0, -1.0)] {
                mesh.quad(at(x, y + h, z), e1, e2, Vec3::Z * nz, uv0, du, dv, 0);
            }
            x += w;
        }
    }
    let thick = Vec3::new(0.0, 0.0, d);
    // Top and bottom edges: runs along x.
    for (dy, ny) in [(-1isize, 1.0f32), (1, -1.0)] {
        for y in 0..n {
            let mut x = 0;
            while x < n {
                let open = |x: usize| solid(x as isize, y as isize) && !solid(x as isize, y as isize + dy);
                if !open(x) {
                    x += 1;
                    continue;
                }
                let mut w = 1;
                while x + w < n && open(x + w) {
                    w += 1;
                }
                let edge_y = if ny > 0.0 { y } else { y + 1 };
                mesh.quad(
                    at(x, edge_y, -d / 2.0),
                    Vec3::new(w as f32 * k, 0.0, 0.0),
                    thick,
                    Vec3::Y * ny,
                    Vec2::new(x as f32 * k, (y as f32 + 0.5) * k),
                    Vec2::new(w as f32 * k, 0.0),
                    Vec2::ZERO,
                    0,
                );
                x += w;
            }
        }
    }
    // Left and right edges: runs along y.
    for (dx, nx) in [(-1isize, -1.0f32), (1, 1.0)] {
        for x in 0..n {
            let mut y = 0;
            while y < n {
                let open = |y: usize| solid(x as isize, y as isize) && !solid(x as isize + dx, y as isize);
                if !open(y) {
                    y += 1;
                    continue;
                }
                let mut h = 1;
                while y + h < n && open(y + h) {
                    h += 1;
                }
                let edge_x = if nx > 0.0 { x + 1 } else { x };
                mesh.quad(
                    at(edge_x, y + h, -d / 2.0),
                    Vec3::new(0.0, h as f32 * k, 0.0),
                    thick,
                    Vec3::X * nx,
                    Vec2::new((x as f32 + 0.5) * k, (y + h) as f32 * k),
                    Vec2::new(0.0, -(h as f32) * k),
                    Vec2::ZERO,
                    0,
                );
                y += h;
            }
        }
    }
    // Hard edges: the kept pixels opaque, the rest clear.
    let mut layer = rgba.to_vec();
    for px in layer.chunks_exact_mut(4) {
        px[3] = if px[3] >= 128 { 255 } else { 0 };
    }
    mesh.layers.push(layer);
    mesh
}

/// A unit cube centred on the origin: top on layer 0, the four sides on
/// layer 1, the bottom on layer 2.
pub fn cube(layers: [Vec<u8>; 3]) -> HeldMesh {
    let mut mesh = HeldMesh::default();
    let h = 0.5;
    let full = (Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0));
    // Sides: texture v runs down the face.
    for (n, right) in [
        (Vec3::Z, Vec3::X),
        (Vec3::X, -Vec3::Z),
        (-Vec3::Z, -Vec3::X),
        (-Vec3::X, Vec3::Z),
    ] {
        let o = n * h - right * h - Vec3::Y * h;
        mesh.quad(o, right, Vec3::Y, n, Vec2::new(0.0, 1.0), full.0, full.1, 1);
    }
    mesh.quad(
        Vec3::new(-h, h, h),
        Vec3::X,
        -Vec3::Z,
        Vec3::Y,
        Vec2::new(0.0, 1.0),
        full.0,
        full.1,
        0,
    );
    mesh.quad(
        Vec3::new(-h, -h, -h),
        Vec3::X,
        Vec3::Z,
        -Vec3::Y,
        Vec2::new(0.0, 1.0),
        full.0,
        full.1,
        2,
    );
    mesh.layers = layers.into();
    mesh
}

/// A block's three faces (top, side, bottom) as `ICON_SIZE` layers, painted
/// as its icon is: tinted, or repainted in `colour` (linear) keeping each
/// texel's light and shade. Holes (leaves, glass) take the face's colour,
/// so the held cube is whole.
pub fn block_layers(
    def: &BlockDef,
    faces_entry: [u32; 4],
    layers: &[textures::Layer],
    tint: [u8; 3],
    colour: Option<[f32; 3]>,
) -> [Vec<u8>; 3] {
    let n = ICON_SIZE;
    let tint = tint.map(textures::srgb_to_linear);
    let colours = [def.top_color.unwrap_or(def.color), def.color, def.color];
    std::array::from_fn(|f| {
        let entry = faces_entry[f];
        let layer = (entry != NO_TEXTURE)
            .then(|| layers.get((entry & 0xffff) as usize))
            .flatten();
        let base = crate::colour::face_luma(entry, layers)
            .unwrap_or_else(|| crate::colour::luma(crate::colour::srgb_to_linear(colours[f])));
        let mut out = vec![255u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let rgb = match layer {
                    Some(px) => {
                        let s = textures::layer_size(px);
                        let i = ((y * s / n) * s + x * s / n) * 4;
                        if px[i + 3] >= 128 {
                            [px[i], px[i + 1], px[i + 2]]
                        } else {
                            colours[f]
                        }
                    }
                    None => colours[f],
                };
                let lin = crate::colour::srgb_to_linear(rgb);
                let lit = match colour {
                    Some(c) => crate::colour::recolour(lin, base, c),
                    None => std::array::from_fn(|c| lin[c] * tint[c]),
                };
                let o = (y * n + x) * 4;
                for c in 0..3 {
                    out[o + c] = textures::linear_to_srgb(lit[c]);
                }
            }
        }
        out
    })
}

/// How an item is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Tools, chisels, spanners, torches: by the handle, head up.
    Tool,
    /// A block item: a cube in the palm.
    Block,
    /// Slings: by the grip, fork up.
    Sling,
    /// A bag of bombs.
    Bombs,
    /// A grapple.
    Grapple,
    /// Everything else (food, brews, materials): shown flat in the palm.
    Item,
}

/// Whether `def` is drawn as a block cube (as its icon is).
fn is_cube(def: &ItemDef, blocks: &BlockRegistry) -> bool {
    matches!(def.kind, ItemKind::Block(b) if blocks.contains(b) && !blocks.get(b).is_prop())
}

pub fn look_of(def: &ItemDef, blocks: &BlockRegistry) -> Look {
    match def.kind {
        _ if is_cube(def, blocks) => Look::Block,
        ItemKind::Block(_) | ItemKind::Tool(_) | ItemKind::Chisel { .. } => Look::Tool,
        ItemKind::Sling(_) => Look::Sling,
        ItemKind::Bomb(_) => Look::Bombs,
        ItemKind::Grapple { .. } => Look::Grapple,
        _ => Look::Item,
    }
}

/// The cache key of an item's model: the item, the stack's colour, and for
/// a block the planet's tint of it (a block looks in the hand as in the
/// ground here).
pub fn model_key(item: u16, colour: Colour, tint: [u8; 3]) -> u64 {
    item as u64 | (colour.0 as u64) << 16 | (u32::from_le_bytes([tint[0], tint[1], tint[2], 0]) as u64) << 32
}

/// The planet's tint of a block item (white for any other).
pub fn tint_of(def: &ItemDef, blocks: &BlockRegistry, tints: &[[u8; 3]]) -> [u8; 3] {
    match def.kind {
        ItemKind::Block(b) if is_cube(def, blocks) => tints.get(b.0 as usize).copied().unwrap_or(NO_TINT),
        _ => NO_TINT,
    }
}

/// Builds an item's held model: a block's cube, or its icon's slab. None
/// for an item with no picture (its initials tile would look wrong in a
/// hand).
pub fn model_for(
    def: &ItemDef,
    blocks: &BlockRegistry,
    set: &textures::TextureSet,
    items_dir: &Path,
    tints: &[[u8; 3]],
    colour: Option<[f32; 3]>,
) -> Option<HeldMesh> {
    if let ItemKind::Block(b) = def.kind
        && is_cube(def, blocks)
    {
        let faces = set.faces.get(b.0 as usize).copied().unwrap_or([NO_TEXTURE; 4]);
        let tint = tints.get(b.0 as usize).copied().unwrap_or(NO_TINT);
        return Some(cube(block_layers(blocks.get(b), faces, &set.layers, tint, colour)));
    }
    let file = def.icon.as_deref().unwrap_or(&def.name);
    let px = crate::icons::load_item_png(&items_dir.join(format!("{file}.png")))?;
    Some(extrude(&px, ICON_SIZE, SLAB_PX))
}

/// A use of a held item, drawn over its own time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// A tool's strike: an arc down and in, over the swing time.
    Swing,
    /// A block placed or a chisel's cut: a short jab.
    Jab,
    /// A sling's shot: drawn back, then let go.
    Draw,
    /// A bomb thrown.
    Throw,
    /// Food or a brew to the mouth.
    Eat,
    /// A grapple's kick as the hook flies.
    Recoil,
}

/// Where a use is: started at `start` (ticks), lasting `length` ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Acting {
    pub act: Act,
    pub start: f64,
    pub length: f64,
}

impl Acting {
    /// How far along at `now` (ticks), 0 to 1; None once done.
    pub fn phase(&self, now: f64) -> Option<f32> {
        let p = (now - self.start) / self.length.max(1.0);
        (0.0..1.0).contains(&p).then_some(p as f32)
    }
}

/// The right hand's rest place for each look: the grip's view position,
/// the model's size, the grip in model space, and its turn.
struct Rest {
    at: Vec3,
    scale: f32,
    grip: Vec3,
    turn: Quat,
}

fn rest(look: Look) -> Rest {
    // The tool icons run handle low left to head high right; held by the
    // handle, the head stands up and leans in towards the crosshair, the
    // face turned a little outwards so the slab shows its depth.
    let tool = Quat::from_rotation_y(0.55) * Quat::from_rotation_z(0.5);
    match look {
        Look::Tool => Rest {
            at: Vec3::new(0.40, -0.33, -0.62),
            scale: 0.40,
            grip: Vec3::new(-0.27, -0.27, 0.0),
            turn: Quat::from_rotation_x(-0.12) * tool,
        },
        Look::Sling => Rest {
            at: Vec3::new(0.40, -0.32, -0.62),
            scale: 0.36,
            grip: Vec3::new(-0.22, -0.22, 0.0),
            turn: tool,
        },
        Look::Block => Rest {
            at: Vec3::new(0.38, -0.28, -0.60),
            scale: 0.13,
            grip: Vec3::ZERO,
            turn: Quat::from_rotation_x(0.25) * Quat::from_rotation_y(-0.6),
        },
        Look::Grapple => Rest {
            at: Vec3::new(0.38, -0.29, -0.62),
            scale: 0.32,
            grip: Vec3::new(-0.05, -0.12, 0.0),
            turn: Quat::from_rotation_y(-0.35) * Quat::from_rotation_z(0.1),
        },
        Look::Bombs | Look::Item => Rest {
            at: Vec3::new(0.38, -0.29, -0.62),
            scale: 0.26,
            grip: Vec3::new(0.0, -0.15, 0.0),
            turn: Quat::from_rotation_x(-0.15) * Quat::from_rotation_y(-0.35),
        },
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A use's offset (view space) and turn about the grip at `phase`, for the
/// right hand. Both are nothing at 0 and at 1.
fn action(act: Act, phase: f32) -> (Vec3, Quat) {
    let p = phase.clamp(0.0, 1.0);
    let wave = (p * std::f32::consts::PI).sin();
    match act {
        Act::Swing => {
            // A quick stroke down and in, a slower return.
            let c = if p < 0.4 {
                (p / 0.4 * std::f32::consts::FRAC_PI_2).sin()
            } else {
                1.0 - smooth((p - 0.4) / 0.6)
            };
            (
                Vec3::new(-0.07, 0.01, -0.06) * c,
                Quat::from_rotation_y(0.35 * c) * Quat::from_rotation_x(-1.05 * c),
            )
        }
        Act::Jab => (
            Vec3::new(-0.03, 0.03, -0.12) * wave,
            Quat::from_rotation_x(-0.25 * wave),
        ),
        Act::Draw => {
            // Pulled back fast, then let go and settled.
            let c = if p < 0.25 {
                smooth(p / 0.25)
            } else {
                1.0 - smooth((p - 0.25) / 0.75)
            };
            (Vec3::new(0.0, 0.015, 0.07) * c, Quat::from_rotation_x(0.2 * c))
        }
        Act::Throw => (Vec3::new(-0.05, 0.10, -0.14) * wave, Quat::from_rotation_x(-1.1 * wave)),
        Act::Eat => {
            let up = smooth(p / 0.2) * (1.0 - smooth((p - 0.8) / 0.2));
            let chew = (p * std::f32::consts::TAU * 3.0).sin() * 0.012 * up;
            (
                Vec3::new(-0.22, 0.13, 0.22) * up + Vec3::Y * chew,
                Quat::from_rotation_x(0.55 * up) * Quat::from_rotation_z(0.3 * up),
            )
        }
        Act::Recoil => {
            let c = (1.0 - p) * (1.0 - p) * smooth(p / 0.1);
            (Vec3::new(0.0, 0.02, 0.07) * c, Quat::from_rotation_x(0.35 * c))
        }
    }
}

/// Everything that moves a held item besides its use.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// The walk's bob and the camera's sway, view space.
    pub offset: Vec3,
    /// 0 at rest to 1 lowered (sprinting, swimming).
    pub lower: f32,
}

/// The model matrix (model space to view space) of an item held in the
/// right hand (`left` false) or the left, using `act` at `phase`.
pub fn pose(look: Look, left: bool, act: Option<(Act, f32)>, motion: Motion) -> Mat4 {
    let r = rest(look);
    let (off, turn) = act.map_or((Vec3::ZERO, Quat::IDENTITY), |(a, p)| action(a, p));
    let l = smooth(motion.lower);
    let lowered = (Vec3::new(0.03, -0.13, 0.05) * l, Quat::from_rotation_x(-0.55 * l));
    let right = Mat4::from_translation(r.at + off + lowered.0 + motion.offset)
        * Mat4::from_quat(lowered.1 * turn * r.turn)
        * Mat4::from_scale(Vec3::splat(r.scale))
        * Mat4::from_translation(-r.grip);
    if left { mirror(right) } else { right }
}

/// The left hand's pose from the right's: mirrored across the view's
/// centre, for the mirrored mesh (so it stays a proper turn).
pub fn mirror(m: Mat4) -> Mat4 {
    let s = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    s * m * s
}

/// The motion a view-space offset and sway take as the frame's inputs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// The input clock: our latest input plus the part of a tick drawn.
    pub tick: f64,
    pub dt: f32,
    pub yaw: f64,
    pub pitch: f64,
    /// The hand that acted last (0 left, 1 right).
    pub hand: usize,
    /// What each hand holds, drawn how.
    pub looks: [Option<Look>; 2],
    /// The last swing, shot or throw (`Swinger::last`): its input and cadence.
    pub swing: Option<(u32, u32)>,
    /// Edits sent so far (places, chisel cuts, swings at blocks).
    pub edits: u32,
    /// Whether each hand's hook is out.
    pub hooks: [bool; 2],
    /// Eating ticks left (`PlayerState::chew`).
    pub chew: u16,
    /// Speed over the ground (blocks a second).
    pub speed: f32,
    pub on_ground: bool,
    pub sprinting: bool,
    pub swimming: bool,
}

/// What the held items are doing, frame to frame.
#[derive(Clone, Debug, Default)]
pub struct HeldMotion {
    pub acting: [Option<Acting>; 2],
    looks: [Option<Look>; 2],
    seen: Option<Frame>,
    sway: Vec2,
    bob_phase: f32,
    bob: f32,
    lower: f32,
    /// Models that cannot be made (no picture): not tried again.
    missing: std::collections::HashSet<u64>,
}

impl HeldMotion {
    /// Takes in one frame: starts a use where the frame shows one began,
    /// and moves the bob, sway and lowering on.
    pub fn update(&mut self, f: &Frame) {
        let dt = f.dt.clamp(0.0, 0.25);
        // A hand whose item changed drops what it was doing.
        for h in 0..2 {
            if f.looks[h] != self.looks[h] {
                self.acting[h] = None;
            }
        }
        self.looks = f.looks;
        if let Some(was) = self.seen {
            let mut start = |hand: usize, act: Act, start: f64, length: f64| {
                if f.looks[hand].is_some() {
                    self.acting[hand] = Some(Acting { act, start, length });
                }
            };
            if f.swing != was.swing
                && let Some((at, ticks)) = f.swing
            {
                let h = f.hand;
                match f.looks[h] {
                    Some(Look::Sling) => start(h, Act::Draw, at as f64, (ticks as f64).min(SHOT_TICKS_MAX)),
                    Some(Look::Bombs) => start(h, Act::Throw, at as f64, (ticks as f64).min(SHOT_TICKS_MAX)),
                    _ => start(h, Act::Swing, at as f64, ticks as f64),
                }
            } else if f.edits != was.edits {
                start(f.hand, Act::Jab, f.tick, JAB_TICKS);
            }
            if f.chew > was.chew {
                start(f.hand, Act::Eat, f.tick, f.chew as f64);
            }
            for h in 0..2 {
                if f.hooks[h] && !was.hooks[h] {
                    start(h, Act::Recoil, f.tick, RECOIL_TICKS);
                }
            }
            // Sway: the item trails a turn of the view and settles back.
            if dt > 0.0 {
                let mut dyaw = (f.yaw - was.yaw) as f32;
                dyaw = (dyaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                let dpitch = (f.pitch - was.pitch) as f32;
                let want = (Vec2::new(-dyaw, -dpitch) / dt * 0.006).clamp_length_max(0.045);
                self.sway += (want - self.sway) * (1.0 - (-dt * 9.0).exp());
            }
        }
        // Bob with the stride, on the ground only.
        let walking = f.on_ground && !f.swimming;
        let want = if walking { (f.speed / 4.0).min(1.0) } else { 0.0 };
        self.bob += (want - self.bob) * (1.0 - (-dt * 6.0).exp());
        self.bob_phase = (self.bob_phase + f.speed * dt * std::f32::consts::PI / 1.7) % std::f32::consts::TAU;
        let want = if (f.sprinting && f.on_ground) || f.swimming {
            1.0
        } else {
            0.0
        };
        self.lower += (want - self.lower) * (1.0 - (-dt * 7.0).exp());
        for a in &mut self.acting {
            if a.is_some_and(|x| x.phase(f.tick).is_none() && f.tick >= x.start) {
                *a = None;
            }
        }
        self.seen = Some(*f);
    }

    /// The bob, sway and lowering now.
    pub fn motion(&self) -> Motion {
        let s = self.bob_phase.sin();
        let bob = Vec3::new(0.011 * s, -0.016 * s * s, 0.0) * self.bob;
        Motion {
            offset: bob + self.sway.extend(0.0),
            lower: self.lower,
        }
    }

    /// Hand `hand`'s use and how far along at `tick`.
    pub fn act(&self, hand: usize, tick: f64) -> Option<(Act, f32)> {
        let a = self.acting[hand]?;
        Some((a.act, a.phase(tick).unwrap_or(0.0)))
    }
}

/// One held model to draw this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldDraw {
    /// `model_key`: the cached model.
    pub key: u64,
    /// Model space to view space.
    pub model: Mat4,
    /// Drawn with the mirrored mesh (the left hand).
    pub mirror: bool,
    /// The light where the player stands, as `BoxInstance::light` has it.
    pub light: [f32; 4],
}

/// Boxes (view space, as `BoxInstance` has them) as held-model triangles
/// with no texture: the lit bomb and its fuse.
pub fn boxes_mesh(boxes: &[crate::render::BoxInstance]) -> HeldMesh {
    let mut mesh = HeldMesh::default();
    for b in boxes {
        let c = Vec3::from(b.center);
        let half = Vec3::from(b.half_size);
        let q = Quat::from_array(b.rotation);
        let start = mesh.vertices.len();
        for n in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            let (u, v) = (n.any_orthonormal_vector(), n.cross(n.any_orthonormal_vector()));
            let (e1, e2) = (q * (u * half * 2.0), q * (v * half * 2.0));
            let o = c + q * (n * half) - e1 / 2.0 - e2 / 2.0;
            mesh.quad(o, e1, e2, q * n, Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, NO_LAYER);
        }
        let glow = b.light[3];
        for v in &mut mesh.vertices[start..] {
            v.color = [b.color[0], b.color[1], b.color[2], glow];
        }
    }
    mesh
}

/// One hand's item this frame.
pub struct Hand<'a> {
    pub stack: bm_core::ItemStack,
    pub def: &'a ItemDef,
}

/// What the held items need from the session this frame.
pub struct Scene<'a> {
    pub hands: [Option<Hand<'a>>; 2],
    pub blocks: &'a BlockRegistry,
    pub textures: &'a textures::TextureSet,
    /// The assets folder (icons are in `textures/items`).
    pub assets: &'a Path,
    pub tints: &'a [[u8; 3]],
    pub palette: &'a bm_core::ColourPalette,
    /// The light where the player stands (`BoxInstance::light_of`).
    pub light: [f32; 4],
    /// The hand that acted last, and how far the bomb lit in hand has burned.
    pub hand: usize,
    pub burned: Option<f32>,
    pub time: f32,
    pub tick: f64,
}

/// What each hand holds, drawn how (the same slot in both hands is the
/// left hand's: the caller passes the right hand empty then).
pub fn looks(scene: &Scene) -> [Option<Look>; 2] {
    std::array::from_fn(|h| scene.hands[h].as_ref().map(|x| look_of(x.def, scene.blocks)))
}

/// Builds (once) and places this frame's held items, and hands them to the
/// renderer.
pub fn draw(motion: &mut HeldMotion, scene: &Scene, renderer: &mut crate::render::Renderer) {
    let mut draws = Vec::new();
    let mut boxes = Vec::new();
    for (h, hand) in scene.hands.iter().enumerate() {
        let Some(hand) = hand else { continue };
        let look = look_of(hand.def, scene.blocks);
        let colour = match hand.def.kind {
            ItemKind::Block(b) if look == Look::Block && scene.blocks.get(b).colourable => hand.stack.colour,
            _ => Colour::NATURAL,
        };
        let key = model_key(hand.stack.item.0, colour, tint_of(hand.def, scene.blocks, scene.tints));
        if !renderer.has_held(key) {
            if motion.missing.contains(&key) {
                continue;
            }
            let items_dir = scene.assets.join("textures").join("items");
            let linear = crate::colour::linear(scene.palette, colour);
            match model_for(hand.def, scene.blocks, scene.textures, &items_dir, scene.tints, linear) {
                Some(mesh) => renderer.upload_held(key, &mesh),
                None => {
                    motion.missing.insert(key);
                    continue;
                }
            }
        }
        let model = pose(look, h == 0, motion.act(h, scene.tick), motion.motion());
        draws.push(HeldDraw {
            key,
            model,
            mirror: h == 0,
            light: scene.light,
        });
        // The bomb lit in the hand that holds the bag, its fuse burning.
        if look == Look::Bombs
            && let Some(b) = scene.burned
            && (scene.hand == h || scene.hands[scene.hand].is_none())
        {
            let (_, turn, _) = model.to_scale_rotation_translation();
            let at = model.transform_point3(Vec3::new(0.0, 0.5, 0.0));
            crate::effects::bomb_boxes(
                &mut boxes,
                at,
                Some(b),
                scene.time,
                scene.light,
                turn,
                crate::effects::HELD_BOMB_SCALE * 0.8,
            );
        }
    }
    renderer.set_held(&draws, &boxes_mesh(&boxes), scene.light);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(n: usize, on: impl Fn(usize, usize) -> bool) -> Vec<u8> {
        let mut px = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                if on(x, y) {
                    let i = (y * n + x) * 4;
                    px[i..i + 4].copy_from_slice(&[(x * 9) as u8, (y * 7) as u8, 99, 255]);
                }
            }
        }
        px
    }

    fn faces_facing(m: &HeldMesh, n: Vec3) -> usize {
        m.vertices
            .chunks_exact(4)
            .filter(|q| Vec3::from(q[0].normal) == n)
            .count()
    }

    #[test]
    fn one_opaque_pixel_is_a_closed_column() {
        let px = icon(8, |x, y| (x, y) == (3, 5));
        let m = extrude(&px, 8, 2.0);
        assert_eq!(m.triangles(), 12, "six faces");
        for n in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            assert_eq!(faces_facing(&m, n), 1, "{n}");
        }
        // The front covers exactly that pixel, the column 2 pixels deep.
        let k = 1.0 / 8.0;
        let xs: Vec<f32> = m.vertices.iter().map(|v| v.pos[0]).collect();
        let zs: Vec<f32> = m.vertices.iter().map(|v| v.pos[2]).collect();
        let (lo, hi) = (
            xs.iter().cloned().fold(9.0, f32::min),
            xs.iter().cloned().fold(-9.0, f32::max),
        );
        assert!((lo - (3.0 * k - 0.5)).abs() < 1e-6 && (hi - (4.0 * k - 0.5)).abs() < 1e-6);
        let depth = zs.iter().cloned().fold(-9.0, f32::max) - zs.iter().cloned().fold(9.0, f32::min);
        assert!((depth - 2.0 * k).abs() < 1e-6);
        // Each side samples the pixel itself.
        for v in &m.vertices {
            let (u, w) = (v.uv[0] * 8.0, v.uv[1] * 8.0);
            assert!((3.0..=4.0).contains(&u) && (5.0..=6.0).contains(&w), "{:?}", v.uv);
        }
    }

    #[test]
    fn transparent_pixels_give_nothing() {
        let px = icon(16, |_, _| false);
        let m = extrude(&px, 16, SLAB_PX);
        assert!(m.vertices.is_empty() && m.indices.is_empty());
        // A half-clear edge (alpha 127) is clear.
        let mut px = icon(4, |x, _| x == 0);
        px[3] = 127;
        let m = extrude(&px, 4, 1.0);
        assert_eq!(faces_facing(&m, Vec3::Z), 1);
        assert_eq!(m.layers[0][3], 0, "the clear pixel stays clear in the texture");
    }

    #[test]
    fn greedy_merging_bounds_the_faces() {
        // A solid square: one face each way.
        let m = extrude(&icon(64, |_, _| true), 64, SLAB_PX);
        assert_eq!(m.triangles(), 12);
        // An L: two rectangles each side, an edge run per straight side.
        let m = extrude(&icon(16, |x, y| x < 4 || y >= 12), 16, SLAB_PX);
        assert_eq!(faces_facing(&m, Vec3::Z), 2);
        assert_eq!(faces_facing(&m, -Vec3::Z), 2);
        assert_eq!(m.vertices.len() / 4, 2 + 2 + 6, "the L's six sides");
        // A real painted icon: far fewer quads than pixels.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/textures/items/iron_hammer.png");
        let px = crate::icons::load_item_png(&path).expect("the hammer's icon");
        let pixels = px.chunks_exact(4).filter(|p| p[3] >= 128).count();
        let m = extrude(&px, ICON_SIZE, SLAB_PX);
        let quads = m.vertices.len() / 4;
        assert!(pixels > 500, "{pixels}");
        assert!(quads * 2 < pixels, "{quads} quads for {pixels} pixels");
        assert!(m.triangles() < 2000, "{}", m.triangles());
    }

    #[test]
    fn every_face_points_out_and_winds_to_its_normal() {
        let m = extrude(&icon(16, |x, y| (x + y) % 5 != 0 && x > 2), 16, SLAB_PX);
        for (mesh, sign) in [(m.clone(), 1.0), (m.mirrored(), -1.0)] {
            for t in mesh.indices.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.vertices[i as usize].pos));
                let n = Vec3::from(mesh.vertices[t[0] as usize].normal);
                assert!((b - a).cross(c - a).dot(n) > 0.0, "wound towards its normal");
                let _ = sign;
            }
        }
        let c = cube([vec![0; 16], vec![0; 16], vec![0; 16]]);
        assert_eq!(c.triangles(), 12);
        for t in c.indices.chunks_exact(3) {
            let [a, b, cc] = [t[0], t[1], t[2]].map(|i| Vec3::from(c.vertices[i as usize].pos));
            let n = Vec3::from(c.vertices[t[0] as usize].normal);
            assert!((b - a).cross(cc - a).dot(n) > 0.0);
            assert!(
                ((a + b + cc) / 3.0).dot(n) > 0.0,
                "the face is on its own side of the cube"
            );
        }
    }

    #[test]
    fn each_hand_holds_its_item_on_its_own_side() {
        for look in [
            Look::Tool,
            Look::Block,
            Look::Item,
            Look::Sling,
            Look::Bombs,
            Look::Grapple,
        ] {
            let right = pose(look, false, None, Motion::default());
            let left = pose(look, true, None, Motion::default());
            let centre = |m: Mat4| m.transform_point3(Vec3::ZERO);
            let (r, l) = (centre(right), centre(left));
            assert!(r.x > 0.1 && l.x < -0.1, "{look:?}: {r} {l}");
            assert!(r.y < 0.0 && r.z < -0.3, "low and in front: {r}");
            assert!((r.x + l.x).abs() < 1e-5 && (r.y - l.y).abs() < 1e-5, "mirror images");
            // The left hand's mirrored mesh is a proper turn (no flipped faces).
            assert!(left.determinant() > 0.0);
            // Seen in the held projection, both are inside the view, low.
            let p = crate::render::projection(HELD_FOV_DEG.to_radians(), 16.0 / 9.0);
            for c in [r, l] {
                let clip = p * c.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                assert!(ndc.x.abs() < 1.0 && ndc.y < -0.2 && ndc.y > -1.0, "{look:?} {ndc}");
            }
        }
    }

    #[test]
    fn a_swing_follows_the_tools_cadence() {
        let mut m = HeldMotion::default();
        let frame = |tick: f64, swing: Option<(u32, u32)>| Frame {
            tick,
            dt: 1.0 / 60.0,
            hand: 1,
            looks: [None, Some(Look::Tool)],
            swing,
            ..Default::default()
        };
        let rest = pose(Look::Tool, false, None, Motion::default());
        m.update(&frame(100.0, None));
        for ticks in [20u32, 45] {
            let at = 100 + ticks * 4;
            m.update(&frame(at as f64, Some((at, ticks))));
            let at_f = at as f64;
            let p = |t: f64| pose(Look::Tool, false, m.act(1, t), Motion::default());
            assert_eq!(m.act(1, at_f), Some((Act::Swing, 0.0)));
            assert!(p(at_f).abs_diff_eq(rest, 1e-5), "starts at rest");
            let mid = p(at_f + ticks as f64 * 0.4);
            assert!(!mid.abs_diff_eq(rest, 0.05), "struck at 40%");
            // The head has come down and in.
            let head = |m: Mat4| m.transform_point3(Vec3::new(0.3, 0.3, 0.0));
            assert!(head(mid).y < head(rest).y - 0.05);
            assert!(
                p(at_f + ticks as f64 * 0.999).abs_diff_eq(rest, 0.01),
                "back by the next swing"
            );
            // Once its time is up the swing is over: the next swing starts fresh.
            m.update(&frame(at_f + ticks as f64, Some((at, ticks))));
            assert_eq!(m.act(1, at_f + ticks as f64), None);
        }
        // A use goes to the hand that acted.
        assert!(m.acting[0].is_none());
    }

    #[test]
    fn uses_start_only_from_real_actions() {
        let mut m = HeldMotion::default();
        let mut f = Frame {
            tick: 10.0,
            dt: 1.0 / 60.0,
            hand: 0,
            looks: [Some(Look::Block), Some(Look::Grapple)],
            ..Default::default()
        };
        m.update(&f);
        f.tick = 11.0;
        m.update(&f);
        assert_eq!(m.acting, [None, None], "nothing happened, nothing moves");
        f.edits = 1;
        f.hooks = [false, true];
        m.update(&f);
        assert_eq!(m.act(0, 11.0).map(|a| a.0), Some(Act::Jab));
        assert_eq!(m.act(1, 11.0).map(|a| a.0), Some(Act::Recoil));
        // A swing at a block also sends an edit: it is a swing, not a jab.
        f.looks[0] = Some(Look::Tool);
        f.tick = 40.0;
        m.update(&f);
        f.edits = 2;
        f.swing = Some((40, 30));
        m.update(&f);
        assert_eq!(m.act(0, 40.0).map(|a| a.0), Some(Act::Swing));
        // Eating, for the eating time.
        f.looks[0] = Some(Look::Item);
        m.update(&f);
        f.chew = 60;
        m.update(&f);
        assert_eq!(m.acting[0].map(|a| (a.act, a.length)), Some((Act::Eat, 60.0)));
    }

    #[test]
    fn sprinting_lowers_and_walking_bobs() {
        let mut m = HeldMotion::default();
        let mut f = Frame {
            dt: 1.0 / 60.0,
            on_ground: true,
            speed: 3.0,
            ..Default::default()
        };
        for i in 0..120 {
            f.tick = i as f64;
            m.update(&f);
        }
        assert!(m.motion().lower < 0.01);
        let ys: Vec<f32> = (0..60)
            .map(|i| {
                f.tick = 120.0 + i as f64;
                m.update(&f);
                m.motion().offset.y
            })
            .collect();
        assert!(ys.iter().cloned().fold(0.0, f32::min) < -0.005, "it bobs");
        f.sprinting = true;
        for i in 0..60 {
            f.tick = 200.0 + i as f64;
            m.update(&f);
        }
        assert!(m.motion().lower > 0.9);
        let low = pose(Look::Tool, false, None, m.motion());
        let up = pose(Look::Tool, false, None, Motion::default());
        assert!(low.transform_point3(Vec3::ZERO).y < up.transform_point3(Vec3::ZERO).y - 0.05);
    }

    #[test]
    fn boxes_become_untextured_triangles() {
        let b = crate::render::BoxInstance::new(
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::splat(0.1),
            [1.0, 0.5, 0.2, 1.0],
            [1.0, 0.0, 1.0, 1.0],
        );
        let m = boxes_mesh(&[b]);
        assert_eq!(m.triangles(), 12);
        assert!(
            m.vertices
                .iter()
                .all(|v| v.layer == NO_LAYER && v.color == [1.0, 0.5, 0.2, 1.0])
        );
        for t in m.indices.chunks_exact(3) {
            let [a, bb, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.vertices[i as usize].pos));
            let n = Vec3::from(m.vertices[t[0] as usize].normal);
            assert!((bb - a).cross(c - a).dot(n) > 0.0);
            assert!(((a + bb + c) / 3.0 - Vec3::new(0.0, 0.0, -1.0)).dot(n) > 0.0);
        }
    }
}
