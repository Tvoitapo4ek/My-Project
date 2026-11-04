//! The player: a Minecraft-sized character (0.6 × 1.8 m) driven by a
//! dynamic capsule. Creative only: flying toggles with a double tap of Space.
//!
//! Controls: WASD move, Ctrl sprint, Shift crouch (does not fall off block
//! edges) / fly down, Space jump / fly up, double Space toggles flight,
//! F5 camera (first person / behind / in front).

pub mod camera;
pub mod model;
pub mod movement;

use avian3d::prelude::*;
use bevy::prelude::*;

pub const HEIGHT: f32 = 1.8;
pub const CROUCH_HEIGHT: f32 = 1.5;
pub const RADIUS: f32 = 0.3;
/// Eye height above the feet.
pub const EYE: f32 = 1.62;
pub const CROUCH_EYE: f32 = 1.27;

#[derive(Component)]
pub struct Player;

/// Player look and move state.
#[derive(Component, Debug, Clone)]
pub struct Body {
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub crouching: bool,
    pub sprinting: bool,
    pub flying: bool,
    /// Horizontal speed (m/s) for animations.
    pub speed: f32,
    /// Arm swing timer (break / place / use), 0..1.
    pub swing: f32,
    /// Time since the last Space press (double-tap → flight).
    pub last_jump_press: f32,
}

impl Default for Body {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.0, grounded: false, crouching: false, sprinting: false, flying: false, speed: 0.0, swing: 0.0, last_jump_press: 10.0 }
    }
}

impl Body {
    pub fn height(&self) -> f32 {
        if self.crouching { CROUCH_HEIGHT } else { HEIGHT }
    }
    pub fn eye(&self) -> f32 {
        if self.crouching { CROUCH_EYE } else { EYE }
    }
    pub fn look(&self) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0) * Vec3::NEG_Z
    }
}

/// While seated the player follows the seat (set by `vehicle`).
#[derive(Component, Debug, Clone, Copy)]
pub struct Seated {
    pub seat: Entity,
}

pub fn collider(height: f32) -> Collider {
    Collider::capsule(RADIUS, height - 2.0 * RADIUS)
}

fn spawn_player(mut commands: Commands) {
    // Position = capsule centre.
    let p = crate::world::SPAWN + Vec3::Y * (HEIGHT * 0.5 + 0.05);
    commands.spawn((
        Name::new("Player"),
        Player,
        Body::default(),
        crate::inventory::starting(),
        Transform::from_translation(p),
        Visibility::default(),
        RigidBody::Dynamic,
        collider(HEIGHT),
        LockedAxes::ROTATION_LOCKED,
        Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
        Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
        GravityScale(2.6),
        Mass(70.0),
        SleepingDisabled,
        LinearDamping(0.0),
    ));
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player).add_plugins((movement::MovementPlugin, camera::CameraPlugin, model::ModelPlugin));
    }
}
