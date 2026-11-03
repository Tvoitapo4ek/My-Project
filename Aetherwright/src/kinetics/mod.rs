//! Kinetic network (mechanical transmission), Create-style.
//!
//! Grid blocks (shafts, cogwheels, gearboxes, clutches, gearshifts, cranks,
//! motors, bearings) are static entities on cells — not physics bodies.
//! `network::solve` turns them into speeds whenever something changes
//! (placed, removed, toggled, a source starts or stops); rotors are then
//! only *drawn* rotating (angle += rpm·dt). Machines take rotation from an
//! adjacent block (`KineticConsumer`), bearings hand it to physics through
//! a hinge motor — the only place where physics is involved.
//!
//! Numbers come from `content/kinetics.ron`; see `docs/KINETICS.md`.

pub mod network;

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::{light::NotShadowCaster, prelude::*};
use serde::{Deserialize, Serialize};

use network::{Axis, Node, Role, Solution};
use crate::{
    building::preview::{GhostShape, Ghosts, Validity},
    content::{KineticDef, registry},
    core::{events::Notify, layers::Layer, rules::ModeRules, states::gameplay_active},
    items::{ItemId, Use, inventory::Inventory},
    machinery::components::{Part, PartKind},
    physics::{
        joints::{Connection, JointKind},
        rigid_bodies::CollectProp,
    },
    player::Player,
    save::world_save::{SaveHook, SaveHookExt},
    tools::{ActiveTool, EntityTarget, manipulator::MechanicsUse, scanning::Scannable},
    world::{chunks::VoxelWorld, interaction::BlockTarget},
};

/// A kinetic block on the grid.
#[derive(Component, Debug, Clone)]
pub struct KineticBlock {
    pub key: String,
    pub cell: IVec3,
    pub axis: Axis,
    /// Clutch disengaged / gearshift reversed.
    pub toggled: bool,
    /// Motor switched on.
    pub on: bool,
    /// Motor RPM setting.
    pub rpm: f32,
    /// Crank: seconds of turning left.
    pub crank: f32,
    /// Motor: had energy on the last tick.
    pub powered: bool,
    /// Share of the nominal power a source can give now (magic engine:
    /// how much aether of what stability it gets).
    pub power_scale: f32,
}

impl KineticBlock {
    pub fn def(&self) -> Option<&'static KineticDef> {
        registry().kinetic(&self.key)
    }
}

/// Result of the last solve for this block.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct KineticState {
    pub rpm: f32,
    pub net: usize,
}

/// Machine that takes rotation from an adjacent kinetic block.
#[derive(Component, Debug, Clone, Copy)]
pub struct KineticConsumer {
    /// SU per RPM while working.
    pub impact: f32,
}

/// RPM delivered to a consumer.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct KineticDrive(pub f32);

/// The glowing core of a static device (logic shows its output with it).
#[derive(Component)]
pub struct Core;

#[derive(Component)]
struct Rotor {
    angle: f32,
}

#[derive(Resource, Default, Debug)]
pub struct KineticGrid {
    pub blocks: HashMap<IVec3, Entity>,
    pub consumers: HashMap<IVec3, Entity>,
    pub dirty: bool,
    pub solution: Solution,
}

/// Cell of a world position.
pub fn cell_of(p: Vec3) -> IVec3 {
    p.floor().as_ivec3()
}

fn axis_rotation(axis: Axis) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, axis.unit().as_vec3())
}

#[derive(Resource, Clone)]
struct KineticAssets {
    shaft: Handle<Mesh>,
    cog: Handle<Mesh>,
    tooth: Handle<Mesh>,
    large_cog: Handle<Mesh>,
    casing: Handle<Mesh>,
    plate: Handle<Mesh>,
    handle: Handle<Mesh>,
    andesite: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    brass: Handle<StandardMaterial>,
    copper: Handle<StandardMaterial>,
    glow: Handle<StandardMaterial>,
    /// Tinted housings by colour (built at startup for every item colour).
    tints: Vec<(Color, Handle<StandardMaterial>)>,
}

