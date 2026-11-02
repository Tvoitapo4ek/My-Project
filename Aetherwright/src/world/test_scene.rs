//! Stage-1 test polygon: flat ground, steps, a ramp and a few loose crates.
//! Replaced by voxel terrain in stage 2.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    core::states::AppState,
    player::{Layer, SpawnPoint},
};

fn spawn_test_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut spawn: ResMut<SpawnPoint>,
    mut next: ResMut<NextState<AppState>>,
) {
    let ground_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.5, 0.3),
        perceptual_roughness: 0.95,
        ..default()
    });
    let stone_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.5, 0.52),
        perceptual_roughness: 0.9,
        ..default()
    });
    let crate_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.42, 0.24),
        ..default()
    });
    let terrain_layers = CollisionLayers::new(Layer::Terrain, LayerMask::ALL);

    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(Cuboid::new(80.0, 1.0, 80.0))),
        MeshMaterial3d(ground_mat),
        Transform::from_xyz(0.0, -0.5, 0.0),
        RigidBody::Static,
        Collider::cuboid(80.0, 1.0, 80.0),
        terrain_layers,
    ));
    // Steps of 1 block to test auto-step.
    for i in 0..4 {
        let h = (i + 1) as f32;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(2.0, h, 2.0))),
            MeshMaterial3d(stone_mat.clone()),
            Transform::from_xyz(6.0 + i as f32 * 2.0, h * 0.5, -6.0),
            RigidBody::Static,
            Collider::cuboid(2.0, h, 2.0),
            terrain_layers,
        ));
    }
    // Ramp.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(4.0, 0.5, 10.0))),
        MeshMaterial3d(stone_mat),
        Transform::from_xyz(-8.0, 1.5, -6.0).with_rotation(Quat::from_rotation_x(0.35)),
        RigidBody::Static,
        Collider::cuboid(4.0, 0.5, 10.0),
        terrain_layers,
    ));
    // Loose crates to push around.
    for i in 0..6 {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(crate_mat.clone()),
            Transform::from_xyz(-3.0 + i as f32 * 1.2, 0.5 + (i % 2) as f32 * 2.0, 4.0),
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0, 1.0),
            CollisionLayers::new(Layer::Prop, LayerMask::ALL),
        ));
    }
    spawn.0 = Vec3::new(0.0, 1.5, 0.0);
    next.set(AppState::InGame);
}

pub struct TestScenePlugin;

impl Plugin for TestScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Loading), spawn_test_scene);
    }
}
