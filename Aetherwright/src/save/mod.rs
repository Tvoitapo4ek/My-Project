//! Persistence: versioned save files for the world and the player.

pub mod serialization;
pub mod world_save;

use bevy::prelude::*;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(world_save::WorldSavePlugin);
    }
}
