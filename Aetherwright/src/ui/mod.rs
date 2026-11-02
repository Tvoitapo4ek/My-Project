//! User interface: HUD panels, notifications and the debug overlay.

pub mod build_menu;
pub mod debug;
pub mod hud;
pub mod inventory;
pub mod slots;
pub mod menus;
pub mod theme;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, theme::load_fonts)
            .add_systems(PostUpdate, theme::apply_font)
            .add_plugins((hud::HudPlugin, debug::DebugUiPlugin, menus::MenusPlugin, slots::SlotsPlugin, inventory::InventoryUiPlugin, build_menu::BuildMenuPlugin));
    }
}
