//! Physics collision layers shared by all subsystems.

use avian3d::prelude::*;

/// Physics layers used across the game.
#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    #[default]
    Default,
    /// Static voxel terrain.
    Terrain,
    Player,
    /// Free dynamic props, parts and machines.
    Prop,
    /// Dropped items (collide with terrain/props, not with the player).
    Item,
    Creature,
    Projectile,
}

