//! User interface: HUD panels, notifications and the debug overlay.

pub mod debug;
pub mod hud;
pub mod menus;
pub mod theme;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((hud::HudPlugin, debug::DebugUiPlugin, menus::MenusPlugin));
    }
}
