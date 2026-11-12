//! Graphics objects: bitmaps on the stack, written GROB w h data as on the
//! calculator, and the commands that make and combine them. PICT, the
//! plot's picture, can be read and written as one, and the display shows
//! them in braille characters, a 2 × 4 block of pixels to a character.

use crate::calc::{Calc, Error, Res};
use crate::obj::Obj;
use crate::pause::Pause;
use crate::pretty;

/// The display's size in pixels.
pub const LCD: (usize, usize) = (131, 64);

#[derive(Clone, Debug, PartialEq)]
pub struct Grob {
    pub w: usize,
    pub h: usize,
    px: Vec<bool>,
}

/// How GOR, GXOR and REPL put one grob on another.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Repl,
    Or,
    Xor,
}

impl Grob {
    pub fn new(w: usize, h: usize) -> Grob {
        Grob {
            w,
            h,
            px: vec![false; w * h],
        }
    }

    fn inside(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    pub fn get(&self, x: i64, y: i64) -> bool {
        self.inside(x, y) && self.px[y as usize * self.w + x as usize]
    }

    /// Sets a pixel; those outside are left off, as drawing is clipped.
    pub fn set(&mut self, x: i64, y: i64, on: bool) {
        if self.inside(x, y) {
            self.px[y as usize * self.w + x as usize] = on;
        }
    }

    /// The pixels as the calculator writes them: rows of whole bytes in
    /// hex, each digit four pixels with the leftmost in its lowest bit.
    pub fn hex(&self) -> String {
        let mut s = String::new();
        for y in 0..self.h as i64 {
            for n in 0..self.w.div_ceil(8) as i64 * 2 {
                let v = (0..4)
                    .filter(|b| self.get(n * 4 + b, y))
                    .map(|b| 1 << b)
                    .sum::<u32>();
                s.push(char::from_digit(v, 16).unwrap().to_ascii_uppercase());
            }
        }
        s
    }

    pub fn from_hex(w: usize, h: usize, s: &str) -> Option<Grob> {
        let per = w.div_ceil(8) * 2;
        let digits: Vec<u32> = s.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
        if digits.len() != per * h {
            return None;
        }
        let mut g = Grob::new(w, h);
        for y in 0..h {
            for x in 0..w {
                g.px[y * w + x] = digits[y * per + x / 4] >> (x % 4) & 1 == 1;
            }
        }
        Some(g)
    }

    /// Puts `src` with its top left corner at (x, y), clipped to this grob.
    pub fn paste(&mut self, src: &Grob, x: i64, y: i64, mode: Mode) {
        for j in 0..src.h {
            for i in 0..src.w {
                let (tx, ty) = (x + i as i64, y + j as i64);
                let (s, d) = (src.px[j * src.w + i], self.get(tx, ty));
                self.set(
                    tx,
                    ty,
                    match mode {
                        Mode::Repl => s,
                        Mode::Or => d || s,
                        Mode::Xor => d != s,
                    },
                );
            }
        }
    }

    /// The part between two corners, both included, clipped to the grob.
    pub fn sub(&self, (x0, y0): (i64, i64), (x1, y1): (i64, i64)) -> Grob {
        let clip = |v: i64, n: usize| v.clamp(0, n as i64 - 1);
        if self.w == 0 || self.h == 0 {
            return Grob::new(0, 0);
        }
        let (x0, x1) = (clip(x0.min(x1), self.w), clip(x0.max(x1), self.w));
        let (y0, y1) = (clip(y0.min(y1), self.h), clip(y0.max(y1), self.h));
        let mut g = Grob::new((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
        g.paste(self, -x0, -y0, Mode::Repl);
        g
    }

    pub fn invert(&self) -> Grob {
        Grob {
            px: self.px.iter().map(|p| !p).collect(),
            ..self.clone()
        }
    }

    /// A line between two points in pixels, the part inside drawn.
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        let Some((a, b)) = clip(a, b, (-1.0, -1.0), (self.w as f64, self.h as f64)) else {
            return;
        };
        let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0);
        for k in 0..=steps as i64 {
            let t = k as f64 / steps;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            self.set(x.round() as i64, y.round() as i64, true);
        }
    }

    /// Rows of braille characters, each a 2 × 4 block of pixels.
    pub fn braille(&self) -> Vec<String> {
        // The dots' bits by column and row within a character.
        const DOT: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
        (0..self.h.div_ceil(4) as i64)
            .map(|r| {
                (0..self.w.div_ceil(2) as i64)
                    .map(|c| {
                        let mut bits = 0;
                        for (dx, col) in DOT.iter().enumerate() {
                            for (dy, bit) in col.iter().enumerate() {
                                if self.get(c * 2 + dx as i64, r * 4 + dy as i64) {
                                    bits |= bit;
                                }
                            }
                        }
                        char::from_u32(0x2800 + bits).unwrap()
                    })
                    .collect()
            })
            .collect()
    }
}

/// The segment from a to b within the box from lo to hi (Liang and
/// Barsky's clipping), or None if none of it is; None too where a point is
/// undefined.
fn clip(
    a: (f64, f64),
    b: (f64, f64),
    lo: (f64, f64),
    hi: (f64, f64),
) -> Option<((f64, f64), (f64, f64))> {
    if ![a.0, a.1, b.0, b.1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a.0 - lo.0),
        (dx, hi.0 - a.0),
        (-dy, a.1 - lo.1),
        (dy, hi.1 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then_some((
        (a.0 + dx * t0, a.1 + dy * t0),
        (a.0 + dx * t1, a.1 + dy * t1),
    ))
}

/// The small font's characters, 3 × 5 pixels in a 4 × 6 cell: a row to a
/// number, its high bit the left pixel. From space to `, then { | } ~;
/// letters are capitals.
const SMALL: [[u8; 5]; 69] = [
    [0, 0, 0, 0, 0],
    [2, 2, 2, 0, 2],
    [5, 5, 0, 0, 0],
    [5, 7, 5, 7, 5],
    [3, 6, 2, 3, 6],
    [5, 1, 2, 4, 5],
    [2, 5, 2, 5, 3],
    [2, 2, 0, 0, 0],
    [1, 2, 2, 2, 1],
    [4, 2, 2, 2, 4],
    [0, 5, 2, 5, 0],
    [0, 2, 7, 2, 0],
    [0, 0, 0, 2, 4],
    [0, 0, 7, 0, 0],
    [0, 0, 0, 0, 2],
    [1, 1, 2, 4, 4],
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 3, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 2, 2],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
    [0, 2, 0, 2, 0],
    [0, 2, 0, 2, 4],
    [1, 2, 4, 2, 1],
    [0, 7, 0, 7, 0],
    [4, 2, 1, 2, 4],
    [7, 1, 3, 0, 2],
    [2, 5, 7, 4, 3],
    [2, 5, 7, 5, 5],
    [6, 5, 6, 5, 6],
    [3, 4, 4, 4, 3],
    [6, 5, 5, 5, 6],
    [7, 4, 6, 4, 7],
    [7, 4, 6, 4, 4],
    [3, 4, 5, 5, 3],
    [5, 5, 7, 5, 5],
    [7, 2, 2, 2, 7],
    [1, 1, 1, 5, 2],
    [5, 5, 6, 5, 5],
    [4, 4, 4, 4, 7],
    [5, 7, 7, 5, 5],
    [6, 5, 5, 5, 5],
    [2, 5, 5, 5, 2],
    [6, 5, 6, 4, 4],
    [2, 5, 5, 6, 3],
    [6, 5, 6, 5, 5],
    [3, 4, 2, 1, 6],
    [7, 2, 2, 2, 2],
    [5, 5, 5, 5, 7],
    [5, 5, 5, 5, 2],
    [5, 5, 7, 7, 5],
    [5, 5, 2, 5, 5],
    [5, 5, 2, 2, 2],
    [7, 1, 2, 4, 7],
    [3, 2, 2, 2, 3],
    [4, 4, 2, 1, 1],
    [6, 2, 2, 2, 6],
    [2, 5, 0, 0, 0],
    [0, 0, 0, 0, 7],
    [4, 2, 0, 0, 0],
    [3, 2, 6, 2, 3],
    [2, 2, 2, 2, 2],
    [6, 2, 3, 2, 6],
    [0, 3, 6, 0, 0],
];

fn small(c: char) -> [u8; 5] {
    let c = c.to_ascii_uppercase();
    match c {
        ' '..='`' => SMALL[c as usize - 0x20],
        '{'..='~' => SMALL[c as usize - 0x7b + 65],
        'π' => [0, 7, 5, 5, 5],
        'Σ' => [7, 4, 2, 4, 7],
        '→' => [0, 1, 7, 1, 0],
        '←' => [0, 4, 7, 4, 0],
        '√' => [3, 2, 2, 6, 2],
        '∫' => [1, 2, 2, 2, 4],
        'θ' => [2, 5, 7, 5, 2],
        '∠' => [0, 1, 2, 4, 7],
        '×' => [0, 5, 2, 5, 0],
        '÷' => [2, 0, 7, 0, 2],
        '°' => [2, 5, 2, 0, 0],
        '…' => [0, 0, 0, 0, 5],
        _ => [0, 7, 5, 7, 0],
    }
}

/// The medium font's characters, 5 × 7 pixels in a 6 × 8 cell: a column to
/// a number, its low bit the top pixel. From space to ~.
const MEDIUM: [[u8; 5]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00],
    [0x00, 0x00, 0x5F, 0x00, 0x00],
    [0x00, 0x07, 0x00, 0x07, 0x00],
    [0x14, 0x7F, 0x14, 0x7F, 0x14],
    [0x24, 0x2A, 0x7F, 0x2A, 0x12],
    [0x23, 0x13, 0x08, 0x64, 0x62],
    [0x36, 0x49, 0x56, 0x20, 0x50],
    [0x00, 0x00, 0x07, 0x00, 0x00],
    [0x00, 0x1C, 0x22, 0x41, 0x00],
    [0x00, 0x41, 0x22, 0x1C, 0x00],
    [0x14, 0x08, 0x3E, 0x08, 0x14],
    [0x08, 0x08, 0x3E, 0x08, 0x08],
    [0x00, 0x50, 0x30, 0x00, 0x00],
    [0x08, 0x08, 0x08, 0x08, 0x08],
    [0x00, 0x60, 0x60, 0x00, 0x00],
    [0x20, 0x10, 0x08, 0x04, 0x02],
    [0x3E, 0x51, 0x49, 0x45, 0x3E],
    [0x00, 0x42, 0x7F, 0x40, 0x00],
    [0x42, 0x61, 0x51, 0x49, 0x46],
    [0x21, 0x41, 0x45, 0x4B, 0x31],
    [0x18, 0x14, 0x12, 0x7F, 0x10],
    [0x27, 0x45, 0x45, 0x45, 0x39],
    [0x3C, 0x4A, 0x49, 0x49, 0x30],
    [0x01, 0x71, 0x09, 0x05, 0x03],
    [0x36, 0x49, 0x49, 0x49, 0x36],
    [0x06, 0x49, 0x49, 0x29, 0x1E],
    [0x00, 0x36, 0x36, 0x00, 0x00],
    [0x00, 0x56, 0x36, 0x00, 0x00],
    [0x08, 0x14, 0x22, 0x41, 0x00],
    [0x14, 0x14, 0x14, 0x14, 0x14],
    [0x00, 0x41, 0x22, 0x14, 0x08],
    [0x02, 0x01, 0x51, 0x09, 0x06],
    [0x32, 0x49, 0x79, 0x41, 0x3E],
    [0x7E, 0x11, 0x11, 0x11, 0x7E],
    [0x7F, 0x49, 0x49, 0x49, 0x36],
    [0x3E, 0x41, 0x41, 0x41, 0x22],
    [0x7F, 0x41, 0x41, 0x22, 0x1C],
    [0x7F, 0x49, 0x49, 0x49, 0x41],
    [0x7F, 0x09, 0x09, 0x09, 0x01],
    [0x3E, 0x41, 0x49, 0x49, 0x7A],
    [0x7F, 0x08, 0x08, 0x08, 0x7F],
    [0x00, 0x41, 0x7F, 0x41, 0x00],
    [0x20, 0x40, 0x41, 0x3F, 0x01],
    [0x7F, 0x08, 0x14, 0x22, 0x41],
    [0x7F, 0x40, 0x40, 0x40, 0x40],
    [0x7F, 0x02, 0x0C, 0x02, 0x7F],
    [0x7F, 0x04, 0x08, 0x10, 0x7F],
    [0x3E, 0x41, 0x41, 0x41, 0x3E],
    [0x7F, 0x09, 0x09, 0x09, 0x06],
    [0x3E, 0x41, 0x51, 0x21, 0x5E],
    [0x7F, 0x09, 0x19, 0x29, 0x46],
    [0x46, 0x49, 0x49, 0x49, 0x31],
    [0x01, 0x01, 0x7F, 0x01, 0x01],
    [0x3F, 0x40, 0x40, 0x40, 0x3F],
    [0x1F, 0x20, 0x40, 0x20, 0x1F],
    [0x3F, 0x40, 0x38, 0x40, 0x3F],
    [0x63, 0x14, 0x08, 0x14, 0x63],
    [0x07, 0x08, 0x70, 0x08, 0x07],
    [0x61, 0x51, 0x49, 0x45, 0x43],
    [0x00, 0x7F, 0x41, 0x41, 0x00],
    [0x02, 0x04, 0x08, 0x10, 0x20],
    [0x00, 0x41, 0x41, 0x7F, 0x00],
    [0x04, 0x02, 0x01, 0x02, 0x04],
    [0x40, 0x40, 0x40, 0x40, 0x40],
    [0x00, 0x01, 0x02, 0x04, 0x00],
    [0x20, 0x54, 0x54, 0x54, 0x78],
    [0x7F, 0x48, 0x44, 0x44, 0x38],
    [0x38, 0x44, 0x44, 0x44, 0x20],
    [0x38, 0x44, 0x44, 0x48, 0x7F],
    [0x38, 0x54, 0x54, 0x54, 0x18],
    [0x08, 0x7E, 0x09, 0x01, 0x02],
    [0x0C, 0x52, 0x52, 0x52, 0x3E],
    [0x7F, 0x08, 0x04, 0x04, 0x78],
    [0x00, 0x44, 0x7D, 0x40, 0x00],
    [0x20, 0x40, 0x44, 0x3D, 0x00],
    [0x7F, 0x10, 0x28, 0x44, 0x00],
    [0x00, 0x41, 0x7F, 0x40, 0x00],
    [0x7C, 0x04, 0x18, 0x04, 0x78],
    [0x7C, 0x08, 0x04, 0x04, 0x78],
    [0x38, 0x44, 0x44, 0x44, 0x38],
    [0x7C, 0x14, 0x14, 0x14, 0x08],
    [0x08, 0x14, 0x14, 0x18, 0x7C],
    [0x7C, 0x08, 0x04, 0x04, 0x08],
    [0x48, 0x54, 0x54, 0x54, 0x20],
    [0x04, 0x3F, 0x44, 0x40, 0x20],
    [0x3C, 0x40, 0x40, 0x20, 0x7C],
    [0x1C, 0x20, 0x40, 0x20, 0x1C],
    [0x3C, 0x40, 0x30, 0x40, 0x3C],
    [0x44, 0x28, 0x10, 0x28, 0x44],
    [0x0C, 0x50, 0x50, 0x50, 0x3C],
    [0x44, 0x64, 0x54, 0x4C, 0x44],
    [0x00, 0x08, 0x36, 0x41, 0x00],
    [0x00, 0x00, 0x7F, 0x00, 0x00],
    [0x00, 0x41, 0x36, 0x08, 0x00],
    [0x08, 0x04, 0x08, 0x10, 0x08],
];

