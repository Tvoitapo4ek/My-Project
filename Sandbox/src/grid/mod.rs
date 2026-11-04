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

use crate::content::{BlockId, Collision};

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
pub fn cell_boxes(c: IVec3, cell: Cell) -> Option<(Vec3, Vec3)> {
    let center = c.as_vec3() + Vec3::splat(0.5);
    match cell.id.def().collision {
        Collision::None => None,
        Collision::Full => Some((center, Vec3::splat(0.5))),
        Collision::Box(x, y, z) => {
            let half = (facing_rot(cell.facing) * Vec3::new(x, y, z) * 0.5).abs();
            Some((center, half))
        }
    }
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
