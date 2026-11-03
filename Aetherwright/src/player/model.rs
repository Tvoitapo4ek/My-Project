//! Visual model of the player: a chunky Hytale-like explorer built from
//! pixel boxes (docs/STYLE_GUIDE.md) with a simple walk cycle. Replaced by
//! `models/characters/explorer.glb` when that file exists. Visible only in
//! third person.

use bevy::prelude::*;

use super::{
    Player,
    camera::{CameraMode, CameraRig},
    movement::CharacterState,
};

#[derive(Component)]
pub struct PlayerModel;

/// A limb that swings while walking (pivot at its top).
#[derive(Component)]
pub struct Swing {
    /// +1 / −1: opposite limbs swing in opposite phase.
    pub side: f32,
    pub amplitude: f32,
}

fn attach_model(
    mut commands: Commands,
    added: Query<Entity, Added<Player>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    style: Option<Res<crate::style::StyleMaterials>>,
) {
    let Some(style) = style else { return };
    for player in &added {
        // Style (docs/STYLE_GUIDE.md): chunky Hytale-like explorer — big
        // head with a painted face, tunic, belt, boots and a backpack with
        // an aether core. Body centre is 0.9 m above the feet; front is −Z.
        let core = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(1.0, 3.0, 4.0)), unlit: true, ..default() });
        let mut bx = |x: f32, y: f32, z: f32| meshes.add(crate::style::pixel_box(Vec3::new(x, y, z)));
        let m = |k: &str| MeshMaterial3d(style.get(k));
        let pv = crate::models::PrimitiveVisual;
        let parts: Vec<(Handle<Mesh>, MeshMaterial3d<StandardMaterial>, Vec3)> = vec![
            (bx(0.5, 0.56, 0.3), m("char_tunic"), Vec3::new(0.0, 0.02, 0.0)),
            (bx(0.54, 0.09, 0.33), m("char_leather"), Vec3::new(0.0, -0.22, 0.0)),
            (bx(0.1, 0.08, 0.04), m("gold_trim"), Vec3::new(0.0, -0.22, -0.17)),
            (bx(0.46, 0.46, 0.46), m("char_skin"), Vec3::new(0.0, 0.54, 0.0)),
            (bx(0.44, 0.44, 0.01), m("char_face"), Vec3::new(0.0, 0.54, -0.236)),
            (bx(0.5, 0.16, 0.5), m("char_hair"), Vec3::new(0.0, 0.76, 0.01)),
            (bx(0.5, 0.32, 0.08), m("char_hair"), Vec3::new(0.0, 0.62, 0.23)),
            (bx(0.4, 0.48, 0.2), m("char_leather"), Vec3::new(0.0, 0.04, 0.25)),
            (bx(0.44, 0.08, 0.22), m("brass"), Vec3::new(0.0, 0.3, 0.25)),
        ];
        let core_mesh = bx(0.12, 0.12, 0.12);
        let leg = bx(0.22, 0.5, 0.24);
        let boot = bx(0.24, 0.16, 0.3);
        let arm = bx(0.17, 0.5, 0.2);
        let hand = bx(0.17, 0.12, 0.2);
        let model = commands
            .spawn((
                Name::new("Player Model"),
                PlayerModel,
                Transform::default(),
                Visibility::Hidden,
                crate::models::ModelSlot {
                    category: "characters",
                    key: "explorer".into(),
                },
                crate::models::AnimationRequest("idle".into()),
            ))
            .with_children(|mm| {
                for (mesh, mat, at) in parts {
                    mm.spawn((Mesh3d(mesh), mat, Transform::from_translation(at), pv));
                }
                mm.spawn((Mesh3d(core_mesh), MeshMaterial3d(core.clone()), Transform::from_xyz(0.0, 0.08, 0.36), pv));
                // Legs: pivot at the hip.
                for (x, side) in [(-0.12f32, 1.0f32), (0.12, -1.0)] {
                    mm.spawn((Transform::from_xyz(x, -0.26, 0.0), Visibility::default(), Swing { side, amplitude: 0.6 }))
                        .with_children(|l| {
                            l.spawn((Mesh3d(leg.clone()), m("char_pants"), Transform::from_xyz(0.0, -0.25, 0.0), pv));
                            l.spawn((Mesh3d(boot.clone()), m("char_leather"), Transform::from_xyz(0.0, -0.56, -0.03), pv));
                        });
                }
                // Arms: pivot at the shoulder, swing against the legs.
                for (x, side) in [(-0.34f32, -1.0f32), (0.34, 1.0)] {
                    mm.spawn((Transform::from_xyz(x, 0.27, 0.0), Visibility::default(), Swing { side, amplitude: 0.5 }))
                        .with_children(|l| {
                            l.spawn((Mesh3d(arm.clone()), m("char_tunic"), Transform::from_xyz(0.0, -0.22, 0.0), pv));
                            l.spawn((Mesh3d(hand.clone()), m("char_skin"), Transform::from_xyz(0.0, -0.52, 0.0), pv));
                        });
                }
            })
            .id();
        commands.entity(player).add_child(model);
    }
}

/// Walk cycle for the primitive model (a glTF model plays its own clips).
fn swing_limbs(
    time: Res<Time>,
    player: Query<&avian3d::prelude::LinearVelocity, With<Player>>,
    mut limbs: Query<(&Swing, &mut Transform)>,
    mut phase: Local<f32>,
) {
    let Ok(vel) = player.single() else { return };
    let speed = Vec2::new(vel.x, vel.z).length();
    *phase += time.delta_secs() * (2.0 + speed * 1.6);
    let k = (speed / 4.0).clamp(0.0, 1.0);
    for (s, mut t) in &mut limbs {
        t.rotation = Quat::from_rotation_x((*phase).sin() * s.amplitude * k * s.side);
    }
}

fn update_model(
    rig: Res<CameraRig>,
    player: Query<(&CharacterState, &avian3d::prelude::LinearVelocity), With<Player>>,
    mut model: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut crate::models::AnimationRequest,
        ),
        With<PlayerModel>,
    >,
) {
    let Ok((state, vel)) = player.single() else {
        return;
    };
    let speed = Vec2::new(vel.x, vel.z).length();
    let clip = if speed > 6.5 {
        "run"
    } else if speed > 0.6 {
        "walk"
    } else {
        "idle"
    };
    for (_, _, mut a) in &mut model {
        if a.0 != clip {
            a.0 = clip.to_string();
        }
    }
    for (mut t, mut vis, _) in &mut model {
        *vis = if rig.mode == CameraMode::ThirdPerson {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        t.rotation = Quat::from_rotation_y(rig.yaw);
        // Crouch squash keeps the feet on the ground.
        let s = state.height / super::PLAYER_HEIGHT;
        t.scale = Vec3::new(1.0, s, 1.0);
    }
}

pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach_model, update_model, swing_limbs));
    }
}
