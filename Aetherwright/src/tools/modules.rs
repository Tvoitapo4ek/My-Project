//! Manipulator modules: holding a module item and pressing RMB installs it
//! (the item is consumed). Modules extend what the manipulator can do.

use bevy::prelude::*;

use super::{manipulator::Manipulator, ActiveTool};
use crate::{
    core::{events::Notify, states::gameplay_active},
    items::{inventory::Inventory, Use},
    player::Player,
};

/// Applies a module to the manipulator. Returns false if already installed.
pub fn install(m: &mut Manipulator, module: &str) -> bool {
    match module {
        "drill2" if m.drill_tier < 2 => {
            m.drill_tier = 2;
            true
        }
        "cutter" if !m.cutter => {
            m.cutter = true;
            true
        }
        _ => false,
    }
}

fn install_module(
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    mut player: Query<(&mut Inventory, &mut Manipulator), With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let ActiveTool::Module(item) = *tool else { return };
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Ok((mut inv, mut m)) = player.single_mut() else { return };
    let Use::Module(key) = item.def().use_ else { return };
    if install(&mut m, key) {
        inv.remove(item, 1);
        notify.write(Notify::ok(format!("Установлен: {}", item.def().name)));
    } else {
        notify.write(Notify::warn("Модуль уже установлен"));
    }
}

pub struct ModulesPlugin;

impl Plugin for ModulesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, install_module.run_if(gameplay_active));
    }
}
