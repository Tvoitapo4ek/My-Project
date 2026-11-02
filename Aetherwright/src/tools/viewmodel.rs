//! First-person view model: the device on the player's arm. Its colour
//! (energy core) follows the active tool / manipulator mode, and it recoils
//! slightly while mining.

use bevy::prelude::*;

use super::{
    ActiveTool,
    manipulator::{Manipulator, ManipulatorMode, MiningProgress},
};
use crate::{
    core::resources::MainCamera,
    player::{
        Player,
        camera::{CameraMode, CameraRig},
    },
};

#[derive(Component)]
struct ViewModel;

#[derive(Component)]
struct ViewCore;

#[derive(Resource)]
struct CoreMaterial(Handle<StandardMaterial>);

fn spawn_viewmodel(
    mut commands: Commands,
    camera: Query<Entity, Added<MainCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(cam) = camera.single() else { return };
    let body = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.34, 0.38),
        metallic: 0.6,
        perceptual_roughness: 0.45,
        ..default()
    });
    let brass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.56, 0.3),
        metallic: 0.8,
        perceptual_roughness: 0.35,
        ..default()
    });
    let core = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.9, 1.0),
        emissive: LinearRgba::rgb(0.6, 3.0, 4.0),
        ..default()
    });
    commands.insert_resource(CoreMaterial(core.clone()));
    let vm = commands
        .spawn((
            ViewModel,
            Transform::from_xyz(0.36, -0.32, -0.62).with_scale(Vec3::splat(0.75)),
            Visibility::default(),
        ))
        .with_children(|p| {
            // Housing, barrel, brass rings and an exposed energy core.
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.14, 0.13, 0.34))),
                MeshMaterial3d(body.clone()),
                Transform::default(),
            ));
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.035, 0.22))),
                MeshMaterial3d(body),
                Transform::from_xyz(0.0, 0.02, -0.25)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));
            for z in [-0.2, -0.3] {
                p.spawn((
                    Mesh3d(meshes.add(Torus::new(0.035, 0.055))),
                    MeshMaterial3d(brass.clone()),
                    Transform::from_xyz(0.0, 0.02, z)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            }
            p.spawn((
                ViewCore,
                Mesh3d(meshes.add(Sphere::new(0.045).mesh().ico(2).unwrap())),
                MeshMaterial3d(core),
                Transform::from_xyz(0.0, 0.085, 0.03),
            ));
        })
        .id();
    commands.entity(cam).add_child(vm);
}

fn update_viewmodel(
    time: Res<Time>,
    rig: Res<CameraRig>,
    tool: Res<ActiveTool>,
    progress: Res<MiningProgress>,
    player: Query<&Manipulator, With<Player>>,
    core: Option<Res<CoreMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut vm: Query<(&mut Transform, &mut Visibility), With<ViewModel>>,
) {
    let color = match *tool {
        ActiveTool::Manipulator => player
            .single()
            .map(|m| m.mode.color())
            .unwrap_or(Color::WHITE),
        ActiveTool::PhysGun => Color::srgb(1.0, 0.65, 0.2),
        ActiveTool::Focus => Color::srgb(0.75, 0.45, 1.0),
        ActiveTool::Melee | ActiveTool::Ranged => Color::srgb(1.0, 0.35, 0.3),
        _ => ManipulatorMode::Mine.color(),
    };
    if let Some(core) = core
        && let Some(mut m) = materials.get_mut(&core.0)
    {
        m.base_color = color;
        m.emissive = color.to_linear() * 3.5;
    }
    for (mut t, mut vis) in &mut vm {
        *vis = if rig.mode == CameraMode::FirstPerson {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let shake = if progress.target.is_some() {
            (time.elapsed_secs() * 40.0).sin() * 0.006
        } else {
            0.0
        };
        t.translation = Vec3::new(0.36, -0.32 + shake, -0.62);
    }
}

pub struct ViewModelPlugin;

impl Plugin for ViewModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_viewmodel, update_viewmodel));
    }
}
