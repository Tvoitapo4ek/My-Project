//! Bearings and pistons: a moving part on top of its base.
//!
//! * **Mechanical bearing** — the structure glued to its face (not crossing
//!   the bearing itself) becomes a separate body on a hinge (Avian
//!   `RevoluteJoint`) to the grid the bearing is in: the world or a
//!   physical contraption (a crane on a car). It turns with the speed of the
//!   shaft at the bearing's back; while it gets a signal it turns to
//!   «Угол фиксации» and stays there. When it stops on a multiple of 90°,
//!   the blocks go back into the base grid.
//! * **Mechanical piston** — the structure glued in front is pushed out
//!   (`PrismaticJoint`): one block per turn of the shaft at its back, up to
//!   «Ход» blocks; the other direction pulls it back. Stopped on a whole
//!   block, the structure goes back into the base grid and poles fill the
//!   gap.
//!
//! The drive is a joint motor following the target angle / extension, so
//! the moving part stays physical: it carries load and is stopped by
//! obstacles.

use std::{
    collections::HashSet,
    f32::consts::{FRAC_PI_2, PI, TAU},
};

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    assembly::{mass_of, merge_into, split_off},
    content::{BlockId, Mech},
    contraption::{Contraption, glued_group_except},
    grid::{Cell, VoxelGrid},
    mech::{BlockEntity, MechSet, Params, Signal, Spin},
    style::{Layer, LayerNode},
    tools::glue::Glue,
};

/// State of a bearing (angle, rad) or piston (extension, blocks).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Mount {
    pub value: f32,
    pub sub: Option<SubBody>,
    /// Seconds since it last moved.
    idle: f32,
}

/// The moving part: its grid entity, the joint, and `value` when it was
/// assembled.
#[derive(Debug, Clone, Copy)]
pub struct SubBody {
    pub grid: Entity,
    pub joint: Entity,
    pub start: f32,
}

/// On a moving part's grid: the bearing / piston block entity and the joint.
#[derive(Component, Debug, Clone, Copy)]
pub struct MountedOn {
    pub mount: Entity,
    pub joint: Entity,
}

/// Turn speed while holding a set angle (rad/s) when the shaft is slower.
pub const HOLD_RATE: f32 = std::f32::consts::FRAC_PI_6;
/// Moving parts larger than this are not assembled (a glued floor).
const MAX_BLOCKS: usize = 4096;

const MOTOR: MotorModel = MotorModel::SpringDamper { frequency: 6.0, damping_ratio: 1.0 };

pub fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// One step of a bearing: free turning at `omega` (rad/s), or towards the
/// `hold` angle. Returns (angle, rate rad/s).
pub fn bearing_step(angle: f32, omega: f32, hold: Option<f32>, dt: f32) -> (f32, f32) {
    match hold {
        Some(t) => {
            let diff = wrap(t - angle);
            let rate = omega.abs().max(HOLD_RATE);
            if diff.abs() <= rate * dt {
                (angle + diff, 0.0)
            } else {
                (angle + rate * dt * diff.signum(), rate * diff.signum())
            }
        }
        None => (angle + omega * dt, omega),
    }
}

/// One step of a piston: `speed` in blocks/s (+ out, − in) up to `max`;
/// stopped half-way it settles on the nearest whole block. Returns
/// (extension, rate blocks/s).
pub fn piston_step(ext: f32, speed: f32, max: f32, dt: f32) -> (f32, f32) {
    let (target, rate) = if speed > 0.0 {
        (max, speed)
    } else if speed < 0.0 {
        (0.0, -speed)
    } else {
        (ext.round().clamp(0.0, max), 1.0)
    };
    let diff = target - ext;
    if diff.abs() <= rate * dt {
        (target, 0.0)
    } else {
        (ext + rate * dt * diff.signum(), rate * diff.signum())
    }
}

fn add_mounts(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Mount>)>) {
    for (e, b) in &added {
        if matches!(b.mech(), Some(Mech::Bearing | Mech::Piston)) {
            commands.entity(e).insert(Mount::default());
        }
    }
}

