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

/// State of the network a block is in (settings panel, telemetry, wheels).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct NetState {
    pub conflict: bool,
    /// Part of a network with a source (see `NetInfo::driven`).
    pub driven: bool,
    /// Stress / capacity of its networks (joined through speed controllers).
    pub load: f32,
    pub stress: f32,
    pub capacity: f32,
    /// Speed factor from overload (1 = full speed, 0 = stopped).
    pub k: f32,
    pub overstressed: bool,
}

/// Flywheel: kinetic energy kept to cover short overloads (SU·s).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Flywheel {
    pub energy: f32,
}
/// Up to `FLY_RATE` SU for `FLY_MAX / FLY_RATE` seconds.
pub const FLY_MAX: f32 = 2000.0;
pub const FLY_RATE: f32 = 1000.0;
/// Recharge (SU of spare capacity) per flywheel.
const FLY_CHARGE: f32 = 500.0;
/// A stalled diesel starts again when its network would run at this
/// speed factor (throttle released or a lower gear).
pub const RESTART: f32 = 0.5;

/// Extra stress of a driven wheel on the ground (roll and slope) at
/// 32 RPM, from `contraption`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct WheelLoad(pub f32);

/// Stress of a block at 32 RPM (SU).
pub fn base_stress(m: Mech) -> f32 {
    match m {
        Mech::Shaft | Mech::CasedShaft | Mech::Suspension | Mech::Wheel | Mech::Clutch | Mech::Gearshift | Mech::TrackDrive | Mech::TrackRoller => 0.5,
        Mech::BeltPulley => 1.0,
        Mech::Cog | Mech::MechanicalDisplay | Mech::SpeedController | Mech::Flywheel => 1.0,
        Mech::LargeCog | Mech::Gearbox | Mech::Bearing | Mech::Piston => 2.0,
        Mech::Pump => 12.0,
        _ => 0.0,
    }
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
            match m {
                Mech::Flywheel => {
                    commands.entity(e).insert(Flywheel::default());
                }
                Mech::Wheel | Mech::TrackDrive | Mech::TrackRoller => {
                    commands.entity(e).insert(WheelLoad::default());
                }
                _ => {}
            }
            spawn_parts(&mut commands, e, &models::block_model(m), &mut meshes, &mut cache, &mats, true);
            grid.entities.insert(c, e);
        }
    }
}

/// World (grid) direction of a block's local +X — the side axis of
/// upright blocks (displays).
pub fn side_axis(b: &BlockEntity) -> IVec3 {
    (transform_for(b.cell, b.id, b.facing).rotation * Vec3::X).round().as_ivec3()
}

/// The node a block contributes to the network (pass 2 = speed
/// controllers on their output side).
/// `running`: a diesel engine has fuel and air (`engine`).
fn node_for(b: &BlockEntity, p: &Params, signal: f32, out_pass: Option<bool>, running: bool) -> Option<Node> {
    let d = b.dir();
    let axis = Axis::of(d);
    let s = (d.x + d.y + d.z) as f32;
    let on = signal.abs() > 0.5;
    let m = b.mech()?;
    let node = Some(match m {
        Mech::Motor => {
            let speed = p.get(b.id, "speed");
            let ccw = p.get(b.id, "direction") >= 0.5;
            Node::new(Role::Shaft, axis).one_port(d).source(if ccw { s * speed } else { -s * speed }).capacity(p.get(b.id, "capacity"))
        }
        Mech::Shaft | Mech::CasedShaft | Mech::Suspension | Mech::Wheel | Mech::Flywheel | Mech::BeltPulley | Mech::TrackDrive | Mech::TrackRoller => Node::new(Role::Shaft, axis),
        // Pumps mesh like cogwheels around their pipe axis.
        Mech::Pump => Node::new(Role::Cog, axis),
        Mech::DieselEngine => {
            let speed = if running { p.get(b.id, "speed") } else { 0.0 };
            let ccw = p.get(b.id, "direction") >= 0.5;
            let n = Node::new(Role::Shaft, axis).one_port(d);
            if running { n.source(if ccw { s * speed } else { -s * speed }).capacity(crate::engine::POWER) } else { n }
        }
        // Powered from the side: it is a cog whose axis runs left–right, so
        // a shaft or cogwheel on its side (or a cog meshing in its plane)
        // turns it.
        Mech::MechanicalDisplay => Node::new(Role::Cog, Axis::of(side_axis(b))),
        // Driven from the back; the face carries the moving part.
        Mech::Bearing | Mech::Piston => Node::new(Role::Shaft, axis).one_port(-d),
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
    });
    node.map(|n| n.stress(base_stress(m)))
}

