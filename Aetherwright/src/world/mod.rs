//! The voxel world: storage, generation, meshing, colliders, editing and
//! the environment (sun, sky, fog).
//!
//! Lifecycle:
//! 1. `AppState::Loading`: every chunk is generated on the async compute
//!    pool, then meshed on the pool from padded snapshots.
//! 2. Chunk entities are spawned (one static body with a voxel collider per
//!    chunk, one mesh child per material).
//! 3. In game, edits mark chunks dirty; dirty chunks are re-meshed on the
//!    main thread with a per-frame budget (an edit is visible and solid
//!    on the next frame), neighbours across borders included.

pub mod chunks;
pub mod destruction;
pub mod environment;
pub mod generation;
pub mod interaction;
pub mod materials;
pub mod meshing;
pub mod sky;
pub mod voxel;

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
};

use crate::core::{config::GameConfig, layers::Layer, resources::SpawnPoint, states::AppState};
use chunks::{ChunkData, VoxelWorld};
use generation::GenParams;
use materials::BlockMaterials;
use meshing::{ChunkMesh, Padded, mesh_chunk};

/// Root entity of a chunk (static body + collider); children carry meshes.
#[derive(Component, Debug)]
pub struct Chunk {
    pub coord: IVec3,
}

/// Map chunk coord → chunk entity.
#[derive(Resource, Default)]
pub struct ChunkEntities(pub HashMap<IVec3, Entity>);

/// Progress of the initial load (shown on the loading screen).
#[derive(Resource, Default, Debug, Clone)]
pub struct LoadProgress {
    pub total: usize,
    pub generated: usize,
    pub meshed: usize,
    /// Set by the save system when a save replaced generated chunks.
    pub stage: &'static str,
}

#[derive(Resource, Default)]
struct GenTasks(Vec<(IVec3, Task<ChunkData>)>);

#[derive(Resource, Default)]
struct MeshTasks(Vec<(IVec3, Task<ChunkMesh>)>);

/// Hook for systems that must modify freshly generated chunks before they
/// are meshed (e.g. applying a save file). Runs once.
#[derive(Resource, Default)]
pub struct GenerationFinished(pub bool);

#[derive(Message, Debug, Clone, Copy)]
pub struct WorldReady;

/// Meshing of the freshly generated world has been started.
#[derive(Resource, Default)]
pub struct MeshingStarted(pub bool);

/// Chunk contents that replace generated ones before meshing (save loading).
#[derive(Resource, Default)]
pub struct PendingChunkPatches(pub Vec<(IVec3, ChunkData)>);

pub fn gen_params(world: &VoxelWorld) -> GenParams {
    GenParams {
        seed: world.seed,
        chunk_size: world.chunk_size,
        size: world.size_blocks(),
        sea_level: world.sea_level,
    }
}

fn new_world(cfg: &GameConfig) -> VoxelWorld {
    let w = &cfg.world;
    VoxelWorld::new(
        w.chunk_size,
        IVec3::new(w.size_chunks_xz, w.height_chunks, w.size_chunks_xz),
        w.seed,
        w.sea_level,
    )
}

fn start_generation(
    world: Res<VoxelWorld>,
    mut tasks: ResMut<GenTasks>,
    mut progress: ResMut<LoadProgress>,
) {
    let pool = AsyncComputeTaskPool::get();
    let params = gen_params(&world);
    for y in 0..world.size_chunks.y {
        for z in 0..world.size_chunks.z {
            for x in 0..world.size_chunks.x {
                let c = IVec3::new(x, y, z);
                tasks.0.push((
                    c,
                    pool.spawn(async move { generation::generate_chunk(&params, c) }),
                ));
            }
        }
    }
    *progress = LoadProgress {
        total: tasks.0.len(),
        generated: 0,
        meshed: 0,
        stage: "Генерация мира",
    };
}

fn poll_generation(
    mut tasks: ResMut<GenTasks>,
    mut world: ResMut<VoxelWorld>,
    mut progress: ResMut<LoadProgress>,
    mut finished: ResMut<GenerationFinished>,
) {
    if finished.0 {
        return;
    }
    tasks.0.retain_mut(|(c, task)| match check_ready(task) {
        Some(data) => {
            world.chunks.insert(*c, data);
            progress.generated += 1;
            false
        }
        None => true,
    });
    if tasks.0.is_empty() && progress.generated == progress.total && progress.total > 0 {
        finished.0 = true;
    }
}