/// Consecutive pole cells in front of a piston.
fn poles(grid: &VoxelGrid, b: &BlockEntity) -> i32 {
    let pole = BlockId::k("piston_pole");
    (1..=16).take_while(|k| grid.get(b.cell + b.dir() * k).id == pole).count() as i32
}

/// Splits the moving part off and hinges it to the base.
fn assemble(world: &mut World, e: Entity, b: &BlockEntity, value: f32) -> Option<SubBody> {
    let d = b.dir();
    let grid = world.get::<VoxelGrid>(b.grid)?;
    let (start, except) = match b.mech()? {
        Mech::Bearing => (b.cell + d, HashSet::from([b.cell])),
        _ => {
            let n = poles(grid, b);
            (b.cell + d * (n + 1), (0..=n).map(|k| b.cell + d * k).collect())
        }
    };
    let group = glued_group_except(grid, world.get::<Glue>(b.grid), start, &except);
    if group.is_empty() || group.len() > MAX_BLOCKS {
        return None;
    }
    let mass = mass_of(grid, &group);
    let lv = world.get::<LinearVelocity>(b.grid).map(|v| v.0).unwrap_or_default();
    let av = world.get::<AngularVelocity>(b.grid).map(|v| v.0).unwrap_or_default();
    let sub = split_off(world, b.grid, &group, false);
    let axis = d.as_vec3();
    let centre = b.cell.as_vec3() + Vec3::splat(0.5);
    let joint = if b.mech() == Some(Mech::Bearing) {
        let p = centre + axis * 0.5;
        world.spawn((Name::new("Bearing joint"), RevoluteJoint::new(b.grid, sub).with_hinge_axis(axis).with_local_anchor1(p).with_local_anchor2(p).with_motor(AngularMotor::new(MOTOR)), JointCollisionDisabled)).id()
    } else {
        world.spawn((Name::new("Piston joint"), PrismaticJoint::new(b.grid, sub).with_slider_axis(axis).with_local_anchor1(centre).with_local_anchor2(centre).with_motor(LinearMotor::new(MOTOR)), JointCollisionDisabled)).id()
    };
    world.entity_mut(sub).insert((
        Name::new(if b.mech() == Some(Mech::Bearing) { "Bearing part" } else { "Piston part" }),
        Contraption { mass },
        RigidBody::Dynamic,
        LinearVelocity(lv),
        AngularVelocity(av),
        LinearDamping(0.05),
        AngularDamping(0.3),
        MountedOn { mount: e, joint },
    ));
    Some(SubBody { grid: sub, joint, start: value })
}

/// Pose of the moving part relative to its base: (angle about the axis,
/// offset along it).
fn relative(world: &World, b: &BlockEntity, sb: SubBody) -> Option<(f32, f32)> {
    let tb = world.get::<GlobalTransform>(b.grid)?.compute_transform();
    let ts = world.get::<GlobalTransform>(sb.grid)?.compute_transform();
    let axis = b.dir().as_vec3();
    let rel = tb.rotation.inverse() * ts.rotation;
    let (ax, ang) = rel.to_axis_angle();
    let angle = if ang.abs() < 1e-5 { 0.0 } else { wrap(ang) * ax.dot(axis).signum() };
    let offset = (tb.rotation.inverse() * (ts.translation - tb.translation)).dot(axis);
    Some((angle, offset))
}

/// Puts the moving part exactly on the nearest whole pose (90° / one
/// block), so it can go back into the grid.
fn snap_pose(world: &mut World, b: &BlockEntity, sb: SubBody) -> Option<f32> {
    let (angle, offset) = relative(world, b, sb)?;
    let tb = world.get::<GlobalTransform>(b.grid)?.compute_transform();
    let axis = b.dir().as_vec3();
    let (local, snapped) = if b.mech() == Some(Mech::Bearing) {
        let a = (angle / FRAC_PI_2).round() * FRAC_PI_2;
        let p = b.cell.as_vec3() + Vec3::splat(0.5) + axis * 0.5;
        let r = Quat::from_axis_angle(axis, a);
        (Transform::from_translation(p - r * p).with_rotation(r), a)
    } else {
        let o = offset.round();
        (Transform::from_translation(axis * o), o)
    };
    let t = tb * local;
    if let Some(mut tr) = world.get_mut::<Transform>(sb.grid) {
        *tr = t;
    }
    if let Some(mut g) = world.get_mut::<GlobalTransform>(sb.grid) {
        *g = GlobalTransform::from(t);
    }
    if let Some(mut p) = world.get_mut::<Position>(sb.grid) {
        p.0 = t.translation;
    }
    if let Some(mut r) = world.get_mut::<Rotation>(sb.grid) {
        r.0 = t.rotation;
    }
    Some(snapped)
}

