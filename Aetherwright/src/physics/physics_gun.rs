//! Physics gun ("гравитационный захват").
//!
//! * **Grab** (LMB hold): the object's velocity is steered towards a point in
//!   front of the player with an acceleration limited to `GUN_FORCE / mass`.
//!   Heavy objects therefore lag, and objects heavier than ~`GUN_FORCE / g`
//!   cannot be lifted at all (they can still be dragged). Collisions keep
//!   working because the body is never teleported.
//!   Wheel — distance; R + mouse — rotate; RMB — freeze in space; releasing
//!   LMB keeps the momentum (throw).
//! * **Weld** / **Hinge**: click the part, then the base (another body or the
//!   terrain). Hinge snaps the part's clicked face onto the base face and
//!   rotates about the face normal. **Unlink**: click to remove all joints of
//!   an object. Q cycles modes.

use avian3d::prelude::*;
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use super::{
    joints::{spawn_connection, Connection, JointKind, LocalFrame},
    Frozen, Immovable,
};
use crate::{
    core::{events::Notify, resources::EyeRay, states::gameplay_active},
    tools::{ActiveTool, EntityTarget, WheelCaptured},
    world::{interaction::BlockTarget, ChunkEntities, chunks::VoxelWorld},
};

/// Max force of the gun (N). Lift limit ≈ GUN_FORCE / g.
pub const GUN_FORCE: f32 = 4000.0;
const MAX_SPEED: f32 = 18.0;
const MAX_HOLD_DIST: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GunMode {
    #[default]
    Grab,
    Weld,
    Hinge,
    Unlink,
}

impl GunMode {
    pub const ALL: [GunMode; 4] = [GunMode::Grab, GunMode::Weld, GunMode::Hinge, GunMode::Unlink];
    pub fn label(self) -> &'static str {
        match self {
            GunMode::Grab => "Захват",
            GunMode::Weld => "Сварка",
            GunMode::Hinge => "Шарнир",
            GunMode::Unlink => "Разъединение",
        }
    }
}

/// Marker on the body currently held.
#[derive(Component, Debug)]
pub struct Held;

#[derive(Debug, Clone, Copy)]
pub struct HeldInfo {
    pub entity: Entity,
    pub distance: f32,
    /// Grab point in body-local space.
    pub local_point: Vec3,
}

/// First click of a two-click joint operation.
#[derive(Debug, Clone, Copy)]
pub struct PendingPick {
    pub entity: Entity,
    pub local_point: Vec3,
    pub local_normal: Vec3,
}

#[derive(Resource, Debug, Default)]
pub struct PhysGun {
    pub mode: GunMode,
    pub held: Option<HeldInfo>,
    pub pending: Option<PendingPick>,
    /// Rotation input accumulated this frame (radians around yaw/pitch axes).
    pub rotate: Vec2,
}

/// A click target for joint tools: a body or a terrain block.
struct Pick {
    entity: Entity,
    point: Vec3,
    normal: Vec3,
}

fn pick(entity_target: &EntityTarget, block: &BlockTarget, eye: &EyeRay, world: &VoxelWorld, chunks: &ChunkEntities) -> Option<Pick> {
    if let Some(e) = entity_target.entity {
        return Some(Pick { entity: e, point: entity_target.point, normal: entity_target.normal });
    }
    let hit = block.0?;
    let chunk = *chunks.0.get(&world.chunk_of(hit.pos))?;
    Some(Pick { entity: chunk, point: eye.origin + eye.direction * hit.distance, normal: hit.normal.as_vec3() })
}