fn medium(c: char) -> [u8; 5] {
    match c {
        ' '..='~' => MEDIUM[c as usize - 0x20],
        'π' => [0x04, 0x7C, 0x04, 0x7C, 0x04],
        '→' => [0x08, 0x08, 0x2A, 0x1C, 0x08],
        '←' => [0x08, 0x1C, 0x2A, 0x08, 0x08],
        '↑' => [0x04, 0x02, 0x7F, 0x02, 0x04],
        '↓' => [0x10, 0x20, 0x7F, 0x20, 0x10],
        'Σ' => [0x63, 0x55, 0x49, 0x41, 0x63],
        '√' => [0x10, 0x20, 0x40, 0x3F, 0x01],
        '∫' => [0x20, 0x40, 0x3E, 0x01, 0x02],
        '∂' => [0x30, 0x4A, 0x49, 0x49, 0x3E],
        'α' => [0x38, 0x44, 0x44, 0x38, 0x44],
        'θ' => [0x3E, 0x49, 0x49, 0x49, 0x3E],
        'λ' => [0x40, 0x31, 0x0E, 0x30, 0x40],
        'μ' => [0x7C, 0x20, 0x20, 0x10, 0x3C],
        'ρ' => [0x7C, 0x14, 0x14, 0x14, 0x08],
        'σ' => [0x38, 0x44, 0x44, 0x3C, 0x04],
        'τ' => [0x04, 0x3C, 0x44, 0x24, 0x04],
        'ω' => [0x3C, 0x40, 0x30, 0x40, 0x3C],
        'ε' => [0x38, 0x54, 0x54, 0x44, 0x28],
        'Δ' => [0x70, 0x4C, 0x43, 0x4C, 0x70],
        'Π' => [0x7F, 0x01, 0x01, 0x01, 0x7F],
        'Ω' => [0x5E, 0x61, 0x01, 0x61, 0x5E],
        '≤' => [0x44, 0x4A, 0x51, 0x40, 0x40],
        '≥' => [0x40, 0x40, 0x51, 0x4A, 0x44],
        '≠' => [0x14, 0x34, 0x1C, 0x16, 0x14],
        '≡' => [0x2A, 0x2A, 0x2A, 0x2A, 0x2A],
        '∞' => [0x18, 0x24, 0x18, 0x24, 0x18],
        '×' => [0x22, 0x14, 0x08, 0x14, 0x22],
        '÷' => [0x08, 0x08, 0x2A, 0x08, 0x08],
        '°' => [0x00, 0x06, 0x09, 0x06, 0x00],
        '«' => [0x08, 0x14, 0x2A, 0x14, 0x22],
        '»' => [0x22, 0x14, 0x2A, 0x14, 0x08],
        '∠' => [0x60, 0x50, 0x48, 0x44, 0x42],
        '▶' => [0x7F, 0x3E, 0x1C, 0x08, 0x00],
        '…' => [0x40, 0x00, 0x40, 0x00, 0x40],
        '‾' => [0x01, 0x01, 0x01, 0x01, 0x01],
        '⎡' | '⎛' => [0x00, 0x7F, 0x01, 0x00, 0x00],
        '⎢' | '⎜' => [0x00, 0x7F, 0x00, 0x00, 0x00],
        '⎣' | '⎝' => [0x00, 0x7F, 0x40, 0x00, 0x00],
        '⎤' | '⎞' => [0x00, 0x00, 0x01, 0x7F, 0x00],
        '⎥' | '⎟' => [0x00, 0x00, 0x00, 0x7F, 0x00],
        '⎦' | '⎠' => [0x00, 0x00, 0x40, 0x7F, 0x00],
        _ => [0x7F, 0x41, 0x41, 0x41, 0x7F],
    }
}

