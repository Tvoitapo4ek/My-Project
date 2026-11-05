//! Electric network (source → transfer → storage → conversion → use).
//!
//! * Devices get an `ElectricNode`: power cells store, aether dynamos and
//!   kinetic generators produce, engines and kinetic motors consume,
//!   connectors relay.
//! * Edges: **wires** laid with the wire tool (capacity, losses per metre,
//!   max span, they overheat and burn when overloaded for long) and the
//!   **internal bus** of a welded / hinged construction (no limits — parts
//!   bolted together share power, as before).
//! * `network::solve` runs every fixed tick: generation → consumers,
//!   storage → consumers, surplus → storage. Consumers get a satisfaction
//!   0..1; below one half a device stops (engines, motors).
//!
//! Physical, energetic and informational links are separate kinds: wires
//! carry only power, joints carry forces (and, as a bus, power), signals
//! come in V4.

pub mod network;

use std::collections::HashMap;

use bevy::{light::NotShadowCaster, prelude::*};
use serde::{Deserialize, Serialize};

use network::{EEdge, ENode};
use crate::{
    content::WireSpec,
    core::{events::Notify, rules::ModeRules, states::gameplay_active},
    items::{ItemId, Use, inventory::Inventory},
    kinetics::{KineticBlock, KineticGrid, KineticState, network::Role},
    machinery::{
        MachinerySet,
        components::{AetherDynamo, Engine, Part, PartKind, PowerCell},
        power::PowerStats,
    },
    physics::joints::Connection,
    player::Player,
    save::world_save::{LoadedEntityIds, SaveEntityIds, SaveHook, SaveHookExt},
    tools::{ActiveTool, EntityTarget},
};

/// Max charge / discharge of a power cell (energy per second).
pub const CELL_RATE: f32 = 30.0;
/// Seconds a wire may run at full load before it burns.
pub const WIRE_BURN_TIME: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectricRole {
    Storage,
    Generator,
    Consumer,
    Relay,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ElectricNode {
    pub role: ElectricRole,
}

/// Last tick's numbers for a device (UI, diagnostics, tests).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ElectricState {
    pub demand: f32,
    pub served: f32,
    pub generated: f32,
    pub discharged: f32,
    pub charged: f32,
    pub satisfaction: f32,
    pub net: usize,
    /// The device is reachable by at least one wire or bus link.
    pub linked: bool,
}

#[derive(Component, Debug, Clone)]
pub struct Wire {
    pub a: Entity,
    pub b: Entity,
    /// Item key (to give back).
    pub item: String,
    pub spec: WireSpec,
    /// Power through last tick.
    pub flow: f32,
    /// Seconds at full load (burns at `WIRE_BURN_TIME`).
    pub heat: f32,
    /// Current span between the ends (m), updated every frame.
    pub len: f32,
}

impl Wire {
    pub fn load(&self) -> f32 {
        self.flow / self.spec.capacity.max(1e-3)
    }
}

/// Totals per network for panels.
#[derive(Resource, Default, Debug)]
pub struct ElectricStats {
    pub lost: f32,
    pub wires_overloaded: usize,
}

fn attach_nodes(
    mut commands: Commands,
    parts: Query<(Entity, &Part), (Added<Part>, Without<ElectricNode>)>,
    kinetic: Query<(Entity, &KineticBlock), (Added<KineticBlock>, Without<ElectricNode>)>,
) {
    for (e, p) in &parts {
        let role = match p.kind {
            PartKind::PowerCell => ElectricRole::Storage,
            PartKind::AetherDynamo => ElectricRole::Generator,
            PartKind::Engine => ElectricRole::Consumer,
            PartKind::Connector => ElectricRole::Relay,
            _ => continue,
        };
        commands.entity(e).insert((ElectricNode { role }, ElectricState::default()));
    }
    for (e, b) in &kinetic {
        let draws = crate::content::registry().magic(&b.key).is_some_and(|m| m.electric_draw > 0.0);
        let role = match b.def().map(|d| (d.role, d.key.as_str(), d.energy_per_su)) {
            // Only electric motors; the magic engine runs on aether.
            Some((Role::Motor, _, e)) if e > 0.0 => ElectricRole::Consumer,
            Some((Role::Consumer, "electric_generator", _)) => ElectricRole::Generator,
            _ if draws => ElectricRole::Consumer,
            _ => continue,
        };
        commands.entity(e).insert((ElectricNode { role }, ElectricState::default()));
    }
}