fn tinted(a: &KineticAssets, c: Color) -> Handle<StandardMaterial> {
    a.tints.iter().find(|(k, _)| *k == c).map(|(_, h)| h.clone()).unwrap_or_else(|| a.andesite.clone())
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mat = |m: &mut Assets<StandardMaterial>, c: Color, metal: f32| {
        m.add(StandardMaterial { base_color: c, metallic: metal, perceptual_roughness: 0.55, ..default() })
    };
    commands.insert_resource(KineticAssets {
        shaft: meshes.add(Cylinder::new(0.11, 1.0)),
        cog: meshes.add(Cylinder::new(0.4, 0.22)),
        tooth: meshes.add(Cuboid::new(0.14, 0.22, 0.14)),
        large_cog: meshes.add(Cylinder::new(0.88, 0.22)),
        casing: meshes.add(Cuboid::new(0.84, 0.84, 0.84)),
        plate: meshes.add(Cylinder::new(0.4, 0.12)),
        handle: meshes.add(Cuboid::new(0.12, 0.12, 0.5)),
        andesite: mat(&mut mats, Color::srgb(0.58, 0.6, 0.57), 0.1),
        wood: mat(&mut mats, Color::srgb(0.45, 0.32, 0.2), 0.0),
        brass: mat(&mut mats, Color::srgb(0.86, 0.66, 0.3), 0.8),
        copper: mat(&mut mats, Color::srgb(0.78, 0.44, 0.28), 0.7),
        glow: mats.add(StandardMaterial { base_color: Color::srgb(0.8, 0.6, 1.0), emissive: LinearRgba::rgb(2.0, 1.2, 4.0), ..default() }),
        tints: registry()
            .kinetics
            .iter()
            .filter(|k| k.role == Role::Static)
            .filter_map(|k| ItemId::from_key(&k.item).map(|i| i.color()))
            .map(|c| (c, mats.add(StandardMaterial { base_color: c, metallic: 0.3, perceptual_roughness: 0.45, ..default() })))
            .collect(),
    });
}

fn collider(role: Role) -> Collider {
    match role {
        Role::Shaft => Collider::cuboid(0.3, 1.0, 0.3),
        Role::Cog => Collider::cylinder(0.45, 0.3),
        Role::LargeCog => Collider::cylinder(0.92, 0.3),
        _ => Collider::cuboid(0.9, 0.9, 0.9),
    }
}

