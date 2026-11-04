//! Contraptions: the physics toggle gives physics to itself and to every
//! block glued to it (connected through glue, like Create), and takes it
//! back.
//!
//! * **Assemble**: the group leaves the world grid and becomes a new grid
//!   entity with one rigid body (compound collider from its chunks, mass
//!   from its blocks). Block entities move along with their settings, so
//!   mechanisms keep working inside.
//! * **Disassemble**: the body is snapped to the nearest grid orientation
//!   and position and the blocks return to the world (if there is room).
//! * **Driving**: wheels on suspensions are a raycast suspension — springs
//!   push the body up, the wheel's kinetic speed drives it forward, side
//!   friction keeps it on track, steering turns the wheel. Physics only
//!   where it is needed: stable and cheap.

use std::collections::{HashSet, VecDeque};

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    content::Mech,
    core::toast,
    grid::{DIRS, VoxelGrid, WorldGrid, render::ChunkViews},
    interact::UseBlock,
    mech::{BlockEntity, Spin},
    tools::glue::Glue,
    vehicle::Steer,
};

/// A physical grid.
#[derive(Component, Debug, Clone, Copy)]
pub struct Contraption {
    /// Total mass (kg).
    pub mass: f32,
}

/// Cells connected to `start` through glue (face neighbours inside a
/// common glue box). Without glue: only `start`.
pub fn glued_group(grid: &VoxelGrid, glue: Option<&Glue>, start: IVec3) -> Vec<IVec3> {
    glued_group_except(grid, glue, start, &HashSet::new())
}

/// Like `glued_group`, but never enters the `except` cells (the bearing or
/// piston a moving part sits on).
pub fn glued_group_except(grid: &VoxelGrid, glue: Option<&Glue>, start: IVec3, except: &HashSet<IVec3>) -> Vec<IVec3> {
    if grid.get(start).is_air() || except.contains(&start) {
        return Vec::new();
    }
    let mut seen = HashSet::from([start]);
    let mut q = VecDeque::from([start]);
    while let Some(c) = q.pop_front() {
        let Some(g) = glue else { break };
        for d in DIRS {
            let n = c + d;
            if !seen.contains(&n) && !except.contains(&n) && !grid.get(n).is_air() && g.contains(c, n) {
                seen.insert(n);
                q.push_back(n);
            }
        }
    }
    let mut v: Vec<IVec3> = seen.into_iter().collect();
    v.sort_by_key(|c| (c.y, c.x, c.z));
    v
}

