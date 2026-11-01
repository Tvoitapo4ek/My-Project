//! Меш чанка: видимые грани блоков + мягкие тени в углах (ambient occlusion).
//!
//! Вершины в **локальных координатах сетки**, поэтому одна матрица на сетку
//! двигает все её чанки — так рисуются и мир, и корабли.
//! Позже сюда можно добавить жадный мешинг — интерфейс `mesh_chunk` останется.

use bytemuck::{Pod, Zeroable};
use glam::IVec3;
use vx_core::{AIR, CHUNK, Face, Grid, Registry};

/// Вершина: позиция, UV, слой текстуры, освещённость (0..1).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub layer: u32,
    pub light: f32,
}

#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// (нормаль, ось U, ось V) для каждой грани; U × V = нормаль,
/// поэтому вершины идут против часовой стрелки, если смотреть снаружи.
fn axes(face: Face) -> (IVec3, IVec3, IVec3) {
    use IVec3 as V;
    match face {
        Face::PosX => (V::X, V::NEG_Z, V::Y),
        Face::NegX => (V::NEG_X, V::Z, V::Y),
        Face::PosY => (V::Y, V::X, V::NEG_Z),
        Face::NegY => (V::NEG_Y, V::X, V::Z),
        Face::PosZ => (V::Z, V::X, V::Y),
        Face::NegZ => (V::NEG_Z, V::NEG_X, V::Y),
    }
}

/// Направленное затенение граней (как в Minecraft).
fn face_shade(face: Face) -> f32 {
    match face {
        Face::PosY => 1.0,
        Face::NegY => 0.5,
        Face::PosX | Face::NegX => 0.8,
        Face::PosZ | Face::NegZ => 0.65,
    }
}

const AO: [f32; 4] = [0.45, 0.65, 0.82, 1.0];

/// Построить меш чанка `cpos` сетки `grid`.
pub fn mesh_chunk(grid: &Grid, cpos: IVec3, reg: &Registry) -> Mesh {
    let mut mesh = Mesh::default();
    let Some(chunk) = grid.chunk(cpos) else { return mesh };
    let origin = cpos * CHUNK;
    let opaque = |p: IVec3| reg.opaque(grid.get(p));

    for (local, id) in chunk.iter() {
        let pos = origin + local;
        let def = reg.get(id);
        for face in Face::ALL {
            let (n, u, v) = axes(face);
            let neighbour = grid.get(pos + n);
            // Грань не видна за непрозрачным блоком или за таким же прозрачным (стекло к стеклу).
            if neighbour != AIR && (reg.opaque(neighbour) || neighbour == id) {
                continue;
            }
            // Угол грани: сдвиги по нормали/U/V, лежащие в положительной части куба.
            let base = n.max(IVec3::ZERO) + (-u).max(IVec3::ZERO) + (-v).max(IVec3::ZERO);
            let outer = pos + n;
            let mut ao = [0usize; 4];
            let start = mesh.vertices.len() as u32;
            for (k, (i, j)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
                let du = if i == 0 { -u } else { u };
                let dv = if j == 0 { -v } else { v };
                let (s1, s2, c) = (opaque(outer + du), opaque(outer + dv), opaque(outer + du + dv));
                ao[k] = if s1 && s2 { 0 } else { 3 - (s1 as usize + s2 as usize + c as usize) };
                let corner = pos + base + u * i + v * j;
                mesh.vertices.push(Vertex {
                    pos: corner.as_vec3().to_array(),
                    uv: [i as f32, 1.0 - j as f32],
                    layer: def.faces[face as usize],
                    light: face_shade(face) * AO[ao[k]],
                });
            }
            // Диагональ квада выбираем так, чтобы AO интерполировался без «бабочки».
            let quad = if ao[0] + ao[2] >= ao[1] + ao[3] { [0, 1, 2, 0, 2, 3] } else { [1, 2, 3, 1, 3, 0] };
            mesh.indices.extend(quad.map(|q| start + q));
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use vx_core::GridKind;

    #[test]
    fn hidden_faces_are_culled() {
        let reg = Registry::from_toml("[[block]]\nname='stone'").unwrap();
        let mut g = Grid::new(GridKind::Static);
        let stone = reg.id("stone").unwrap();
        g.set(IVec3::ZERO, stone);
        assert_eq!(mesh_chunk(&g, IVec3::ZERO, &reg).indices.len(), 6 * 6);
        g.set(IVec3::X, stone); // два блока: 10 видимых граней
        assert_eq!(mesh_chunk(&g, IVec3::ZERO, &reg).indices.len(), 10 * 6);
    }

    #[test]
    fn winding_is_ccw_from_outside() {
        let reg = Registry::from_toml("[[block]]\nname='stone'").unwrap();
        let mut g = Grid::new(GridKind::Static);
        g.set(IVec3::ZERO, 1);
        let m = mesh_chunk(&g, IVec3::ZERO, &reg);
        for tri in m.indices.chunks_exact(3) {
            let p = [tri[0], tri[1], tri[2]].map(|i| glam::Vec3::from(m.vertices[i as usize].pos));
            let normal = (p[1] - p[0]).cross(p[2] - p[0]);
            let centre = (p[0] + p[1] + p[2]) / 3.0 - glam::Vec3::splat(0.5);
            assert!(normal.dot(centre) > 0.0, "треугольник смотрит внутрь");
        }
    }
}