/// One fixed tick of the whole electric world.
#[allow(clippy::type_complexity)]
fn solve_power(
    time: Res<Time>,
    nodes: Query<(Entity, &ElectricNode)>,
    parts: Query<Entity, With<Part>>,
    exists: Query<()>,
    joints: Query<&Connection>,
    mut wires: Query<(Entity, &mut Wire)>,
    mut cells: Query<&mut PowerCell>,
    mut dynamos: Query<&mut AetherDynamo>,
    mut engines: Query<&mut Engine>,
    mut kblocks: Query<(&mut KineticBlock, &KineticState)>,
    mut states: Query<&mut ElectricState>,
    mut kgrid: ResMut<KineticGrid>,
    mut stats: ResMut<PowerStats>,
    mut estats: ResMut<ElectricStats>,
    mut commands: Commands,
    mut notify: MessageWriter<Notify>,
) {
    let dt = time.delta_secs().max(1e-4);
    // Graph nodes: electric devices + every part (frames relay the bus).
    let mut list: Vec<Entity> = nodes.iter().map(|(e, _)| e).collect();
    for p in &parts {
        if !list.contains(&p) {
            list.push(p);
        }
    }
    // Wire ends that exist but have no node yet (just spawned) relay.
    for (_, w) in &wires {
        for e in [w.a, w.b] {
            if !list.contains(&e) && exists.contains(e) {
                list.push(e);
            }
        }
    }
    let index: HashMap<Entity, usize> = list.iter().enumerate().map(|(i, e)| (*e, i)).collect();
    let mut enodes = vec![ENode::default(); list.len()];
    for (i, e) in list.iter().enumerate() {
        let Ok((_, node)) = nodes.get(*e) else { continue };
        match node.role {
            ElectricRole::Storage => {
                if let Ok(c) = cells.get(*e) {
                    enodes[i].discharge = CELL_RATE.min(c.energy / dt);
                    enodes[i].charge = CELL_RATE.min((c.capacity - c.energy).max(0.0) / dt);
                }
            }
            ElectricRole::Generator => {
                if let Ok(d) = dynamos.get(*e) {
                    enodes[i].generation = d.buffer / dt;
                } else if let Ok((b, s)) = kblocks.get(*e) {
                    let def = b.def();
                    enodes[i].generation = def.map(|d| s.rpm.abs() * d.impact * d.energy_per_su).unwrap_or(0.0);
                }
            }
            ElectricRole::Consumer => {
                if let Ok(en) = engines.get(*e) {
                    enodes[i].demand = if en.on { en.draw * en.throttle } else { 0.0 };
                } else if let Ok((b, _)) = kblocks.get(*e) {
                    enodes[i].demand = match crate::content::registry().magic(&b.key) {
                        // Magical devices that need power (stabilizer).
                        Some(m) => m.electric_draw,
                        None => motor_demand(b, &kgrid),
                    };
                }
            }
            ElectricRole::Relay => {}
        }
    }
    // Edges: internal bus (joints) + wires.
    let mut edges = Vec::new();
    for c in &joints {
        if let (Some(&a), Some(&b)) = (index.get(&c.a), index.get(&c.b)) {
            edges.push(EEdge { a, b, capacity: f32::INFINITY, loss: 0.0 });
        }
    }
    let bus_edges = edges.len();
    let mut wire_ids = Vec::new();
    for (we, w) in &wires {
        let (Some(&a), Some(&b)) = (index.get(&w.a), index.get(&w.b)) else {
            // An end is gone: the wire falls off.
            if commands.get_entity(we).is_ok() {
                commands.entity(we).despawn();
            }
            continue;
        };
        edges.push(EEdge { a, b, capacity: w.spec.capacity, loss: (w.spec.loss_per_m * w.len).min(0.9) });
        wire_ids.push(we);
    }
    let r = network::solve(&enodes, &edges);
    let comps = network::components(list.len(), &edges);
    // Apply results.
    for (i, e) in list.iter().enumerate() {
        let Ok((_, node)) = nodes.get(*e) else { continue };
        if let Ok(mut c) = cells.get_mut(*e) {
            c.energy = (c.energy - r.discharged[i] * dt + r.charged[i] * dt).clamp(0.0, c.capacity);
        }
        if let Ok(mut d) = dynamos.get_mut(*e) {
            d.buffer = (d.buffer - r.generated[i] * dt).max(0.0);
        }
        let sat = r.satisfaction(i, enodes[i].demand);
        if let Ok(mut en) = engines.get_mut(*e) {
            let p = en.on && sat >= 0.5;
            if en.powered != p {
                en.powered = p;
            }
        }
        if node.role == ElectricRole::Consumer
            && let Ok((mut b, _)) = kblocks.get_mut(*e)
            && b.def().is_some_and(|d| d.role == Role::Motor)
        {
            let p = sat >= 0.5 && enodes[i].demand > 0.0;
            if b.powered != p {
                b.powered = p;
                kgrid.dirty = true;
            }
        }
        if let Ok(mut s) = states.get_mut(*e) {
            *s = ElectricState {
                demand: enodes[i].demand,
                served: r.served[i],
                generated: r.generated[i],
                discharged: r.discharged[i],
                charged: r.charged[i],
                satisfaction: sat,
                net: comps[i],
                linked: edges.iter().any(|x| x.a == i || x.b == i),
            };
        }
    }
    // Wires: load, heat, burning.
    estats.lost = r.lost;
    estats.wires_overloaded = 0;
    for (k, we) in wire_ids.iter().enumerate() {
        let Ok((_, mut w)) = wires.get_mut(*we) else { continue };
        w.flow = r.flow[bus_edges + k];
        if w.load() >= 0.98 {
            estats.wires_overloaded += 1;
            w.heat += dt;
            if w.heat >= WIRE_BURN_TIME {
                notify.write(Notify::warn("Провод перегорел от перегрузки"));
                commands.entity(*we).despawn();
            }
        } else {
            w.heat = (w.heat - dt * 0.5).max(0.0);
        }
    }
    // Stored energy per network (engineering panel, scanner).
    let mut per_net: HashMap<usize, (f32, f32)> = HashMap::new();
    for (i, e) in list.iter().enumerate() {
        if let Ok(c) = cells.get(*e) {
            let t = per_net.entry(comps[i]).or_default();
            t.0 += c.energy;
            t.1 += c.capacity;
        }
    }
    stats.by_entity.clear();
    for (i, e) in list.iter().enumerate() {
        if let Some(v) = per_net.get(&comps[i]) {
            stats.by_entity.insert(*e, *v);
        }
    }
}