#[allow(clippy::too_many_arguments)]
fn gun_input(
    mut commands: Commands,
    tool: Res<ActiveTool>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    eye: Res<EyeRay>,
    entity_target: Res<EntityTarget>,
    block_target: Res<BlockTarget>,
    voxels: Res<VoxelWorld>,
    chunks: Res<ChunkEntities>,
    mut gun: ResMut<PhysGun>,
    mut wheel: ResMut<WheelCaptured>,
    mut bodies: Query<(&RigidBody, &mut Position, &mut Rotation, &ComputedMass, Has<Immovable>, Has<Frozen>)>,
    joints: Query<(Entity, &Connection)>,
    mut notify: MessageWriter<Notify>,
) {
    if *tool != ActiveTool::PhysGun {
        if let Some(h) = gun.held.take() {
            commands.entity(h.entity).remove::<Held>();
        }
        gun.pending = None;
        wheel.0 = false;
        return;
    }
    if keys.just_pressed(KeyCode::KeyQ) && gun.held.is_none() {
        let i = GunMode::ALL.iter().position(|m| *m == gun.mode).unwrap_or(0);
        gun.mode = GunMode::ALL[(i + 1) % GunMode::ALL.len()];
        gun.pending = None;
        notify.write(Notify::info(format!("Гравизахват: {}", gun.mode.label())));
    }

    match gun.mode {
        GunMode::Grab => {
            if let Some(h) = gun.held {
                // Distance with the wheel.
                let d = match scroll.unit {
                    MouseScrollUnit::Line => scroll.delta.y * 0.6,
                    MouseScrollUnit::Pixel => scroll.delta.y * 0.02,
                };
                gun.held.as_mut().unwrap().distance = (h.distance + d).clamp(1.5, MAX_HOLD_DIST);
                gun.rotate = if keys.pressed(KeyCode::KeyR) { motion.delta * 0.01 } else { Vec2::ZERO };
                if mouse.just_pressed(MouseButton::Right) {
                    // Freeze in space.
                    commands.entity(h.entity).remove::<Held>().insert((Frozen, RigidBody::Static));
                    gun.held = None;
                    notify.write(Notify::info("Объект зафиксирован"));
                } else if !mouse.pressed(MouseButton::Left) {
                    commands.entity(h.entity).remove::<Held>();
                    gun.held = None;
                }
            } else if mouse.just_pressed(MouseButton::Left)
                && let Some(e) = entity_target.entity
                && let Ok((rb, pos, rot, mass, immovable, frozen)) = bodies.get(e)
            {
                if immovable {
                    notify.write(Notify::warn("Системный объект нельзя перемещать"));
                } else if rb.is_static() && !frozen {
                    notify.write(Notify::warn("Объект закреплён"));
                } else {
                    if frozen {
                        commands.entity(e).remove::<Frozen>().insert(RigidBody::Dynamic);
                    }
                    if mass.value() * 19.0 > GUN_FORCE {
                        notify.write(Notify::warn(format!("Слишком тяжело поднять ({:.0} кг) — только волочь", mass.value())));
                    }
                    let local_point = rot.0.inverse() * (entity_target.point - pos.0);
                    gun.held = Some(HeldInfo { entity: e, distance: entity_target.distance.max(1.5), local_point });
                    commands.entity(e).insert(Held);
                }
            } else if mouse.just_pressed(MouseButton::Right)
                && let Some(e) = entity_target.entity
                && let Ok((_, _, _, _, _, true)) = bodies.get(e)
            {
                commands.entity(e).remove::<Frozen>().insert(RigidBody::Dynamic);
                notify.write(Notify::info("Фиксация снята"));
            }
            wheel.0 = gun.held.is_some();
        }
        GunMode::Weld | GunMode::Hinge => {
            wheel.0 = false;
            if mouse.just_pressed(MouseButton::Right) {
                gun.pending = None;
            }
            if !mouse.just_pressed(MouseButton::Left) {
                return;
            }
            let Some(p) = pick(&entity_target, &block_target, &eye, &voxels, &chunks) else { return };
            match gun.pending {
                None => {
                    // The first pick must be a movable body (the part).
                    let Ok((rb, pos, rot, ..)) = bodies.get(p.entity) else { return };
                    if rb.is_static() && chunks.0.values().any(|c| *c == p.entity) {
                        notify.write(Notify::warn("Сначала выберите деталь, затем основание"));
                        return;
                    }
                    gun.pending = Some(PendingPick {
                        entity: p.entity,
                        local_point: rot.0.inverse() * (p.point - pos.0),
                        local_normal: rot.0.inverse() * p.normal,
                    });
                }
                Some(first) => {
                    gun.pending = None;
                    if first.entity == p.entity {
                        return;
                    }
                    let exists = joints.iter().any(|(_, c)| (c.a == first.entity && c.b == p.entity) || (c.b == first.entity && c.a == p.entity));
                    if exists {
                        notify.write(Notify::warn("Уже соединены"));
                        return;
                    }
                    let Ok((_, pos_b, rot_b, ..)) = bodies.get(p.entity) else { return };
                    let (pos_b, rot_b) = (pos_b.0, rot_b.0);
                    let kind = if gun.mode == GunMode::Weld { JointKind::Weld } else { JointKind::Hinge };
                    let Ok((_, mut pos_a, mut rot_a, ..)) = bodies.get_mut(first.entity) else { return };
                    let (anchor, basis) = if kind == JointKind::Hinge {
                        // Snap the part's face onto the base face.
                        let n_a = rot_a.0 * first.local_normal;
                        let q = Quat::from_rotation_arc(n_a.normalize(), -p.normal.normalize());
                        let new_rot = (q * rot_a.0).normalize();
                        let new_pos = p.point - new_rot * first.local_point + p.normal * 0.02;
                        rot_a.0 = new_rot;
                        pos_a.0 = new_pos;
                        commands.entity(first.entity).insert((Transform::from_translation(new_pos).with_rotation(new_rot), LinearVelocity::ZERO, AngularVelocity::ZERO));
                        (p.point, Quat::from_rotation_arc(Vec3::Z, p.normal.normalize()))
                    } else {
                        (p.point, Quat::IDENTITY)
                    };
                    let (pa, ra) = (pos_a.0, rot_a.0);
                    spawn_connection(
                        &mut commands,
                        Connection {
                            a: p.entity,
                            b: first.entity,
                            kind,
                            frame_a: LocalFrame::from_world(pos_b, rot_b, anchor, basis),
                            frame_b: LocalFrame::from_world(pa, ra, anchor, basis),
                        },
                    );
                    notify.write(Notify::ok(if kind == JointKind::Weld { "Сварено" } else { "Шарнир установлен" }));
                }
            }
        }
        GunMode::Unlink => {
            wheel.0 = false;
            if mouse.just_pressed(MouseButton::Left)
                && let Some(e) = entity_target.entity
            {
                let mut n = 0;
                for (je, c) in &joints {
                    if c.a == e || c.b == e {
                        commands.entity(je).despawn();
                        n += 1;
                    }
                }
                notify.write(Notify::info(format!("Разъединено соединений: {n}")));
            }
        }
    }
}

