//! Control: seat → control panel → sensors → blocks.
//!
//! * A **sensor** is attached to the block it controls (it faces into it).
//!   With the wrench: LMB on a control panel selects it, LMB on a sensor
//!   links them. The sensor's settings choose a key for "+" and for "−".
//! * While the player sits on a **seat**, every control panel of the same
//!   grid within `PANEL_REACH` blocks of the seat reads the keyboard; each
//!   linked sensor sends `+1` / `−1` / `0` to its block:
//!   suspension → steering, clutch → stop, gearshift → reverse,
//!   speed controller → blend to its "signal speed".

use bevy::prelude::*;

use crate::{
    content::Mech,
    core::BINDABLE_KEYS,
    grid::VoxelGrid,
    mech::{BlockEntity, Params, Signal},
    player::{Player, Seated},
};

pub const PANEL_REACH: f32 = 3.5;

/// A control panel, by grid and cell (survives entity respawns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelRef {
    pub grid: Entity,
    pub cell: IVec3,
}

/// The panel a sensor listens to.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Linked(pub Option<PanelRef>);

/// Panel chosen with the wrench, waiting for a sensor.
#[derive(Resource, Default)]
pub struct LinkMode(pub Option<PanelRef>);

/// Panels that read the keyboard this frame.
#[derive(Resource, Default)]
pub struct ActivePanels(pub Vec<PanelRef>);

/// Keys held this frame (keyboard, or set by scripts).
#[derive(Resource, Default)]
pub struct PanelKeys {
    pub held: Vec<KeyCode>,
    pub scripted: bool,
}

fn read_keys(keys: Res<ButtonInput<KeyCode>>, mut pk: ResMut<PanelKeys>) {
    if pk.scripted {
        return;
    }
    pk.held = BINDABLE_KEYS.iter().filter_map(|(_, k)| *k).filter(|k| keys.pressed(*k)).collect();
}

fn attach_links(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Linked>)>) {
    for (e, b) in &added {
        if b.mech() == Some(Mech::Sensor) {
            commands.entity(e).insert(Linked::default());
        }
    }
}

/// Panels near the seat the player sits on.
fn find_active(players: Query<&Seated, With<Player>>, blocks: Query<(&BlockEntity, &GlobalTransform)>, grids: Query<&VoxelGrid>, mut active: ResMut<ActivePanels>) {
    active.0.clear();
    let Ok(seated) = players.single() else { return };
    let Ok((seat, _)) = blocks.get(seated.seat) else { return };
    let Ok(grid) = grids.get(seat.grid) else { return };
    for (&c, &e) in &grid.entities {
        if let Ok((b, _)) = blocks.get(e)
            && b.mech() == Some(Mech::ControlPanel)
            && c.as_vec3().distance(seat.cell.as_vec3()) <= PANEL_REACH
        {
            active.0.push(PanelRef { grid: seat.grid, cell: c });
        }
    }
}

/// Signal value of a sensor from the held keys.
pub fn sensor_value(key_pos: f32, key_neg: f32, held: &[KeyCode]) -> f32 {
    let pressed = |i: f32| BINDABLE_KEYS.get(i as usize).and_then(|k| k.1).is_some_and(|k| held.contains(&k));
    (pressed(key_pos) as i32 - pressed(key_neg) as i32) as f32
}

fn send_signals(active: Res<ActivePanels>, keys: Res<PanelKeys>, sensors: Query<(&BlockEntity, &Params, &Linked)>, grids: Query<&VoxelGrid>, mut targets: Query<&mut Signal>) {
    for mut s in &mut targets {
        s.0 = 0.0;
    }
    for (b, p, link) in &sensors {
        let Some(panel) = link.0 else { continue };
        if !active.0.contains(&panel) {
            continue;
        }
        let v = sensor_value(p.get(b.id, "key_pos"), p.get(b.id, "key_neg"), &keys.held);
        if v == 0.0 {
            continue;
        }
        let Ok(grid) = grids.get(b.grid) else { continue };
        if let Some(&t) = grid.entities.get(&(b.cell + b.dir()))
            && let Ok(mut s) = targets.get_mut(t)
        {
            s.0 = (s.0 + v).clamp(-1.0, 1.0);
        }
    }
}

pub struct SignalPlugin;

impl Plugin for SignalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LinkMode>()
            .init_resource::<ActivePanels>()
            .init_resource::<PanelKeys>()
            .add_systems(Update, (attach_links, read_keys, find_active, send_signals).chain().before(crate::mech::MechSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_make_signals() {
        // key 1 = W, 2 = S
        assert_eq!(sensor_value(1.0, 2.0, &[KeyCode::KeyW]), 1.0);
        assert_eq!(sensor_value(1.0, 2.0, &[KeyCode::KeyS]), -1.0);
        assert_eq!(sensor_value(1.0, 2.0, &[KeyCode::KeyW, KeyCode::KeyS]), 0.0);
        assert_eq!(sensor_value(1.0, 0.0, &[]), 0.0);
    }
}
