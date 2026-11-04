//! Sliding doors (like Create's): two blocks tall; RMB opens / closes, a
//! signal (logic, wires, a lever…) opens it while on and closes it when it
//! goes off. The panel slides sideways; an open door lets you through.
//!
//! The state is the block itself: `sliding_door` / `sliding_door_open`
//! (and their hidden top halves), so it moves with contraptions and
//! blueprints like any block.

use bevy::prelude::*;

use crate::{
    content::{BlockId, Mech},
    grid::{Cell, VoxelGrid},
    interact::{Edited, UseBlock},
    mech::{BlockEntity, MechSet, Signal},
    style::{Layer, LayerNode},
};

/// How far the panel slides (blocks) and how fast (blocks/s).
const SLIDE: f32 = 0.86;
const SPEED: f32 = 2.5;

/// Door keys: (closed bottom, closed top, open bottom, open top).
const KEYS: [&str; 4] = ["sliding_door", "sliding_door_top", "sliding_door_open", "sliding_door_top_open"];

/// Which part of a door a block is: (is top, is open).
pub fn part(id: BlockId) -> Option<(bool, bool)> {
    let k = id.def().key.as_str();
    KEYS.iter().position(|x| *x == k).map(|i| (i % 2 == 1, i >= 2))
}

fn id(top: bool, open: bool) -> BlockId {
    BlockId::k(KEYS[top as usize + 2 * open as usize])
}

/// Sets a door (both halves) open or closed.
pub fn set_open(grid: &mut VoxelGrid, cell: IVec3, open: bool) -> bool {
    let Some((top, was)) = part(grid.get(cell).id) else { return false };
    if was == open {
        return false;
    }
    let bottom = if top { cell - IVec3::Y } else { cell };
    let facing = grid.get(cell).facing;
    for (c, t) in [(bottom, false), (bottom + IVec3::Y, true)] {
        if part(grid.get(c).id).is_some_and(|(pt, _)| pt == t) {
            grid.set(c, Cell::new(id(t, open), facing));
        }
    }
    true
}

/// Slide animation and the signal edge seen last.
#[derive(Component, Debug, Clone, Copy)]
pub struct Door {
    pub offset: f32,
    last_signal: bool,
}

fn add_doors(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Door>)>) {
    for (e, b) in &added {
        if let Some((_, open)) = part(b.id) {
            // A new block replaced the other state: start where it was.
            commands.entity(e).insert(Door { offset: if open { 0.0 } else { SLIDE }, last_signal: false });
        }
    }
}

/// Placing the bottom adds the top; breaking a half removes the other.
fn pair(mut edited: MessageReader<Edited>, mut grids: Query<&mut VoxelGrid>) {
    for e in edited.read() {
        let Ok(mut g) = grids.get_mut(e.grid) else { continue };
        let Some((top, open)) = part(e.block) else { continue };
        let other = if top { e.cell - IVec3::Y } else { e.cell + IVec3::Y };
        if e.placed {
            let facing = g.get(e.cell).facing;
            if !top && g.get(other).is_air() {
                g.set(other, Cell::new(id(true, open), facing));
            }
        } else if part(g.get(other).id).is_some_and(|(t, _)| t != top) {
            g.set(other, Cell::AIR);
        }
    }
}

fn use_doors(mut used: MessageReader<UseBlock>, mut grids: Query<&mut VoxelGrid>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    for u in used.read() {
        if u.mech != Mech::SlidingDoor {
            continue;
        }
        let Ok(mut g) = grids.get_mut(u.grid) else { continue };
        let Some((_, open)) = part(g.get(u.cell).id) else { continue };
        if set_open(&mut g, u.cell, !open) {
            sounds.write(crate::sound::sfx("toggle"));
        }
    }
}

/// A signal on either half: on → open, off → closed (on the edge, so RMB
/// still works while nothing drives it).
fn signals(mut doors: Query<(&BlockEntity, &Signal, &mut Door)>, mut grids: Query<&mut VoxelGrid>) {
    for (b, sig, mut d) in &mut doors {
        let on = sig.0.abs() > 0.5;
        if on == d.last_signal {
            continue;
        }
        d.last_signal = on;
        if let Ok(mut g) = grids.get_mut(b.grid) {
            set_open(&mut g, b.cell, on);
        }
    }
}

fn animate(time: Res<Time>, mut doors: Query<(&BlockEntity, &mut Door, &Children)>, mut nodes: Query<(&LayerNode, &mut Transform)>) {
    let dt = time.delta_secs();
    for (b, mut d, children) in &mut doors {
        let target = if part(b.id).is_some_and(|(_, open)| open) { SLIDE } else { 0.0 };
        d.offset += (target - d.offset).clamp(-SPEED * dt, SPEED * dt);
        for c in children.iter() {
            if let Ok((l, mut t)) = nodes.get_mut(c)
                && l.0 == Layer::Mount
            {
                t.translation.x = d.offset;
            }
        }
    }
}

pub struct DoorsPlugin;

impl Plugin for DoorsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_doors, pair, use_doors, signals.after(crate::logic::LogicSystems).before(MechSet), animate).chain().after(crate::interact::InteractSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_halves_open_together() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Cell::new(id(false, false), 4));
        g.set(IVec3::Y, Cell::new(id(true, false), 4));
        assert!(set_open(&mut g, IVec3::Y, true), "clicking the top opens both");
        assert_eq!(part(g.get(IVec3::ZERO).id), Some((false, true)));
        assert_eq!(part(g.get(IVec3::Y).id), Some((true, true)));
        assert_eq!(g.get(IVec3::Y).facing, 4);
        assert!(!set_open(&mut g, IVec3::ZERO, true), "already open");
        // An open door lets you through, a closed one does not.
        assert!(crate::grid::cell_boxes(IVec3::ZERO, g.get(IVec3::ZERO)).is_none());
        set_open(&mut g, IVec3::ZERO, false);
        assert!(crate::grid::cell_boxes(IVec3::ZERO, g.get(IVec3::ZERO)).is_some());
    }
}
