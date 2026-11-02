//! Block targeting and the block-edit pipeline.
//!
//! * `voxel_raycast` — exact grid traversal (Amanatides & Woo DDA); returns
//!   the first targetable block and the face that was hit.
//! * `BlockTarget` — what the player looks at this frame (within reach).
//! * `BlockEdit` (request) → validated and applied by `apply_block_edits` →
//!   `BlockChanged` (fact). Tools never write to `VoxelWorld` directly, so
//!   validation (bounds, overlap with bodies) lives in one place.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    chunks::{EditCause, VoxelWorld},
    voxel::BlockId,
};
use crate::core::{
    config::GameConfig,
    layers::Layer,
    resources::EyeRay,
    states::{AppState, gameplay_active},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoxelHit {
    /// The block that was hit.
    pub pos: IVec3,
    /// Outward normal of the face that was hit (unit axis vector).
    pub normal: IVec3,
    pub block: BlockId,
    pub distance: f32,
}

impl VoxelHit {
    /// Cell in front of the hit face (where a new block would go).
    pub fn place_pos(&self) -> IVec3 {
        self.pos + self.normal
    }
}

/// Walks the voxel grid along a ray.
pub fn voxel_raycast(
    world: &VoxelWorld,
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    accept: impl Fn(BlockId) -> bool,
) -> Option<VoxelHit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let mut cell = origin.floor().as_ivec3();
    let step = IVec3::new(
        dir.x.signum() as i32,
        dir.y.signum() as i32,
        dir.z.signum() as i32,
    );
    let next_boundary = |o: f32, c: i32, s: i32| {
        if s > 0 {
            (c + 1) as f32 - o
        } else {
            o - c as f32
        }
    };
    let inv = |d: f32| {
        if d.abs() < 1e-9 {
            f32::INFINITY
        } else {
            1.0 / d.abs()
        }
    };
    let t_delta = Vec3::new(inv(dir.x), inv(dir.y), inv(dir.z));
    let mut t_max = Vec3::new(
        next_boundary(origin.x, cell.x, step.x) * t_delta.x,
        next_boundary(origin.y, cell.y, step.y) * t_delta.y,
        next_boundary(origin.z, cell.z, step.z) * t_delta.z,
    );
    let mut normal = IVec3::ZERO;
    let mut t = 0.0;
    // Starting inside a block counts as a hit on that block.
    loop {
        let b = world.get(cell);
        if accept(b) && (world.in_bounds(cell) || b.is_solid()) && cell.y >= 0 {
            return Some(VoxelHit {
                pos: cell,
                normal,
                block: b,
                distance: t,
            });
        }
        if t_max.x < t_max.y && t_max.x < t_max.z {
            t = t_max.x;
            t_max.x += t_delta.x;
            cell.x += step.x;
            normal = IVec3::new(-step.x, 0, 0);
        } else if t_max.y < t_max.z {
            t = t_max.y;
            t_max.y += t_delta.y;
            cell.y += step.y;
            normal = IVec3::new(0, -step.y, 0);
        } else {
            t = t_max.z;
            t_max.z += t_delta.z;
            cell.z += step.z;
            normal = IVec3::new(0, 0, -step.z);
        }
        if t > max_dist {
            return None;
        }
    }
}

/// Systems that update the per-frame targeting.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InteractionSet;

/// The block under the crosshair this frame (if within reach).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct BlockTarget(pub Option<VoxelHit>);

/// Request to change one block.
#[derive(Message, Debug, Clone, Copy)]
pub struct BlockEdit {
    pub pos: IVec3,
    pub block: BlockId,
    pub cause: EditCause,
}

/// A block actually changed.
#[derive(Message, Debug, Clone, Copy)]
pub struct BlockChanged {
    pub pos: IVec3,
    pub old: BlockId,
    pub new: BlockId,
    pub cause: EditCause,
}

/// Why an edit was refused (sent back to the requester via `Notify`).
#[derive(Message, Debug, Clone, Copy)]
pub struct BlockEditRejected {
    pub pos: IVec3,
    pub reason: RejectReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    OutOfWorld,
    Occupied,
    Unbreakable,
}

fn update_target(
    eye: Res<EyeRay>,
    world: Res<VoxelWorld>,
    cfg: Res<GameConfig>,
    mut target: ResMut<BlockTarget>,
) {
    target.0 = voxel_raycast(&world, eye.origin, eye.direction, cfg.player.reach, |b| {
        b.is_targetable()
    });
}