/// Merges the moving part back into the base. False if there is no room.
fn disassemble(world: &mut World, e: Entity, b: &BlockEntity, sb: SubBody) -> bool {
    let Some(snapped) = snap_pose(world, b, sb) else { return false };
    if !merge_into(world, sb.grid, b.grid) {
        return false;
    }
    if let Ok(j) = world.get_entity_mut(sb.joint) {
        j.despawn();
    }
    if let Some(mut m) = world.get_mut::<Mount>(e) {
        m.sub = None;
        m.value = sb.start + snapped;
    }
    true
}

/// Merges every moving part mounted on `grid` (used before the grid itself
/// is merged somewhere). False if one of them has no room.
pub fn collapse_all(world: &mut World, grid: Entity) -> bool {
    let Some(g) = world.get::<VoxelGrid>(grid) else { return true };
    let mounts: Vec<Entity> = g.entities.values().copied().collect();
    for e in mounts {
        let (Some(b), Some(m)) = (world.get::<BlockEntity>(e).copied(), world.get::<Mount>(e).copied()) else { continue };
        if let Some(sb) = m.sub
            && world.get_entity(sb.grid).is_ok()
            && !disassemble(world, e, &b, sb)
        {
            return false;
        }
    }
    true
}

/// Poles of a piston: present up to the extension (whole blocks).
fn update_poles(world: &mut World, b: &BlockEntity, ext: f32) {
    let pole = BlockId::k("piston_pole");
    let n = (ext + 1e-3).floor() as i32;
    let Some(mut grid) = world.get_mut::<VoxelGrid>(b.grid) else { return };
    for k in 1..=16 {
        let c = b.cell + b.dir() * k;
        let here = grid.get(c);
        if k <= n && here.is_air() {
            grid.set(c, Cell::new(pole, b.facing));
        } else if k > n && here.id == pole {
            grid.set(c, Cell::AIR);
        }
    }
}

fn drive(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    // Moving parts whose bearing / piston was removed fall free.
    let mut q = world.query::<(Entity, &MountedOn)>();
    let orphans: Vec<(Entity, MountedOn)> = q.iter(world).filter(|(_, m)| world.get::<Mount>(m.mount).is_none()).map(|(e, m)| (e, *m)).collect();
    for (e, m) in orphans {
        world.entity_mut(e).remove::<MountedOn>();
        if let Ok(j) = world.get_entity_mut(m.joint) {
            j.despawn();
        }
    }
    let mut q = world.query::<(Entity, &BlockEntity, &Spin, &Signal, &Params, &Mount)>();
    let list: Vec<(Entity, BlockEntity, f32, f32, f32, Mount)> = q
        .iter(world)
        .filter_map(|(e, b, s, sig, p, m)| {
            let param = match b.mech()? {
                Mech::Bearing => p.get(b.id, "hold_angle"),
                Mech::Piston => p.get(b.id, "length"),
                _ => return None,
            };
            Some((e, *b, s.rpm, sig.0, param, *m))
        })
        .collect();
    for (e, b, rpm, sig, param, mut m) in list {
        if let Some(sb) = m.sub
            && world.get_entity(sb.grid).is_err()
        {
            if let Ok(j) = world.get_entity_mut(sb.joint) {
                j.despawn();
            }
            m.sub = None;
        }
        let d = b.dir();
        let s = (d.x + d.y + d.z) as f32;
        let bearing = b.mech() == Some(Mech::Bearing);
        let (value, rate) = if bearing {
            bearing_step(m.value, rpm * s / 60.0 * TAU, (sig.abs() > 0.5).then(|| param.to_radians()), dt)
        } else {
            piston_step(m.value, rpm * s / 60.0, param, dt)
        };
        let moving = (value - m.value).abs() > 1e-6;
        if moving && m.sub.is_none() {
            m.sub = assemble(world, e, &b, m.value);
        }
        m.value = value;
        m.idle = if moving { 0.0 } else { m.idle + dt };
        if let Some(sb) = m.sub {
            let target = m.value - sb.start;
            if let Some(mut j) = world.get_mut::<RevoluteJoint>(sb.joint) {
                j.body1 = b.grid;
                j.motor.target_position = wrap(target);
                j.motor.target_velocity = rate;
            }
            if let Some(mut j) = world.get_mut::<PrismaticJoint>(sb.joint) {
                j.body1 = b.grid;
                j.motor.target_position = target;
                j.motor.target_velocity = rate;
            }
        }
        world.entity_mut(e).insert(m);
        // Stopped on a whole pose: back into the grid.
        if let Some(sb) = m.sub
            && !moving
            && m.idle > 0.3
            && let Some((angle, offset)) = relative(world, &b, sb)
        {
            let aligned = if bearing {
                (angle - (angle / FRAC_PI_2).round() * FRAC_PI_2).abs() < 2f32.to_radians()
            } else {
                (offset - offset.round()).abs() < 0.06
            };
            if aligned {
                disassemble(world, e, &b, sb);
            }
        }
        if !bearing {
            let ext = world.get::<Mount>(e).map(|m| m.value).unwrap_or(0.0);
            update_poles(world, &b, ext);
        }
    }
}

