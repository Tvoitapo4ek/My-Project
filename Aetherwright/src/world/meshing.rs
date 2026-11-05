//! Greedy chunk mesher (pure; runs on worker threads or the main thread).
//!
//! Input is a padded (size+2)³ block array (chunk + 1-block border copied
//! from neighbours), so faces on chunk borders are culled correctly and the
//! mesher never touches the world.
//!
//! For each axis and direction, every slice builds a 2D mask of visible
//! faces keyed by (texture, per-corner ambient occlusion); equal adjacent
//! cells are merged into maximal rectangles. Hidden faces between opaque
//! neighbours are never emitted. Including AO in the key keeps the shading
//! exact while flat open surfaces still merge into large quads.

use std::collections::BTreeMap;

use bevy::math::IVec3;

use super::voxel::{BlockId, RenderKind, TexId, blocks};

#[derive(Default, Debug, Clone)]
pub struct MeshBuffers {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshBuffers {
    pub fn quad_count(&self) -> usize {
        self.positions.len() / 4
    }
}

#[derive(Default, Debug)]
pub struct ChunkMesh {
    /// One sub-mesh per texture/material.
    pub parts: BTreeMap<TexId, MeshBuffers>,
    /// Local grid coordinates of every solid voxel (collider input).
    pub solid_voxels: Vec<IVec3>,
}

impl ChunkMesh {
    pub fn quad_count(&self) -> usize {
        self.parts.values().map(|m| m.quad_count()).sum()
    }
}

const AO_CURVE: [f32; 4] = [0.42, 0.62, 0.8, 1.0];
const WATER_DROP: f32 = 0.12;

#[derive(Clone, Copy, PartialEq, Eq)]
struct FaceKey {
    tex: TexId,
    ao: [u8; 4],
    lowered: bool,
}

pub struct Padded<'a> {
    pub data: &'a [BlockId],
    pub n: i32,
}

impl Padded<'_> {
    #[inline]
    fn get(&self, p: IVec3) -> BlockId {
        // p is in chunk-local coordinates, valid range -1..=n.
        let s = self.n + 2;
        let (x, y, z) = (p.x + 1, p.y + 1, p.z + 1);
        self.data[(x + s * (z + s * y)) as usize]
    }
}

fn face_visible(b: BlockId, neighbour: BlockId) -> bool {
    match b.def().render {
        RenderKind::Cube => !neighbour.is_opaque(),
        RenderKind::Transparent => !neighbour.is_opaque() && neighbour != b,
        RenderKind::Liquid => matches!(
            neighbour.def().render,
            RenderKind::Invisible | RenderKind::Cross
        ),
        RenderKind::Invisible | RenderKind::Cross => false,
    }
}

fn face_tex(b: BlockId, d: usize, side: i32) -> TexId {
    let t = b.def().tex;
    match (d, side) {
        (1, 1) => t[0],
        (1, _) => t[2],
        _ => t[1],
    }
}

fn axis(i: usize) -> IVec3 {
    let mut v = IVec3::ZERO;
    v[i] = 1;
    v
}

