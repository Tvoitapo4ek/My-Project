//! Voxel grids. The world is one grid; every physical contraption is
//! another grid with its own transform. The same storage, mesher, colliders,
//! raycast and mechanisms work in both — so machines behave the same in
//! block mode and in physics mode.
//!
//! * `VoxelGrid` (component) — chunked block storage in grid-local cells.
//!   Cell `c` occupies `c .. c + 1` in grid-local space.
//! * Cube blocks are drawn and collided per chunk (`render`); mechanisms
//!   are block entities (children of the grid entity) with models.

pub mod mesh;
pub mod render;

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::content::{BlockId, Collision, Mech, Shape};

pub const CHUNK: i32 = 16;
const VOLUME: usize = (CHUNK * CHUNK * CHUNK) as usize;

/// A block in a cell: type + orientation (`facing` 0..5 = +X −X +Y −Y +Z −Z).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Cell {
    pub id: BlockId,
    pub facing: u8,
}

impl Cell {
    pub const AIR: Cell = Cell { id: BlockId::AIR, facing: 2 };
    pub fn new(id: BlockId, facing: u8) -> Self {
        Self { id, facing }
    }
    pub fn is_air(&self) -> bool {
        self.id.is_air()
    }
}

pub const DIRS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

pub fn facing_dir(f: u8) -> IVec3 {
    DIRS[(f as usize).min(5)]
}

pub fn facing_of(d: IVec3) -> u8 {
    DIRS.iter().position(|x| *x == d).unwrap_or(2) as u8
}

/// Rotation that turns block-local +Y into the facing direction.
pub fn facing_rot(f: u8) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, facing_dir(f).as_vec3())
}

/// Facing after a 90° turn about `axis` (a unit grid direction).
pub fn rotate_facing(f: u8, axis: IVec3) -> u8 {
    let d = facing_dir(f);
    let r = Quat::from_axis_angle(axis.as_vec3(), std::f32::consts::FRAC_PI_2) * d.as_vec3();
    facing_of(r.round().as_ivec3())
}

#[derive(Clone)]
pub struct Chunk {
    pub cells: Box<[Cell; VOLUME]>,
    pub count: u32,
}

impl Default for Chunk {
    fn default() -> Self {
        Self { cells: Box::new([Cell::AIR; VOLUME]), count: 0 }
    }
}

fn split(c: IVec3) -> (IVec3, usize) {
    let k = c.div_euclid(IVec3::splat(CHUNK));
    let l = c.rem_euclid(IVec3::splat(CHUNK));
    (k, (l.x + l.z * CHUNK + l.y * CHUNK * CHUNK) as usize)
}

#[derive(Component, Default, Clone)]
pub struct VoxelGrid {
    pub chunks: HashMap<IVec3, Chunk>,
    /// Chunks whose mesh / collider must be rebuilt.
    pub dirty: HashSet<IVec3>,
    /// Block entities (mechanisms) by cell.
    pub entities: HashMap<IVec3, Entity>,
    /// Cells changed since the block-entity sync last ran.
    pub changed: Vec<IVec3>,
    /// Mechanism cells by kind (kept by `set`): systems visit only their
    /// own blocks instead of every block entity of the grid.
    pub by_mech: HashMap<Mech, HashSet<IVec3>>,
}

impl VoxelGrid {
    pub fn get(&self, c: IVec3) -> Cell {
        let (k, i) = split(c);
        self.chunks.get(&k).map(|ch| ch.cells[i]).unwrap_or(Cell::AIR)
    }