/// Once generation (and any save application, which runs between) is done,
/// mesh every chunk in parallel.
fn start_meshing(
    finished: Res<GenerationFinished>,
    mut world: ResMut<VoxelWorld>,
    mut tasks: ResMut<MeshTasks>,
    mut started: ResMut<MeshingStarted>,
    mut patches: ResMut<PendingChunkPatches>,
    mut progress: ResMut<LoadProgress>,
) {
    if !finished.0 || started.0 {
        return;
    }
    started.0 = true;
    // Saved chunks replace generated ones (and stay "modified" for the next save).
    for (c, data) in patches.0.drain(..) {
        if world.chunks.contains_key(&c) {
            world.chunks.insert(c, data);
            world.modified.insert(c);
        }
    }
    progress.stage = "Построение геометрии";
    let pool = AsyncComputeTaskPool::get();
    let n = world.chunk_size;
    let coords: Vec<IVec3> = world.chunks.keys().copied().collect();
    for c in coords {
        let padded = world.padded(c);
        tasks.0.push((
            c,
            pool.spawn(async move { mesh_chunk(&Padded { data: &padded, n }) }),
        ));
    }
    world.dirty.clear();
}

#[allow(clippy::too_many_arguments)]
fn poll_meshing(
    mut commands: Commands,
    mut tasks: ResMut<MeshTasks>,
    world: Res<VoxelWorld>,
    mut entities: ResMut<ChunkEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Res<BlockMaterials>,
    mut progress: ResMut<LoadProgress>,
    mut spawn: ResMut<SpawnPoint>,
    mut next: ResMut<NextState<AppState>>,
    finished: Res<GenerationFinished>,
    mut ready: MessageWriter<WorldReady>,
) {
    if !finished.0 {
        return;
    }
    let mut done = Vec::new();
    tasks.0.retain_mut(|(c, task)| match check_ready(task) {
        Some(mesh) => {
            done.push((*c, mesh));
            false
        }
        None => true,
    });
    for (c, mesh) in done {
        apply_chunk_mesh(
            &mut commands,
            &mut entities,
            &mut meshes,
            &mats,
            &world,
            c,
            mesh,
        );
        progress.meshed += 1;
    }
    if tasks.0.is_empty() && progress.meshed >= progress.total && progress.total > 0 {
        if spawn.0 == SpawnPoint::default().0 {
            spawn.0 = find_spawn(&world);
        }
        info!("World ready: {} chunks", progress.total);
        ready.write(WorldReady);
        next.set(AppState::InGame);
    }
}

/// A dry, flat, open spot near the world centre: level 5×5 ground with no
/// overhanging leaves (so the player starts with clear sky above).
pub fn find_spawn(world: &VoxelWorld) -> Vec3 {
    let size = world.size_blocks();
    let center = IVec2::new(size.x / 2, size.z / 2);
    let open_column =
        |x: i32, z: i32, y: i32| (1..=7).all(|h| world.get(IVec3::new(x, y + h, z)).is_air());
    let good = |x: i32, z: i32| -> Option<i32> {
        let y = world.surface_y(x, z)?;
        let top = world.get(IVec3::new(x, y, z));
        if !top.is_solid() || y <= world.sea_level + 1 {
            return None;
        }
        for dz in -2..=2 {
            for dx in -2..=2 {
                let (cx, cz) = (x + dx, z + dz);
                if world.surface_y(cx, cz) != Some(y)
                    || !world.get(IVec3::new(cx, y, cz)).is_solid()
                {
                    return None;
                }
                // Plants on the ground are fine, anything else above is not.
                let above = world.get(IVec3::new(cx, y + 1, cz));
                if !(above.is_air() || above.def().render == voxel::RenderKind::Cross) {
                    return None;
                }
                if !(2..=8).all(|h| world.get(IVec3::new(cx, y + h, cz)).is_air()) {
                    return None;
                }
            }
        }
        open_column(x, z, y).then_some(y)
    };
    for r in 0..size.x / 2 - 3 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let (x, z) = (center.x + dx, center.y + dz);
                if let Some(y) = good(x, z) {
                    return Vec3::new(x as f32 + 0.5, y as f32 + 2.0, z as f32 + 0.5);
                }
            }
        }
    }
    let y = world.surface_y(center.x, center.y).unwrap_or(size.y - 2);
    Vec3::new(center.x as f32 + 0.5, y as f32 + 2.5, center.y as f32 + 0.5)
}