/// Spawns a kinetic block (also used by loading and tests).
pub fn spawn_kinetic(commands: &mut Commands, a: &KineticAssetsRef, key: &str, cell: IVec3, axis: Axis) -> Option<Entity> {
    let a = &a.0;
    let def = registry().kinetic(key)?;
    let name = ItemId::from_key(&def.item).map(|i| i.def().name.clone()).unwrap_or_else(|| key.to_string());
    let info = ItemId::from_key(&def.item).map(|i| i.def().info.clone()).unwrap_or_default();
    let mut e = commands.spawn((
        Name::new(format!("Kinetic {key}")),
        KineticBlock {
            key: key.to_string(),
            cell,
            axis,
            toggled: false,
            on: true,
            rpm: def.source_rpm,
            crank: 0.0,
            powered: false,
            power_scale: 1.0,
        },
        KineticState::default(),
        Scannable { title: name, info },
        Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5)).with_rotation(axis_rotation(axis)),
        Visibility::default(),
        RigidBody::Static,
        collider(def.role),
        CollisionLayers::new(Layer::Prop, LayerMask::ALL),
    ));
    let role = def.role;
    e.with_children(|p| {
        // Non-rotating housing.
        match role {
            Role::Gearbox => {
                p.spawn((Mesh3d(a.casing.clone()), MeshMaterial3d(a.andesite.clone()), Transform::default()));
            }
            Role::Clutch | Role::Gearshift => {
                p.spawn((
                    Mesh3d(a.casing.clone()),
                    MeshMaterial3d(if role == Role::Clutch { a.andesite.clone() } else { a.brass.clone() }),
                    Transform::from_scale(Vec3::new(0.9, 0.55, 0.9)),
                ));
            }
            Role::Motor => {
                p.spawn((Mesh3d(a.casing.clone()), MeshMaterial3d(a.copper.clone()), Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::new(1.0, 0.8, 1.0))));
            }
            Role::Static => {
                // Magical / passive device: tinted housing and a glowing core.
                let c = ItemId::from_key(&def.item).map(|i| i.color()).unwrap_or(Color::WHITE);
                p.spawn((Mesh3d(a.casing.clone()), MeshMaterial3d(tinted(&a, c)), Transform::from_scale(Vec3::new(0.8, 0.8, 0.8))));
                p.spawn((Mesh3d(a.plate.clone()), MeshMaterial3d(a.glow.clone()), Transform::from_xyz(0.0, 0.36, 0.0).with_scale(Vec3::new(0.5, 0.6, 0.5)), Core));
            }
            Role::Bearing | Role::Crank => {
                p.spawn((Mesh3d(a.casing.clone()), MeshMaterial3d(a.andesite.clone()), Transform::from_xyz(0.0, -0.15, 0.0).with_scale(Vec3::new(1.0, 0.65, 1.0))));
            }
            _ => {}
        }
        // Rotor (rotates about local Y = block axis).
        p.spawn((Rotor { angle: 0.0 }, Transform::default(), Visibility::default())).with_children(|r| {
            match role {
                Role::Cog | Role::LargeCog => {
                    let (mesh, rad, teeth) = if role == Role::Cog { (a.cog.clone(), 0.4, 8) } else { (a.large_cog.clone(), 0.88, 16) };
                    r.spawn((Mesh3d(mesh), MeshMaterial3d(a.wood.clone()), Transform::default()));
                    for i in 0..teeth {
                        let ang = i as f32 / teeth as f32 * std::f32::consts::TAU;
                        r.spawn((
                            Mesh3d(a.tooth.clone()),
                            MeshMaterial3d(a.andesite.clone()),
                            Transform::from_xyz(ang.cos() * (rad + 0.05), 0.0, ang.sin() * (rad + 0.05)).with_rotation(Quat::from_rotation_y(-ang)),
                            NotShadowCaster,
                        ));
                    }
                    r.spawn((Mesh3d(a.shaft.clone()), MeshMaterial3d(a.andesite.clone()), Transform::default()));
                }
                Role::Gearbox | Role::Static => {}
                Role::Crank => {
                    r.spawn((Mesh3d(a.shaft.clone()), MeshMaterial3d(a.andesite.clone()), Transform::from_scale(Vec3::new(1.0, 1.0, 1.0))));
                    r.spawn((Mesh3d(a.handle.clone()), MeshMaterial3d(a.wood.clone()), Transform::from_xyz(0.0, 0.45, 0.2)));
                }
                Role::Bearing => {
                    r.spawn((Mesh3d(a.plate.clone()), MeshMaterial3d(a.brass.clone()), Transform::from_xyz(0.0, 0.38, 0.0)));
                }
                _ => {
                    r.spawn((Mesh3d(a.shaft.clone()), MeshMaterial3d(a.andesite.clone()), Transform::default()));
                    // A notch makes the rotation visible.
                    r.spawn((Mesh3d(a.tooth.clone()), MeshMaterial3d(a.brass.clone()), Transform::from_xyz(0.1, 0.0, 0.0).with_scale(Vec3::new(0.6, 0.5, 0.6))));
                }
            }
        });
    });
    Some(e.id())
}

/// Opaque handle so other modules (save, tests) can spawn blocks.
#[derive(Clone)]
pub struct KineticAssetsRef(KineticAssets);


pub fn assets_ref(world: &World) -> Option<KineticAssetsRef> {
    world.get_resource::<KineticAssets>().map(|a| KineticAssetsRef(a.clone()))
}

