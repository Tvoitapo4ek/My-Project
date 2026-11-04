//! Mechanisms: block entities, settings and the kinetic network.
//!
//! * Every mechanism cell of a grid gets a block entity (child of the grid
//!   entity) with its model, settings (`Params`) and state. It is kept in
//!   sync with the grid automatically, so mechanisms work the same in the
//!   world and inside physical contraptions.
//! * Each frame the kinetic network of every grid is solved
//!   (`network::solve`, two passes for speed controllers) and rotors spin
//!   by their speed.

pub mod network;

use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    content::{BlockId, Mech, ParamKind},
    grid::{VoxelGrid, facing_dir},
    models,
    style::{BoxCache, Layer, LayerNode, Materials, spawn_parts},
};
use network::{Axis, Node, Role};

#[derive(Component, Debug, Clone, Copy)]
pub struct BlockEntity {
    pub grid: Entity,
    pub cell: IVec3,
    pub id: BlockId,
    pub facing: u8,
}

impl BlockEntity {
    pub fn mech(&self) -> Option<Mech> {
        self.id.def().mech
    }
    pub fn dir(&self) -> IVec3 {
        facing_dir(self.facing)
    }
}

/// Settings (wrench), index-aligned with the block's `params`.
#[derive(Component, Debug, Clone, Default)]
pub struct Params(pub Vec<f32>);

impl Params {
    pub fn defaults(id: BlockId) -> Self {
        Self(id.def().params.iter().map(|p| p.default).collect())
    }
    pub fn get(&self, id: BlockId, key: &str) -> f32 {
        id.def().param(key).and_then(|(i, _)| self.0.get(i).copied()).unwrap_or(0.0)
    }
}

/// Control signal received from a sensor (−1..1), set by `signal`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Signal(pub f32);

/// Rotation: RPM about the world (grid) +axis of the block, and the angle.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Spin {
    pub rpm: f32,
    pub angle: f32,
}

/// Displayed speed of a speed controller / conflicts (settings panel).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct NetState {
    pub conflict: bool,
}

pub fn transform_for(cell: IVec3, id: BlockId, facing: u8) -> Transform {
    let def = id.def();
    Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5)).with_rotation(models::block_rot(def.placement, facing))
}

