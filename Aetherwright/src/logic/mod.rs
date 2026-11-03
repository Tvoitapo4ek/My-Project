//! Logic and automation (MASTER PROMPT §13): the third kind of link.
//!
//! * Forces travel through joints and shafts, power through wires and
//!   magical links, **information** through signal cables and radio.
//! * Logic elements are grid blocks (`kinetics` infrastructure) with a
//!   `LogicNode`; `circuit::tick` evaluates them every fixed tick.
//! * A cable from a device **into a sensor** is telemetry (the sensor
//!   reads that device); a cable from a logic output **into a machine** is a
//!   command (motor / engine on, clutch engaged, gearshift reversed).
//!   Without telemetry cables sensors read their neighbours.
//! * The core of every element glows while its output is on.

pub mod circuit;

use std::collections::HashMap;

use bevy::{light::NotShadowCaster, prelude::*};
use serde::{Deserialize, Serialize};

use circuit::{Element, External, Gate, SensorKind};
use crate::{
    arcana::MagicNode,
    content::registry,
    core::{events::Notify, states::gameplay_active},
    items::{Use, inventory::Inventory},
    kinetics::{Core, KineticBlock, KineticGrid, KineticState, network::Role},
    machinery::{MachinerySet, components::Engine, power::PowerStats},
    player::Player,
    save::world_save::{LoadedEntityIds, SaveEntityIds, SaveHook, SaveHookExt},
    survival::creatures::Creature,
    tools::{ActiveTool, EntityTarget, manipulator::MechanicsUse},
};

#[derive(Component, Debug, Clone, Copy)]
pub struct LogicNode {
    pub gate: Gate,
    /// Mechanics click this tick (buttons / levers).
    pub pressed: bool,
}

#[derive(Component, Debug, Clone)]
pub struct SignalLink {
    pub from: Entity,
    pub to: Entity,
    pub value: f32,
}

/// Command value a machine received last tick (diagnostics).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Commanded(pub f32);

fn attach_nodes(mut commands: Commands, added: Query<(Entity, &KineticBlock), (Added<KineticBlock>, Without<LogicNode>)>) {
    for (e, b) in &added {
        if let Some(def) = registry().logic(&b.key) {
            commands.entity(e).insert(LogicNode { gate: Gate::new(def.element), pressed: false });
        }
    }
}

const NEIGHBOURS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

/// A sensor's reading from one device (0..1).
fn read_device(world: &World, kind: SensorKind, e: Entity) -> Option<f32> {
    match kind {
        SensorKind::Rotation => world.get::<KineticState>(e).map(|s| s.rpm.abs() / crate::kinetics::network::MAX_RPM),
        SensorKind::Energy => world.resource::<PowerStats>().by_entity.get(&e).map(|(en, cap)| if *cap > 0.0 { en / cap } else { 0.0 }),
        SensorKind::Aether => world.get::<MagicNode>(e).and_then(|n| n.def().map(|d| n.amount / d.capacity.max(1e-3))),
        SensorKind::Distortion => world.get::<MagicNode>(e).map(|n| if n.amount > 1e-3 { n.quality.distortion } else { 0.0 }),
        SensorKind::Presence | SensorKind::Anomaly => None,
    }
}

