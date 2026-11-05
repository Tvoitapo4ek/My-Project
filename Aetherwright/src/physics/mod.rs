//! Physical objects that are not terrain: loose blocks, props, and the
//! connections (joints) between them, plus the physics gun.
//!
//! Every free object is a real Avian rigid body. Ordinary terrain blocks are
//! never simulated individually — only blocks the player cuts out or that
//! lose support become bodies.

pub mod collision;
pub mod impulses;
pub mod joints;
pub mod physics_gun;
pub mod rigid_bodies;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What a free physical object is (used for saving, collecting, scanning).
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Prop {
    /// A voxel block turned into a body (block key).
    Block(String),
    /// A functional part / station (part key, see `machinery`).
    Part(String),
}

/// Bodies the physics gun must not move (system objects).
#[derive(Component, Debug)]
pub struct Immovable;

/// Frozen in place by the physics gun (static until grabbed again).
#[derive(Component, Debug)]
pub struct Frozen;

/// Safety limits for every body: bounded speculative margin and speeds.
/// Without them a body that gets violently ejected (e.g. snapped into
/// overlap) expands its AABB across many voxel chunks and stalls the
/// narrow phase.
fn add_limits(
    mut commands: Commands,
    added: Query<
        Entity,
        (
            Added<avian3d::prelude::RigidBody>,
            Without<avian3d::prelude::SpeculativeCcd>,
        ),
    >,
) {
    use avian3d::prelude::*;
    for e in &added {
        commands.entity(e).insert((
            SpeculativeCcd::new(1.0),
            MaxLinearSpeed(80.0),
            MaxAngularSpeed(60.0),
        ));
    }
}

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, add_limits).add_plugins((
            rigid_bodies::RigidBodiesPlugin,
            joints::JointsPlugin,
            physics_gun::PhysicsGunPlugin,
            collision::CollisionPlugin,
            impulses::ImpulsesPlugin,
        ));
    }
}
