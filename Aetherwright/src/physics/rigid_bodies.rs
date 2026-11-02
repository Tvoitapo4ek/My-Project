//! Loose blocks: voxels turned into rigid bodies (cut by the manipulator or
//! detached when unsupported). Loose sand/gravel settles back into the grid
//! once it comes to rest; cut blocks stay free objects for building.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::Prop;
use crate::{
    core::{events::Notify, layers::Layer, rules::ModeRules},
    items::{block_drop, inventory::Inventory, ItemId},
    player::Player,
    tools::scanning::Scannable,
    world::{
        chunks::{EditCause, VoxelWorld},
        destruction::{is_loose, DetachedBlock},
        interaction::BlockEdit,
        materials::BlockMaterials,
        voxel::{BlockId, RenderKind},
    },
};

#[derive(Component, Debug)]
pub struct LooseBlock {
    pub block: BlockId,
    /// Re-enter the voxel grid when at rest (sand, gravel).
    pub settles: bool,
    rest_time: f32,
}

#[derive(Resource)]
struct BlockMesh(Handle<Mesh>);

/// Request to pick a physical object up into the inventory.
#[derive(Message, Debug, Clone, Copy)]
pub struct CollectProp(pub Entity);

/// Mass of a loose block (kg). Game-scale, not real 1 m³ densities: light
/// blocks can be carried by the physics gun, dense ones only dragged.
pub fn block_mass(b: BlockId) -> f32 {
    let h = b.def().hardness;
    if h.is_finite() { 40.0 + h * 60.0 } else { 1000.0 }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(BlockMesh(meshes.add(Cuboid::new(1.0, 1.0, 1.0))));
}

/// Spawns a loose block body (also used by saves and blueprints).
pub fn spawn_loose_block(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    mats: &BlockMaterials,
    block: BlockId,
    transform: Transform,
    settles: bool,
) -> Entity {
    let def = block.def();
    commands
        .spawn((
            Name::new(format!("Loose {}", def.key)),
            Prop::Block(def.key.to_string()),
            LooseBlock { block, settles, rest_time: 0.0 },
            Scannable { title: format!("{} (свободный блок)", def.name), info: def.info.to_string() },
            Mesh3d(mesh),
            MeshMaterial3d(mats.get(def.tex[1])),
            transform,
            RigidBody::Dynamic,
            Collider::cuboid(0.98, 0.98, 0.98),
            Mass(block_mass(block)),
            Friction::new(0.7),
            CollisionLayers::new(Layer::Prop, LayerMask::ALL),
        ))
        .id()
}

fn on_detached(
    mut commands: Commands,
    mut reader: MessageReader<DetachedBlock>,
    mesh: Res<BlockMesh>,
    mats: Res<BlockMaterials>,
) {
    for d in reader.read() {
        if d.block.def().render != RenderKind::Cube && d.block.def().render != RenderKind::Transparent {
            continue;
        }
        let t = Transform::from_translation(d.pos.as_vec3() + Vec3::splat(0.5));
        spawn_loose_block(&mut commands, mesh.0.clone(), &mats, d.block, t, is_loose(d.block));
    }
}

/// Sand and gravel at rest snap back into the voxel grid.
fn settle(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<VoxelWorld>,
    mut q: Query<(Entity, &Transform, &LinearVelocity, &mut LooseBlock), Without<super::physics_gun::Held>>,
    mut edits: MessageWriter<BlockEdit>,
) {
    for (e, t, v, mut lb) in &mut q {
        if !lb.settles {
            continue;
        }
        if v.0.length() < 0.15 {
            lb.rest_time += time.delta_secs();
        } else {
            lb.rest_time = 0.0;
        }
        if lb.rest_time > 0.6 {
            let cell = t.translation.floor().as_ivec3();
            if world.get(cell).is_replaceable() {
                commands.entity(e).despawn();
                edits.write(BlockEdit { pos: cell, block: lb.block, cause: EditCause::Script });
            } else {
                lb.settles = false;
            }
        }
    }
}

/// Collecting a loose block or a part gives its item back.
pub fn collect_props(
    mut commands: Commands,
    mut reader: MessageReader<CollectProp>,
    rules: Res<ModeRules>,
    props: Query<&Prop>,
    joints: Query<(Entity, &super::joints::Connection)>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let Ok(mut inv) = player.single_mut() else { return };
    for CollectProp(e) in reader.read() {
        let Ok(prop) = props.get(*e) else { continue };
        let item = match prop {
            Prop::Block(k) => BlockId::from_key(k).and_then(block_drop),
            Prop::Part(k) => ItemId::from_key(k),
        };
        if rules.consume_items
            && let Some(item) = item
            && inv.add(item, 1) > 0
        {
            notify.write(Notify::warn("Инвентарь полон"));
            continue;
        }
        // Joints attached to the object go away with it.
        for (je, c) in &joints {
            if c.a == *e || c.b == *e {
                commands.entity(je).despawn();
            }
        }
        commands.entity(*e).despawn();
    }
}

pub struct RigidBodiesPlugin;

impl Plugin for RigidBodiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CollectProp>()
            .add_systems(Startup, setup)
            .add_systems(PostUpdate, on_detached.after(crate::world::interaction::apply_block_edits))
            .add_systems(Update, (settle, collect_props).run_if(in_state(crate::core::states::AppState::InGame)));
    }
}

pub fn loose_block_mesh(world: &World) -> Handle<Mesh> {
    world.resource::<BlockMesh>().0.clone()
}
