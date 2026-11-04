//! Camera: first person, third person behind, third person in front (F5).

use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
};

use super::{Body, Player};
use crate::core::Ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    First,
    Behind,
    Front,
}

#[derive(Resource, Default)]
pub struct CameraView(pub View);

#[derive(Component)]
pub struct MainCamera;

/// The physgun rotates held objects with the mouse while E is held.
#[derive(Resource, Default)]
pub struct LookLocked(pub bool);

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Camera"),
        MainCamera,
        Camera3d::default(),
        IsDefaultUiCamera,
        Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, far: 800.0, ..default() }),
        Tonemapping::TonyMcMapface,
        // Soft contact shadows where parts meet (needs MSAA off).
        bevy::pbr::ScreenSpaceAmbientOcclusion { quality_level: bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Medium, ..default() },
        Msaa::Off,
        bevy::camera::Hdr,
        bevy::post_process::bloom::Bloom { intensity: 0.15, ..bevy::post_process::bloom::Bloom::NATURAL },
        Transform::from_xyz(0.0, 3.0, 6.0),
    ));
}

fn mouse_look(motion: Res<AccumulatedMouseMotion>, ui: Res<State<Ui>>, locked: Res<LookLocked>, mut q: Query<&mut Body, With<Player>>) {
    if *ui.get() != Ui::Game || locked.0 {
        return;
    }
    for mut b in &mut q {
        b.yaw -= motion.delta.x * 0.0025;
        b.pitch = (b.pitch - motion.delta.y * 0.0025).clamp(-1.55, 1.55);
    }
}

fn toggle_view(keys: Res<ButtonInput<KeyCode>>, ui: Res<State<Ui>>, mut view: ResMut<CameraView>) {
    if *ui.get() == Ui::Game && keys.just_pressed(KeyCode::F5) {
        view.0 = match view.0 {
            View::First => View::Behind,
            View::Behind => View::Front,
            View::Front => View::First,
        };
    }
}

fn follow(view: Res<CameraView>, players: Query<(Entity, &GlobalTransform, &Body), With<Player>>, mut cams: Query<&mut Transform, With<MainCamera>>, spatial: avian3d::prelude::SpatialQuery) {
    let (Ok((me, pt, body)), Ok(mut cam)) = (players.single(), cams.single_mut()) else { return };
    let feet = pt.translation() - Vec3::Y * body.height() * 0.5;
    let eye = feet + Vec3::Y * body.eye();
    let rot = Quat::from_euler(EulerRot::YXZ, body.yaw, body.pitch, 0.0);
    let look = rot * Vec3::NEG_Z;
    let (pos, facing) = match view.0 {
        View::First => (eye, rot),
        View::Behind | View::Front => {
            let back = if view.0 == View::Behind { -look } else { look };
            // Keep the camera out of walls.
            let max = 4.0;
            let dist = spatial
                .cast_ray(eye, Dir3::new(back).unwrap_or(Dir3::Z), max, true, &avian3d::prelude::SpatialQueryFilter::default().with_excluded_entities([me]))
                .map(|h| (h.distance - 0.2).max(0.3))
                .unwrap_or(max);
            let p = eye + back * dist;
            (p, Transform::from_translation(p).looking_at(eye, Vec3::Y).rotation)
        }
    };
    cam.translation = pos;
    cam.rotation = facing;
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraView>()
            .init_resource::<LookLocked>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, (mouse_look, toggle_view))
            .add_systems(PostUpdate, follow.after(avian3d::prelude::PhysicsSystems::Writeback).before(TransformSystems::Propagate));
    }
}