/// Whole logic tick with world access: sensors, evaluation, commands.
fn run_logic(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let mut q = world.query::<(Entity, &LogicNode, &KineticBlock, &GlobalTransform)>();
    let nodes: Vec<(Entity, LogicNode, IVec3, Vec3)> = q.iter(world).map(|(e, n, k, t)| (e, *n, k.cell, t.translation())).collect();
    if nodes.is_empty() {
        return;
    }
    let index: HashMap<Entity, usize> = nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();
    let mut lq = world.query::<(Entity, &SignalLink)>();
    let links: Vec<(Entity, SignalLink)> = lq.iter(world).map(|(e, l)| (e, l.clone())).collect();
    let mut wires: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut telemetry: Vec<Vec<Entity>> = vec![Vec::new(); nodes.len()];
    for (_, l) in &links {
        match (index.get(&l.from), index.get(&l.to)) {
            (Some(&a), Some(&b)) => wires[b].push(a),
            (None, Some(&b)) => telemetry[b].push(l.from),
            _ => {}
        }
    }
    // Sensor readings.
    let grid = world.resource::<KineticGrid>().blocks.clone();
    let player = {
        let mut pq = world.query_filtered::<&GlobalTransform, With<Player>>();
        pq.iter(world).next().map(|t| t.translation())
    };
    let creatures: Vec<Vec3> = {
        let mut cq = world.query_filtered::<&GlobalTransform, With<Creature>>();
        cq.iter(world).map(|t| t.translation()).collect()
    };
    let parts: Vec<(Entity, Vec3)> = {
        let mut pq = world.query_filtered::<(Entity, &GlobalTransform), With<crate::machinery::components::Part>>();
        pq.iter(world).map(|(e, t)| (e, t.translation())).collect()
    };
    let mut ext = External { sensor: vec![0.0; nodes.len()], pressed: nodes.iter().map(|n| n.1.pressed).collect() };
    for (i, (_, n, cell, pos)) in nodes.iter().enumerate() {
        let Element::Sensor(kind) = n.gate.element else { continue };
        ext.sensor[i] = match kind {
            SensorKind::Presence => {
                let near = |p: &Vec3| p.distance(*pos) < 4.0;
                if player.as_ref().is_some_and(near) || creatures.iter().any(near) { 1.0 } else { 0.0 }
            }
            SensorKind::Anomaly => crate::anomaly::strength_at(world, *pos),
            _ => {
                let mut sources: Vec<Entity> = telemetry[i].clone();
                if sources.is_empty() {
                    sources.extend(NEIGHBOURS.iter().filter_map(|d| grid.get(&(*cell + *d)).copied()));
                    if kind == SensorKind::Energy {
                        sources.extend(parts.iter().filter(|(_, p)| p.distance(*pos) < 2.0).map(|(e, _)| *e));
                    }
                }
                sources.iter().filter_map(|e| read_device(world, kind, *e)).fold(0.0, f32::max)
            }
        };
    }
    let mut gates: Vec<Gate> = nodes.iter().map(|n| n.1.gate).collect();
    circuit::tick(&mut gates, &wires, &ext, dt);
    for (i, (e, _, _, _)) in nodes.iter().enumerate() {
        if let Some(mut n) = world.get_mut::<LogicNode>(*e) {
            n.gate = gates[i];
            n.pressed = false;
        }
    }
    // Link values and commands into machines.
    let mut commands: HashMap<Entity, f32> = HashMap::new();
    for (le, l) in &links {
        let v = index.get(&l.from).map(|i| gates[*i].out).or_else(|| index.get(&l.to).map(|_| 0.0)).unwrap_or(0.0);
        let shown = match index.get(&l.from) {
            Some(i) => gates[*i].out,
            None => index.get(&l.to).map(|i| ext.sensor[*i]).unwrap_or(0.0),
        };
        if let Some(mut s) = world.get_mut::<SignalLink>(*le) {
            s.value = shown;
        }
        if index.contains_key(&l.from) && !index.contains_key(&l.to) {
            let c = commands.entry(l.to).or_insert(0.0);
            *c = c.max(v);
        }
    }
    let mut dirty = false;
    for (target, v) in commands {
        let on = v > circuit::ON;
        if let Ok(mut em) = world.get_entity_mut(target) {
            em.insert(Commanded(v));
            if let Some(mut en) = em.get_mut::<Engine>() {
                if en.on != on {
                    en.on = on;
                }
            } else if let Some(mut k) = em.get_mut::<KineticBlock>() {
                // Anomaly generators are switched like motors.
                if registry().magic(&k.key).is_some_and(|m| m.role == crate::content::MagicRole::Generator) {
                    if k.on != on {
                        k.on = on;
                    }
                    continue;
                }
                match k.def().map(|d| d.role) {
                    Some(Role::Motor) if k.on != on => {
                        k.on = on;
                        dirty = true;
                    }
                    Some(Role::Clutch) if k.toggled == on => {
                        k.toggled = !on;
                        dirty = true;
                    }
                    Some(Role::Gearshift) if k.toggled != on => {
                        k.toggled = on;
                        dirty = true;
                    }
                    _ => {}
                }
            }
        }
    }
    if dirty {
        world.resource_mut::<KineticGrid>().dirty = true;
    }
}

