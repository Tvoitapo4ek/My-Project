//! Hand tools. Each tool reads the shared clicks (`interact::Clicks`) when
//! it is the held item.
//! * Wrench — RMB turns a block 90° about the clicked face (like Create);
//!   LMB opens the block's settings; LMB on a control panel then on a
//!   sensor links them.
//! * Glue, gravity gun, physgun — `glue`, `guns`.

pub mod blueprint;
pub mod glue;
pub mod guns;
pub mod hand;
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
    mut buses: Query<&mut crate::logic::Bus>,
    wire_color: Res<crate::logic::WireColor>,
    mut sensors: Query<&mut Linked>,
    mut link: ResMut<LinkMode>,
    mut editing: ResMut<Editing>,
    mut next: ResMut<NextState<Ui>>,
    mut toasts: MessageWriter<Toast>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
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
    if clicks.right && body.crouching {
        // Shift + RMB: take the casing off a cased shaft / pipe.
        let bare = match def.mech {
            Some(Mech::CasedShaft) => Some("shaft"),
            Some(Mech::CasedPipe) => Some("fluid_pipe"),
            _ => None,
        };
        if let Some(k) = bare {
            grid.set(a.cell, Cell::new(crate::content::BlockId::k(k), cell.facing));
            sounds.write(crate::sound::sfx("rotate"));
        }
        return;
    }
    if clicks.right {
        // Turn about the clicked face's axis; upright blocks only about Y.
        if def.placement == Placement::None || a.normal == IVec3::ZERO {
            return;
        }
        let axis = if def.placement == Placement::Horizontal { IVec3::Y } else { a.normal };
        let f = rotate_facing(cell.facing, axis);
        if f != cell.facing {
            grid.set(a.cell, Cell::new(cell.id, f));
            sounds.write(crate::sound::sfx("rotate"));
        }
        return;
    }
    let entity = grid.entities.get(&a.cell).copied();
    let here = PanelRef { grid: a.grid, cell: a.cell };
    match (def.mech, link.0) {
        (Some(Mech::Wire), _) => {
            // Take the chosen colour out of the bus.
            if let Some(e) = entity
                && let Ok(mut b) = buses.get_mut(e)
            {
                let k = wire_color.0;
                if b.0 & (1 << k) == 0 {
                    toasts.write(toast(format!("{} в этой шине нет (Shift + колесо с проводом — выбрать цвет)", crate::logic::COLORS[k].0)));
                } else if b.0.count_ones() == 1 {
                    toasts.write(toast("Последний провод шины — сломайте блок"));
                } else {
                    b.0 &= !(1 << k);
                    toasts.write(toast(format!("{} провод убран из шины", crate::logic::COLORS[k].0)));
                    sounds.write(crate::sound::sfx("ui_click"));
                }
            }
        }
        (Some(Mech::ControlPanel), _) => {
            link.0 = Some((here, Mech::ControlPanel));
            toasts.write(toast("Пульт выбран: теперь ЛКМ ключом по датчику, чтобы связать"));
            sounds.write(crate::sound::sfx("ui_click"));
        }
        (Some(Mech::Sensor), Some((panel, Mech::ControlPanel))) => {
            if let Some(e) = entity
                && let Ok(mut l) = sensors.get_mut(e)
            {
                l.0 = Some(panel);
                link.0 = None;
                toasts.write(toast("Датчик связан с пультом — выберите клавиши"));
                sounds.write(crate::sound::sfx("link"));
                editing.0 = Some(e);
                next.set(Ui::Settings);
            }
        }
        (Some(Mech::TelemetrySensor), _) => {
            link.0 = Some((here, Mech::TelemetrySensor));
            toasts.write(toast("Датчик телеметрии выбран: теперь ЛКМ ключом по механическому экрану"));
            sounds.write(crate::sound::sfx("ui_click"));
        }
        (Some(Mech::MechanicalDisplay), Some((sensor, Mech::TelemetrySensor))) => {
            let s = grids.get(sensor.grid).ok().and_then(|g| g.entities.get(&sensor.cell).copied());
            if let Some(Ok(mut l)) = s.map(|s| sensors.get_mut(s)) {
                l.0 = Some(here);
                toasts.write(toast("Экран связан с датчиком телеметрии"));
                sounds.write(crate::sound::sfx("link"));
            }
            link.0 = None;
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
            .add_plugins((glue::GluePlugin, guns::GunsPlugin, viewmodel::ViewmodelPlugin, blueprint::BlueprintPlugin, hand::HandPlugin));
    }
}
