//! Hand tools. Each tool reads the shared clicks (`interact::Clicks`) when
//! it is the held item.
//! * Wrench — RMB turns a block 90° about the clicked face (like Create);
//!   LMB opens the block's settings; LMB on a control panel then on a
//!   sensor links them.
//! * Glue, gravity gun, physgun — `glue`, `guns`.

pub mod glue;
pub mod guns;
pub mod viewmodel;

use bevy::prelude::*;

use crate::{
    content::{Mech, Placement, Tool},
    core::{Toast, Ui, toast},
    grid::{Cell, VoxelGrid, rotate_facing},
    interact::{Clicks, Target},
    inventory::Inventory,
    mech::BlockEntity,
    player::{Body, Player},
    settings::Editing,
    signal::{LinkMode, Linked, PanelRef},
};

pub fn held_tool(inv: &Inventory) -> Option<Tool> {
    inv.held().and_then(|s| s.item.tool())
}

#[allow(clippy::too_many_arguments)]
fn wrench(
    clicks: Res<Clicks>,
    target: Res<Target>,
    mut players: Query<(&Inventory, &mut Body), With<Player>>,
    mut grids: Query<&mut VoxelGrid>,
    blocks: Query<&BlockEntity>,
    mut sensors: Query<&mut Linked>,
    mut link: ResMut<LinkMode>,
    mut editing: ResMut<Editing>,
    mut next: ResMut<NextState<Ui>>,
    mut toasts: MessageWriter<Toast>,
) {
    let Ok((inv, mut body)) = players.single_mut() else { return };
    if held_tool(inv) != Some(Tool::Wrench) || !(clicks.left || clicks.right) {
        return;
    }
    let Some(a) = target.hit else { return };
    let Ok(mut grid) = grids.get_mut(a.grid) else { return };
    let cell = grid.get(a.cell);
    if cell.is_air() {
        return;
    }
    body.swing = 1.0;
    let def = cell.id.def();
    if clicks.right {
        // Turn about the clicked face's axis; upright blocks only about Y.
        if def.placement == Placement::None || a.normal == IVec3::ZERO {
            return;
        }
        let axis = if def.placement == Placement::Horizontal { IVec3::Y } else { a.normal };
        let f = rotate_facing(cell.facing, axis);
        if f != cell.facing {
            grid.set(a.cell, Cell::new(cell.id, f));
        }
        return;
    }
    let entity = grid.entities.get(&a.cell).copied();
    match def.mech {
        Some(Mech::ControlPanel) => {
            link.0 = Some(PanelRef { grid: a.grid, cell: a.cell });
            toasts.write(toast("Пульт выбран: теперь ЛКМ ключом по датчику, чтобы связать"));
        }
        Some(Mech::Sensor) if link.0.is_some() => {
            if let Some(e) = entity
                && let Ok(mut l) = sensors.get_mut(e)
            {
                l.0 = link.0.take();
                toasts.write(toast("Датчик связан с пультом — выберите клавиши"));
                editing.0 = Some(e);
                next.set(Ui::Settings);
            }
        }
        _ if !def.params.is_empty() => {
            if let Some(e) = entity.filter(|e| blocks.contains(*e)) {
                editing.0 = Some(e);
                next.set(Ui::Settings);
            }
        }
        _ => {
            toasts.write(toast(format!("{}: нет настроек", def.name)));
        }
    }
}

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, wrench.after(crate::interact::InteractSet))
            .add_plugins((glue::GluePlugin, guns::GunsPlugin, viewmodel::ViewmodelPlugin));
    }
}