pub fn mesh_chunk(input: &Padded) -> ChunkMesh {
    let n = input.n;
    let mut out = ChunkMesh::default();
    let mut mask: Vec<Option<FaceKey>> = vec![None; (n * n) as usize];

    for d in 0..3usize {
        let (u, v) = ((d + 1) % 3, (d + 2) % 3);
        let (eu, ev, ed) = (axis(u), axis(v), axis(d));
        for side in [1i32, -1] {
            for slice in 0..n {
                // Build the mask for this slice.
                for j in 0..n {
                    for i in 0..n {
                        let mut p = IVec3::ZERO;
                        p[d] = slice;
                        p[u] = i;
                        p[v] = j;
                        let b = input.get(p);
                        let q = p + ed * side;
                        let nb = input.get(q);
                        mask[(i + j * n) as usize] = if face_visible(b, nb) {
                            let ao = if b.def().render == RenderKind::Cube {
                                let occ = |o: IVec3| input.get(q + o).is_opaque() as u8;
                                let mut ao = [3u8; 4];
                                for (k, (su, sv)) in
                                    [(-1, -1), (1, -1), (1, 1), (-1, 1)].into_iter().enumerate()
                                {
                                    let s1 = occ(eu * su);
                                    let s2 = occ(ev * sv);
                                    let c = occ(eu * su + ev * sv);
                                    ao[k] = if s1 == 1 && s2 == 1 {
                                        0
                                    } else {
                                        3 - (s1 + s2 + c)
                                    };
                                }
                                ao
                            } else {
                                [3; 4]
                            };
                            let lowered = b.is_liquid()
                                && d == 1
                                && side == 1
                                && !input.get(p + IVec3::Y).is_liquid();
                            Some(FaceKey {
                                tex: face_tex(b, d, side),
                                ao,
                                lowered,
                            })
                        } else {
                            None
                        };
                    }
                }
                // Greedy merge.
                for j in 0..n {
                    let mut i = 0;
                    while i < n {
                        let Some(key) = mask[(i + j * n) as usize] else {
                            i += 1;
                            continue;
                        };
                        let mut w = 1;
                        while i + w < n && mask[(i + w + j * n) as usize] == Some(key) {
                            w += 1;
                        }
                        let mut h = 1;
                        'grow: while j + h < n {
                            for k in 0..w {
                                if mask[(i + k + (j + h) * n) as usize] != Some(key) {
                                    break 'grow;
                                }
                            }
                            h += 1;
                        }
                        for jj in 0..h {
                            for ii in 0..w {
                                mask[(i + ii + (j + jj) * n) as usize] = None;
                            }
                        }
                        emit_quad(&mut out, d, u, v, side, slice, i, j, w, h, key);
                        i += w;
                    }
                }
            }
        }
    }

    // Plants: two crossed quads per block. Solid voxels for the collider.
    for y in 0..n {
        for z in 0..n {
            for x in 0..n {
                let p = IVec3::new(x, y, z);
                let b = input.get(p);
                if b.is_solid() {
                    out.solid_voxels.push(p);
                }
                if b.def().render == RenderKind::Cross {
                    emit_cross(&mut out, p, b);
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    out: &mut ChunkMesh,
    d: usize,
    u: usize,
    v: usize,
    side: i32,
    slice: i32,
    i: i32,
    j: i32,
    w: i32,
    h: i32,
    key: FaceKey,
) {
    let buf = out.parts.entry(key.tex).or_default();
    let mut base = [0.0f32; 3];
    base[d] = (slice + if side > 0 { 1 } else { 0 }) as f32;
    if key.lowered {
        base[d] -= WATER_DROP;
    }
    base[u] = i as f32;
    base[v] = j as f32;
    let corner = |du: i32, dv: i32| {
        let mut c = base;
        c[u] += du as f32;
        c[v] += dv as f32;
        c
    };
    let corners = [corner(0, 0), corner(w, 0), corner(w, h), corner(0, h)];
    let mut normal = [0.0f32; 3];
    normal[d] = side as f32;

    let uv_of = |c: [f32; 3]| -> [f32; 2] {
        match d {
            1 => [c[0], c[2]],
            0 => [c[2], -c[1]],
            _ => [c[0], -c[1]],
        }
    };
    let start = buf.positions.len() as u32;
    for (k, c) in corners.iter().enumerate() {
        buf.positions.push(*c);
        buf.normals.push(normal);
        buf.uvs.push(uv_of(*c));
        let a = AO_CURVE[key.ao[k] as usize];
        buf.colors.push([a, a, a, 1.0]);
    }
    // Flip the diagonal to avoid AO interpolation artifacts.
    let flip = key.ao[0] as u32 + key.ao[2] as u32 > key.ao[1] as u32 + key.ao[3] as u32;
    let tris: [u32; 6] = if !flip {
        [0, 1, 2, 0, 2, 3]
    } else {
        [1, 2, 3, 1, 3, 0]
    };
    for t in tris {
        buf.indices.push(start + t);
    }
    if side < 0 {
        // Reverse winding so the face points along −axis.
        let len = buf.indices.len();
        buf.indices[len - 6..len - 3].reverse();
        buf.indices[len - 3..].reverse();
    }
}

fn emit_cross(out: &mut ChunkMesh, p: IVec3, b: BlockId) {
    let buf = out.parts.entry(b.def().tex[1]).or_default();
    let (x, y, z) = (p.x as f32, p.y as f32, p.z as f32);
    let inset = 0.15;
    let h = if b == blocks::BERRY_BUSH { 0.8 } else { 0.9 };
    let quads = [
        ([x + inset, z + inset], [x + 1.0 - inset, z + 1.0 - inset]),
        ([x + 1.0 - inset, z + inset], [x + inset, z + 1.0 - inset]),
    ];
    for (a, c) in quads {
        let start = buf.positions.len() as u32;
        let verts = [
            [a[0], y, a[1]],
            [c[0], y, c[1]],
            [c[0], y + h, c[1]],
            [a[0], y + h, a[1]],
        ];
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        for (vtx, uv) in verts.iter().zip(uvs) {
            buf.positions.push(*vtx);
            buf.normals.push([0.0, 1.0, 0.0]);
            buf.uvs.push(uv);
            buf.colors.push([1.0, 1.0, 1.0, 1.0]);
        }
        for t in [0, 1, 2, 0, 2, 3] {
            buf.indices.push(start + t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(n: i32, f: impl Fn(IVec3) -> BlockId) -> Vec<BlockId> {
        let s = n + 2;
        let mut v = vec![blocks::AIR; (s * s * s) as usize];
        for y in 0..s {
            for z in 0..s {
                for x in 0..s {
                    v[(x + s * (z + s * y)) as usize] = f(IVec3::new(x - 1, y - 1, z - 1));
                }
            }
        }
        v
    }

    fn tex(name: &str) -> TexId {
        TexId(crate::content::registry().texture_id(name).unwrap())
    }

    #[test]
    fn single_block_has_six_faces() {
        let data = padded(4, |p| {
            if p == IVec3::new(1, 1, 1) {
                blocks::STONE
            } else {
                blocks::AIR
            }
        });
        let m = mesh_chunk(&Padded { data: &data, n: 4 });
        assert_eq!(m.quad_count(), 6);
        assert_eq!(m.solid_voxels, vec![IVec3::new(1, 1, 1)]);
    }

    #[test]
    fn hidden_faces_between_neighbours_are_culled() {
        // Two adjacent blocks: 10 visible faces, not 12.
        let data = padded(4, |p| {
            if p == IVec3::new(1, 1, 1) || p == IVec3::new(2, 1, 1) {
                blocks::STONE
            } else {
                blocks::AIR
            }
        });
        let m = mesh_chunk(&Padded { data: &data, n: 4 });
        // Greedy merges the 2×1 sides into single quads: 6 quads total.
        assert_eq!(m.quad_count(), 6);
    }

    #[test]
    fn full_solid_chunk_surrounded_by_solid_has_no_faces() {
        let data = padded(8, |_| blocks::STONE);
        let m = mesh_chunk(&Padded { data: &data, n: 8 });
        assert_eq!(m.quad_count(), 0, "no faces inside solid terrain");
        assert_eq!(m.solid_voxels.len(), 512);
    }

    #[test]
    fn flat_floor_merges_into_few_quads() {
        // y=0 layer of stone, air above, stone below (padding).
        let data = padded(8, |p| if p.y <= 0 { blocks::STONE } else { blocks::AIR });
        let m = mesh_chunk(&Padded { data: &data, n: 8 });
        // Only the top face is visible (sides touch padded stone neighbours
        // at y=0 and air for x/z out of range is stone? no: padding outside
        // x/z is stone only for y<=0) → one 8×8 quad.
        assert_eq!(m.quad_count(), 1);
    }

    #[test]
    fn border_face_culled_by_neighbour_chunk_data() {
        // Block at x = n-1 with a solid neighbour in the padding (next chunk).
        let n = 4;
        let data = padded(n, |p| {
            if p == IVec3::new(n - 1, 1, 1) || p == IVec3::new(n, 1, 1) {
                blocks::STONE
            } else {
                blocks::AIR
            }
        });
        let m = mesh_chunk(&Padded { data: &data, n });
        assert_eq!(
            m.quad_count(),
            5,
            "+X face hidden by the neighbouring chunk"
        );
    }

    #[test]
    fn water_surface_only_against_air() {
        let data = padded(4, |p| if p.y <= 1 { blocks::WATER } else { blocks::AIR });
        let m = mesh_chunk(&Padded { data: &data, n: 4 });
        assert_eq!(m.quad_count(), 1);
        let water = &m.parts[&tex("water")];
        assert!((water.positions[0][1] - (2.0 - WATER_DROP)).abs() < 1e-5);
        assert!(m.solid_voxels.is_empty());
    }

    #[test]
    fn quads_face_outwards() {
        let data = padded(4, |p| {
            if p == IVec3::new(1, 1, 1) {
                blocks::STONE
            } else {
                blocks::AIR
            }
        });
        let m = mesh_chunk(&Padded { data: &data, n: 4 });
        let b = &m.parts[&tex("stone")];
        for tri in b.indices.chunks(3) {
            let p = |i: u32| bevy::math::Vec3::from(b.positions[i as usize]);
            let (a, bb, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
            let geometric = (bb - a).cross(c - a).normalize();
            let declared = bevy::math::Vec3::from(b.normals[tri[0] as usize]);
            assert!(
                geometric.dot(declared) > 0.99,
                "CCW winding must match the normal"
            );
        }
    }
}