/// Mechanics mode: buttons / levers react to LMB; other elements: LMB
/// cycles the parameter, RMB the mode.
fn mechanics_logic(mut reader: MessageReader<MechanicsUse>, mut nodes: Query<&mut LogicNode>, mut notify: MessageWriter<Notify>) {
    for u in reader.read() {
        let Ok(mut n) = nodes.get_mut(u.entity) else { continue };
        let el = n.gate.element;
        match (el, u.secondary) {
            (Element::Button | Element::Lever, _) => n.pressed = true,
            (_, false) if el.params() > 1 => {
                n.gate.param = (n.gate.param + 1) % el.params();
                notify.write(Notify::info(format!("{}: {}", el.label(), setting(&n.gate))));
            }
            (_, true) if el.modes() > 1 => {
                n.gate.mode = (n.gate.mode + 1) % el.modes();
                notify.write(Notify::info(format!("{}: {}", el.label(), setting(&n.gate))));
            }
            _ => {}
        }
    }
}

pub fn setting(g: &Gate) -> String {
    match g.element {
        Element::Timer => format!("период {} с", circuit::TIMER_PERIODS[g.param as usize % 4]),
        Element::Counter => format!("цель {}", circuit::COUNTER_GOALS[g.param as usize % 4]),
        Element::Comparator => format!("{} {:.1}", if g.mode == 0 { "больше" } else { "меньше" }, (g.param as f32 + 1.0) / 10.0),
        Element::Controller => ["включать по сигналу", "выключать по сигналу", "переключать по импульсу"][g.mode as usize % 3].into(),
        Element::Transmitter | Element::Receiver => format!("канал {}", g.param + 1),
        _ => String::new(),
    }
}

/// Glowing cores show outputs.
fn show_outputs(nodes: Query<(&LogicNode, &Children), Changed<LogicNode>>, mut cores: Query<&mut Visibility, With<Core>>) {
    for (n, ch) in &nodes {
        for c in ch.iter() {
            if let Ok(mut v) = cores.get_mut(c) {
                let want = if n.gate.out > circuit::ON { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
        }
    }
}

#[derive(Resource)]
struct CableAssets {
    mesh: Handle<Mesh>,
    on: Handle<StandardMaterial>,
    off: Handle<StandardMaterial>,
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(CableAssets {
        mesh: meshes.add(Cylinder::new(1.0, 1.0)),
        on: mats.add(StandardMaterial { base_color: Color::srgb(0.3, 1.0, 0.5), emissive: LinearRgba::rgb(0.6, 2.5, 1.0), ..default() }),
        off: mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.3, 0.18), perceptual_roughness: 0.8, ..default() }),
    });
}

pub fn spawn_signal(commands: &mut Commands, from: Entity, to: Entity) -> Entity {
    commands
        .spawn((Name::new("Signal cable"), SignalLink { from, to, value: 0.0 }, Transform::default(), Visibility::default()))
        .id()
}