    /// Sets a cell; returns the previous one. Marks chunks (and edge
    /// neighbours) dirty.
    pub fn set(&mut self, c: IVec3, cell: Cell) -> Cell {
        let (k, i) = split(c);
        let ch = self.chunks.entry(k).or_default();
        let old = ch.cells[i];
        if old == cell {
            return old;
        }
        ch.cells[i] = cell;
        self.changed.push(c);
        if let Some(m) = (!old.is_air()).then(|| old.id.def().mech).flatten()
            && let Some(set) = self.by_mech.get_mut(&m)
        {
            set.remove(&c);
        }
        if let Some(m) = (!cell.is_air()).then(|| cell.id.def().mech).flatten() {
            self.by_mech.entry(m).or_default().insert(c);
        }
        match (old.is_air(), cell.is_air()) {
            (true, false) => ch.count += 1,
            (false, true) => ch.count -= 1,
            _ => {}
        }
        self.dirty.insert(k);
        let l = c.rem_euclid(IVec3::splat(CHUNK));
        for d in DIRS {
            let n = l + d;
            if n.min_element() < 0 || n.max_element() >= CHUNK {
                self.dirty.insert(k + d);
            }
        }
        old
    }

    /// Cells holding any of these mechanisms.
    pub fn cells_of<'a>(&'a self, kinds: &'a [Mech]) -> impl Iterator<Item = IVec3> + 'a {
        kinds.iter().filter_map(|m| self.by_mech.get(m)).flatten().copied()
    }

    /// Block entities of these mechanisms: (cell, entity).
    pub fn entities_of<'a>(&'a self, kinds: &'a [Mech]) -> impl Iterator<Item = (IVec3, Entity)> + 'a {
        self.cells_of(kinds).filter_map(|c| self.entities.get(&c).map(|e| (c, *e)))
    }

    /// Every non-air cell.
    pub fn cells(&self) -> Vec<(IVec3, Cell)> {
        let mut out = Vec::new();
        for (k, ch) in &self.chunks {
            if ch.count == 0 {
                continue;
            }
            for (i, cell) in ch.cells.iter().enumerate() {
                if !cell.is_air() {
                    let i = i as i32;
                    let l = IVec3::new(i % CHUNK, i / (CHUNK * CHUNK), (i / CHUNK) % CHUNK);
                    out.push((*k * CHUNK + l, *cell));
                }
            }
        }
        out
    }

    /// First non-air cell along a ray in grid-local space (voxel DDA).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
        let dir = dir.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }
        let mut cell = origin.floor().as_ivec3();
        let step = dir.signum().as_ivec3();
        let inv = Vec3::new(1.0 / dir.x.abs().max(1e-9), 1.0 / dir.y.abs().max(1e-9), 1.0 / dir.z.abs().max(1e-9));
        let mut t_max = Vec3::ZERO;
        for a in 0..3 {
            let o = origin[a];
            let c = cell[a] as f32;
            t_max[a] = if dir[a] > 0.0 { (c + 1.0 - o) * inv[a] } else { (o - c) * inv[a] };
        }
        let mut normal = IVec3::ZERO;
        let mut t = 0.0;
        while t <= max {
            if !self.get(cell).is_air() {
                return Some(RayHit { cell, normal, distance: t });
            }
            let a = if t_max.x < t_max.y { if t_max.x < t_max.z { 0 } else { 2 } } else if t_max.y < t_max.z { 1 } else { 2 };
            t = t_max[a];
            t_max[a] += inv[a];
            cell[a] += step[a];
            normal = IVec3::ZERO;
            normal[a] = -step[a];
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub cell: IVec3,
    /// Face normal of the hit (zero when the ray starts inside a block).
    pub normal: IVec3,
    pub distance: f32,
}

/// Collision boxes of a cell in grid-local space: (centre, half extents).
/// Wedges are convex hulls (`cell_hull`).
pub fn cell_boxes(c: IVec3, cell: Cell) -> Option<(Vec3, Vec3)> {
    let center = c.as_vec3() + Vec3::splat(0.5);
    let def = cell.id.def();
    match def.shape {
        Shape::Slab => {
            let r = crate::models::block_rot(def.placement, cell.facing);
            return Some((center + r * Vec3::new(0.0, 0.25, 0.0), (r * Vec3::new(0.5, 0.25, 0.5)).abs()));
        }
        Shape::Wedge(..) | Shape::Stairs | Shape::CornerOuter | Shape::CornerInner => return None,
        Shape::Cube => {}
    }
    match def.collision {
        Collision::None => None,
        Collision::Full => Some((center, Vec3::splat(0.5))),
        Collision::Box(x, y, z) => {
            let half = (facing_rot(cell.facing) * Vec3::new(x, y, z) * 0.5).abs();
            Some((center, half))
        }
    }
}