/// Nearest rotation that maps grid axes onto grid axes.
pub fn snap_rotation(q: Quat) -> Quat {
    let snap = |v: Vec3| {
        let a = v.abs();
        if a.x >= a.y && a.x >= a.z {
            Vec3::X * v.x.signum()
        } else if a.y >= a.z {
            Vec3::Y * v.y.signum()
        } else {
            Vec3::Z * v.z.signum()
        }
    };
    let x = snap(q * Vec3::X);
    let mut y = snap(q * Vec3::Y);
    if y.dot(x).abs() > 0.5 {
        // Degenerate: pick the next best axis for Y.
        let v = q * Vec3::Y - x * (q * Vec3::Y).dot(x);
        y = snap(v);
    }
    let z = x.cross(y);
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

/// RMB on a physics toggle: assemble its glued group into a body, or put
/// the body back into the world grid.
fn toggle(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<UseBlock>>) {
    let uses: Vec<UseBlock> = cursor.read(world.resource::<Messages<UseBlock>>()).copied().filter(|u| u.mech == Mech::PhysicsToggle).collect();
    for u in uses {
        let Some(gt) = world.get::<GlobalTransform>(u.grid).copied() else { continue };
        let is_world = world.get::<WorldGrid>(u.grid).is_some();
        if world.get::<crate::mounts::MountedOn>(u.grid).is_some() {
            world.write_message(toast("Это подвижная часть подшипника или поршня"));
            continue;
        }
        if is_world {
            // ---- assemble
            let (cells, mass) = {
                let grid = world.get::<VoxelGrid>(u.grid).unwrap();
                let group = glued_group(grid, world.get::<Glue>(u.grid), u.cell);
                let mass = crate::assembly::mass_of(grid, &group);
                (group, mass)
            };
            let body = crate::assembly::split_off(world, u.grid, &cells, true);
            world.entity_mut(body).insert((Contraption { mass }, RigidBody::Dynamic, LinearDamping(0.05), AngularDamping(0.3)));
            world.write_message(toast(format!("Физика включена: блоков {}, масса {:.0} кг", cells.len(), mass)));
            world.write_message(crate::sound::sfx_at("physics_on", gt.translation() + u.cell.as_vec3()));
        } else if world.get::<Contraption>(u.grid).is_some() {
            // ---- disassemble
            let world_e = world.resource::<crate::world::WorldEntity>().0;
            let at = gt.translation();
            if crate::assembly::merge_into(world, u.grid, world_e) {
                world.write_message(toast("Физика выключена: блоки вернулись в сетку"));
                world.write_message(crate::sound::sfx_at("physics_off", at));
            } else {
                world.write_message(toast("Нет места, чтобы вернуть блоки в сетку"));
            }
        }
    }
}

/// Slightly over half a block: blocks at axle height clear the ground.
pub const WHEEL_RADIUS: f32 = 0.6;
const TRAVEL: f32 = 0.35;
/// Ground clearance added by the springs: blocks at axle level ride this
/// much above the ground instead of scraping it.
pub const RIDE: f32 = 0.25;

/// How far the wheel hangs below its built position (suspension travel),
/// for the visuals.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct WheelDrop(pub f32);
/// Radius of the wheel's ground probe (m).
const PROBE: f32 = 0.25;

/// Raycast suspension, drive and side grip of every wheel.
#[allow(clippy::type_complexity)]
fn drive(
    time: Res<Time>,
    spatial: SpatialQuery,
    gravity: Res<Gravity>,
    mut bodies: Query<(Entity, &VoxelGrid, &ChunkViews, &GlobalTransform, &ComputedMass, Forces)>,
    mut wheels: Query<(&BlockEntity, &mut Spin, Option<&mut WheelDrop>, &crate::mech::NetState, Option<&mut crate::mech::WheelLoad>), Without<Steer>>,
    mut commands: Commands,
    susp: Query<(&BlockEntity, &Steer, &Spin, &crate::mech::Params)>,
) {
    let dt = time.delta_secs().max(1e-4);
    let g = gravity.0.length();
    for (_, grid, views, gt, mass, mut forces) in &mut bodies {
        let list: Vec<Entity> = grid.entities.values().copied().filter(|e| wheels.get(*e).is_ok_and(|(b, ..)| b.mech() == Some(Mech::Wheel))).collect();
        if list.is_empty() {
            continue;
        }
        // No colliders yet (just assembled): no mass to work with.
        if !mass.value().is_finite() || mass.value() < 1.0 {
            continue;
        }
        let n = list.len() as f32;
        let per_wheel = mass.value() / n;
        let filter = SpatialQueryFilter::default().with_excluded_entities(views.0.values().copied());
        let t = gt.compute_transform();
        let up = t.rotation * Vec3::Y;
        for e in list {
            let Ok((wb, mut spin, drop, net, mut load)) = wheels.get_mut(e) else { continue };
            if let Some(l) = load.as_mut() {
                l.0 = 0.0;
            }
            // The suspension holding this wheel (on its axle side).
            let s = grid.entities.get(&(wb.cell - wb.dir())).and_then(|se| susp.get(*se).ok()).filter(|(b, ..)| b.mech() == Some(Mech::Suspension));
            let Some((sb, steer, _, params)) = s else { continue };
            let stiffness = params.get(sb.id, "stiffness") / 5.0;
            let center = t.transform_point(wb.cell.as_vec3() + Vec3::splat(0.5));
            let axle0 = t.rotation * wb.dir().as_vec3();
            let axle = Quat::from_axis_angle(up, steer.0) * axle0;
            let forward = axle.cross(up).normalize_or_zero();
            // Spring rate: at rest each wheel carries its share of the weight
            // with `eq` compression; the ray is that much longer, so the car
            // rests at the height it was built.
            let k = per_wheel * g / (TRAVEL * 0.5) * stiffness;
            let eq = per_wheel * g / k;
            // Distances from the wheel centre: the spring is free at `rest`.
            let rest = WHEEL_RADIUS + RIDE + eq;
            let origin = center + up * TRAVEL;
            // A small sphere instead of a thin ray: it rolls over block
            // edges, steps and slope joints instead of snapping at them.
            let hit = spatial.cast_shape(&Collider::sphere(PROBE), origin, Quat::IDENTITY, Dir3::new(-up).unwrap_or(Dir3::NEG_Y), &ShapeCastConfig::from_max_distance(TRAVEL + rest + TRAVEL - PROBE), &filter);
            let ground = hit.map(|h| h.distance + PROBE - TRAVEL).unwrap_or(rest + TRAVEL);
            let hang = (ground - WHEEL_RADIUS).clamp(-TRAVEL, RIDE + eq);
            match drop {
                Some(mut d) => d.0 = hang,
                None => {
                    commands.entity(e).insert(WheelDrop(hang));
                }
            }
            let compression = rest - ground;
            if hit.is_none() || compression <= 0.0 {
                continue;
            }
            let contact = center - up * ground;
            let v = forces.velocity_at_point(contact);
            // Spring + damper along the body's up.
            let damp = 2.0 * (k * per_wheel).sqrt() * 0.6;
            let normal_force = (k * compression - damp * v.dot(up)).max(0.0);
            forces.apply_force_at_point(up * normal_force, contact);
            let grip = normal_force * 1.1;
            // Drive: the wheel's network speed is the wanted ground speed.
            let v_long = v.dot(forward);
            // Network speed is about the grid +axis; turn it into a speed
            // about the wheel's own facing.
            let d = wb.dir();
            let rpm = spin.rpm * (d.x + d.y + d.z) as f32;
            // Held by a drivetrain with a source (0 RPM = brake); otherwise free.
            let driven = net.driven;
            if driven {
                let target = rpm / 60.0 * std::f32::consts::TAU * WHEEL_RADIUS;
                // Stress on the drivetrain (per 32 RPM): rolling, plus the
                // climb when it drives uphill (not when it brakes downhill).
                if let Some(l) = load.as_mut() {
                    let uphill = if target != 0.0 { (forward * target.signum()).y.max(0.0) } else { 0.0 };
                    l.0 = wheel_stress(mass.value(), uphill);
                }
                let f = ((target - v_long) * per_wheel / dt * 0.2).clamp(-grip, grip);
                forces.apply_force_at_point(forward * f, contact);
            } else {
                // Free wheel: roll with the ground (visual).
                spin.angle += v_long / WHEEL_RADIUS * dt * (d.x + d.y + d.z) as f32;
            }
            // Side grip.
            let v_lat = v.dot(axle);
            let f_lat = (-v_lat * per_wheel / dt * 0.3).clamp(-grip, grip);
            forces.apply_force_at_point(axle * f_lat, contact);
        }
    }
}

/// Stress of a driven wheel at 32 RPM: rolling (mass/4 × 0.08) and the
/// slope (mass × sin θ uphill).
pub fn wheel_stress(mass: f32, sin_uphill: f32) -> f32 {
    mass / 4.0 * 0.08 + mass * sin_uphill.max(0.0)
}

pub struct ContraptionPlugin;

impl Plugin for ContraptionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle).add_systems(FixedUpdate, drive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::BlockId, grid::Cell};

    #[test]
    fn only_glued_blocks_join() {
        let mut g = VoxelGrid::default();
        let b = Cell::new(BlockId::k("stone"), 2);
        for x in 0..4 {
            g.set(IVec3::new(x, 0, 0), b);
        }
        assert_eq!(glued_group(&g, None, IVec3::ZERO), vec![IVec3::ZERO]);
        let glue = Glue(vec![(IVec3::ZERO, IVec3::new(2, 0, 0))]);
        assert_eq!(glued_group(&g, Some(&glue), IVec3::ZERO).len(), 3);
    }

    #[test]
    fn wheels_load_the_engine_on_a_climb() {
        // A 4 t car: 80 SU rolling per wheel, a 30° climb adds 2000 SU.
        assert_eq!(wheel_stress(4000.0, 0.0), 80.0);
        assert!((wheel_stress(4000.0, 0.5) - 2080.0).abs() < 1e-3);
        assert_eq!(wheel_stress(4000.0, -0.5), 80.0, "downhill: no extra load");
    }

    #[test]
    fn snapping_rotations() {
        let q = Quat::from_rotation_y(0.2) * Quat::from_rotation_x(-0.1);
        assert!(snap_rotation(q).angle_between(Quat::IDENTITY) < 1e-4);
        let q = Quat::from_rotation_y(1.4);
        assert!(snap_rotation(q).angle_between(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)) < 1e-4);
    }
}
