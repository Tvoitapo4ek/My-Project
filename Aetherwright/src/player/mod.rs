//! The player avatar: physical body, input intent, movement and camera.

pub mod camera;
pub mod input;
pub mod model;
pub mod movement;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::core::{layers::Layer, resources::SpawnPoint, states::AppState};

/// Marker for the player's physical body (root entity).
#[derive(Component, Debug)]
pub struct Player;

pub const PLAYER_RADIUS: f32 = 0.35;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const PLAYER_CROUCH_HEIGHT: f32 = 1.3;
pub const EYE_FROM_TOP: f32 = 0.16;

pub fn player_collider(height: f32) -> Collider {
    Collider::capsule(PLAYER_RADIUS, height - 2.0 * PLAYER_RADIUS)
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
                input::InputPlugin,
                movement::MovementPlugin,
                camera::CameraPlugin,
                model::ModelPlugin,
            ))
            .add_systems(OnEnter(AppState::InGame), spawn_player);
    }
}

fn spawn_player(mut commands: Commands, spawn: Res<SpawnPoint>, existing: Query<(), With<Player>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("Player"),
        Player,
        crate::items::inventory::starter_inventory(),
        crate::tools::manipulator::Manipulator::default(),
        crate::items::drops::Picker,
        movement::CharacterState::default(),
        Transform::from_translation(spawn.0),
        Visibility::default(),
        RigidBody::Dynamic,
        player_collider(PLAYER_HEIGHT),
        LockedAxes::ROTATION_LOCKED,
        // Zero friction against walls; the controller handles ground braking.
        Friction::new(0.0).with_combine_rule(CoefficientCombine::Min),
        Restitution::new(0.0).with_combine_rule(CoefficientCombine::Min),
        (
            Mass(80.0),
            GravityScale(1.0),
            SleepingDisabled,
            SweptCcd::default(),
        ),
        CollisionLayers::new(
            Layer::Player,
            [Layer::Default, Layer::Terrain, Layer::Prop, Layer::Creature],
        ),
    ));
}