type NetBlock<'a> = (&'a BlockEntity, &'a Params, &'a Signal, &'a mut Spin, &'a mut NetState, Option<&'a mut crate::engine::Engine>, Option<&'a mut Flywheel>, Option<&'a WheelLoad>);

fn solve_networks(time: Res<Time>, grids: Query<(&VoxelGrid, Option<&crate::chains::Chains>)>, mut blocks: Query<NetBlock>) {
    let dt = time.delta_secs();
    for (grid, chains) in &grids {
        if grid.entities.is_empty() {
            continue;
        }
        let mut nodes: HashMap<IVec3, Node> = HashMap::new();
        let mut controllers = Vec::new();
        let mut engines = Vec::new();
        let mut flywheels = Vec::new();
        for (&c, &e) in &grid.entities {
            let Ok((b, p, sig, _, _, eng, fly, wl)) = blocks.get(e) else { continue };
            // A stalled diesel counts as running here: is it worth restarting?
            if let Some(mut n) = node_for(b, p, sig.0, None, eng.as_ref().is_some_and(|e| e.ready)) {
                n.stress += wl.map(|w| w.0).unwrap_or(0.0);
                nodes.insert(c, n);
                match b.mech() {
                    Some(Mech::SpeedController) => controllers.push((c, e, b.dir())),
                    Some(Mech::DieselEngine) => engines.push((c, e)),
                    _ if fly.is_some() => flywheels.push((c, e)),
                    _ => {}
                }
            }
        }
        // Chains (and other links between distant blocks).
        let mut extra = Vec::new();
        if let Some(ch) = chains {
            extra.extend(crate::chains::links(ch, |c| grid.entities.get(&c).and_then(|e| blocks.get(*e).ok()).and_then(|b| b.0.mech())));
        }
        extra.extend(crate::belts::links(grid, &nodes, |c| grid.entities.get(&c).and_then(|e| blocks.get(*e).ok()).and_then(|b| b.0.mech())));
        let mut sol = network::solve_with(&nodes, &extra);
        if !controllers.is_empty() {
            for &(c, e, _) in &controllers {
                let running = sol.speed.get(&c).is_some_and(|v| v.abs() > 1e-3);
                if let Ok((b, p, sig, _, _, _, _, _)) = blocks.get(e)
                    && let Some(n) = node_for(b, p, sig.0, Some(running), false)
                {
                    nodes.insert(c, n);
                }
            }
            sol = network::solve_with(&nodes, &extra);
        }
        // Networks joined through speed controllers share power and stress.
        let mut root: Vec<usize> = (0..sol.nets.len()).collect();
        fn find(r: &mut [usize], x: usize) -> usize {
            let mut x = x;
            while r[x] != x {
                r[x] = r[r[x]];
                x = r[x];
            }
            x
        }
        for &(c, _, d) in &controllers {
            if let (Some(&a), Some(&b)) = (sol.net_of.get(&c), sol.net_of.get(&(c - d))) {
                let (ra, rb) = (find(&mut root, a), find(&mut root, b));
                root[ra] = rb;
            }
        }
        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for n in 0..sol.nets.len() {
            let r = find(&mut root, n);
            groups.entry(r).or_default().push(n);
        }
        let mut per_net: HashMap<usize, (f32, f32, f32)> = HashMap::new();
        for nets in groups.values() {
            let cap: f32 = nets.iter().map(|n| sol.nets[*n].capacity).sum();
            let stress: f32 = nets.iter().map(|n| sol.nets[*n].stress).sum();
            let in_group = |c: &IVec3| sol.net_of.get(c).is_some_and(|n| nets.contains(n));
            // Flywheels cover a short overload, and recharge from spare power.
            let flys: Vec<Entity> = flywheels.iter().filter(|(c, _)| in_group(c)).map(|(_, e)| *e).collect();
            let deficit = (stress - cap).max(0.0);
            let mut boost = 0.0;
            if !flys.is_empty() && dt > 0.0 {
                let avail: Vec<f32> = flys.iter().map(|e| blocks.get(*e).ok().and_then(|x| x.6.map(|f| (f.energy / dt).min(FLY_RATE))).unwrap_or(0.0)).collect();
                let total: f32 = avail.iter().sum();
                boost = deficit.min(total);
                for (e, a) in flys.iter().zip(&avail) {
                    if let Ok((.., Some(mut f), _)) = blocks.get_mut(*e) {
                        if boost > 0.0 && total > 0.0 {
                            f.energy = (f.energy - boost * dt * a / total).max(0.0);
                        } else if cap > stress {
                            f.energy = (f.energy + (cap - stress).min(FLY_CHARGE) * dt).min(FLY_MAX);
                        }
                    }
                }
            }
            let mut k = network::factor(cap + boost, stress);
            let group_engines: Vec<Entity> = engines.iter().filter(|(c, _)| in_group(c)).map(|(_, e)| *e).collect();
            let stalled = group_engines.iter().any(|e| blocks.get(*e).is_ok_and(|x| x.5.as_ref().is_some_and(|g| g.overload)));
            if stalled {
                // Stays dead until it would run comfortably.
                if network::factor(cap, stress) >= RESTART {
                    for e in &group_engines {
                        if let Ok((.., Some(mut g), _, _)) = blocks.get_mut(*e) {
                            g.overload = false;
                        }
                    }
                } else {
                    k = 0.0;
                }
            } else if k == 0.0 && cap > 0.0 {
                for e in &group_engines {
                    if let Ok((.., Some(mut g), _, _)) = blocks.get_mut(*e)
                        && g.ready
                    {
                        g.overload = true;
                        g.burst = true;
                    }
                }
            }
            network::scale(&mut sol, nets, k);
            for n in nets {
                per_net.insert(*n, (k, stress, cap));
            }
        }
        for (&c, &e) in &grid.entities {
            let Ok((_, _, _, mut spin, mut st, ..)) = blocks.get_mut(e) else { continue };
            let rpm = sol.speed.get(&c).copied().unwrap_or(0.0);
            spin.rpm = rpm;
            spin.angle = (spin.angle + rpm / 60.0 * std::f32::consts::TAU * dt).rem_euclid(std::f32::consts::TAU * 64.0);
            let net = sol.net_of.get(&c);
            st.conflict = net.is_some_and(|n| sol.nets[*n].conflict);
            st.driven = net.is_some_and(|n| sol.nets[*n].driven);
            let (k, stress, cap) = net.and_then(|n| per_net.get(n).copied()).unwrap_or((1.0, 0.0, 0.0));
            st.k = k;
            st.stress = stress;
            st.capacity = cap;
            st.load = if cap > 0.0 { stress / cap } else if stress > 0.0 { f32::INFINITY } else { 0.0 };
            st.overstressed = k == 0.0 && stress > cap;
        }
    }
}