/// A font's cell in pixels: 1 small, 2 medium, 3 large.
fn cell(size: u8) -> (usize, usize) {
    match size {
        1 => (4, 6),
        2 => (6, 8),
        _ => (8, 10),
    }
}

/// Draws a character with its cell's top left corner at (x, y). The large
/// font is the medium one in bold, a pixel down.
fn draw_char(g: &mut Grob, c: char, x: i64, y: i64, size: u8) {
    let (cw, ch) = cell(size);
    // Rules join up across cells, as in the equation writer's fractions.
    match c {
        '─' => (0..cw as i64).for_each(|i| g.set(x + i, y + ch as i64 / 2 - 1, true)),
        '│' => (0..ch as i64).for_each(|j| g.set(x + cw as i64 / 2 - 1, y + j, true)),
        _ if size == 1 => {
            for (j, row) in small(c).iter().enumerate() {
                for i in 0..3 {
                    if row >> (2 - i) & 1 == 1 {
                        g.set(x + i, y + j as i64, true);
                    }
                }
            }
        }
        _ => {
            let (bold, down) = if size == 3 { (1, 1) } else { (0, 0) };
            for (i, col) in medium(c).iter().enumerate() {
                for j in 0..7 {
                    if col >> j & 1 == 1 {
                        for b in 0..=bold {
                            g.set(x + i as i64 + b, y + j + down, true);
                        }
                    }
                }
            }
        }
    }
}

