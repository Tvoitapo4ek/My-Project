//! Consequences of block changes:
//! * debris particles when a block is broken (visual only, no physics cost);
//! * plants lose support when the block below disappears;
//! * loose materials (sand, gravel) lose support and detach as physical
//!   blocks (`DetachedBlock` — turned into rigid bodies by the physics module).

use bevy::prelude::*;

use super::{
    chunks::{EditCause, VoxelWorld},
    interaction::{BlockChanged, BlockEdit},
    materials::BlockMaterials,
    voxel::{BlockId, RenderKind, blocks},
};

/// A block that lost support and must become a dynamic body.
#[derive(Message, Debug, Clone, Copy)]
pub struct DetachedBlock {
    pub pos: IVec3,
    pub block: BlockId,
}

#[derive(Component)]
struct Debris {
    velocity: Vec3,
    ttl: f32,
}

#[derive(Resource)]
struct DebrisMesh(Handle<Mesh>);

const MAX_DEBRIS: usize = 400;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(DebrisMesh(meshes.add(Cuboid::new(0.14, 0.14, 0.14))));
}

pub fn is_loose(b: BlockId) -> bool {
    b == blocks::SAND || b == blocks::GRAVEL
}

fn on_block_changed(
    mut commands: Commands,
    mut changed: MessageReader<BlockChanged>,
    world: Res<VoxelWorld>,
    mesh: Res<DebrisMesh>,
    mats: Res<BlockMaterials>,
    debris: Query<(), With<Debris>>,
    mut edits: MessageWriter<BlockEdit>,
    mut detached: MessageWriter<DetachedBlock>,
) {
    let mut budget = MAX_DEBRIS.saturating_sub(debris.iter().count());
    for c in changed.read() {
        if c.cause == EditCause::Load {
            continue;
        }
        // Debris burst.
        if matches!(c.cause, EditCause::Mined)
            && c.old.def().render != RenderKind::Invisible
            && budget > 0
        {
            let mat = mats.get(c.old.def().tex[1]);
            let center = c.pos.as_vec3() + Vec3::splat(0.5);
            for i in 0..8.min(budget) {
                let a = i as f32 * 0.785 + (c.pos.x + c.pos.z) as f32;
                let v = Vec3::new(a.cos() * 2.2, 3.0 + (i % 3) as f32, a.sin() * 2.2);
                commands.spawn((
                    Mesh3d(mesh.0.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(center + v * 0.08),
                    Debris {
                        velocity: v,
                        ttl: 0.7,
                    },
                    bevy::light::NotShadowCaster,
                ));
            }
            budget = budget.saturating_sub(8);
        }
        // Support: something removed (or replaced by a non-solid block).
        if c.old.is_solid() && !c.new.is_solid() {
            let above = c.pos + IVec3::Y;
            let b = world.get(above);
            if b.def().render == RenderKind::Cross {
                edits.write(BlockEdit {
                    pos: above,
                    block: blocks::AIR,
                    cause: EditCause::Mined,
                });
            } else if is_loose(b) {
                edits.write(BlockEdit {
                    pos: above,
                    block: blocks::AIR,
                    cause: EditCause::Cut,
                });
                detached.write(DetachedBlock {
                    pos: above,
                    block: b,
                });
            }
        }
    }
}

fn update_debris(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut Debris)>,
) {
    let dt = time.delta_secs();
    for (e, mut t, mut d) in &mut q {
        d.ttl -= dt;
        if d.ttl <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        d.velocity.y -= 18.0 * dt;
        t.translation += d.velocity * dt;
        t.scale = Vec3::splat((d.ttl / 0.7).clamp(0.2, 1.0));
    }
}

pub struct DestructionPlugin;

impl Plugin for DestructionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DetachedBlock>()
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                on_block_changed.after(super::interaction::apply_block_edits),
            )
            .add_systems(Update, update_debris);
    }
}