fn to_mesh(buf: meshing::MeshBuffers) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, buf.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, buf.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, buf.uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, buf.colors)
    .with_inserted_indices(Indices::U32(buf.indices))
}

/// Replaces the meshes and collider of a chunk (spawning the chunk entity
/// on first use).
pub fn apply_chunk_mesh(
    commands: &mut Commands,
    entities: &mut ChunkEntities,
    meshes: &mut Assets<Mesh>,
    mats: &BlockMaterials,
    world: &VoxelWorld,
    c: IVec3,
    mesh: ChunkMesh,
) {
    let origin = (c * world.chunk_size).as_vec3();
    let entity = *entities.0.entry(c).or_insert_with(|| {
        commands
            .spawn((
                Name::new(format!("Chunk {c}")),
                Chunk { coord: c },
                Transform::from_translation(origin),
                Visibility::default(),
                RigidBody::Static,
                CollisionLayers::new(Layer::Terrain, LayerMask::ALL),
            ))
            .id()
    });
    let mut ec = commands.entity(entity);
    ec.despawn_related::<Children>();
    if mesh.solid_voxels.is_empty() {
        ec.remove::<Collider>();
    } else {
        ec.insert(Collider::voxels(Vec3::ONE, &mesh.solid_voxels));
    }
    ec.with_children(|p| {
        for (tex, buf) in mesh.parts {
            if buf.indices.is_empty() {
                continue;
            }
            let mut child = p.spawn((
                Mesh3d(meshes.add(to_mesh(buf))),
                MeshMaterial3d(mats.get(tex)),
            ));
            if !crate::content::registry()
                .textures
                .get(tex.0 as usize)
                .is_some_and(|t| t.shadows)
            {
                child.insert(bevy::light::NotShadowCaster);
            }
        }
    });
}

/// Re-meshes edited chunks (synchronously, budgeted).
fn remesh_dirty(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut entities: ResMut<ChunkEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Res<BlockMaterials>,
    cfg: Res<GameConfig>,
) {
    if world.dirty.is_empty() {
        return;
    }
    let budget = cfg.world.remesh_budget_per_frame.max(1);
    let batch: Vec<IVec3> = world.dirty.iter().copied().take(budget).collect();
    let n = world.chunk_size;
    for c in batch {
        world.dirty.remove(&c);
        let padded = world.padded(c);
        let mesh = mesh_chunk(&Padded { data: &padded, n });
        apply_chunk_mesh(
            &mut commands,
            &mut entities,
            &mut meshes,
            &mats,
            &world,
            c,
            mesh,
        );
    }
}

fn pause_physics(mut time: ResMut<Time<Physics>>) {
    time.pause();
}

fn resume_physics(mut time: ResMut<Time<Physics>>) {
    time.unpause();
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        // The config is inserted before game plugins are built (see app.rs).
        let world = new_world(app.world().resource::<GameConfig>());
        app.insert_resource(world);
        app.add_plugins((
            environment::EnvironmentPlugin,
            materials::BlockMaterialsPlugin,
            interaction::InteractionPlugin,
            destruction::DestructionPlugin,
            sky::SkyPlugin,
        ))
        .init_resource::<ChunkEntities>()
        .init_resource::<LoadProgress>()
        .init_resource::<GenTasks>()
        .init_resource::<MeshTasks>()
        .init_resource::<GenerationFinished>()
        .init_resource::<MeshingStarted>()
        .init_resource::<PendingChunkPatches>()
        .add_message::<WorldReady>()
        .add_systems(
            OnEnter(AppState::Loading),
            (start_generation, pause_physics),
        )
        .add_systems(OnEnter(AppState::InGame), resume_physics)
        .add_systems(
            Update,
            (poll_generation, start_meshing, poll_meshing)
                .chain()
                .run_if(in_state(AppState::Loading)),
        )
        .add_systems(PostUpdate, remesh_dirty.run_if(in_state(AppState::InGame)));
    }
}