/// Lines of text in one of the fonts.
fn text_rows(rows: &[Vec<char>], size: u8) -> Grob {
    let (cw, ch) = cell(size);
    let w = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut g = Grob::new(w * cw, rows.len() * ch);
    for (r, row) in rows.iter().enumerate() {
        for (i, &c) in row.iter().enumerate() {
            draw_char(&mut g, c, (i * cw) as i64, (r * ch) as i64, size);
        }
    }
    g
}

pub fn text(s: &str, size: u8) -> Grob {
    let rows: Vec<Vec<char>> = s.split('\n').map(|l| l.chars().collect()).collect();
    text_rows(&rows, size)
}

fn is_pict(o: Option<&Obj>) -> bool {
    matches!(o, Some(Obj::Name(n)) if n == "PICT")
}

impl Calc {
    /// The grob of an object: strings as their text, algebraics and units
    /// drawn as the equation writer draws them when the size is 0.
    fn to_grob(&self, o: &Obj, size: u8) -> Grob {
        match o {
            Obj::Grob(g) => g.clone(),
            Obj::Alg(_) | Obj::Unit(..) if size == 0 => {
                text_rows(&pretty::view(o, self.fmt).rows, 3)
            }
            Obj::Str(s) => text(s, size),
            o => text(&o.display(self.fmt), size),
        }
    }