fn track_blocks(
    added: Query<(Entity, &KineticBlock), Added<KineticBlock>>,
    mut removed: RemovedComponents<KineticBlock>,
    mut grid: ResMut<KineticGrid>,
) {
    for (e, b) in &added {
        grid.blocks.insert(b.cell, e);
        grid.dirty = true;
    }
    for e in removed.read() {
        grid.blocks.retain(|_, v| *v != e);
        grid.dirty = true;
    }
}

/// Grinders (and future machines) become consumers on their cell.
fn track_consumers(
    mut commands: Commands,
    added: Query<(Entity, &Part, &Transform), Added<Part>>,
    mut removed: RemovedComponents<KineticConsumer>,
    mut grid: ResMut<KineticGrid>,
) {
    for (e, part, t) in &added {
        if part.kind == PartKind::Grinder {
            commands.entity(e).insert((KineticConsumer { impact: 2.0 }, KineticDrive::default()));
            grid.consumers.insert(cell_of(t.translation), e);
            grid.dirty = true;
        }
    }
    for e in removed.read() {
        grid.consumers.retain(|_, v| *v != e);
        grid.dirty = true;
    }
}

/// Source state: crank timers, motors' access to power.
/// Crank timers (motors get their power state from `electric`).
fn update_sources(time: Res<Time>, mut blocks: Query<&mut KineticBlock>, mut grid: ResMut<KineticGrid>) {
    let dt = time.delta_secs();
    for mut b in &mut blocks {
        if b.def().is_some_and(|d| d.role == Role::Crank) && b.crank > 0.0 {
            b.crank = (b.crank - dt).max(0.0);
            if b.crank == 0.0 {
                grid.dirty = true;
            }
        }
    }
}

/// Builds the solver input from the world.
pub fn build_nodes(
    blocks: &Query<(Entity, &KineticBlock)>,
    consumers: &Query<(Entity, &KineticConsumer, &GlobalTransform)>,
    integrity: &Query<&crate::wear::Integrity>,
) -> HashMap<IVec3, Node> {
    let mut g = HashMap::new();
    for (e, b) in blocks.iter() {
        let Some(def) = b.def() else { continue };
        let mut n = Node::new(def.role, b.axis);
        // Worn parts rub (more friction) and broken ones stop working.
        let i = integrity.get(e).map(|x| x.value).unwrap_or(1.0);
        n.broken = i <= 0.0;
        n.impact = def.impact * (1.0 + (1.0 - i) * 3.0);
        n.strength = def.strength.unwrap_or(f32::INFINITY);
        n.toggled = b.toggled;
        n.source = match def.role {
            Role::Crank if b.crank > 0.0 => Some((def.source_rpm, def.source_power)),
            Role::Motor if b.on && b.powered => Some((b.rpm, def.source_power * b.power_scale * i)),
            Role::Crank | Role::Motor => Some((0.0, 0.0)),
            _ => None,
        };
        g.insert(b.cell, n);
    }
    for (e, c, gt) in consumers.iter() {
        let mut n = Node::new(Role::Consumer, Axis::Y);
        n.impact = c.impact;
        n.broken = integrity.get(e).is_ok_and(|x| x.value <= 0.0);
        g.insert(cell_of(gt.translation()), n);
    }
    g
}

#[allow(clippy::too_many_arguments)]
fn solve_networks(
    mut grid: ResMut<KineticGrid>,
    blocks: Query<(Entity, &KineticBlock)>,
    consumers: Query<(Entity, &KineticConsumer, &GlobalTransform)>,
    integrity: Query<&crate::wear::Integrity>,
    mut states: Query<&mut KineticState>,
    mut drives: Query<&mut KineticDrive>,
) {
    if !grid.dirty {
        return;
    }
    grid.dirty = false;
    let nodes = build_nodes(&blocks, &consumers, &integrity);
    let sol = network::solve(&nodes);
    for (e, b) in blocks.iter() {
        if let Ok(mut s) = states.get_mut(e) {
            let rpm = sol.speed.get(&b.cell).copied().unwrap_or(0.0);
            let net = sol.net_of.get(&b.cell).copied().unwrap_or(0);
            if s.rpm != rpm || s.net != net {
                *s = KineticState { rpm, net };
            }
        }
    }
    for (e, _, gt) in consumers.iter() {
        if let Ok(mut d) = drives.get_mut(e) {
            d.0 = sol.speed.get(&cell_of(gt.translation())).copied().unwrap_or(0.0).abs();
        }
    }
    grid.solution = sol;
}