/// Convex collision hulls of a shaped cell in grid-local space (corner
/// points) with their volumes (m³). Cubes and slabs collide by boxes.
pub fn cell_hulls(c: IVec3, cell: Cell) -> Vec<(Vec<Vec3>, f32)> {
    let def = cell.id.def();
    if matches!(def.shape, Shape::Cube | Shape::Slab) || cell.is_air() {
        return Vec::new();
    }
    let r = crate::models::block_rot(def.placement, cell.facing);
    let center = c.as_vec3() + Vec3::splat(0.5);
    shape_pieces(def.shape).into_iter().map(|p| (p.points.iter().map(|q| center + r * *q).collect(), p.volume)).collect()
}

/// A convex part of a shaped block (block space, centred): its faces,
/// corner points, centre and volume.
#[derive(Debug, Clone)]
pub struct Piece {
    pub polys: Vec<Vec<Vec3>>,
    pub points: Vec<Vec3>,
    pub centroid: Vec3,
    pub volume: f32,
}

fn box_piece(lo: Vec3, hi: Vec3) -> Piece {
    let p = |x: bool, y: bool, z: bool| Vec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z });
    let q = |a, b, c, d| vec![a, b, c, d];
    let (f, t) = (false, true);
    let polys = vec![
        q(p(f, f, f), p(t, f, f), p(t, f, t), p(f, f, t)),
        q(p(f, t, f), p(t, t, f), p(t, t, t), p(f, t, t)),
        q(p(f, f, f), p(t, f, f), p(t, t, f), p(f, t, f)),
        q(p(f, f, t), p(t, f, t), p(t, t, t), p(f, t, t)),
        q(p(f, f, f), p(f, f, t), p(f, t, t), p(f, t, f)),
        q(p(t, f, f), p(t, f, t), p(t, t, t), p(t, t, f)),
    ];
    let s = hi - lo;
    let mut points = Vec::new();
    for x in [f, t] {
        for y in [f, t] {
            for z in [f, t] {
                points.push(p(x, y, z));
            }
        }
    }
    Piece { polys, points, centroid: (lo + hi) * 0.5, volume: s.x * s.y * s.z }
}

fn wedge_piece(y0: f32, y1: f32, turn: Quat) -> Piece {
    let [b0, b1, b2, b3, t0, t1, t2, t3] = wedge_points(y0, y1).map(|p| turn * p);
    let q = |a, b, c, d| vec![a, b, c, d];
    let mut polys = vec![q(b0, b1, b2, b3), q(t0, t1, t2, t3), q(b0, b1, t1, t0), q(b3, b0, t0, t3), q(b1, b2, t2, t1)];
    if y0 > 0.001 {
        polys.push(q(b2, b3, t3, t2));
    }
    let points = vec![b0, b1, b2, b3, t0, t1, t2, t3];
    let centroid = turn * Vec3::new(0.0, -0.5 + (y0 + y1) * 0.25, -0.1);
    Piece { polys, points, centroid, volume: (y0 + y1) * 0.5 }
}