/// Rotor layers turn about the block's local Y by the network speed.
fn spin_rotors(blocks: Query<(&BlockEntity, &Spin, &Children)>, mut nodes: Query<(&LayerNode, &mut Transform)>) {
    for (b, spin, children) in &blocks {
        let d = b.dir();
        let s = (d.x + d.y + d.z) as f32;
        let x = side_axis(b);
        let sx = (x.x + x.y + x.z) as f32;
        // Meshing cogs are offset by half a tooth so the teeth interleave.
        let offset = match b.mech() {
            Some(Mech::Cog) if (b.cell.x + b.cell.y + b.cell.z).rem_euclid(2) == 1 => std::f32::consts::PI / 8.0,
            _ => 0.0,
        };
        for c in children.iter() {
            if let Ok((l, mut t)) = nodes.get_mut(c) {
                match l.0 {
                    Layer::Rotor => t.rotation = Quat::from_rotation_y(spin.angle * s + offset),
                    Layer::RotorX => t.rotation = Quat::from_rotation_x(spin.angle * sx),
                    Layer::RotorZ => t.rotation = Quat::from_rotation_z(spin.angle * s),
                    _ => {}
                }
            }
        }
    }
}

/// Text of a parameter value.
pub fn param_text(kind: &ParamKind, v: f32) -> String {
    match kind {
        ParamKind::Number { unit, step, .. } if step.fract() != 0.0 => format!("{v:.1} {unit}").trim().to_string(),
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
        let n = node_for(&b, &p, 0.0, None, false).unwrap();
        assert_eq!(n.only, Some(IVec3::X));
        assert_eq!(n.source, Some(-32.0), "clockwise seen from the output face");
        p.0[1] = 1.0;
        p.0[0] = 64.0;
        assert_eq!(node_for(&b, &p, 0.0, None, false).unwrap().source, Some(64.0));
    }

    #[test]
    fn speed_controller_blends_by_signal() {
        let (b, p) = be("speed_controller", 2);
        assert_eq!(node_for(&b, &p, 0.0, Some(true), false).unwrap().source, Some(32.0));
        assert_eq!(node_for(&b, &p, 1.0, Some(true), false).unwrap().source, Some(128.0));
        assert_eq!(node_for(&b, &p, -1.0, Some(true), false).unwrap().source, Some(-128.0), "negative signal reverses");
        assert_eq!(node_for(&b, &p, 1.0, Some(false), false).unwrap().source, None, "no input → no output");
    }

    #[test]
    fn display_is_driven_by_a_cog_on_its_side() {
        // Display facing −Z (front towards the player at +Z… seen from −Z):
        // its side axis is X. A motor → cogwheel line along X ends at it.
        let disp = BlockEntity { grid: Entity::PLACEHOLDER, cell: IVec3::new(2, 0, 0), id: BlockId::k("mechanical_display"), facing: crate::grid::facing_of(IVec3::Z) };
        let side = side_axis(&disp);
        assert_eq!(side.y, 0);
        assert_eq!(side.x.abs() + side.z.abs(), 1);
        let mut g = HashMap::new();
        let ax = Axis::of(side);
        g.insert(IVec3::ZERO, Node::new(Role::Shaft, ax).one_port(side).source(64.0));
        g.insert(side, Node::new(Role::Cog, ax));
        g.insert(side * 2, node_for(&disp, &Params::defaults(disp.id), 0.0, None, false).unwrap());
        let sol = network::solve(&g);
        assert_eq!(sol.speed[&(side * 2)].abs(), 64.0);
        // Nothing turns it → stopped.
        g.remove(&IVec3::ZERO);
        assert_eq!(network::solve(&g).speed[&(side * 2)], 0.0);
    }

    #[test]
    fn param_steps_wrap_and_clamp() {
        let k = ParamKind::Number { min: 0.0, max: 256.0, step: 8.0, unit: "".into() };
        assert_eq!(param_step(&k, 256.0, 1.0), 256.0);
        assert_eq!(param_step(&ParamKind::Choice(vec!["a".into(), "b".into()]), 1.0, 1.0), 0.0);
        assert_eq!(param_text(&ParamKind::Key, 1.0), "W");
    }
}
