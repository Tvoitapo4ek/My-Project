//! Core: app-wide states, configuration, cross-system messages and shared
//! resources. Every other module may depend on `core`; `core` depends on none.

pub mod config;
pub mod events;
pub mod resources;
pub mod states;

use bevy::prelude::*;

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            states::StatesPlugin,
            events::EventsPlugin,
            resources::ResourcesPlugin,
        ));
    }
}