/// The convex parts of a block shape (block space; the low side of slopes
/// and stairs at +Z, towards the player when placed).
pub fn shape_pieces(shape: Shape) -> Vec<Piece> {
    match shape {
        Shape::Cube => vec![box_piece(Vec3::splat(-0.5), Vec3::splat(0.5))],
        Shape::Slab => vec![box_piece(Vec3::splat(-0.5), Vec3::new(0.5, 0.0, 0.5))],
        Shape::Wedge(y0, y1) => vec![wedge_piece(y0, y1, Quat::IDENTITY)],
        Shape::Stairs => vec![box_piece(Vec3::splat(-0.5), Vec3::new(0.5, 0.0, 0.5)), box_piece(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.5, 0.0))],
        Shape::CornerOuter => {
            // Square base, apex above the back-left corner.
            let (b0, b1, b2, b3) = (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, -0.5, -0.5), Vec3::new(0.5, -0.5, 0.5), Vec3::new(-0.5, -0.5, 0.5));
            let a = Vec3::new(-0.5, 0.5, -0.5);
            let polys = vec![vec![b0, b1, b2, b3], vec![b0, b1, a], vec![b3, b0, a], vec![b1, b2, a], vec![b2, b3, a]];
            vec![Piece { polys, points: vec![b0, b1, b2, b3, a], centroid: Vec3::new(-0.25, -0.25, -0.25), volume: 1.0 / 3.0 }]
        }
        // A slope rising to the back and one rising to the left.
        Shape::CornerInner => vec![wedge_piece(0.0, 1.0, Quat::IDENTITY), wedge_piece(0.0, 1.0, Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))],
    }
}

/// Wedge corners in block space (centred): bottom 4, then top back 2
/// (height y1 at −Z), top front 2 (height y0 at +Z).
pub fn wedge_points(y0: f32, y1: f32) -> [Vec3; 8] {
    let (b, f, k) = (-0.5, -0.5 + y0.max(0.001), -0.5 + y1);
    [
        Vec3::new(-0.5, b, -0.5),
        Vec3::new(0.5, b, -0.5),
        Vec3::new(0.5, b, 0.5),
        Vec3::new(-0.5, b, 0.5),
        Vec3::new(-0.5, k, -0.5),
        Vec3::new(0.5, k, -0.5),
        Vec3::new(0.5, f, 0.5),
        Vec3::new(-0.5, f, 0.5),
    ]
}

/// The world grid (static, at the origin).
#[derive(Component)]
pub struct WorldGrid;

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(render::RenderPlugin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_count() {
        let mut g = VoxelGrid::default();
        let b = BlockId(1);
        assert!(g.get(IVec3::new(-1, 5, 17)).is_air());
        g.set(IVec3::new(-1, 5, 17), Cell::new(b, 2));
        assert_eq!(g.get(IVec3::new(-1, 5, 17)).id, b);
        assert_eq!(g.cells().len(), 1);
        assert_eq!(g.cells()[0].0, IVec3::new(-1, 5, 17));
        g.set(IVec3::new(-1, 5, 17), Cell::AIR);
        assert!(g.cells().is_empty());
    }

    #[test]
    fn raycast_finds_face() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(3, 0, 0), Cell::new(BlockId(1), 2));
        let h = g.raycast(Vec3::new(0.5, 0.5, 0.5), Vec3::X, 10.0).unwrap();
        assert_eq!(h.cell, IVec3::new(3, 0, 0));
        assert_eq!(h.normal, IVec3::NEG_X);
        assert!((h.distance - 2.5).abs() < 1e-4);
        assert!(g.raycast(Vec3::new(0.5, 0.5, 0.5), Vec3::NEG_X, 10.0).is_none());
        // Downwards onto a floor.
        g.set(IVec3::new(0, -1, 0), Cell::new(BlockId(1), 2));
        let h = g.raycast(Vec3::new(0.2, 3.0, 0.7), Vec3::new(0.0, -1.0, 0.0), 10.0).unwrap();
        assert_eq!((h.cell, h.normal), (IVec3::new(0, -1, 0), IVec3::Y));
    }

    #[test]
    fn facing_rotation() {
        assert_eq!(facing_dir(facing_of(IVec3::NEG_Z)), IVec3::NEG_Z);
        // +X turned about Y by 90° → −Z.
        assert_eq!(facing_dir(rotate_facing(0, IVec3::Y)), IVec3::NEG_Z);
        for f in 0..6 {
            let v = facing_rot(f) * Vec3::Y;
            assert!((v - facing_dir(f).as_vec3()).length() < 1e-5);
        }
    }
}