fn spin_rotors(
    time: Res<Time>,
    blocks: Query<(&KineticState, &Children)>,
    mut rotors: Query<(&mut Rotor, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (s, children) in &blocks {
        if s.rpm == 0.0 {
            continue;
        }
        for c in children.iter() {
            if let Ok((mut r, mut t)) = rotors.get_mut(c) {
                r.angle = (r.angle + s.rpm / 60.0 * std::f32::consts::TAU * dt).rem_euclid(std::f32::consts::TAU);
                t.rotation = Quat::from_rotation_y(r.angle);
            }
        }
    }
}

/// Bearings drive physics bodies hinged to them (the physics boundary).
fn drive_bearings(
    grid: Res<KineticGrid>,
    blocks: Query<(&KineticBlock, &KineticState)>,
    mut joints: Query<(&Connection, &mut RevoluteJoint)>,
) {
    for (c, mut joint) in &mut joints {
        if c.kind != JointKind::Hinge {
            continue;
        }
        let (b, s, sign) = match (blocks.get(c.a), blocks.get(c.b)) {
            (Ok((b, s)), _) => (b, s, crate::machinery::engines::axis_sign(c.frame_a.basis * Vec3::Z)),
            (_, Ok((b, s))) => (b, s, -crate::machinery::engines::axis_sign(c.frame_b.basis * Vec3::Z)),
            _ => continue,
        };
        if b.def().is_none_or(|d| d.role != Role::Bearing) {
            continue;
        }
        let power = grid.solution.net_of.get(&b.cell).and_then(|n| grid.solution.nets.get(*n)).map(|n| n.capacity).unwrap_or(0.0);
        let target = s.rpm / 60.0 * std::f32::consts::TAU * sign;
        let enabled = s.rpm != 0.0;
        let m = &mut joint.motor;
        if m.enabled != enabled || (m.target_velocity - target).abs() > 1e-3 {
            *m = AngularMotor {
                enabled,
                target_velocity: target,
                // 1 SU ≈ 4 N·m of available torque (temporary scale).
                max_torque: power * 4.0,
                motor_model: MotorModel::AccelerationBased { stiffness: 0.0, damping: 1.0 },
                ..AngularMotor::new_disabled(MotorModel::DEFAULT)
            };
        }
    }
}

/// Mechanics mode: crank, switch motors, toggle clutches / gearshifts.
fn mechanics_use(mut reader: MessageReader<MechanicsUse>, mut blocks: Query<&mut KineticBlock>, mut grid: ResMut<KineticGrid>, mut notify: MessageWriter<Notify>) {
    for u in reader.read() {
        let Ok(mut b) = blocks.get_mut(u.entity) else { continue };
        let Some(role) = b.def().map(|d| d.role) else { continue };
        match (role, u.secondary) {
            (Role::Crank, _) => {
                b.crank = 4.0;
                notify.write(Notify::info("Вы крутите привод"));
            }
            (Role::Motor, false) => {
                b.on = !b.on;
                notify.write(Notify::info(if b.on { "Мотор включён" } else { "Мотор выключен" }));
            }
            (Role::Motor, true) => {
                let steps = [16.0, 32.0, 64.0, 128.0, 256.0];
                let i = steps.iter().position(|s| *s >= b.rpm.abs()).map(|i| (i + 1) % steps.len()).unwrap_or(0);
                b.rpm = steps[i] * b.rpm.signum().max(1.0);
                notify.write(Notify::info(format!("Обороты мотора: {:.0}", b.rpm)));
            }
            (Role::Clutch, _) => {
                b.toggled = !b.toggled;
                notify.write(Notify::info(if b.toggled { "Муфта разъединена" } else { "Муфта соединена" }));
            }
            (Role::Gearshift, _) => {
                b.toggled = !b.toggled;
                notify.write(Notify::info(if b.toggled { "Реверс включён" } else { "Реверс выключен" }));
            }
            _ => continue,
        }
        grid.dirty = true;
    }
}

/// Mining a kinetic block takes it back.
fn collect_blocks(
    mut commands: Commands,
    mut reader: MessageReader<CollectProp>,
    rules: Res<ModeRules>,
    blocks: Query<&KineticBlock>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    for CollectProp(e) in reader.read() {
        let Ok(b) = blocks.get(*e) else { continue };
        if rules.consume_items
            && let (Some(item), Ok(mut inv)) = (b.def().and_then(|d| ItemId::from_key(&d.item)), player.single_mut())
            && inv.add(item, 1) > 0
        {
            notify.write(Notify::warn("Инвентарь полон"));
            continue;
        }
        commands.entity(*e).despawn();
    }
}

#[derive(Resource, Default)]
pub struct KineticPlacement {
    /// R cycles a forced axis (None = automatic).
    pub axis_override: Option<Axis>,
}

/// Where a kinetic block would go: cell and axis.
pub fn kinetic_pose(
    target_block: &BlockTarget,
    entity: &EntityTarget,
    kinetic: &Query<&KineticBlock>,
    parts: &Query<&Transform, With<Part>>,
    forced: Option<Axis>,
) -> Option<(IVec3, Axis)> {
    let (cell, auto_axis) = if let Some(e) = entity.entity {
        let n = entity.normal.round().as_ivec3();
        let n_axis = Axis::of(n)?;
        if let Ok(k) = kinetic.get(e) {
            // Continue a shaft line / sit next to a cog: keep its axis.
            let keep = k.def().is_some_and(|d| !matches!(d.role, Role::Gearbox));
            (k.cell + n, if keep { k.axis } else { n_axis })
        } else if let Ok(t) = parts.get(e) {
            (cell_of(t.translation) + n, n_axis)
        } else {
            return None;
        }
    } else {
        let hit = target_block.0?;
        (hit.place_pos(), Axis::of(hit.normal)?)
    };
    Some((cell, forced.unwrap_or(auto_axis)))
}

#[allow(clippy::too_many_arguments)]
fn place_blocks(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    target_block: Res<BlockTarget>,
    entity: Res<EntityTarget>,
    kinetic: Query<&KineticBlock>,
    parts: Query<&Transform, With<Part>>,
    world: Option<Res<VoxelWorld>>,
    grid: Res<KineticGrid>,
    assets: Option<Res<KineticAssets>>,
    mut state: ResMut<KineticPlacement>,
    mut ghosts: ResMut<Ghosts>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let ActiveTool::PlaceKinetic(item) = *tool else {
        return;
    };
    let (Some(assets), Some(world)) = (assets, world) else { return };
    let Use::Kinetic(key) = &item.def().use_ else { return };
    let Some(def) = registry().kinetic(key) else { return };
    if keys.just_pressed(KeyCode::KeyR) {
        state.axis_override = match state.axis_override {
            None => Some(Axis::X),
            Some(Axis::X) => Some(Axis::Y),
            Some(Axis::Y) => Some(Axis::Z),
            Some(Axis::Z) => None,
        };
        notify.write(Notify::info(match state.axis_override {
            None => "Ось: автоматически".to_string(),
            Some(a) => format!("Ось: {}", a.label()),
        }));
    }
    let Some((cell, axis)) = kinetic_pose(&target_block, &entity, &kinetic, &parts, state.axis_override) else {
        return;
    };
    let free = !grid.blocks.contains_key(&cell)
        && !grid.consumers.contains_key(&cell)
        && world.in_bounds(cell)
        && !world.get(cell).is_solid();
    let validity = if free { Validity::Ok } else { Validity::Invalid };
    let shape = match def.role {
        Role::Shaft => GhostShape::Box(Vec3::new(0.3, 1.0, 0.3)),
        Role::Cog => GhostShape::Cylinder(0.45, 0.3),
        Role::LargeCog => GhostShape::Cylinder(0.92, 0.3),
        _ => GhostShape::Box(Vec3::splat(0.9)),
    };
    ghosts.0.push((shape, Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5)).with_rotation(axis_rotation(axis)), validity));
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    if !free {
        notify.write(Notify::warn("Здесь нельзя: клетка занята"));
        return;
    }
    let Ok(mut inv) = player.single_mut() else { return };
    if rules.consume_items && !inv.remove(item, 1) {
        return;
    }
    spawn_kinetic(&mut commands, &KineticAssetsRef(assets.clone()), key, cell, axis);
}