#[allow(clippy::type_complexity)]
fn draw_cables(
    mut commands: Commands,
    assets: Option<Res<CableAssets>>,
    ends: Query<&GlobalTransform>,
    mut links: Query<(Entity, &SignalLink, &mut Transform, Option<&MeshMaterial3d<StandardMaterial>>)>,
) {
    let Some(a) = assets else { return };
    for (e, l, mut t, mat) in &mut links {
        let (Ok(pa), Ok(pb)) = (ends.get(l.from), ends.get(l.to)) else {
            commands.entity(e).despawn();
            continue;
        };
        let (pa, pb) = (pa.translation() + Vec3::Y * 0.3, pb.translation() + Vec3::Y * 0.3);
        let d = pb - pa;
        *t = Transform::from_translation(pa + d / 2.0)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y)))
            .with_scale(Vec3::new(0.018, d.length().max(0.01), 0.018));
        let want = if l.value > circuit::ON { a.on.clone() } else { a.off.clone() };
        if mat.is_none_or(|m| m.0 != want) {
            commands.entity(e).insert((Mesh3d(a.mesh.clone()), MeshMaterial3d(want), NotShadowCaster));
        }
    }
}

#[derive(Resource, Default)]
pub struct CableTool {
    pub pending: Option<Entity>,
}

