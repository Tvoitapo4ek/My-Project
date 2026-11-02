//! The world: environment (sun, sky, fog) and terrain.

pub mod environment;
pub mod test_scene;

use bevy::prelude::*;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((environment::EnvironmentPlugin, test_scene::TestScenePlugin));
    }
}
