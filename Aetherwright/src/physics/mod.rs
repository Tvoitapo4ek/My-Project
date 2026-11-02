//! Physical objects that are not terrain: loose blocks, props, and the
//! connections (joints) between them, plus the physics gun.
//!
//! Every free object is a real Avian rigid body. Ordinary terrain blocks are
//! never simulated individually — only blocks the player cuts out or that
//! lose support become bodies.

pub mod collision;
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

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            rigid_bodies::RigidBodiesPlugin,
            joints::JointsPlugin,
            physics_gun::PhysicsGunPlugin,
            collision::CollisionPlugin,
        ));
    }
}