/// Keeps block entities in step with the grid cells.
#[allow(clippy::too_many_arguments)]
fn sync_block_entities(
    mut commands: Commands,
    mut grids: Query<(Entity, &mut VoxelGrid)>,
    mut blocks: Query<(&mut BlockEntity, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (ge, mut grid) in &mut grids {
        if grid.changed.is_empty() {
            continue;
        }
        let changed: Vec<IVec3> = std::mem::take(&mut grid.changed);
        for c in changed {
            let cell = grid.get(c);
            let mech = (!cell.is_air()).then(|| cell.id.def().mech).flatten();
            let existing = grid.entities.get(&c).copied();
            if let Some(e) = existing {
                // Same block turned by the wrench: keep it (and its settings).
                if mech.is_some()
                    && let Ok((mut b, mut t)) = blocks.get_mut(e)
                    && b.id == cell.id
                {
                    b.facing = cell.facing;
                    *t = transform_for(c, cell.id, cell.facing);
                    continue;
                }
                commands.entity(e).despawn();
                grid.entities.remove(&c);
            }
            let Some(m) = mech else { continue };
            let e = commands
                .spawn((
                    Name::new(cell.id.def().name.clone()),
                    BlockEntity { grid: ge, cell: c, id: cell.id, facing: cell.facing },
                    Params::defaults(cell.id),
                    Signal::default(),
                    Spin::default(),
                    NetState::default(),
                    transform_for(c, cell.id, cell.facing),
                    Visibility::default(),
                    ChildOf(ge),
                ))
                .id();
            spawn_parts(&mut commands, e, &models::block_model(m), &mut meshes, &mut cache, &mats, true);
            grid.entities.insert(c, e);
        }
    }
}

/// The node a block contributes to the network (pass 2 = speed
/// controllers on their output side).
fn node_for(b: &BlockEntity, p: &Params, signal: f32, out_pass: Option<bool>) -> Option<Node> {
    let d = b.dir();
    let axis = Axis::of(d);
    let s = (d.x + d.y + d.z) as f32;
    let on = signal.abs() > 0.5;
    Some(match b.mech()? {
        Mech::Motor => {
            let speed = p.get(b.id, "speed");
            let ccw = p.get(b.id, "direction") >= 0.5;
            Node::new(Role::Shaft, axis).one_port(d).source(if ccw { s * speed } else { -s * speed })
        }
        Mech::Shaft | Mech::Suspension | Mech::Wheel => Node::new(Role::Shaft, axis),
        Mech::Cog => Node::new(Role::Cog, axis),
        Mech::LargeCog => Node::new(Role::LargeCog, axis),
        Mech::Gearbox => Node::new(Role::Gearbox, axis),
        Mech::Clutch => Node { toggled: on, ..Node::new(Role::Clutch, axis) },
        Mech::Gearshift => Node { toggled: on, ..Node::new(Role::Gearshift, axis) },
        Mech::SpeedController => match out_pass {
            // Pass 1: only the input side (−facing) connects.
            None => Node::new(Role::Shaft, axis).one_port(-d),
            Some(running) => {
                let base = p.get(b.id, "speed");
                let sig = p.get(b.id, "signal_speed");
                // The signal blends towards ±"signal speed": with a base
                // of 0 it works as a throttle (+ forward, − reverse).
                let v = base + (sig * signal.signum() - base) * signal.abs().min(1.0);
                let n = Node::new(Role::Shaft, axis).one_port(d);
                if running { n.source(s * v) } else { n }
            }
        },
        _ => return None,
    })
}

#[allow(clippy::type_complexity)]
fn solve_networks(time: Res<Time>, grids: Query<&VoxelGrid>, mut blocks: Query<(&BlockEntity, &Params, &Signal, &mut Spin, &mut NetState)>) {
    let dt = time.delta_secs();
    for grid in &grids {
        if grid.entities.is_empty() {
            continue;
        }
        let mut nodes: HashMap<IVec3, Node> = HashMap::new();
        let mut controllers = Vec::new();
        for (&c, &e) in &grid.entities {
            let Ok((b, p, sig, _, _)) = blocks.get(e) else { continue };
            if let Some(n) = node_for(b, p, sig.0, None) {
                nodes.insert(c, n);
                if b.mech() == Some(Mech::SpeedController) {
                    controllers.push((c, e));
                }
            }
        }
        let mut sol = network::solve(&nodes);
        if !controllers.is_empty() {
            for &(c, e) in &controllers {
                let running = sol.speed.get(&c).is_some_and(|v| v.abs() > 1e-3);
                if let Ok((b, p, sig, _, _)) = blocks.get(e)
                    && let Some(n) = node_for(b, p, sig.0, Some(running))
                {
                    nodes.insert(c, n);
                }
            }
            sol = network::solve(&nodes);
        }
        for (&c, &e) in &grid.entities {
            let Ok((_, _, _, mut spin, mut st)) = blocks.get_mut(e) else { continue };
            let rpm = sol.speed.get(&c).copied().unwrap_or(0.0);
            spin.rpm = rpm;
            spin.angle = (spin.angle + rpm / 60.0 * std::f32::consts::TAU * dt).rem_euclid(std::f32::consts::TAU * 64.0);
            st.conflict = sol.net_of.get(&c).is_some_and(|n| sol.nets[*n].conflict);
        }
    }
}

/// Rotor layers turn about the block's local Y by the network speed.
fn spin_rotors(blocks: Query<(&BlockEntity, &Spin, &Children)>, mut nodes: Query<(&LayerNode, &mut Transform)>) {
    for (b, spin, children) in &blocks {
        let d = b.dir();
        let s = (d.x + d.y + d.z) as f32;
        // Meshing cogs are offset by half a tooth so the teeth interleave.
        let offset = match b.mech() {
            Some(Mech::Cog) if (b.cell.x + b.cell.y + b.cell.z).rem_euclid(2) == 1 => std::f32::consts::PI / 8.0,
            _ => 0.0,
        };
        for c in children.iter() {
            if let Ok((l, mut t)) = nodes.get_mut(c)
                && l.0 == Layer::Rotor
            {
                t.rotation = Quat::from_rotation_y(spin.angle * s + offset);
            }
        }
    }
}

/// Text of a parameter value.
pub fn param_text(kind: &ParamKind, v: f32) -> String {
    match kind {
        ParamKind::Number { unit, .. } => format!("{v:.0} {unit}").trim().to_string(),
        ParamKind::Choice(c) => c.get(v as usize).cloned().unwrap_or_default(),
        ParamKind::Key => crate::core::BINDABLE_KEYS.get(v as usize).map(|k| k.0.to_string()).unwrap_or_default(),
    }
}

/// Next / previous value of a parameter.
pub fn param_step(kind: &ParamKind, v: f32, dir: f32) -> f32 {
    match kind {
        ParamKind::Number { min, max, step, .. } => (v + step * dir).clamp(*min, *max),
        ParamKind::Choice(c) => (v + dir).rem_euclid(c.len() as f32),
        ParamKind::Key => (v + dir).rem_euclid(crate::core::BINDABLE_KEYS.len() as f32),
    }
}

/// Network solving and rotor animation (signals are written before it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MechSet;

pub struct MechPlugin;

impl Plugin for MechPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, sync_block_entities.before(TransformSystems::Propagate))
            .add_systems(Update, (solve_networks, spin_rotors).chain().in_set(MechSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be(key: &str, facing: u8) -> (BlockEntity, Params) {
        let id = BlockId::k(key);
        (BlockEntity { grid: Entity::PLACEHOLDER, cell: IVec3::ZERO, id, facing }, Params::defaults(id))
    }

    #[test]
    fn motor_direction_and_speed_come_from_settings() {
        let (b, mut p) = be("creative_motor", 0); // facing +X
        let n = node_for(&b, &p, 0.0, None).unwrap();
        assert_eq!(n.only, Some(IVec3::X));
        assert_eq!(n.source, Some(-32.0), "clockwise seen from the output face");
        p.0[1] = 1.0;
        p.0[0] = 64.0;
        assert_eq!(node_for(&b, &p, 0.0, None).unwrap().source, Some(64.0));
    }

    #[test]
    fn speed_controller_blends_by_signal() {
        let (b, p) = be("speed_controller", 2);
        assert_eq!(node_for(&b, &p, 0.0, Some(true)).unwrap().source, Some(32.0));
        assert_eq!(node_for(&b, &p, 1.0, Some(true)).unwrap().source, Some(128.0));
        assert_eq!(node_for(&b, &p, -1.0, Some(true)).unwrap().source, Some(-128.0), "negative signal reverses");
        assert_eq!(node_for(&b, &p, 1.0, Some(false)).unwrap().source, None, "no input → no output");
    }

    #[test]
    fn param_steps_wrap_and_clamp() {
        let k = ParamKind::Number { min: 0.0, max: 256.0, step: 8.0, unit: "".into() };
        assert_eq!(param_step(&k, 256.0, 1.0), 256.0);
        assert_eq!(param_step(&ParamKind::Choice(vec!["a".into(), "b".into()]), 1.0, 1.0), 0.0);
        assert_eq!(param_text(&ParamKind::Key, 1.0), "W");
    }
}