/// A motor asks for what it delivers; switched on but idle it asks for a
/// quarter of its nominal draw to find out whether power is there.
fn motor_demand(b: &KineticBlock, grid: &KineticGrid) -> f32 {
    let Some(def) = b.def().filter(|d| d.role == Role::Motor) else {
        return 0.0;
    };
    if !b.on {
        return 0.0;
    }
    let nominal = def.source_power * def.energy_per_su;
    if !b.powered {
        return nominal * 0.25;
    }
    let Some(info) = grid.solution.net_of.get(&b.cell).and_then(|n| grid.solution.nets.get(*n)) else {
        return nominal * 0.25;
    };
    if info.capacity <= 0.0 || info.factor == 0.0 {
        return nominal * 0.25;
    }
    let delivered = (info.demand * info.factor).min(info.capacity);
    (delivered * def.source_power / info.capacity * def.energy_per_su).max(nominal * 0.05)
}

/// Where a wire attaches on a device.
fn anchor(e: Entity, q: &Query<&GlobalTransform>) -> Option<Vec3> {
    q.get(e).ok().map(|t| t.translation())
}

#[derive(Resource)]
struct WireAssets {
    mesh: Handle<Mesh>,
    copper: Handle<StandardMaterial>,
    cable: Handle<StandardMaterial>,
    hot: Handle<StandardMaterial>,
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(WireAssets {
        mesh: meshes.add(Cylinder::new(1.0, 1.0)),
        copper: mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.5, 0.3), metallic: 0.7, perceptual_roughness: 0.4, ..default() }),
        cable: mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.12, 0.14), perceptual_roughness: 0.8, ..default() }),
        hot: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.35, 0.1), emissive: LinearRgba::rgb(4.0, 0.8, 0.1), ..default() }),
    });
}