/// F6: draws links coloured by load (green ok, yellow > 80 %, red stopped).
fn overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut on: Local<bool>,
    grid: Res<KineticGrid>,
    blocks: Query<(Entity, &KineticBlock)>,
    consumers: Query<(Entity, &KineticConsumer, &GlobalTransform)>,
    integrity: Query<&crate::wear::Integrity>,
    mut gizmos: Gizmos,
) {
    if keys.just_pressed(KeyCode::F6) {
        *on = !*on;
    }
    if !*on {
        return;
    }
    let nodes = build_nodes(&blocks, &consumers, &integrity);
    for (&c, _) in &nodes {
        let Some(info) = grid.solution.net_of.get(&c).and_then(|n| grid.solution.nets.get(*n)) else { continue };
        let color = if info.conflict || info.stalled {
            Color::srgb(1.0, 0.2, 0.2)
        } else if info.load() > 0.8 {
            Color::srgb(1.0, 0.85, 0.2)
        } else if info.factor > 0.0 {
            Color::srgb(0.3, 1.0, 0.4)
        } else {
            Color::srgb(0.5, 0.5, 0.55)
        };
        let p = c.as_vec3() + Vec3::splat(0.5);
        for (n, _) in network::links(&nodes, c) {
            gizmos.line(p, n.as_vec3() + Vec3::splat(0.5), color);
        }
        gizmos.cube(Transform::from_translation(p).with_scale(Vec3::splat(0.25)), color);
    }
}