/// Validates and applies edits. Placing a solid block is refused if it
/// would overlap the player, a creature or a physics object.
pub fn apply_block_edits(
    mut edits: MessageReader<BlockEdit>,
    mut world: ResMut<VoxelWorld>,
    spatial: SpatialQuery,
    mut changed: MessageWriter<BlockChanged>,
    mut rejected: MessageWriter<BlockEditRejected>,
) {
    for e in edits.read() {
        if !world.in_bounds(e.pos) {
            rejected.write(BlockEditRejected {
                pos: e.pos,
                reason: RejectReason::OutOfWorld,
            });
            continue;
        }
        let old = world.get(e.pos);
        if !old.is_air()
            && old.def().hardness.is_infinite()
            && !old.is_liquid()
            && e.cause != EditCause::Load
        {
            rejected.write(BlockEditRejected {
                pos: e.pos,
                reason: RejectReason::Unbreakable,
            });
            continue;
        }
        if e.block.is_solid() && !matches!(e.cause, EditCause::Load | EditCause::Script) {
            let center = e.pos.as_vec3() + Vec3::splat(0.5);
            let filter =
                SpatialQueryFilter::from_mask([Layer::Player, Layer::Prop, Layer::Creature]);
            let hits = spatial.shape_intersections(
                &Collider::cuboid(0.98, 0.98, 0.98),
                center,
                Quat::IDENTITY,
                &filter,
            );
            if !hits.is_empty() {
                rejected.write(BlockEditRejected {
                    pos: e.pos,
                    reason: RejectReason::Occupied,
                });
                continue;
            }
        }
        if let Some(old) = world.set(e.pos, e.block) {
            changed.write(BlockChanged {
                pos: e.pos,
                old,
                new: e.block,
                cause: e.cause,
            });
        }
    }
}

fn draw_target(target: Res<BlockTarget>, mut gizmos: Gizmos) {
    if let Some(hit) = target.0 {
        let center = hit.pos.as_vec3() + Vec3::splat(0.5);
        gizmos.cube(
            Transform::from_translation(center).with_scale(Vec3::splat(1.004)),
            Color::srgba(0.4, 0.95, 1.0, 0.9),
        );
    }
}

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockTarget>()
            .add_message::<BlockEdit>()
            .add_message::<BlockChanged>()
            .add_message::<BlockEditRejected>()
            .add_systems(
                Update,
                (update_target, draw_target)
                    .chain()
                    .in_set(InteractionSet)
                    .run_if(gameplay_active),
            )
            .add_systems(
                PostUpdate,
                apply_block_edits.run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunks::ChunkData, voxel::blocks};

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
    fn ray_hits_block_and_reports_face() {
        let mut w = world();
        w.set(IVec3::new(5, 2, 5), blocks::STONE);
        let hit = voxel_raycast(&w, Vec3::new(5.5, 6.5, 5.5), Vec3::NEG_Y, 10.0, |b| {
            b.is_targetable()
        })
        .unwrap();
        assert_eq!(hit.pos, IVec3::new(5, 2, 5));
        assert_eq!(hit.normal, IVec3::Y);
        assert_eq!(hit.place_pos(), IVec3::new(5, 3, 5));
        assert!((hit.distance - 3.5).abs() < 1e-4);
    }

    #[test]
    fn ray_diagonal_and_max_distance() {
        let mut w = world();
        w.set(IVec3::new(9, 3, 3), blocks::DIRT);
        let o = Vec3::new(3.5, 3.5, 3.5);
        let hit = voxel_raycast(&w, o, Vec3::X, 10.0, |b| b.is_targetable()).unwrap();
        assert_eq!(hit.normal, IVec3::NEG_X);
        assert!(voxel_raycast(&w, o, Vec3::X, 4.0, |b| b.is_targetable()).is_none());
        // Diagonal ray from below up into a block.
        w.set(IVec3::new(2, 6, 2), blocks::STONE);
        let hit = voxel_raycast(
            &w,
            Vec3::new(1.5, 4.5, 1.5),
            Vec3::new(1.0, 2.0, 1.0),
            10.0,
            |b| b.is_targetable(),
        );
        assert_eq!(hit.map(|h| h.pos), Some(IVec3::new(2, 6, 2)));
    }

    #[test]
    fn water_is_skipped() {
        let mut w = world();
        w.set(IVec3::new(1, 1, 1), blocks::WATER);
        w.set(IVec3::new(1, 0, 1), blocks::SAND);
        let hit = voxel_raycast(&w, Vec3::new(1.5, 3.5, 1.5), Vec3::NEG_Y, 10.0, |b| {
            b.is_targetable()
        })
        .unwrap();
        assert_eq!(hit.block, blocks::SAND);
    }
}