    /// The display as it is: what DISP put up, or the stack's last seven
    /// levels, and the menu keys' labels below.
    fn lcd(&self) -> Grob {
        let mut g = Grob::new(LCD.0, LCD.1);
        const COLS: usize = 21;
        let lines: Vec<String> = if self.disp.is_empty() {
            (1..=7)
                .rev()
                .map(|l| {
                    let label = format!("{l}:");
                    let text = match self.stack.len().checked_sub(l) {
                        Some(i) => self.stack[i].display(self.fmt),
                        None => String::new(),
                    };
                    let room = COLS - label.len() - 1;
                    let text: String = if text.chars().count() > room {
                        text.chars().take(room - 1).chain(['…']).collect()
                    } else {
                        text
                    };
                    let pad = COLS - label.len() - text.chars().count();
                    format!("{label}{}{text}", " ".repeat(pad))
                })
                .collect()
        } else {
            self.disp.clone()
        };
        for (i, l) in lines.iter().take(7).enumerate() {
            let l: String = l.chars().take(COLS + 1).collect();
            g.paste(&text(&l, 2), 0, i as i64 * 8, Mode::Or);
        }
        for (k, label) in self.menu.page(self, self.page).iter().enumerate() {
            if label.is_empty() {
                continue;
            }
            let x = k as i64 * 22;
            let mut key = Grob::new(21, 8).invert();
            let name = text(&label.chars().take(5).collect::<String>(), 1);
            key.paste(&name, (21 - name.w as i64) / 2 + 1, 1, Mode::Xor);
            g.paste(&key, x, 56, Mode::Repl);
        }
        g
    }