/// One-line summary used by panels and the scanner.
pub fn describe_block(world: &World, e: Entity) -> Option<(String, Vec<String>, Vec<String>)> {
    let b = world.get::<KineticBlock>(e)?;
    let s = world.get::<KineticState>(e).copied().unwrap_or_default();
    let def = b.def()?;
    let grid = world.resource::<KineticGrid>();
    let info = grid.solution.net_of.get(&b.cell).and_then(|n| grid.solution.nets.get(*n));
    let name = ItemId::from_key(&def.item).map(|i| i.def().name.clone()).unwrap_or_default();
    let mut lines = vec![
        format!("Обороты: {:.1} об/мин · ось {}", s.rpm, b.axis.label()),
    ];
    let mut diag = Vec::new();
    if let Some(n) = info {
        lines.push(format!("Сеть: мощность {:.0} SU · нагрузка {:.0} SU ({:.0}%)", n.capacity, n.demand, n.load().min(9.99) * 100.0));
        lines.push(format!("Деталей в сети: {}", n.members.len()));
        if n.conflict {
            diag.push("Конфликт скоростей: источники вращают сеть по-разному".into());
        }
        if n.stalled {
            diag.push("Перегрузка: двигателям не хватает мощности, сеть остановилась".into());
        } else if n.factor > 0.0 && n.factor < 1.0 {
            diag.push(format!("Перегрузка: сеть замедлена до {:.0}%", n.factor * 100.0));
        }
        if n.limited {
            diag.push("Обороты ограничены пределом 256 об/мин".into());
        }
        if n.capacity == 0.0 {
            diag.push("Нет источника вращения".into());
        }
    }
    if let Some(st) = def.strength {
        lines.push(format!("Прочность: {st:.0} SU · трение {:.2} SU/об", def.impact));
    }
    match def.role {
        Role::Motor => {
            lines.push(format!("Мотор: {} · {:.0} об/мин · {:.0} SU", if b.on { "вкл" } else { "выкл" }, b.rpm, def.source_power));
            if !b.powered {
                diag.push("Нет электроэнергии: подключите мотор проводом к энергоячейке или генератору".into());
            }
        }
        Role::Crank => lines.push(if b.crank > 0.0 { format!("Крутится ещё {:.1} с", b.crank) } else { "Не крутится (ЛКМ в режиме «Механизмы»)".into() }),
        Role::Clutch => lines.push(if b.toggled { "Разъединена".into() } else { "Соединена".into() }),
        Role::Gearshift => lines.push(if b.toggled { "Реверс".into() } else { "Прямо".into() }),
        _ => {}
    }
    Some((name, lines, diag))
}

