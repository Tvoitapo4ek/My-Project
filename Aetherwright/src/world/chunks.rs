//! Chunked voxel storage.
//!
//! The world is a finite grid of cubic chunks (`chunk_size`³ blocks each,
//! 1 byte per block). Edits mark the owning chunk — and neighbours when the
//! block lies on a chunk border — dirty for re-meshing, and are recorded in
//! `modified` so saves only store what the player changed.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use super::voxel::{BlockId, blocks};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkData {
    pub size: i32,
    pub blocks: Vec<BlockId>,
    /// Number of non-air blocks (skip meshing empty chunks).
    pub non_air: u32,
}

impl ChunkData {
    pub fn new(size: i32) -> Self {
        Self {
            size,
            blocks: vec![blocks::AIR; (size * size * size) as usize],
            non_air: 0,
        }
    }

    #[inline]
    pub fn index(&self, p: IVec3) -> usize {
        (p.x + self.size * (p.z + self.size * p.y)) as usize
    }

    #[inline]
    pub fn in_bounds(&self, p: IVec3) -> bool {
        p.cmpge(IVec3::ZERO).all() && p.cmplt(IVec3::splat(self.size)).all()
    }

    #[inline]
    pub fn get(&self, p: IVec3) -> BlockId {
        self.blocks[self.index(p)]
    }

    #[inline]
    pub fn set(&mut self, p: IVec3, b: BlockId) -> BlockId {
        let i = self.index(p);
        let old = self.blocks[i];
        if old.is_air() && !b.is_air() {
            self.non_air += 1;
        } else if !old.is_air() && b.is_air() {
            self.non_air -= 1;
        }
        self.blocks[i] = b;
        old
    }

    pub fn recount(&mut self) {
        self.non_air = self.blocks.iter().filter(|b| !b.is_air()).count() as u32;
    }

    /// Run-length encoding used by saves: (count, block) pairs.
    /// Run-length encoding: (count, block id) pairs. Ids are runtime ids;
    /// saves pair them with a key palette (see `save`).
    pub fn to_rle(&self) -> Vec<(u16, u16)> {
        let mut out: Vec<(u16, u16)> = Vec::new();
        for b in &self.blocks {
            match out.last_mut() {
                Some((n, id)) if *id == b.0 && *n < u16::MAX => *n += 1,
                _ => out.push((1, b.0)),
            }
        }
        out
    }

    /// Decodes RLE; `map` translates stored ids (e.g. through a save's key
    /// palette). Returns None for truncated data or unknown ids.
    pub fn from_rle(
        size: i32,
        rle: &[(u16, u16)],
        map: impl Fn(u16) -> Option<BlockId>,
    ) -> Option<Self> {
        let mut c = Self::new(size);
        let mut i = 0usize;
        for &(n, id) in rle {
            let end = i + n as usize;
            if end > c.blocks.len() {
                return None;
            }
            c.blocks[i..end].fill(map(id)?);
            i = end;
        }
        if i != c.blocks.len() {
            return None;
        }
        c.recount();
        Some(c)
    }
}

/// Which code path changed a block (for effects, drops and statistics).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCause {
    Mined,
    Placed,
    Cut,
    Creative,
    Load,
    Script,
}

#[derive(Resource, Debug)]
pub struct VoxelWorld {
    pub chunk_size: i32,
    /// Exclusive chunk-coordinate bounds: [0, size_chunks).
    pub size_chunks: IVec3,
    pub chunks: HashMap<IVec3, ChunkData>,
    /// Chunks that need a new mesh + collider.
    pub dirty: HashSet<IVec3>,
    /// Chunks changed by the player since generation (persisted).
    pub modified: HashSet<IVec3>,
    pub seed: u32,
    pub sea_level: i32,
}

impl VoxelWorld {
    pub fn new(chunk_size: i32, size_chunks: IVec3, seed: u32, sea_level: i32) -> Self {
        Self {
            chunk_size,
            size_chunks,
            chunks: HashMap::new(),
            dirty: HashSet::new(),
            modified: HashSet::new(),
            seed,
            sea_level,
        }
    }

    #[inline]
    pub fn chunk_of(&self, p: IVec3) -> IVec3 {
        p.div_euclid(IVec3::splat(self.chunk_size))
    }

    #[inline]
    pub fn local_of(&self, p: IVec3) -> IVec3 {
        p.rem_euclid(IVec3::splat(self.chunk_size))
    }

    pub fn chunk_in_bounds(&self, c: IVec3) -> bool {
        c.cmpge(IVec3::ZERO).all() && c.cmplt(self.size_chunks).all()
    }

    /// World size in blocks.
    pub fn size_blocks(&self) -> IVec3 {
        self.size_chunks * self.chunk_size
    }

    pub fn in_bounds(&self, p: IVec3) -> bool {
        p.cmpge(IVec3::ZERO).all() && p.cmplt(self.size_blocks()).all()
    }

    /// Block at a world position. Outside the world: bedrock below and at the
    /// sides (an invisible wall), air above.
    pub fn get(&self, p: IVec3) -> BlockId {
        if p.y >= self.size_blocks().y {
            return blocks::AIR;
        }
        if !self.in_bounds(p) {
            return blocks::BEDROCK;
        }
        match self.chunks.get(&self.chunk_of(p)) {
            Some(c) => c.get(self.local_of(p)),
            None => blocks::AIR,
        }
    }

