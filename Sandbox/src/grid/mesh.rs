//! Chunk mesher and collider builder — pure functions (unit-tested).
//!
//! * Faces of cube blocks are emitted where the neighbour does not hide
//!   them, with per-vertex ambient occlusion (the soft voxel look) and atlas
//!   UVs.
//! * Collision: full cubes are merged into boxes (greedy), other shapes
//!   are added one by one.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use super::{CHUNK, Cell, VoxelGrid, cell_boxes};
use crate::content::{Collision, Shape, registry};

/// Hides the faces of its neighbours.
fn opaque(c: Cell) -> bool {
    let d = c.id.def();
    !c.is_air() && d.is_cube() && d.shape == Shape::Cube && !d.transparent
}

/// Polygons (3–4 points, block space, centred) of a shaped block.
fn shape_polys(shape: Shape) -> Vec<Vec<Vec3>> {
    let quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| vec![a, b, c, d];
    match shape {
        Shape::Cube => vec![],
        Shape::Slab => {
            let (lo, hi) = (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.0, 0.5));
            let p = |x: f32, y: f32, z: f32| Vec3::new(if x > 0.0 { hi.x } else { lo.x }, if y > 0.0 { hi.y } else { lo.y }, if z > 0.0 { hi.z } else { lo.z });
            vec![
                quad(p(-1., -1., -1.), p(1., -1., -1.), p(1., -1., 1.), p(-1., -1., 1.)),
                quad(p(-1., 1., -1.), p(1., 1., -1.), p(1., 1., 1.), p(-1., 1., 1.)),
                quad(p(-1., -1., -1.), p(1., -1., -1.), p(1., 1., -1.), p(-1., 1., -1.)),
                quad(p(-1., -1., 1.), p(1., -1., 1.), p(1., 1., 1.), p(-1., 1., 1.)),
                quad(p(-1., -1., -1.), p(-1., -1., 1.), p(-1., 1., 1.), p(-1., 1., -1.)),
                quad(p(1., -1., -1.), p(1., -1., 1.), p(1., 1., 1.), p(1., 1., -1.)),
            ]
        }
        Shape::Wedge(y0, y1) => {
            let [b0, b1, b2, b3, t0, t1, t2, t3] = super::wedge_points(y0, y1);
            let mut v = vec![quad(b0, b1, b2, b3), quad(t0, t1, t2, t3), quad(b0, b1, t1, t0), quad(b3, b0, t0, t3), quad(b1, b2, t2, t1)];
            if y0 > 0.001 {
                v.push(quad(b2, b3, t3, t2));
            }
            v
        }
    }
}

/// Emits the faces of a shaped block (slab / wedge).
fn mesh_shape(out: &mut MeshData, grid: &VoxelGrid, c: IVec3, cell: Cell, uv: UvFn) {
    let def = cell.id.def();
    let Some((top, side, bottom)) = &def.textures else { return };
    let reg = registry();
    let r = crate::models::block_rot(def.placement, cell.facing);
    let center = c.as_vec3() + Vec3::splat(0.5);
    for poly in shape_polys(def.shape) {
        let pts: Vec<Vec3> = poly.iter().map(|p| r * *p).collect();
        let mut n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        if n.length_squared() < 1e-8 {
            n = (pts[2] - pts[0]).cross(pts[3] - pts[0]);
        }
        let n = n.normalize_or_zero();
        let mid = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        // Outward (the shapes are convex and contain the block centre side).
        let inside_y = match def.shape {
            Shape::Wedge(y0, y1) => -0.5 + (y0 + y1) * 0.25,
            _ => -0.25,
        };
        let centroid = r * Vec3::new(0.0, inside_y, 0.0);
        let flip = n.dot(mid - centroid) < 0.0;
        let n = if flip { -n } else { n };
        // A full face on the cell boundary is hidden by an opaque neighbour.
        let axis = n.round().as_ivec3();
        if axis.abs().element_sum() == 1 && n.abs().max_element() > 0.999 && pts.iter().all(|p| (p.dot(n) - 0.5).abs() < 1e-3) && poly_area(&pts) > 0.999 && opaque(grid.get(c + axis)) {
            continue;
        }
        let tex = if n.y > 0.5 { top } else if n.y < -0.5 { bottom } else { side };
        let [u0, v0, u1, v1] = uv(reg.texture_index(tex));
        let start = out.positions.len() as u32;
        for p in &pts {
            let (s, t) = if n.y.abs() >= n.x.abs().max(n.z.abs()) { (p.x, p.z) } else if n.x.abs() >= n.z.abs() { (p.z, p.y) } else { (p.x, p.y) };
            let (s, t) = ((s + 0.5).clamp(0.0, 1.0), (t + 0.5).clamp(0.0, 1.0));
            out.positions.push((center + *p).to_array());
            out.normals.push(n.to_array());
            out.uvs.push([u0 + (u1 - u0) * s, v1 - (v1 - v0) * t]);
            out.colors.push([1.0; 4]);
        }
        let order: [u32; 6] = if flip { [0, 2, 1, 0, 3, 2] } else { [0, 1, 2, 0, 2, 3] };
        out.indices.extend(order.iter().map(|i| start + i));
    }
}