fn scan_extra(world: &World, p: Vec3) -> Option<String> {
    let grid = world.get_resource::<KineticGrid>()?;
    let e = *grid.blocks.get(&cell_of(p))?;
    let (_, lines, _) = describe_block(world, e)?;
    Some(lines.join(" · "))
}

#[derive(Serialize, Deserialize, Default)]
struct SavedBlock {
    cell: IVec3,
    key: String,
    axis: Axis,
    toggled: bool,
    on: bool,
    rpm: f32,
}

fn capture(world: &mut World) -> serde_json::Value {
    let mut q = world.query::<&KineticBlock>();
    let mut v: Vec<SavedBlock> = q
        .iter(world)
        .map(|b| SavedBlock { cell: b.cell, key: b.key.clone(), axis: b.axis, toggled: b.toggled, on: b.on, rpm: b.rpm })
        .collect();
    v.sort_by_key(|b| (b.cell.x, b.cell.y, b.cell.z));
    serde_json::to_value(v).unwrap_or_default()
}

fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let mut q = world.query_filtered::<Entity, With<KineticBlock>>();
    let old: Vec<Entity> = q.iter(world).collect();
    for e in old {
        world.entity_mut(e).despawn();
    }
    world.resource_mut::<KineticGrid>().blocks.clear();
    let saved: Vec<SavedBlock> = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let Some(assets) = assets_ref(world) else { return };
    let mut commands = world.commands();
    let mut spawned = Vec::new();
    for s in &saved {
        if let Some(e) = spawn_kinetic(&mut commands, &assets, &s.key, s.cell, s.axis) {
            spawned.push((e, s.toggled, s.on, s.rpm));
        }
    }
    world.flush();
    for (e, toggled, on, rpm) in spawned {
        if let Some(mut b) = world.get_mut::<KineticBlock>(e) {
            b.toggled = toggled;
            b.on = on;
            b.rpm = rpm;
        }
    }
    world.resource_mut::<KineticGrid>().dirty = true;
}

pub struct KineticsPlugin;

impl Plugin for KineticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KineticGrid>()
            .init_resource::<KineticPlacement>()
            .add_systems(Startup, setup_assets)
            .add_save_hook(SaveHook { key: "kinetics", capture, restore })
            .add_systems(
                Update,
                (
                    (track_blocks, track_consumers, mechanics_use, collect_blocks, update_sources, solve_networks).chain(),
                    (spin_rotors, drive_bearings, overlay),
                    place_blocks.run_if(gameplay_active),
                )
                    .run_if(crate::core::states::in_game),
            );
        app.init_resource::<crate::tools::scanning::ScanExtras>();
        app.world_mut()
            .resource_mut::<crate::tools::scanning::ScanExtras>()
            .0
            .push(scan_extra);
    }
}