/// Turntable / head / pole follow the mount.
fn animate(mounts: Query<(&BlockEntity, &Mount, &Children)>, mut nodes: Query<(&LayerNode, &mut Transform, &mut Visibility)>) {
    for (b, m, children) in &mounts {
        for c in children.iter() {
            let Ok((l, mut t, mut v)) = nodes.get_mut(c) else { continue };
            match (l.0, b.mech()) {
                (Layer::Mount, Some(Mech::Bearing)) => t.rotation = Quat::from_rotation_y(m.value),
                (Layer::Mount, _) => t.translation = Vec3::Y * m.value,
                (Layer::Pole, _) => {
                    t.translation = Vec3::Y * (0.5 + m.value * 0.5);
                    t.scale = Vec3::new(1.0, m.value.max(1e-3), 1.0);
                    v.set_if_neq(if m.value > 0.01 { Visibility::Inherited } else { Visibility::Hidden });
                }
                _ => {}
            }
        }
    }
}

pub struct MountsPlugin;

impl Plugin for MountsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_mounts, drive, animate).chain().after(MechSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearing_turns_or_holds() {
        // Free: follows the shaft.
        let (a, r) = bearing_step(0.0, 1.0, None, 0.5);
        assert!((a - 0.5).abs() < 1e-6 && r == 1.0);
        // Hold 90°: turns at least at HOLD_RATE, stops exactly on it.
        let mut a = 0.0;
        for _ in 0..100 {
            a = bearing_step(a, 0.0, Some(FRAC_PI_2), 0.1).0;
        }
        assert!((a - FRAC_PI_2).abs() < 1e-5);
        assert_eq!(bearing_step(a, 2.0, Some(FRAC_PI_2), 0.1).1, 0.0, "held: no motion");
        // The short way round: 350° → 10° goes forward.
        let (b, _) = bearing_step(350f32.to_radians(), 0.0, Some(10f32.to_radians()), 0.01);
        assert!(b > 350f32.to_radians());
    }

    #[test]
    fn piston_strokes_and_settles() {
        // 1 block per turn: 60 RPM = 1 block/s, up to the stroke.
        let mut e = 0.0;
        for _ in 0..30 {
            e = piston_step(e, 1.0, 2.0, 0.1).0;
        }
        assert_eq!(e, 2.0);
        // Stopped half-way: settles on the nearest whole block.
        let mut e = 1.3;
        for _ in 0..10 {
            e = piston_step(e, 0.0, 4.0, 0.1).0;
        }
        assert_eq!(e, 1.0);
        assert_eq!(piston_step(0.0, -1.0, 4.0, 0.1), (0.0, 0.0), "retracted: stays");
    }
}