fn poly_area(p: &[Vec3]) -> f32 {
    ((p[1] - p[0]).cross(p[2] - p[0]).length() + (p[2] - p[0]).cross(p[3] - p[0]).length()) * 0.5
}

fn face_visible(this: Cell, other: Cell) -> bool {
    if opaque(other) {
        return false;
    }
    // Glass next to glass: no inner faces.
    !(this.id.def().transparent && other.id == this.id)
}

/// (normal, u axis, v axis) per face; vertices go counter-clockwise.
const FACES: [(IVec3, IVec3, IVec3); 6] = [
    (IVec3::X, IVec3::NEG_Z, IVec3::Y),
    (IVec3::NEG_X, IVec3::Z, IVec3::Y),
    (IVec3::Y, IVec3::X, IVec3::NEG_Z),
    (IVec3::NEG_Y, IVec3::X, IVec3::Z),
    (IVec3::Z, IVec3::X, IVec3::Y),
    (IVec3::NEG_Z, IVec3::NEG_X, IVec3::Y),
];

/// Atlas lookup: texture index → (u0, v0, u1, v1).
pub type UvFn<'a> = &'a dyn Fn(usize) -> [f32; 4];

#[derive(Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn into_mesh(self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        m.insert_indices(Indices::U32(self.indices));
        m
    }
}

const AO: [f32; 4] = [0.5, 0.68, 0.84, 1.0];

/// Mesh of one chunk in grid-local coordinates.
pub fn mesh_chunk(grid: &VoxelGrid, key: IVec3, uv: UvFn) -> MeshData {
    let mut out = MeshData::default();
    let Some(chunk) = grid.chunks.get(&key) else { return out };
    if chunk.count == 0 {
        return out;
    }
    let reg = registry();
    let origin = key * CHUNK;
    for y in 0..CHUNK {
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                let c = origin + IVec3::new(x, y, z);
                let cell = grid.get(c);
                let def = cell.id.def();
                if cell.is_air() || !def.is_cube() {
                    continue;
                }
                if def.shape != Shape::Cube {
                    mesh_shape(&mut out, grid, c, cell, uv);
                    continue;
                }
                let Some((top, side, bottom)) = &def.textures else { continue };
                for (n, u, v) in FACES {
                    if !face_visible(cell, grid.get(c + n)) {
                        continue;
                    }
                    let tex = match n.y {
                        1 => top,
                        -1 => bottom,
                        _ => side,
                    };
                    let [u0, v0, u1, v1] = uv(reg.texture_index(tex));
                    let center = c.as_vec3() + Vec3::splat(0.5) + n.as_vec3() * 0.5;
                    let start = out.positions.len() as u32;
                    let mut ao = [0usize; 4];
                    for (k, (su, sv)) in [(-1, -1), (1, -1), (1, 1), (-1, 1)].into_iter().enumerate() {
                        let p = center + (u * su).as_vec3() * 0.5 + (v * sv).as_vec3() * 0.5;
                        out.positions.push(p.to_array());
                        out.normals.push(n.as_vec3().to_array());
                        let (fu, fv) = ((su + 1) / 2, (sv + 1) / 2);
                        out.uvs.push([if fu == 0 { u0 } else { u1 }, if fv == 0 { v1 } else { v0 }]);
                        // Ambient occlusion from the three cells around the corner.
                        let outside = c + n;
                        let s1 = opaque(grid.get(outside + u * su)) as usize;
                        let s2 = opaque(grid.get(outside + v * sv)) as usize;
                        let cr = opaque(grid.get(outside + u * su + v * sv)) as usize;
                        let level = if s1 == 1 && s2 == 1 { 0 } else { 3 - (s1 + s2 + cr) };
                        ao[k] = level;
                        let a = AO[level];
                        out.colors.push([a, a, a, 1.0]);
                    }
                    // Flip the quad diagonal so AO interpolates evenly.
                    if ao[0] + ao[2] > ao[1] + ao[3] {
                        out.indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
                    } else {
                        out.indices.extend_from_slice(&[start + 1, start + 2, start + 3, start + 1, start + 3, start]);
                    }
                }
            }
        }
    }
    out
}