    /// Sets a block; returns the previous block if anything changed.
    /// Out-of-bounds or ungenerated positions are rejected.
    pub fn set(&mut self, p: IVec3, b: BlockId) -> Option<BlockId> {
        if !self.in_bounds(p) {
            return None;
        }
        let cc = self.chunk_of(p);
        let local = self.local_of(p);
        let size = self.chunk_size;
        let chunk = self.chunks.get_mut(&cc)?;
        let old = chunk.get(local);
        if old == b {
            return None;
        }
        chunk.set(local, b);
        self.dirty.insert(cc);
        self.modified.insert(cc);
        // Border blocks affect the neighbour's face culling and AO.
        for axis in 0..3 {
            if local[axis] == 0 {
                let mut n = cc;
                n[axis] -= 1;
                self.mark_dirty(n);
            }
            if local[axis] == size - 1 {
                let mut n = cc;
                n[axis] += 1;
                self.mark_dirty(n);
            }
        }
        Some(old)
    }

    pub fn mark_dirty(&mut self, c: IVec3) {
        if self.chunks.contains_key(&c) {
            self.dirty.insert(c);
        }
    }

    /// Highest non-air, non-plant block in a column (for spawning).
    pub fn surface_y(&self, x: i32, z: i32) -> Option<i32> {
        (0..self.size_blocks().y).rev().find(|&y| {
            let b = self.get(IVec3::new(x, y, z));
            b.is_solid() || b.is_liquid()
        })
    }

    /// Copies a chunk plus a 1-block border from its neighbours into a
    /// padded array of (size+2)³ — the self-contained input of the mesher.
    pub fn padded(&self, c: IVec3) -> Vec<BlockId> {
        let s = self.chunk_size;
        let p = s + 2;
        let origin = c * s - IVec3::ONE;
        let mut out = vec![blocks::AIR; (p * p * p) as usize];
        let center = self.chunks.get(&c);
        for y in 0..p {
            for z in 0..p {
                for x in 0..p {
                    let lp = IVec3::new(x - 1, y - 1, z - 1);
                    let b = match center {
                        Some(ch) if ch.in_bounds(lp) => ch.get(lp),
                        _ => self.get(origin + IVec3::new(x, y, z)),
                    };
                    out[(x + p * (z + p * y)) as usize] = b;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> VoxelWorld {
        let mut w = VoxelWorld::new(8, IVec3::new(2, 2, 2), 1, 4);
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    w.chunks.insert(IVec3::new(x, y, z), ChunkData::new(8));
                }
            }
        }
        w
    }

    #[test]
    fn set_get_roundtrip_and_counts() {
        let mut w = world();
        let p = IVec3::new(9, 3, 2);
        assert_eq!(w.set(p, blocks::STONE), Some(blocks::AIR));
        assert_eq!(w.get(p), blocks::STONE);
        assert_eq!(w.chunks[&IVec3::new(1, 0, 0)].non_air, 1);
        assert_eq!(w.set(p, blocks::STONE), None, "no-op edits are ignored");
    }

    #[test]
    fn border_edit_marks_neighbour_dirty() {
        let mut w = world();
        w.dirty.clear();
        w.set(IVec3::new(8, 3, 3), blocks::DIRT);
        assert!(w.dirty.contains(&IVec3::new(1, 0, 0)));
        assert!(
            w.dirty.contains(&IVec3::new(0, 0, 0)),
            "neighbour across x border"
        );
        w.dirty.clear();
        w.set(IVec3::new(12, 3, 3), blocks::DIRT);
        assert_eq!(w.dirty.len(), 1, "interior edit touches one chunk");
    }

    #[test]
    fn out_of_bounds_is_rejected_and_walls_are_solid() {
        let mut w = world();
        assert_eq!(w.set(IVec3::new(-1, 0, 0), blocks::STONE), None);
        assert_eq!(w.get(IVec3::new(-1, 3, 0)), blocks::BEDROCK);
        assert_eq!(w.get(IVec3::new(0, 100, 0)), blocks::AIR);
    }

    #[test]
    fn rle_roundtrip() {
        let mut c = ChunkData::new(8);
        c.set(IVec3::new(1, 2, 3), blocks::IRON_ORE);
        c.set(IVec3::new(7, 7, 7), blocks::WATER);
        let rle = c.to_rle();
        assert!(rle.len() < 10);
        let back = ChunkData::from_rle(8, &rle, |i| Some(BlockId(i))).unwrap();
        assert_eq!(back, c);
        assert!(
            ChunkData::from_rle(8, &[(3, 1)], |i| Some(BlockId(i))).is_none(),
            "truncated data is rejected"
        );
    }

    #[test]
    fn padded_includes_neighbour_border() {
        let mut w = world();
        w.set(IVec3::new(8, 0, 0), blocks::STONE); // first block of chunk (1,0,0)
        let pad = w.padded(IVec3::ZERO);
        let p = 10;
        // padded x index 9 = world x 8
        assert_eq!(pad[(9 + p * (1 + p * 1)) as usize], blocks::STONE);
    }
}