    /// Shows a grob in the display's top left corner, over the stack, as
    /// DISP shows text.
    fn show_lcd(&mut self, g: &Grob) -> Res<()> {
        let mut screen = Grob::new(LCD.0, LCD.1);
        screen.paste(g, 0, 0, Mode::Repl);
        self.disp = screen.braille();
        let shown = self.disp.join("\n");
        self.pause_for(Pause::Show(shown)).map(drop)
    }

    /// grob1 … grobn n ANIMATE, or { n { #x #y } delay repeat } in place
    /// of n: each grob shown in turn, delay seconds apart (.1 if not
    /// given), the whole repeat times (once if not given, forever if 0).
    /// The grobs stay on the stack.
    fn animate(&mut self) -> Res<()> {
        self.need(1)?;
        let spec = match self.peek(1) {
            Obj::Real(n) => vec![Obj::Real(*n)],
            Obj::List(l) if !l.is_empty() && l.len() <= 4 => l.clone(),
            Obj::List(_) => return Err(Error::BadValue),
            _ => return Err(Error::BadType),
        };
        let Obj::Real(n) = spec[0] else {
            return Err(Error::BadType);
        };
        if n < 1.0 || n.fract() != 0.0 {
            return Err(Error::BadValue);
        }
        let n = n as usize;
        let at = match spec.get(1) {
            Some(p) => self.pixel_of(p)?,
            None => (0, 0),
        };
        let real = |i: usize, default: f64| match spec.get(i) {
            Some(Obj::Real(x)) if *x >= 0.0 => Ok(*x),
            Some(Obj::Real(_)) => Err(Error::BadValue),
            Some(_) => Err(Error::BadType),
            None => Ok(default),
        };
        let (delay, reps) = (real(2, 0.1)?, real(3, 1.0)?);
        self.need(n + 1)?;
        let base = self.stack.len() - 1 - n;
        let frames = self.stack[base..base + n]
            .iter()
            .map(|o| match o {
                Obj::Grob(g) => Ok(g.clone()),
                _ => Err(Error::BadType),
            })
            .collect::<Res<Vec<_>>>()?;
        self.stack.pop();
        // Without a display there is nothing to watch.
        if self.pause.is_none() {
            return Ok(());
        }
        let mut done = 0.0;
        'all: while reps == 0.0 || done < reps {
            for f in &frames {
                let mut screen = Grob::new(LCD.0, LCD.1);
                screen.paste(f, at.0, at.1, Mode::Repl);
                self.show_lcd(&screen)?;
                match self.sleep(delay) {
                    Err(Error::Interrupted) => break 'all,
                    r => r?,
                }
            }
            done += 1.0;
        }
        Ok(())
    }

    /// The grob commands, and those (SUB REPL SIZE + NEG RCL STO PURGE)
    /// whose grob or PICT forms are handled here before the usual ones.
    pub(crate) fn grob_cmd(&mut self, w: &str) -> Option<Res<()>> {
        let level = |i: usize| self.stack.len().checked_sub(i).map(|k| &self.stack[k]);
        let grob = |i: usize| matches!(level(i), Some(Obj::Grob(_)));
        let (g1, g2, g3) = (grob(1), grob(2), grob(3));
        let (p1, p3) = (is_pict(level(1)), is_pict(level(3)));
        let mode = match w {
            "GOR" => Mode::Or,
            "GXOR" => Mode::Xor,
            _ => Mode::Repl,
        };
        Some(match w {
            "PICT" => {
                self.stack.push(Obj::Name("PICT".into()));
                Ok(())
            }
            // obj n →GROB: in the small (1), medium (2) or large (3) font;
            // 0 is large, with algebraics drawn out.
            "→GROB" => self.args(2, |c, a| {
                let Obj::Real(n) = a[1] else {
                    return Err(Error::BadType);
                };
                if !(0.0..=3.0).contains(&n) || n.fract() != 0.0 {
                    return Err(Error::BadValue);
                }
                Ok(vec![Obj::Grob(c.to_grob(&a[0], n as u8))])
            }),
            "BLANK" => self.args(2, |_, a| match (&a[0], &a[1]) {
                (Obj::Bin(w), Obj::Bin(h)) if *w <= 4096 && *h <= 4096 => {
                    Ok(vec![Obj::Grob(Grob::new(*w as usize, *h as usize))])
                }
                (Obj::Bin(_), Obj::Bin(_)) => Err(Error::Memory),
                _ => Err(Error::BadType),
            }),
            // grob1 { #x #y } grob2 GOR: grob2 on grob1 there; PICT in
            // place of grob1 draws on PICT. REPL is the same, replacing.
            "GOR" | "GXOR" | "REPL" if w != "REPL" || g3 || p3 => self.args(3, |c, a| {
                let Obj::Grob(src) = &a[2] else {
                    return Err(Error::BadType);
                };
                let (x, y) = c.pixel_of(&a[1])?;
                match &a[0] {
                    Obj::Grob(g) => {
                        let mut g = g.clone();
                        g.paste(src, x, y, mode);
                        Ok(vec![Obj::Grob(g)])
                    }
                    o if is_pict(Some(o)) => {
                        let mut g = c.pict_grob();
                        g.paste(src, x, y, mode);
                        c.set_pict_grob(g);
                        Ok(vec![])
                    }
                    _ => Err(Error::BadType),
                }
            }),
            // grob { #x1 #y1 } { #x2 #y2 } SUB: the part between the corners.
            "SUB" if g3 || p3 => self.args(3, |c, a| {
                let g = match &a[0] {
                    Obj::Grob(g) => g.clone(),
                    _ => c.pict_grob(),
                };
                let (from, to) = (c.pixel_of(&a[1])?, c.pixel_of(&a[2])?);
                Ok(vec![Obj::Grob(g.sub(from, to))])
            }),
            "SIZE" if g1 || p1 => self.args(1, |c, a| {
                let (w, h) = match &a[0] {
                    Obj::Grob(g) => (g.w, g.h),
                    _ => c.pict_size,
                };
                Ok(vec![Obj::Bin(w as u64), Obj::Bin(h as u64)])
            }),
            // Adding grobs of the same size ORs them; NEG inverts one.
            "+" if g1 && g2 => self.args(2, |_, a| {
                let (Obj::Grob(x), Obj::Grob(y)) = (&a[0], &a[1]) else {
                    unreachable!()
                };
                if (x.w, x.h) != (y.w, y.h) {
                    return Err(Error::BadValue);
                }
                let mut g = x.clone();
                g.paste(y, 0, 0, Mode::Or);
                Ok(vec![Obj::Grob(g)])
            }),
            "NEG" if g1 => self.args(1, |_, a| match &a[0] {
                Obj::Grob(g) => Ok(vec![Obj::Grob(g.invert())]),
                _ => unreachable!(),
            }),
            "RCL" if p1 => self.args(1, |c, _| Ok(vec![Obj::Grob(c.pict_grob())])),
            "STO" if p1 => self.args(2, |c, a| match &a[0] {
                Obj::Grob(g) => {
                    c.set_pict_grob(g.clone());
                    Ok(vec![])
                }
                _ => Err(Error::BadType),
            }),
            "PURGE" if p1 => self.args(1, |c, _| {
                c.pict.clear();
                Ok(vec![])
            }),
            "LCD→" => {
                let g = self.lcd();
                self.stack.push(Obj::Grob(g));
                Ok(())
            }
            "→LCD" => self.need(1).and_then(|_| {
                let Obj::Grob(g) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                self.stack.pop();
                self.show_lcd(&g)
            }),
            "ANIMATE" => self.animate(),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Grob, text};
    use crate::calc::Calc;
    use crate::obj::Obj;

    fn run(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    fn art(g: &Grob) -> Vec<String> {
        (0..g.h as i64)
            .map(|y| {
                (0..g.w as i64)
                    .map(|x| if g.get(x, y) { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn hex() {
        assert_eq!(run("GROB 1 1 10"), ["GROB 1 1 10"]);
        assert_eq!(run("GROB 9 2 FF100000"), ["GROB 9 2 FF100000"]);
        assert_eq!(run("{ GROB 3 1 50 }"), ["{ GROB 3 1 50 }"]);
        let mut c = Calc::new();
        c.exec_line("GROB 4 2 3000").unwrap();
        let Obj::Grob(g) = &c.stack[0] else { panic!() };
        assert_eq!(art(g), ["##..", "...."]);
        assert!(c.exec_line("GROB 4 2 30").is_err());
    }

    #[test]
    fn fonts() {
        assert_eq!(
            art(&text("A1", 2)),
            [
                ".###....#...",
                "#...#..##...",
                "#...#...#...",
                "#...#...#...",
                "#####...#...",
                "#...#...#...",
                "#...#..###..",
                "............",
            ]
        );
        assert_eq!(
            art(&text("Hi", 1)),
            [
                "#.#.###.", "#.#..#..", "###..#..", "#.#..#..", "#.#.###.", "........"
            ]
        );
        assert_eq!(run("\"AB\" 1 →GROB SIZE"), ["# 8d", "# 6d"]);
        assert_eq!(run("\"AB\nC\" 2 →GROB SIZE"), ["# 12d", "# 16d"]);
        assert_eq!(run("\"AB\" 3 →GROB SIZE"), ["# 16d", "# 10d"]);
        assert_eq!(run("'A/B' 0 →GROB SIZE"), ["# 24d", "# 30d"]);
        assert_eq!(run("12 2 →GROB SIZE"), ["# 12d", "# 8d"]);
    }

    #[test]
    fn combining() {
        assert_eq!(run("# 3d # 2d BLANK"), ["GROB 3 2 0000"]);
        assert_eq!(
            run("# 4d # 1d BLANK { # 1d # 0d } GROB 2 1 30 GOR"),
            ["GROB 4 1 60"]
        );
        assert_eq!(
            run("GROB 4 1 F0 { # 1d # 0d } GROB 2 1 30 GXOR"),
            ["GROB 4 1 90"]
        );
        assert_eq!(
            run("GROB 4 1 F0 { # 2d # 0d } GROB 4 1 10 REPL"),
            ["GROB 4 1 70"]
        );
        assert_eq!(
            run("GROB 4 2 F010 { # 0d # 1d } { # 2d # 1d } SUB"),
            ["GROB 3 1 10"]
        );
        assert_eq!(
            run("GROB 4 1 30 NEG GROB 4 1 10 GROB 4 1 40 +"),
            ["GROB 4 1 C0", "GROB 4 1 50"]
        );
        assert_eq!(
            run("GROB 4 1 30 TYPE GROB 1 1 10 →STR"),
            ["11", "\"Graphic 1 × 1\""]
        );
        assert_eq!(run("GROB 2 1 10 DUP SAME"), ["1"]);
        // REPL and SUB on lists and strings are as they were.
        assert_eq!(
            run("\"ABCD\" 2 3 SUB { 1 2 } 1 { 3 } REPL"),
            ["\"BC\"", "{ 3 2 }"]
        );
    }

    #[test]
    fn pict() {
        let mut c = Calc::new();
        c.exec_line("ERASE PICT RCL SIZE").unwrap();
        assert_eq!(c.stack, [Obj::Bin(131), Obj::Bin(64)]);
        // A line across PICT through PPAR's ranges, as pixels.
        c.exec_line("CLEAR { # 0d # 10d } { # 130d # 10d } LINE PICT RCL")
            .unwrap();
        let Obj::Grob(g) = &c.stack[0] else { panic!() };
        assert!((0..131).all(|x| g.get(x, 10)) && !g.get(5, 11));
        // A grob stored in PICT, then drawn on.
        c.exec_line("CLEAR GROB 8 2 FF00 PICT STO PICT RCL")
            .unwrap();
        assert_eq!(c.stack[0].show(c.fmt), "GROB 8 2 FF00");
        c.exec_line("CLEAR PICT { # 4d # 1d } GROB 2 1 30 GOR PICT RCL")
            .unwrap();
        assert_eq!(c.stack[0].show(c.fmt), "GROB 8 2 FF03");
        c.exec_line("CLEAR { # 0d # 1d } PIX? { # 4d # 1d } PIX? { # 3d # 0d } PIXOFF PICT RCL")
            .unwrap();
        assert_eq!(c.stack[..2], [Obj::Real(0.0), Obj::Real(1.0)]);
        assert_eq!(c.stack[2].show(c.fmt), "GROB 8 2 7F03");
        c.exec_line("CLEAR PICT { # 0d # 0d } { # 3d # 1d } SUB PICT SIZE")
            .unwrap();
        assert_eq!(c.stack[0].show(c.fmt), "GROB 4 2 7000");
        assert_eq!(c.stack[1..], [Obj::Bin(8), Obj::Bin(2)]);
        c.exec_line("CLEAR # 140d # 70d PDIM PICT SIZE").unwrap();
        assert_eq!(c.stack, [Obj::Bin(140), Obj::Bin(70)]);
    }

    #[test]
    fn display() {
        let mut c = Calc::new();
        c.exec_line("1 2 LCD→ SIZE").unwrap();
        assert_eq!(c.stack[2..], [Obj::Bin(131), Obj::Bin(64)]);
        c.exec_line("CLEAR GROB 2 4 10101010 →LCD").unwrap();
        assert_eq!(c.disp[0].chars().next(), Some('⡇'));
        assert_eq!(c.disp.len(), 16);
        c.disp.clear();
        assert_eq!(
            run("GROB 1 1 10 GROB 1 1 00 { 2 { # 0d # 0d } .01 1 } ANIMATE"),
            ["GROB 1 1 10", "GROB 1 1 00"]
        );
        assert!(Calc::new().exec_line("GROB 1 1 10 2 ANIMATE").is_err());
    }
}
