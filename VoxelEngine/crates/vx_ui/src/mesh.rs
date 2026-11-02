use bytemuck::{Pod, Zeroable};
use glam::Vec2;

use crate::{DrawList, FontAtlas, Rect, Shape, Texture};

/// Вершина интерфейса для GPU. `layer < 0` — атлас интерфейса,
/// иначе слой текстурного массива блоков. Позиция — в физических пикселях.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub layer: i32,
}

#[derive(Default)]
pub struct UiMesh {
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
}

impl UiMesh {
    fn quad(&mut self, points: [Vec2; 4], uv: Rect, layer: i32, color: [f32; 4], scale: f32) {
        let start = self.vertices.len() as u32;
        let uvs = [uv.min(), Vec2::new(uv.x + uv.w, uv.y), uv.max(), Vec2::new(uv.x, uv.y + uv.h)];
        for (p, t) in points.into_iter().zip(uvs) {
            self.vertices.push(UiVertex { pos: (p * scale).to_array(), uv: t.to_array(), color, layer });
        }
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| start + i));
    }
}

fn corners(r: Rect) -> [Vec2; 4] {
    let (a, b) = (r.min(), r.max());
    [a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)]
}

/// Превратить список примитивов в треугольники (для GPU-бэкендов).
/// `scale` — физических пикселей на логический.
pub fn tessellate(list: &DrawList, font: &FontAtlas, scale: f32) -> UiMesh {
    let mut mesh = UiMesh::default();
    for shape in &list.shapes {
        match shape {
            Shape::Rect { rect, color } => mesh.quad(corners(*rect), font.white, -1, *color, scale),
            Shape::Quad { points, uv, texture, color } => {
                let (uv, layer) = match texture {
                    Texture::Atlas => (font.white, -1),
                    Texture::Block(l) => (*uv, *l as i32),
                };
                mesh.quad(*points, uv, layer, *color, scale);
            }
            Shape::Text { pos, size, color, text } => {
                for (r, uv) in font.layout(text, *pos, *size) {
                    mesh.quad(corners(r), uv, -1, *color, scale);
                }
            }
        }
    }
    mesh
}
