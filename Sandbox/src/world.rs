//! The world: a flat white platform floating in the void.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    content::BlockId,
    grid::{Cell, VoxelGrid, WorldGrid},
};

/// Half size of the platform (blocks); the top of the floor is at y = 0.
pub const PLATFORM_HALF: i32 = 64;

/// Where the player appears.
pub const SPAWN: Vec3 = Vec3::new(0.5, 1.0, 0.5);

/// The world grid entity.
#[derive(Resource)]
pub struct WorldEntity(pub Entity);

fn spawn_world(mut commands: Commands) {
    let mut grid = VoxelGrid::default();
    let floor = BlockId::k("white_block");
    for x in -PLATFORM_HALF..PLATFORM_HALF {
        for z in -PLATFORM_HALF..PLATFORM_HALF {
            grid.set(IVec3::new(x, -1, z), Cell::new(floor, 2));
        }
    }
    let e = commands
        .spawn((Name::new("World grid"), WorldGrid, grid, Transform::default(), Visibility::default(), RigidBody::Static))
        .id();
    commands.insert_resource(WorldEntity(e));
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_world);
    }
}