/// Total mass of the blocks in a chunk (kg).
pub fn chunk_mass(grid: &VoxelGrid, key: IVec3) -> f32 {
    let Some(chunk) = grid.chunks.get(&key) else { return 0.0 };
    chunk.cells.iter().filter(|c| !c.is_air()).map(|c| c.id.def().mass).sum()
}

/// Convex collision hulls of the wedges in a chunk, with their volumes.
pub fn chunk_hulls(grid: &VoxelGrid, key: IVec3) -> Vec<(Vec<Vec3>, f32)> {
    let Some(chunk) = grid.chunks.get(&key) else { return vec![] };
    if chunk.count == 0 {
        return vec![];
    }
    let origin = key * CHUNK;
    let mut out = Vec::new();
    for y in 0..CHUNK {
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                let c = origin + IVec3::new(x, y, z);
                if let Some(h) = super::cell_hull(c, grid.get(c)) {
                    out.push(h);
                }
            }
        }
    }
    out
}

/// Collision boxes of a chunk: (centre, half extents) in grid-local space.
pub fn chunk_boxes(grid: &VoxelGrid, key: IVec3) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    let Some(chunk) = grid.chunks.get(&key) else { return out };
    if chunk.count == 0 {
        return out;
    }
    let origin = key * CHUNK;
    let n = CHUNK as usize;
    let idx = |x: usize, y: usize, z: usize| x + z * n + y * n * n;
    let mut full = vec![false; n * n * n];
    for y in 0..n {
        for z in 0..n {
            for x in 0..n {
                let c = origin + IVec3::new(x as i32, y as i32, z as i32);
                let cell = grid.get(c);
                if cell.is_air() {
                    continue;
                }
                if cell.id.def().collision == Collision::Full && cell.id.def().shape == Shape::Cube {
                    full[idx(x, y, z)] = true;
                } else if let Some(b) = cell_boxes(c, cell) {
                    out.push(b);
                }
            }
        }
    }
    // Greedy merge: grow along x, then z, then y.
    let mut used = vec![false; n * n * n];
    for y in 0..n {
        for z in 0..n {
            for x in 0..n {
                if !full[idx(x, y, z)] || used[idx(x, y, z)] {
                    continue;
                }
                let mut w = 1;
                while x + w < n && full[idx(x + w, y, z)] && !used[idx(x + w, y, z)] {
                    w += 1;
                }
                let row_ok = |zz: usize, yy: usize| (x..x + w).all(|xx| full[idx(xx, yy, zz)] && !used[idx(xx, yy, zz)]);
                let mut d = 1;
                while z + d < n && row_ok(z + d, y) {
                    d += 1;
                }
                let mut h = 1;
                while y + h < n && (z..z + d).all(|zz| row_ok(zz, y + h)) {
                    h += 1;
                }
                for yy in y..y + h {
                    for zz in z..z + d {
                        for xx in x..x + w {
                            used[idx(xx, yy, zz)] = true;
                        }
                    }
                }
                let size = Vec3::new(w as f32, h as f32, d as f32);
                let min = (origin + IVec3::new(x as i32, y as i32, z as i32)).as_vec3();
                out.push((min + size * 0.5, size * 0.5));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::BlockId, grid::Cell};

    fn uv(_: usize) -> [f32; 4] {
        [0.0, 0.0, 1.0, 1.0]
    }

    #[test]
    fn hidden_faces_are_culled() {
        let mut g = VoxelGrid::default();
        let b = BlockId::k("white_block");
        g.set(IVec3::ZERO, Cell::new(b, 2));
        assert_eq!(mesh_chunk(&g, IVec3::ZERO, &uv).indices.len(), 6 * 6);
        g.set(IVec3::X, Cell::new(b, 2));
        // Two cubes side by side: 10 faces.
        assert_eq!(mesh_chunk(&g, IVec3::ZERO, &uv).indices.len(), 10 * 6);
        // Across a chunk border too.
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(15, 0, 0), Cell::new(b, 2));
        g.set(IVec3::new(16, 0, 0), Cell::new(b, 2));
        assert_eq!(mesh_chunk(&g, IVec3::ZERO, &uv).indices.len(), 5 * 6);
    }

    #[test]
    fn shapes_mesh_and_collide() {
        let mut g = VoxelGrid::default();
        // Slab placed on the floor (facing −Y): lower half.
        g.set(IVec3::ZERO, Cell::new(BlockId::k("steel_slab"), 3));
        let b = crate::grid::cell_boxes(IVec3::ZERO, g.get(IVec3::ZERO)).unwrap();
        assert_eq!(chunk_boxes(&g, IVec3::ZERO), vec![b], "a slab is not a full cube");
        assert!((b.0 - Vec3::new(0.5, 0.25, 0.5)).length() < 1e-5 && (b.1 - Vec3::new(0.5, 0.25, 0.5)).length() < 1e-5);
        assert_eq!(mesh_chunk(&g, IVec3::ZERO, &uv).indices.len(), 6 * 6);
        // A 45° wedge: 5 faces, hull of the right volume, its top faces up.
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Cell::new(BlockId::k("steel_slope"), 4));
        let m = mesh_chunk(&g, IVec3::ZERO, &uv);
        assert_eq!(m.indices.len(), 5 * 6);
        let (pts, vol) = crate::grid::cell_hull(IVec3::ZERO, g.get(IVec3::ZERO)).unwrap();
        assert_eq!(pts.len(), 8);
        assert!(chunk_boxes(&g, IVec3::ZERO).is_empty(), "a wedge collides by its hull only");
        assert!((vol - 0.5).abs() < 1e-5);
        assert!(m.normals.iter().any(|n| n[1] > 0.6 && n[1] < 0.8), "sloped top");
        // Every face points outwards.
        for tri in m.indices.chunks(3) {
            let p = |i: u32| Vec3::from(m.positions[i as usize]);
            let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
            let n = (b - a).cross(c - a);
            let mid = (a + b + c) / 3.0;
            assert!(n.dot(mid - Vec3::new(0.5, 0.3, 0.5)) > -1e-4, "inward face");
        }
    }

    #[test]
    fn floor_merges_into_one_box() {
        let mut g = VoxelGrid::default();
        let b = BlockId::k("white_block");
        for x in 0..16 {
            for z in 0..16 {
                g.set(IVec3::new(x, 0, z), Cell::new(b, 2));
            }
        }
        let boxes = chunk_boxes(&g, IVec3::ZERO);
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].1, Vec3::new(8.0, 0.5, 8.0));
        // A shaft adds its own thin box.
        g.set(IVec3::new(0, 1, 0), Cell::new(BlockId::k("shaft"), 0));
        let boxes = chunk_boxes(&g, IVec3::ZERO);
        assert_eq!(boxes.len(), 2);
        assert!((boxes[0].1 - Vec3::new(0.5, 0.1875, 0.1875)).length() < 1e-5);
    }
}
