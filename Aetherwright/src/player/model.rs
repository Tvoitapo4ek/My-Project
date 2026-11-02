//! Visual model of the player built from simple primitives (art direction:
//! readable silhouette, muted suit, bright aether accents). Visible only in
//! third person.

use bevy::prelude::*;

use super::{
    Player,
    camera::{CameraMode, CameraRig},
    movement::CharacterState,
};

#[derive(Component)]
pub struct PlayerModel;

fn attach_model(
    mut commands: Commands,
    added: Query<Entity, Added<Player>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for player in &added {
        let suit = materials.add(StandardMaterial {
            base_color: Color::srgb(0.27, 0.31, 0.36),
            perceptual_roughness: 0.8,
            ..default()
        });
        let leather = materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.30, 0.20),
            perceptual_roughness: 0.9,
            ..default()
        });
        let skin = materials.add(StandardMaterial {
            base_color: Color::srgb(0.80, 0.64, 0.52),
            ..default()
        });
        let glow = materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.9, 1.0),
            emissive: LinearRgba::rgb(0.6, 3.0, 4.0),
            ..default()
        });
        let torso = meshes.add(Capsule3d::new(0.28, 0.55));
        let head = meshes.add(Sphere::new(0.21).mesh().ico(3).unwrap());
        let pack = meshes.add(Cuboid::new(0.42, 0.5, 0.2));
        let core = meshes.add(Sphere::new(0.08).mesh().ico(2).unwrap());
        let visor = meshes.add(Cuboid::new(0.28, 0.07, 0.05));
        let leg = meshes.add(Capsule3d::new(0.1, 0.5));

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
            .with_children(|m| {
                m.spawn((
                    Mesh3d(torso),
                    MeshMaterial3d(suit.clone()),
                    Transform::from_xyz(0.0, 0.05, 0.0),
                    crate::models::PrimitiveVisual,
                ));
                m.spawn((
                    Mesh3d(head),
                    MeshMaterial3d(skin),
                    Transform::from_xyz(0.0, 0.66, 0.0),
                    crate::models::PrimitiveVisual,
                ));
                m.spawn((
                    Mesh3d(visor),
                    MeshMaterial3d(glow.clone()),
                    Transform::from_xyz(0.0, 0.7, -0.19),
                    crate::models::PrimitiveVisual,
                ));
                m.spawn((
                    Mesh3d(pack),
                    MeshMaterial3d(leather),
                    Transform::from_xyz(0.0, 0.18, 0.27),
                    crate::models::PrimitiveVisual,
                ));
                m.spawn((
                    Mesh3d(core),
                    MeshMaterial3d(glow),
                    Transform::from_xyz(0.0, 0.2, 0.39),
                    crate::models::PrimitiveVisual,
                ));
                for x in [-0.13, 0.13] {
                    m.spawn((
                        Mesh3d(leg.clone()),
                        MeshMaterial3d(suit.clone()),
                        Transform::from_xyz(x, -0.6, 0.0),
                        crate::models::PrimitiveVisual,
                    ));
                }
            })
            .id();
        commands.entity(player).add_child(model);
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
        app.add_systems(Update, (attach_model, update_model));
    }
}
