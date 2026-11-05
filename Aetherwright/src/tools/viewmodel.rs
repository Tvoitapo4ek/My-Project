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
    style: Option<Res<crate::style::StyleMaterials>>,
) {
    let Ok(cam) = camera.single() else { return };
    let Some(style) = style else { return };
    // Style: a chunky Create-like tool — brass body, andesite barrel,
    // copper bands, wooden grip and a glowing crystal core.
    let core = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(1.2, 3.6, 4.8)),
        unlit: true,
        ..default()
    });
    commands.insert_resource(CoreMaterial(core.clone()));
    let mut boxed = |size: Vec3| meshes.add(crate::style::pixel_box(size));
    let parts = [
        (Vec3::new(0.15, 0.13, 0.32), "brass_casing", Vec3::ZERO),
        (Vec3::new(0.08, 0.08, 0.26), "andesite", Vec3::new(0.0, 0.02, -0.27)),
        (Vec3::new(0.11, 0.11, 0.04), "copper", Vec3::new(0.0, 0.02, -0.2)),
        (Vec3::new(0.11, 0.11, 0.04), "copper", Vec3::new(0.0, 0.02, -0.33)),
        (Vec3::new(0.08, 0.16, 0.09), "cogwood", Vec3::new(0.0, -0.12, 0.07)),
        (Vec3::new(0.1, 0.04, 0.1), "gold_trim", Vec3::new(0.0, 0.085, 0.03)),
    ];
    let meshes_mats: Vec<_> = parts.iter().map(|(sz, m, at)| (boxed(*sz), style.get(m), *at)).collect();
    let core_mesh = boxed(Vec3::splat(0.07));
    let vm = commands
        .spawn((
            ViewModel,
            Transform::from_xyz(0.36, -0.32, -0.62).with_scale(Vec3::splat(0.75)),
            Visibility::default(),
        ))
        .with_children(|p| {
            for (mesh, mat, at) in meshes_mats {
                p.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(at)));
            }
            p.spawn((
                ViewCore,
                Mesh3d(core_mesh),
                MeshMaterial3d(core),
                Transform::from_xyz(0.0, 0.13, 0.03).with_rotation(Quat::from_rotation_y(0.78)),
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
    // Unlit HDR colour (blooms); touch the asset only when it changes.
    let want = Color::LinearRgba(color.to_linear() * 3.0);
    if let Some(core) = core
        && materials.get(&core.0).is_some_and(|m| m.base_color != want)
        && let Some(mut m) = materials.get_mut(&core.0)
    {
        m.base_color = want;
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
