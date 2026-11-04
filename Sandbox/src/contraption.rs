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
    core::{Toast, toast},
    grid::{Cell, DIRS, VoxelGrid, WorldGrid, facing_dir, facing_of, render::ChunkViews},
    interact::UseBlock,
    mech::{BlockEntity, Spin},
    signal::Linked,
    tools::glue::{Glue, inside},
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
    let mut seen = HashSet::from([start]);
    let mut q = VecDeque::from([start]);
    while let Some(c) = q.pop_front() {
        let Some(g) = glue else { break };
        for d in DIRS {
            let n = c + d;
            if !seen.contains(&n) && !grid.get(n).is_air() && g.contains(c, n) {
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

#[allow(clippy::too_many_arguments)]
fn toggle(
    mut commands: Commands,
    mut used: MessageReader<UseBlock>,
    mut grids: Query<(&mut VoxelGrid, &GlobalTransform, Option<&Glue>, Has<WorldGrid>, Option<&Contraption>)>,
    mut blocks: Query<(&mut BlockEntity, &mut Transform)>,
    mut links: Query<&mut Linked>,
    world_e: Res<crate::world::WorldEntity>,
    mut toasts: MessageWriter<Toast>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
) {
    for u in used.read() {
        if u.mech != Mech::PhysicsToggle {
            continue;
        }
        let Ok((_, gt, _, is_world, contraption)) = grids.get(u.grid) else { continue };
        if is_world {
            // ---- assemble
            let gt = *gt;
            let (cells, moved_cells, glue_boxes, mass) = {
                let (grid, _, glue, _, _) = grids.get(u.grid).unwrap();
                let group = glued_group(&grid, glue, u.cell);
                let cells: Vec<(IVec3, Cell)> = group.iter().map(|c| (*c, grid.get(*c))).collect();
                let boxes: Vec<(IVec3, IVec3)> = glue.map(|g| g.0.iter().copied().filter(|(lo, hi)| group.iter().any(|c| inside(*c, *lo, *hi))).collect()).unwrap_or_default();
                let mass: f32 = cells.iter().map(|(_, c)| c.id.def().mass).sum();
                (cells, group, boxes, mass)
            };
            let body = commands
                .spawn((
                    Name::new("Contraption"),
                    Contraption { mass },
                    VoxelGrid::default(),
                    Glue(glue_boxes.clone()),
                    gt.compute_transform(),
                    Visibility::default(),
                    RigidBody::Dynamic,
                    LinearDamping(0.05),
                    AngularDamping(0.3),
                ))
                .id();
            let mut new_grid = VoxelGrid::default();
            let mut moved_entities = Vec::new();
            {
                let (mut world_grid, _, _, _, _) = grids.get_mut(u.grid).unwrap();
                for (c, cell) in &cells {
                    if let Some(e) = world_grid.entities.remove(c) {
                        new_grid.entities.insert(*c, e);
                        moved_entities.push(e);
                    }
                    world_grid.set(*c, Cell::AIR);
                    new_grid.set(*c, *cell);
                }
            }
            // Remove the glue that went with it from the world.
            if let Ok((_, _, Some(g), _, _)) = grids.get(u.grid) {
                let left: Vec<(IVec3, IVec3)> = g.0.iter().copied().filter(|b| !glue_boxes.contains(b)).collect();
                commands.entity(u.grid).insert(Glue(left));
            }
            for e in moved_entities {
                if let Ok((mut b, _)) = blocks.get_mut(e) {
                    b.grid = body;
                }
                commands.entity(e).insert(ChildOf(body));
            }
            for mut l in &mut links {
                if let Some(p) = l.0.as_mut()
                    && p.grid == u.grid
                    && moved_cells.contains(&p.cell)
                {
                    p.grid = body;
                }
            }
            commands.entity(body).insert(new_grid);
            toasts.write(toast(format!("Физика включена: блоков {}, масса {:.0} кг", cells.len(), mass)));
            sounds.write(crate::sound::sfx_at("physics_on", gt.translation() + u.cell.as_vec3()));
        } else if contraption.is_some() {
            // ---- disassemble
            let world = world_e.0;
            let t = gt.compute_transform();
            let rot = snap_rotation(t.rotation);
            let to_world = |c: IVec3| {
                let p = t.translation + rot * (c.as_vec3() + Vec3::splat(0.5));
                p.floor().as_ivec3()
            };
            let (cells, glue_boxes) = {
                let (grid, _, glue, _, _) = grids.get(u.grid).unwrap();
                (grid.cells(), glue.map(|g| g.0.clone()).unwrap_or_default())
            };
            let blocked = {
                let (wg, _, _, _, _) = grids.get(world).unwrap();
                cells.iter().any(|(c, _)| !wg.get(to_world(*c)).is_air())
            };
            if blocked {
                toasts.write(toast("Нет места, чтобы вернуть блоки в сетку"));
                continue;
            }
            let entities: Vec<(IVec3, Entity)> = grids.get(u.grid).unwrap().0.entities.iter().map(|(c, e)| (*c, *e)).collect();
            {
                let (mut wg, _, _, _, _) = grids.get_mut(world).unwrap();
                for (c, cell) in &cells {
                    let f = facing_of((rot * facing_dir(cell.facing).as_vec3()).round().as_ivec3());
                    let wc = to_world(*c);
                    if let Some((_, e)) = entities.iter().find(|(ec, _)| ec == c) {
                        wg.entities.insert(wc, *e);
                    }
                    wg.set(wc, Cell::new(cell.id, f));
                }
            }
            for (c, e) in &entities {
                if let Ok((mut b, mut tr)) = blocks.get_mut(*e) {
                    b.grid = world;
                    b.cell = to_world(*c);
                    b.facing = facing_of((rot * facing_dir(b.facing).as_vec3()).round().as_ivec3());
                    *tr = crate::mech::transform_for(b.cell, b.id, b.facing);
                }
                commands.entity(*e).insert(ChildOf(world));
            }
            for mut l in &mut links {
                if let Some(p) = l.0.as_mut()
                    && p.grid == u.grid
                {
                    p.grid = world;
                    p.cell = to_world(p.cell);
                }
            }
            let mapped: Vec<(IVec3, IVec3)> = glue_boxes
                .iter()
                .map(|(lo, hi)| {
                    let (a, b) = (to_world(*lo), to_world(*hi));
                    (a.min(b), a.max(b))
                })
                .collect();
            if let Ok((_, _, g, _, _)) = grids.get(world) {
                let mut all = g.map(|g| g.0.clone()).unwrap_or_default();
                all.extend(mapped);
                commands.entity(world).insert(Glue(all));
            }
            {
                let (mut cg, _, _, _, _) = grids.get_mut(u.grid).unwrap();
                cg.entities.clear();
            }
            commands.entity(u.grid).despawn();
            toasts.write(toast("Физика выключена: блоки вернулись в сетку"));
            sounds.write(crate::sound::sfx_at("physics_off", t.translation));
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
    mut wheels: Query<(&BlockEntity, &mut Spin, Option<&mut WheelDrop>, &crate::mech::NetState), Without<Steer>>,
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
            let Ok((wb, mut spin, drop, net)) = wheels.get_mut(e) else { continue };
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

pub struct ContraptionPlugin;

impl Plugin for ContraptionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle).add_systems(FixedUpdate, drive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

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
    fn snapping_rotations() {
        let q = Quat::from_rotation_y(0.2) * Quat::from_rotation_x(-0.1);
        assert!(snap_rotation(q).angle_between(Quat::IDENTITY) < 1e-4);
        let q = Quat::from_rotation_y(1.4);
        assert!(snap_rotation(q).angle_between(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)) < 1e-4);
    }
}
