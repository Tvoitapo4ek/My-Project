use std::collections::HashMap;

use glam::Vec2;

use crate::Rect;

/// Символы, которые попадут в атлас.
const CHARSET: &[(u32, u32)] = &[(0x20, 0x7F), (0xA0, 0x100), (0x400, 0x460), (0x2010, 0x2027), (0x2190, 0x2194)];

#[derive(Clone, Copy)]
struct Glyph {
    uv: Rect,
    /// Смещение левого верхнего угла от «пера» на базовой линии (в пикселях растра).
    offset: Vec2,
    size: Vec2,
    advance: f32,
}

/// Атлас шрифта: TTF/OTF растеризуется один раз в RGBA-картинку
/// (белый цвет + альфа). Бэкенду остаётся загрузить `pixels` как текстуру.
pub struct FontAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    glyphs: HashMap<char, Glyph>,
    raster: f32,
    ascent: f32,
    /// UV белого пикселя — для заливки прямоугольников той же текстурой.
    pub white: Rect,
}

impl FontAtlas {
    /// `raster` — размер растеризации в пикселях (крупнее — чётче при масштабе).
    pub fn new(font_data: &[u8], raster: f32) -> Result<Self, String> {
        let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())?;
        let ascent = font.horizontal_line_metrics(raster).map_or(raster * 0.8, |m| m.ascent);

        let width = 512u32;
        let mut rows: Vec<u8> = vec![0; (width * 4 * 4) as usize];
        let (mut x, mut y, mut row_h) = (4u32, 0u32, 4u32);
        // Белый блок 2×2 в углу.
        for py in 0..2 {
            for px in 0..2 {
                let i = ((py * width + px) * 4) as usize;
                rows[i..i + 4].copy_from_slice(&[255; 4]);
            }
        }
        let mut placed = vec![];
        for &(a, b) in CHARSET {
            for c in (a..b).filter_map(char::from_u32) {
                if font.lookup_glyph_index(c) == 0 && c != ' ' {
                    continue;
                }
                let (m, bitmap) = font.rasterize(c, raster);
                let (w, h) = (m.width as u32, m.height as u32);
                if x + w + 1 > width {
                    (x, y, row_h) = (0, y + row_h + 1, 0);
                }
                let needed = ((y + h + 1) * width * 4) as usize;
                if rows.len() < needed {
                    rows.resize(needed, 0);
                }
                for gy in 0..h {
                    for gx in 0..w {
                        let i = (((y + gy) * width + x + gx) * 4) as usize;
                        rows[i..i + 4].copy_from_slice(&[255, 255, 255, bitmap[(gy * w + gx) as usize]]);
                    }
                }
                placed.push((c, x, y, m));
                x += w + 1;
                row_h = row_h.max(h);
            }
        }
        let height = (y + row_h + 1).next_power_of_two();
        rows.resize((height * width * 4) as usize, 0);

        let (fw, fh) = (width as f32, height as f32);
        let glyphs = placed
            .into_iter()
            .map(|(c, x, y, m)| {
                let size = Vec2::new(m.width as f32, m.height as f32);
                let glyph = Glyph {
                    uv: Rect::new(x as f32 / fw, y as f32 / fh, size.x / fw, size.y / fh),
                    offset: Vec2::new(m.xmin as f32, -(m.ymin as f32 + size.y)),
                    size,
                    advance: m.advance_width,
                };
                (c, glyph)
            })
            .collect();
        let white = Rect::new(0.5 / fw, 0.5 / fh, 1.0 / fw, 1.0 / fh);
        Ok(FontAtlas { width, height, pixels: rows, glyphs, raster, ascent, white })
    }

    fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyphs.get(&c).or_else(|| self.glyphs.get(&'?'))
    }

    /// Ширина строки при высоте `size`.
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        let k = size / self.raster;
        text.lines().map(|l| l.chars().filter_map(|c| self.glyph(c)).map(|g| g.advance * k).sum()).fold(0.0, f32::max)
    }

    /// Прямоугольники глифов строки: (экранный прямоугольник, UV).
    pub(crate) fn layout<'a>(&'a self, text: &'a str, pos: Vec2, size: f32) -> impl Iterator<Item = (Rect, Rect)> + 'a {
        let k = size / self.raster;
        let mut pen = pos;
        text.chars().filter_map(move |c| {
            if c == '\n' {
                pen = Vec2::new(pos.x, pen.y + size * 1.2);
                return None;
            }
            let g = self.glyph(c)?;
            let p = pen + Vec2::new(0.0, self.ascent * k) + g.offset * k;
            pen.x += g.advance * k;
            (g.size.x > 0.0).then(|| (Rect::new(p.x, p.y, g.size.x * k, g.size.y * k), g.uv))
        })
    }
}