#[allow(clippy::too_many_arguments)]
fn cable_tool(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    target: Res<EntityTarget>,
    rules: Res<crate::core::rules::ModeRules>,
    ends: Query<&GlobalTransform>,
    logic: Query<(), With<LogicNode>>,
    links: Query<(Entity, &SignalLink)>,
    mut state: ResMut<CableTool>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let ActiveTool::SignalCable(item) = *tool else {
        state.pending = None;
        return;
    };
    let Use::SignalCable { max_len } = item.def().use_ else { return };
    let aimed = target.entity.filter(|e| ends.contains(*e));
    if mouse.just_pressed(MouseButton::Right) {
        state.pending = None;
        let Some(dev) = aimed else { return };
        let mut n = 0;
        for (le, l) in &links {
            if l.from == dev || l.to == dev {
                commands.entity(le).despawn();
                n += 1;
            }
        }
        if n > 0
            && let Ok(mut inv) = player.single_mut()
        {
            inv.add(item, n);
        }
        notify.write(Notify::info(format!("Снято кабелей: {n}")));
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(dev) = aimed else { return };
    let Some(first) = state.pending else {
        state.pending = Some(dev);
        notify.write(Notify::info("Источник сигнала выбран — выберите получателя"));
        return;
    };
    state.pending = None;
    if first == dev {
        return;
    }
    if !logic.contains(first) && !logic.contains(dev) {
        notify.write(Notify::warn("Хотя бы один конец кабеля — логический элемент"));
        return;
    }
    let (Ok(a), Ok(b)) = (ends.get(first), ends.get(dev)) else { return };
    let len = a.translation().distance(b.translation());
    if len > max_len {
        notify.write(Notify::warn(format!("Слишком далеко: {len:.1} м")));
        return;
    }
    if rules.consume_items {
        let Ok(mut inv) = player.single_mut() else { return };
        if !inv.remove(item, 1) {
            return;
        }
    }
    spawn_signal(&mut commands, first, dev);
    notify.write(Notify::ok(if logic.contains(dev) && !logic.contains(first) {
        "Телеметрия подключена"
    } else if !logic.contains(dev) {
        "Команды механизму подключены"
    } else {
        "Сигнал подключён"
    }));
}

/// F6 overlay: signal values above cables.
fn overlay(keys: Res<ButtonInput<KeyCode>>, mut on: Local<bool>, links: Query<&SignalLink>, ends: Query<&GlobalTransform>, mut gizmos: Gizmos) {
    if keys.just_pressed(KeyCode::F6) {
        *on = !*on;
    }
    if !*on {
        return;
    }
    for l in &links {
        let (Ok(a), Ok(b)) = (ends.get(l.from), ends.get(l.to)) else { continue };
        let c = Color::srgb(0.3, 0.3 + 0.7 * l.value, 0.4);
        gizmos.arrow(a.translation() + Vec3::Y * 0.6, b.translation() + Vec3::Y * 0.6, c);
    }
}

pub fn describe(world: &World, e: Entity) -> Option<(Vec<String>, Vec<String>)> {
    let mut lines = Vec::new();
    let mut diag = Vec::new();
    if let Some(n) = world.get::<LogicNode>(e) {
        let s = setting(&n.gate);
        lines.push(format!("{}: выход {:.2}{}", n.gate.element.label(), n.gate.out, if s.is_empty() { String::new() } else { format!(" · {s}") }));
    }
    if let Some(c) = world.get::<Commanded>(e) {
        lines.push(format!("Команда по кабелю: {}", if c.0 > circuit::ON { "вкл" } else { "выкл" }));
    }
    let mut q = world.try_query::<&SignalLink>()?;
    let ins = q.iter(world).filter(|l| l.to == e).count();
    let outs = q.iter(world).filter(|l| l.from == e).count();
    if ins + outs > 0 {
        lines.push(format!("Сигнальные кабели: входов {ins}, выходов {outs}"));
    }
    if let Some(n) = world.get::<LogicNode>(e)
        && matches!(n.gate.element, Element::And | Element::Or | Element::Not | Element::Xor | Element::Comparator | Element::Controller | Element::Lamp | Element::Transmitter | Element::Counter)
        && ins == 0
    {
        diag.push("Нет входного сигнала".into());
    }
    if lines.is_empty() { None } else { Some((lines, diag)) }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
enum End {
    Prop(u32),
    Kinetic(IVec3),
}

#[derive(Serialize, Deserialize, Default)]
struct SavedLogic {
    gates: Vec<(IVec3, Gate)>,
    cables: Vec<(End, End)>,
}

fn end_of(world: &World, ids: &SaveEntityIds, e: Entity) -> Option<End> {
    if let Some(k) = world.get::<KineticBlock>(e) {
        return Some(End::Kinetic(k.cell));
    }
    ids.0.get(&e).map(|id| End::Prop(*id))
}

fn capture(world: &mut World) -> serde_json::Value {
    let ids = world.get_resource::<SaveEntityIds>().cloned().unwrap_or_default();
    let mut q = world.query::<(&LogicNode, &KineticBlock)>();
    let gates = q.iter(world).map(|(n, k)| (k.cell, n.gate)).collect();
    let mut lq = world.query::<&SignalLink>();
    let cables = lq.iter(world).filter_map(|l| Some((end_of(world, &ids, l.from)?, end_of(world, &ids, l.to)?))).collect();
    serde_json::to_value(SavedLogic { gates, cables }).unwrap_or_default()
}

fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let mut q = world.query_filtered::<Entity, With<SignalLink>>();
    let old: Vec<Entity> = q.iter(world).collect();
    for e in old {
        world.entity_mut(e).despawn();
    }
    let s: SavedLogic = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut kq = world.query::<(Entity, &KineticBlock)>();
    let cells: HashMap<IVec3, Entity> = kq.iter(world).map(|(e, k)| (k.cell, e)).collect();
    for (cell, g) in &s.gates {
        if let Some(&e) = cells.get(cell) {
            world.entity_mut(e).insert(LogicNode { gate: *g, pressed: false });
        }
    }
    let ids = world.get_resource::<LoadedEntityIds>().cloned().unwrap_or_default();
    let resolve = |end: End| match end {
        End::Prop(id) => ids.0.get(&id).copied(),
        End::Kinetic(c) => cells.get(&c).copied(),
    };
    let mut commands = world.commands();
    for (a, b) in &s.cables {
        if let (Some(a), Some(b)) = (resolve(*a), resolve(*b)) {
            spawn_signal(&mut commands, a, b);
        }
    }
    world.flush();
}

pub struct LogicPlugin;

impl Plugin for LogicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CableTool>()
            .add_systems(Startup, setup_assets)
            .add_save_hook(SaveHook { key: "logic", capture, restore })
            .add_systems(Update, (attach_nodes, mechanics_logic, show_outputs, draw_cables, overlay).run_if(crate::core::states::in_game))
            .add_systems(Update, cable_tool.run_if(gameplay_active))
            .add_systems(FixedUpdate, run_logic.before(MachinerySet::Power).run_if(crate::core::states::in_game));
    }
}
