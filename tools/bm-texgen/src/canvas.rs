//! A large RGBA canvas for the contact sheet and the preview, with nearest-
//! neighbour blits and a tiny 3 x 5 pixel font for labels.

use crate::color::Rgba;
use crate::tex::{SIZE, Tex};

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgba>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, fill: Rgba) -> Self {
        Self {
            w,
            h,
            px: vec![fill; w * h],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Rgba {
        self.px[y * self.w + x]
    }

    /// Composites `c` over the pixel (ignored outside the canvas).
    pub fn blend(&mut self, x: i64, y: i64, c: Rgba) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let i = y as usize * self.w + x as usize;
        self.px[i] = self.px[i].over(c);
    }

    pub fn rect(&mut self, x: i64, y: i64, w: usize, h: usize, c: Rgba) {
        for j in 0..h as i64 {
            for i in 0..w as i64 {
                self.blend(x + i, y + j, c);
            }
        }
    }

    /// A checkerboard, so translucent textures show their alpha.
    pub fn checker(&mut self, x: i64, y: i64, w: usize, h: usize, cell: usize) {
        let a = Rgba::hex(0x5a5e6e);
        let b = Rgba::hex(0x80859a);
        for j in 0..h {
            for i in 0..w {
                let c = if (i / cell + j / cell) % 2 == 0 { a } else { b };
                self.blend(x + i as i64, y + j as i64, c);
            }
        }
    }

    /// Draws `tex` at `scale` with its top-left at (x, y).
    pub fn blit(&mut self, tex: &Tex, x: i64, y: i64, scale: usize) {
        for ty in 0..SIZE {
            for tx in 0..SIZE {
                let c = tex.get(tx, ty);
                self.rect(x + (tx * scale) as i64, y + (ty * scale) as i64, scale, scale, c);
            }
        }
    }

    /// Draws `text` (letters, digits, `_`, space) at `scale`.
    pub fn text(&mut self, text: &str, x: i64, y: i64, scale: usize, c: Rgba) {
        let mut cx = x;
        for ch in text.chars() {
            let glyph = glyph(ch.to_ascii_uppercase());
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..3 {
                    if bits & (0b100 >> col) != 0 {
                        self.rect(cx + (col * scale) as i64, y + (row * scale) as i64, scale, scale, c);
                    }
                }
            }
            cx += (4 * scale) as i64;
        }
    }

    pub fn to_image(&self) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(self.w as u32, self.h as u32);
        for (i, p) in img.pixels_mut().enumerate() {
            *p = image::Rgba(self.px[i].to_u8());
        }
        img
    }
}

/// Rows of a 3 x 5 glyph, top first, bit 2 = left column.
fn glyph(c: char) -> [u8; 5] {
    match c {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b110, 0b101, 0b101, 0b101, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b110, 0b011],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b110, 0b001, 0b010, 0b100, 0b111],
        '3' => [0b110, 0b001, 0b010, 0b001, 0b110],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b110, 0b001, 0b110],
        '6' => [0b011, 0b100, 0b110, 0b101, 0b010],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b010, 0b101, 0b010, 0b101, 0b010],
        '9' => [0b010, 0b101, 0b011, 0b001, 0b110],
        '_' => [0, 0, 0, 0, 0b111],
        '-' => [0, 0, 0b111, 0, 0],
        _ => [0; 5],
    }
}