/// Spawns a wire between two devices (tool, loading, tests).
pub fn spawn_wire(commands: &mut Commands, a: Entity, b: Entity, item: &str) -> Option<Entity> {
    let Use::Wire(spec) = ItemId::from_key(item)?.def().use_ else {
        return None;
    };
    Some(
        commands
            .spawn((
                Name::new(format!("Wire {item}")),
                Wire { a, b, item: item.to_string(), spec, flow: 0.0, heat: 0.0, len: 0.0 },
                Transform::default(),
                Visibility::default(),
            ))
            .id(),
    )
}

/// Wires follow their ends; colour shows overheating.
fn draw_wires(
    mut commands: Commands,
    assets: Option<Res<WireAssets>>,
    ends: Query<&GlobalTransform>,
    mut wires: Query<(Entity, &mut Wire, &mut Transform, Option<&MeshMaterial3d<StandardMaterial>>)>,
) {
    let Some(a) = assets else { return };
    for (e, mut w, mut t, mat) in &mut wires {
        let (Some(pa), Some(pb)) = (anchor(w.a, &ends), anchor(w.b, &ends)) else { continue };
        let d = pb - pa;
        w.len = d.length();
        let thick = if w.spec.capacity > 30.0 { 0.045 } else { 0.025 };
        *t = Transform::from_translation(pa + d / 2.0)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y)))
            .with_scale(Vec3::new(thick, w.len.max(0.01), thick));
        let want = if w.heat > WIRE_BURN_TIME * 0.4 {
            a.hot.clone()
        } else if w.spec.capacity > 30.0 {
            a.cable.clone()
        } else {
            a.copper.clone()
        };
        match mat {
            None => {
                commands.entity(e).insert((Mesh3d(a.mesh.clone()), MeshMaterial3d(want), NotShadowCaster));
            }
            Some(m) if m.0 != want => {
                commands.entity(e).insert(MeshMaterial3d(want));
            }
            _ => {}
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct WireTool {
    pub pending: Option<Entity>,
}

/// Cost in wire items for a span.
pub fn wire_cost(len: f32) -> u32 {
    (len / 4.0).ceil().max(1.0) as u32
}

#[allow(clippy::too_many_arguments)]
fn wire_tool(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    target: Res<EntityTarget>,
    devices: Query<(), Or<(With<ElectricNode>, With<Part>)>>,
    ends: Query<&GlobalTransform>,
    wires: Query<(Entity, &Wire)>,
    mut state: ResMut<WireTool>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
    mut gizmos: Gizmos,
) {
    let ActiveTool::Wire(item) = *tool else {
        state.pending = None;
        return;
    };
    let Use::Wire(spec) = item.def().use_ else { return };
    let aimed = target.entity.filter(|e| devices.contains(*e));
    if let Some(p) = state.pending.and_then(|p| anchor(p, &ends)) {
        let to = if target.entity.is_some() { target.point } else { p };
        let ok = to.distance(p) <= spec.max_len;
        gizmos.line(p, to, if ok { Color::srgb(1.0, 0.7, 0.3) } else { Color::srgb(1.0, 0.2, 0.2) });
    }
    if mouse.just_pressed(MouseButton::Right) {
        // Remove all wires of a device and get them back.
        state.pending = None;
        let Some(dev) = aimed else { return };
        let mut back: HashMap<String, u32> = HashMap::new();
        for (we, w) in &wires {
            if w.a == dev || w.b == dev {
                *back.entry(w.item.clone()).or_default() += wire_cost(w.len);
                commands.entity(we).despawn();
            }
        }
        if let Ok(mut inv) = player.single_mut() {
            for (k, n) in &back {
                if let Some(i) = ItemId::from_key(k) {
                    inv.add(i, *n);
                }
            }
        }
        notify.write(Notify::info(format!("Снято проводов: {}", back.values().count())));
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(dev) = aimed else {
        notify.write(Notify::warn("Провод подключается к устройству или детали"));
        return;
    };
    let Some(first) = state.pending else {
        state.pending = Some(dev);
        notify.write(Notify::info("Выберите второе устройство (ПКМ — отмена)"));
        return;
    };
    state.pending = None;
    if first == dev {
        return;
    }
    if wires.iter().any(|(_, w)| (w.a == first && w.b == dev) || (w.a == dev && w.b == first)) {
        notify.write(Notify::warn("Уже соединены проводом"));
        return;
    }
    let (Some(pa), Some(pb)) = (anchor(first, &ends), anchor(dev, &ends)) else { return };
    let len = pa.distance(pb);
    if len > spec.max_len {
        notify.write(Notify::warn(format!("Слишком далеко: {len:.1} м (провод до {:.0} м)", spec.max_len)));
        return;
    }
    let cost = wire_cost(len);
    if rules.consume_items {
        let Ok(mut inv) = player.single_mut() else { return };
        if !inv.remove(item, cost) {
            notify.write(Notify::warn(format!("Нужно провода: {cost}")));
            return;
        }
    }
    spawn_wire(&mut commands, first, dev, &item.def().key);
    notify.write(Notify::ok(format!("Провод проложен: {len:.1} м")));
}

/// Wires break when an end is removed (handled in `solve_power`); this
/// draws loads in the F6 overlay together with kinetic networks.
fn overlay(keys: Res<ButtonInput<KeyCode>>, mut on: Local<bool>, wires: Query<&Wire>, ends: Query<&GlobalTransform>, mut gizmos: Gizmos) {
    if keys.just_pressed(KeyCode::F6) {
        *on = !*on;
    }
    if !*on {
        return;
    }
    for w in &wires {
        let (Some(a), Some(b)) = (anchor(w.a, &ends), anchor(w.b, &ends)) else { continue };
        let l = w.load();
        let c = if l >= 0.98 {
            Color::srgb(1.0, 0.2, 0.2)
        } else if l > 0.7 {
            Color::srgb(1.0, 0.85, 0.2)
        } else if l > 0.0 {
            Color::srgb(0.3, 0.8, 1.0)
        } else {
            Color::srgb(0.5, 0.5, 0.55)
        };
        gizmos.line(a + Vec3::Y * 0.05, b + Vec3::Y * 0.05, c);
    }
}

/// Text for engineering panels: (lines, diagnostics).
pub fn describe(world: &World, e: Entity) -> Option<(Vec<String>, Vec<String>)> {
    let node = world.get::<ElectricNode>(e)?;
    let s = world.get::<ElectricState>(e).copied().unwrap_or_default();
    let mut lines = Vec::new();
    let mut diag = Vec::new();
    match node.role {
        ElectricRole::Storage => lines.push(format!("Отдача {:.1} Вт · заряд {:.1} Вт (предел {CELL_RATE:.0} Вт)", s.discharged, s.charged)),
        ElectricRole::Generator => lines.push(format!("Выработка в сеть: {:.1} Вт", s.generated)),
        ElectricRole::Consumer => {
            lines.push(format!("Потребление: {:.1} из {:.1} Вт ({:.0}%)", s.served, s.demand, s.satisfaction * 100.0));
            if s.demand > 0.0 && s.satisfaction < 0.5 {
                diag.push("Не хватает электроэнергии".into());
            }
        }
        ElectricRole::Relay => lines.push("Соединитель цепи".into()),
    }
    let mut wq = world.try_query::<&Wire>()?;
    let mine: Vec<&Wire> = wq.iter(world).filter(|w| w.a == e || w.b == e).collect();
    if !mine.is_empty() {
        let max = mine.iter().map(|w| w.load()).fold(0.0, f32::max);
        lines.push(format!("Проводов: {} · макс. нагрузка {:.0}%", mine.len(), max * 100.0));
        if mine.iter().any(|w| w.heat > 0.0) {
            diag.push("Провод перегружен и нагревается — замените на кабель или разделите нагрузку".into());
        }
    }
    if !s.linked && node.role != ElectricRole::Relay {
        diag.push("Не подключено к сети: проложите провод".into());
    }
    Some((lines, diag))
}

/// Saved wire end: a body by its save id, or a kinetic block by its cell.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
enum End {
    Prop(u32),
    Kinetic(IVec3),
}

#[derive(Serialize, Deserialize)]
struct SavedWire {
    a: End,
    b: End,
    item: String,
    heat: f32,
}

fn end_of(world: &World, ids: &SaveEntityIds, e: Entity) -> Option<End> {
    if let Some(k) = world.get::<KineticBlock>(e) {
        return Some(End::Kinetic(k.cell));
    }
    ids.0.get(&e).map(|id| End::Prop(*id))
}

fn capture(world: &mut World) -> serde_json::Value {
    let ids = world.get_resource::<SaveEntityIds>().cloned().unwrap_or_default();
    let mut q = world.query::<&Wire>();
    let v: Vec<SavedWire> = q
        .iter(world)
        .filter_map(|w| Some(SavedWire { a: end_of(world, &ids, w.a)?, b: end_of(world, &ids, w.b)?, item: w.item.clone(), heat: w.heat }))
        .collect();
    serde_json::to_value(v).unwrap_or_default()
}

fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let mut q = world.query_filtered::<Entity, With<Wire>>();
    let old: Vec<Entity> = q.iter(world).collect();
    for e in old {
        world.entity_mut(e).despawn();
    }
    let saved: Vec<SavedWire> = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let ids = world.get_resource::<LoadedEntityIds>().cloned().unwrap_or_default();
    let cells: HashMap<IVec3, Entity> = world.resource::<KineticGrid>().blocks.clone();
    // Kinetic blocks restored in this same pass are not in the grid yet.
    let mut kq = world.query::<(Entity, &KineticBlock)>();
    let kin: HashMap<IVec3, Entity> = kq.iter(world).map(|(e, k)| (k.cell, e)).chain(cells).collect();
    let resolve = |end: End| match end {
        End::Prop(id) => ids.0.get(&id).copied(),
        End::Kinetic(c) => kin.get(&c).copied(),
    };
    let mut commands = world.commands();
    let mut made = Vec::new();
    for s in &saved {
        if let (Some(a), Some(b)) = (resolve(s.a), resolve(s.b))
            && let Some(e) = spawn_wire(&mut commands, a, b, &s.item)
        {
            made.push((e, s.heat));
        }
    }
    world.flush();
    for (e, heat) in made {
        if let Some(mut w) = world.get_mut::<Wire>(e) {
            w.heat = heat;
        }
    }
}

/// Nodes and the solver only (used by the game and by headless tests).
pub struct ElectricCorePlugin;

impl Plugin for ElectricCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ElectricStats>()
            .init_resource::<PowerStats>()
            .init_resource::<KineticGrid>()
            .add_message::<Notify>()
            .add_systems(Update, attach_nodes)
            .add_systems(FixedUpdate, solve_power.in_set(MachinerySet::Power));
    }
}

pub struct ElectricPlugin;

impl Plugin for ElectricPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ElectricCorePlugin)
            .init_resource::<WireTool>()
            .add_systems(Startup, setup_assets)
            .add_save_hook(SaveHook { key: "wires", capture, restore })
            .add_systems(Update, (draw_wires, overlay).run_if(crate::core::states::in_game))
            .add_systems(Update, wire_tool.run_if(gameplay_active));
    }
}
