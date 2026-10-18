//! The HUD, built as plain triangles in pixel coordinates each frame. No
//! GPU calls here, so layout is unit-testable; `render.rs` uploads the result
//! and draws it in one call over the finished frame.

use crate::textures::NO_TEXTURE;

/// Must match `UiIn` in shaders/ui.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    /// Pixels from the top-left of the window.
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    /// Texture layer, or `NO_TEXTURE` for a flat colour.
    pub layer: u32,
    /// Multiplies the texture (or is the colour), linear RGBA.
    pub color: [f32; 4],
}

/// What a hotbar slot shows: the block's three face layers (top, side,
/// bottom, as in the texture face table) and its colours for blocks without
/// textures.
#[derive(Clone, Copy, Debug)]
pub struct Icon {
    pub layers: [u32; 3],
    pub colors: [[f32; 3]; 3],
}

pub struct Hotbar<'a> {
    pub icons: &'a [Icon],
    pub selected: usize,
}

/// Slots shown at once; the window scrolls to keep the selection in view.
pub const VISIBLE_SLOTS: usize = 10;
const SLOT: f32 = 54.0;
const GAP: f32 = 6.0;
const MARGIN: f32 = 18.0;

const PANEL: [f32; 4] = [0.02, 0.02, 0.035, 0.55];
const RIM: [f32; 4] = [0.85, 0.78, 0.62, 0.35];
const GOLD: [f32; 4] = [1.0, 0.72, 0.22, 1.0];

#[derive(Default)]
pub struct UiBuilder {
    pub verts: Vec<UiVertex>,
}

impl UiBuilder {
    fn tri(&mut self, p: [[f32; 2]; 3], uv: [[f32; 2]; 3], layer: u32, color: [f32; 4]) {
        for i in 0..3 {
            self.verts.push(UiVertex {
                pos: p[i],
                uv: uv[i],
                layer,
                color,
            });
        }
    }

    /// A quad with corners in order (top-left, top-right, bottom-right,
    /// bottom-left), textured with the full layer.
    fn quad(&mut self, p: [[f32; 2]; 4], layer: u32, color: [f32; 4]) {
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.tri([p[0], p[1], p[2]], [uv[0], uv[1], uv[2]], layer, color);
        self.tri([p[0], p[2], p[3]], [uv[0], uv[2], uv[3]], layer, color);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.quad([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], NO_TEXTURE, color);
    }

    /// A rectangular outline `t` pixels thick, inside the given box.
    pub fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, t: f32, color: [f32; 4]) {
        self.rect(x, y, w, t, color);
        self.rect(x, y + h - t, w, t, color);
        self.rect(x, y + t, t, h - 2.0 * t, color);
        self.rect(x + w - t, y + t, t, h - 2.0 * t, color);
    }

    /// A block drawn as a small isometric cube: top, left and right faces
    /// with the classic light from above and the left.
    pub fn block_icon(&mut self, cx: f32, cy: f32, size: f32, icon: &Icon) {
        let w = size * 0.44;
        let top = [cx, cy - size * 0.5];
        let right = [cx + w, cy - size * 0.25];
        let mid = [cx, cy];
        let left = [cx - w, cy - size * 0.25];
        let left_b = [cx - w, cy + size * 0.25];
        let right_b = [cx + w, cy + size * 0.25];
        let bottom = [cx, cy + size * 0.5];
        let face = |i: usize, shade: f32| {
            let c = icon.colors[i];
            if icon.layers[i] == NO_TEXTURE {
                [c[0] * shade, c[1] * shade, c[2] * shade, 1.0]
            } else {
                [shade, shade, shade, 1.0]
            }
        };
        self.quad([top, right, mid, left], icon.layers[0], face(0, 1.0));
        self.quad([left, mid, bottom, left_b], icon.layers[1], face(1, 0.80));
        self.quad([mid, right, right_b, bottom], icon.layers[1], face(1, 0.62));
    }

    pub fn hotbar(&mut self, screen: [f32; 2], bar: &Hotbar) {
        if bar.icons.is_empty() {
            return;
        }
        let shown = bar.icons.len().min(VISIBLE_SLOTS);
        // Scroll so the selection stays inside the visible window.
        let first = bar.selected.saturating_sub(shown - 1).min(bar.icons.len() - shown);
        let width = shown as f32 * SLOT + (shown - 1) as f32 * GAP;
        let x0 = ((screen[0] - width) / 2.0).round();
        let y0 = (screen[1] - MARGIN - SLOT).round();
        for i in 0..shown {
            let idx = first + i;
            let x = x0 + i as f32 * (SLOT + GAP);
            let selected = idx == bar.selected;
            // The selected slot rises a little, like a card lifted from a hand.
            let y = if selected { y0 - 6.0 } else { y0 };
            self.rect(x, y, SLOT, SLOT, PANEL);
            if selected {
                self.frame(x - 2.0, y - 2.0, SLOT + 4.0, SLOT + 4.0, 3.0, GOLD);
            } else {
                self.frame(x, y, SLOT, SLOT, 2.0, RIM);
            }
            self.block_icon(x + SLOT / 2.0, y + SLOT / 2.0, SLOT * 0.62, &bar.icons[idx]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon() -> Icon {
        Icon {
            layers: [1, 2, 3],
            colors: [[1.0; 3]; 3],
        }
    }

    fn bounds(v: &[UiVertex]) -> ([f32; 2], [f32; 2]) {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for p in v {
            for a in 0..2 {
                lo[a] = lo[a].min(p.pos[a]);
                hi[a] = hi[a].max(p.pos[a]);
            }
        }
        (lo, hi)
    }

    #[test]
    fn hotbar_sits_centred_at_the_bottom() {
        let icons = vec![icon(); 5];
        let mut ui = UiBuilder::default();
        ui.hotbar(
            [1280.0, 720.0],
            &Hotbar {
                icons: &icons,
                selected: 2,
            },
        );
        let (lo, hi) = bounds(&ui.verts);
        assert!(((lo[0] + hi[0]) / 2.0 - 640.0).abs() < 1.0, "centred: {lo:?} {hi:?}");
        assert!(hi[1] <= 720.0 - MARGIN + 0.5 && hi[1] > 600.0);
        assert!(ui.verts.len() % 3 == 0);
    }

    #[test]
    fn long_hotbars_scroll_to_keep_the_selection_visible() {
        let icons: Vec<Icon> = (0..15)
            .map(|i| Icon {
                layers: [i, i, i],
                colors: [[1.0; 3]; 3],
            })
            .collect();
        let mut ui = UiBuilder::default();
        ui.hotbar(
            [1280.0, 720.0],
            &Hotbar {
                icons: &icons,
                selected: 13,
            },
        );
        let layers: std::collections::BTreeSet<u32> = ui.verts.iter().map(|v| v.layer).collect();
        assert!(layers.contains(&13), "selected icon drawn");
        assert!(!layers.contains(&0), "first icon scrolled away");
    }

    #[test]
    fn untextured_icons_use_their_colour() {
        let mut ui = UiBuilder::default();
        ui.block_icon(
            50.0,
            50.0,
            32.0,
            &Icon {
                layers: [NO_TEXTURE; 3],
                colors: [[0.5, 0.25, 0.0]; 3],
            },
        );
        assert_eq!(ui.verts[0].color, [0.5, 0.25, 0.0, 1.0]);
    }
}