/// Fixed-step drive of the held body (mass-limited acceleration).
///
/// The whole construction (bodies linked by joints) is driven together:
/// the acceleration limit uses its total mass and every dynamic member gets
/// the same velocity change, so welded groups move as one piece instead of
/// the joint solver fighting the gun. If the construction is anchored to
/// something static, only the held body is pushed (the anchor resists).
#[allow(clippy::type_complexity)]
fn drive_held(
    time: Res<Time>,
    gun: Res<PhysGun>,
    eye: Res<EyeRay>,
    gravity: Res<Gravity>,
    mut commands: Commands,
    joints: Query<&Connection>,
    mut bodies: Query<(&RigidBody, &Position, &Rotation, &mut LinearVelocity, &mut AngularVelocity, &ComputedMass)>,
) {
    let Some(h) = gun.held else { return };
    let pairs: Vec<(Entity, Entity)> = joints.iter().map(|c| (c.a, c.b)).collect();
    let group = super::joints::construction_of(h.entity, &pairs);
    let anchored = group.iter().any(|e| bodies.get(*e).is_ok_and(|b| !b.0.is_dynamic()));
    let members: Vec<Entity> = if anchored { vec![h.entity] } else { group };
    let total_mass: f32 = members.iter().filter_map(|e| bodies.get(*e).ok()).map(|b| b.5.value()).sum();

    let Ok((_, pos, rot, lin, _, _)) = bodies.get(h.entity) else { return };
    let dt = time.delta_secs();
    let target = eye.origin + eye.direction * h.distance;
    let grab = pos.0 + rot.0 * h.local_point;
    if grab.distance(eye.origin) > MAX_HOLD_DIST * 1.8 {
        commands.entity(h.entity).remove::<Held>();
        return;
    }
    let desired = ((target - grab) * 10.0).clamp_length_max(MAX_SPEED);
    let a_max = GUN_FORCE / total_mass.max(0.1);
    // Velocity change needed this tick, including holding against gravity.
    let dv = (desired - lin.0 - gravity.0 * dt).clamp_length_max(a_max * dt);
    for e in members {
        if let Ok((rb, _, _, mut lin, mut ang, _)) = bodies.get_mut(e) {
            if !rb.is_dynamic() {
                continue;
            }
            lin.0 += dv;
            if e == h.entity && gun.rotate != Vec2::ZERO {
                // R + mouse spins around the view axes.
                let right = eye.direction.cross(Vec3::Y).normalize_or_zero();
                ang.0 = Vec3::Y * gun.rotate.x * 60.0 + right * gun.rotate.y * 60.0;
            } else {
                ang.0 *= 0.85;
            }
        }
    }
}

fn draw_gun(
    tool: Res<ActiveTool>,
    gun: Res<PhysGun>,
    eye: Res<EyeRay>,
    bodies: Query<(&Position, &Rotation)>,
    mut gizmos: Gizmos,
) {
    if *tool != ActiveTool::PhysGun {
        return;
    }
    let right = eye.direction.cross(Vec3::Y).normalize_or_zero();
    let start = eye.origin + eye.direction * 0.5 + right * 0.28 - Vec3::Y * 0.22;
    let color = Color::srgb(1.0, 0.65, 0.2);
    if let Some(h) = gun.held
        && let Ok((p, r)) = bodies.get(h.entity)
    {
        let grab = p.0 + r.0 * h.local_point;
        let mid = (start + grab) * 0.5 + Vec3::Y * 0.3;
        gizmos.line(start, mid, color);
        gizmos.line(mid, grab, color);
    }
    if let Some(pp) = gun.pending
        && let Ok((p, r)) = bodies.get(pp.entity)
    {
        let pt = p.0 + r.0 * pp.local_point;
        gizmos.sphere(Isometry3d::from_translation(pt), 0.12, Color::srgb(0.4, 1.0, 0.8));
        gizmos.line(pt, pt + r.0 * pp.local_normal * 0.6, Color::srgb(0.4, 1.0, 0.8));
    }
}

pub struct PhysicsGunPlugin;

impl Plugin for PhysicsGunPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PhysGun>()
            .add_systems(
                Update,
                (gun_input, draw_gun)
                    .chain()
                    .after(crate::tools::update_entity_target)
                    .run_if(gameplay_active),
            )
            .add_systems(FixedUpdate, drive_held.run_if(crate::core::states::in_game));
    }
}
