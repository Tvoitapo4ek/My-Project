//! Blueprints: a copied piece of construction (blocks + parts + joints),
//! stored relative to its minimum corner. Pure data; rotatable by 90° steps.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    machinery::components::PartKind,
    physics::joints::{JointKind, LocalFrame},
    world::voxel::BlockId,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPart {
    pub kind: PartKind,
    /// Position relative to the blueprint origin (min corner).
    pub offset: Vec3,
    pub rotation: Quat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintJoint {
    pub a: usize,
    pub b: usize,
    pub kind: JointKind,
    pub frame_a: LocalFrame,
    pub frame_b: LocalFrame,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    /// Size of the block grid (blocks are inside [0, size)).
    pub size: IVec3,
    pub blocks: Vec<(IVec3, BlockId)>,
    pub parts: Vec<BlueprintPart>,
    pub joints: Vec<BlueprintJoint>,
}

impl Blueprint {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.parts.is_empty()
    }

    /// Rotates 90° counter-clockwise about +Y (seen from above), keeping
    /// everything inside the positive octant.
    pub fn rotated_y90(&self) -> Blueprint {
        let s = self.size;
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        // (x, z) → (z, s.x-1-x) for cells; continuous: (x, z) → (z, s.x - x).
        Blueprint {
            size: IVec3::new(s.z, s.y, s.x),
            blocks: self.blocks.iter().map(|(p, b)| (IVec3::new(p.z, p.y, s.x - 1 - p.x), *b)).collect(),
            parts: self
                .parts
                .iter()
                .map(|p| BlueprintPart {
                    kind: p.kind,
                    offset: Vec3::new(p.offset.z, p.offset.y, s.x as f32 - p.offset.x),
                    rotation: (q * p.rotation).normalize(),
                })
                .collect(),
            // Joint frames are body-local: rotating whole bodies keeps them valid.
            joints: self.joints.clone(),
        }
    }

    /// Items needed to build it: (item key, count).
    pub fn cost(&self) -> Vec<(crate::items::ItemId, u32)> {
        let mut out: Vec<(crate::items::ItemId, u32)> = Vec::new();
        let mut add = |item: crate::items::ItemId| {
            if let Some(e) = out.iter_mut().find(|(i, _)| *i == item) {
                e.1 += 1;
            } else {
                out.push((item, 1));
            }
        };
        for (_, b) in &self.blocks {
            if let Some(i) = crate::items::item_for_block(*b) {
                add(i);
            }
        }
        for p in &self.parts {
            if let Some(i) = crate::items::ItemId::from_key(p.kind.key()) {
                add(i);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::voxel::blocks;

    fn sample() -> Blueprint {
        Blueprint {
            size: IVec3::new(3, 1, 2),
            blocks: vec![(IVec3::new(0, 0, 0), blocks::STONE), (IVec3::new(2, 0, 1), blocks::PLANKS)],
            parts: vec![BlueprintPart { kind: PartKind::Frame, offset: Vec3::new(0.5, 0.5, 0.5), rotation: Quat::IDENTITY }],
            joints: vec![],
        }
    }

    #[test]
    fn four_rotations_are_identity() {
        let b = sample();
        let r = b.rotated_y90().rotated_y90().rotated_y90().rotated_y90();
        assert_eq!(r.size, b.size);
        let mut x = r.blocks.clone();
        let mut y = b.blocks.clone();
        x.sort_by_key(|(p, _)| (p.x, p.y, p.z));
        y.sort_by_key(|(p, _)| (p.x, p.y, p.z));
        assert_eq!(x, y);
        assert!((r.parts[0].offset - b.parts[0].offset).length() < 1e-4);
    }

    #[test]
    fn rotation_keeps_blocks_in_bounds() {
        let r = sample().rotated_y90();
        assert_eq!(r.size, IVec3::new(2, 1, 3));
        for (p, _) in &r.blocks {
            assert!(p.cmpge(IVec3::ZERO).all() && p.cmplt(r.size).all(), "{p}");
        }
    }

    #[test]
    fn cost_counts_items() {
        let c = sample().cost();
        assert_eq!(c.len(), 3);
        assert!(c.iter().all(|(_, n)| *n == 1));
    }
}
