//! Magical network: source → collection → processing → stabilization →
//! storage → transfer → consumer (MASTER PROMPT §11).
//!
//! Devices are grid blocks (shared infrastructure of `kinetics`) with a
//! `MagicNode`; the old aether jars (physics parts) take part as storage.
//! Transfer is hybrid, each way with its own limits:
//! * **crystal conductor** — physical, both ways, short, reliable;
//! * **rune channel** — one way, longer, needs a clear line of sight;
//! * **resonance** — wireless between transmitter and receivers of one
//!   frequency: weak flow, losses grow with distance, other transmitters
//!   on the same frequency add noise (distortion).
//! Runes inscribed on devices filter kinds, limit flow, fix direction and
//! set the frequency. The stabilizer needs electricity (V2); the magic
//! engine is a source of the kinetic network (V1).

pub mod network;

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::{light::NotShadowCaster, prelude::*};
use serde::{Deserialize, Serialize};

use network::{MEdge, MNode, Quality, Refusal, kind_index};
use crate::{
    content::{AetherKind, LinkMode, LinkSpec, MagicDef, MagicRole, RuneKind, registry},
    core::{events::Notify, layers::Layer, rules::ModeRules, states::gameplay_active},
    electric::ElectricState,
    items::{ItemId, Use, inventory::Inventory},
    kinetics::{KineticBlock, KineticGrid},
    machinery::MachinerySet,
    magic::sources::{AetherSources, AetherStore},
    physics::rigid_bodies::CollectProp,
    player::Player,
    save::world_save::{LoadedEntityIds, SaveEntityIds, SaveHook, SaveHookExt},
    tools::{ActiveTool, EntityTarget, manipulator::MechanicsUse},
};

pub const MAX_RUNES: usize = 3;
/// Outflow allowed by a full «Ограничение» rune (aether / s).
pub const RUNE_LIMIT_BASE: f32 = 2.0;

/// Magic held by a device.
#[derive(Component, Debug, Clone)]
pub struct MagicNode {
    pub key: String,
    pub amount: f32,
    pub quality: Quality,
    /// Inscribed runes with their parameter.
    pub runes: Vec<(RuneKind, u8)>,
    /// Engine: share of nominal it got last tick.
    pub output: f32,
    /// Why something could not enter last tick (diagnostics).
    pub refused: Option<Refusal>,
}

impl MagicNode {
    pub fn def(&self) -> Option<&'static MagicDef> {
        registry().magic(&self.key)
    }
    pub fn rune(&self, k: RuneKind) -> Option<u8> {
        self.runes.iter().find(|(r, _)| *r == k).map(|(_, p)| *p)
    }
    pub fn frequency(&self) -> u8 {
        self.rune(RuneKind::Link).unwrap_or(0) + 1
    }
}

#[derive(Component, Debug, Clone)]
pub struct MagicLink {
    pub a: Entity,
    pub b: Entity,
    pub item: String,
    pub spec: LinkSpec,
    pub len: f32,
    pub flow: f32,
    /// Rune channel without line of sight.
    pub blocked: bool,
    pub refused: Option<Refusal>,
}

/// Active resonance pairs this tick (drawn, shown in panels).
#[derive(Resource, Default, Debug)]
pub struct Resonance(pub Vec<(Entity, Entity, f32)>);

fn attach_nodes(mut commands: Commands, added: Query<(Entity, &KineticBlock), (Added<KineticBlock>, Without<MagicNode>)>) {
    for (e, b) in &added {
        if registry().magic(&b.key).is_some() {
            commands.entity(e).insert(MagicNode {
                key: b.key.clone(),
                amount: 0.0,
                quality: Quality::raw(AetherKind::Calm),
                runes: Vec::new(),
                output: 0.0,
                refused: None,
            });
        }
    }
}

fn mnode_for(n: &MagicNode, def: &MagicDef, pos: Vec3, sources: &AetherSources, power: f32, engine_on: bool, integrity: f32) -> MNode {
    let mut m = MNode { amount: n.amount, capacity: def.capacity, quality: n.quality, max_distortion: def.max_distortion, ..Default::default() };
    if let Some(list) = &def.accepts {
        m.accepts = [false; 5];
        for k in list {
            m.accepts[kind_index(*k)] = true;
        }
    }
    match def.role {
        MagicRole::Collector => {
            let f = sources.sample(pos);
            m.inflow = Some((Quality::raw(f.kind), f.strength * def.rate * integrity));
        }
        MagicRole::Stabilizer => m.stabilize = def.stabilize * power * integrity,
        MagicRole::Engine if engine_on => m.consume = def.rate,
        _ => {}
    }
    for (r, p) in &n.runes {
        match r {
            RuneKind::Filter => {
                let k = AetherKind::ALL[*p as usize % 5];
                let allowed = m.accepts[kind_index(k)];
                m.accepts = [false; 5];
                m.accepts[kind_index(k)] = allowed;
            }
            RuneKind::Limit => m.out_limit = RUNE_LIMIT_BASE * (*p as f32 + 1.0) / 4.0,
            RuneKind::Direction => {
                if *p % 2 == 0 {
                    m.receive = false;
                } else {
                    m.send = false;
                }
            }
            RuneKind::Link => {}
        }
    }
    if integrity <= 0.0 {
        // Broken: holds what it has, takes and gives nothing.
        m.send = false;
        m.receive = false;
        m.inflow = None;
        m.consume = 0.0;
    }
    m
}

#[allow(clippy::too_many_arguments)]
fn step_magic(
    time: Res<Time>,
    sources: Res<AetherSources>,
    mut devices: Query<(Entity, &mut MagicNode, &GlobalTransform, Option<&ElectricState>, Option<&crate::wear::Integrity>)>,
    mut jars: Query<(Entity, &mut AetherStore, &GlobalTransform), Without<MagicNode>>,
    mut links: Query<&mut MagicLink>,
    mut kblocks: Query<&mut KineticBlock>,
    mut resonance: ResMut<Resonance>,
    mut kgrid: ResMut<KineticGrid>,
) {
    let dt = time.delta_secs().max(1e-4);
    let mut ents = Vec::new();
    let mut nodes = Vec::new();
    let mut pos = Vec::new();
    for (e, n, gt, el, integ) in &devices {
        let Some(def) = n.def() else { continue };
        let power = el.map(|s| s.satisfaction).unwrap_or(0.0);
        let on = kblocks.get(e).is_ok_and(|k| k.on);
        ents.push(e);
        nodes.push(mnode_for(&n, def, gt.translation(), &sources, power, on, integ.map(|i| i.value).unwrap_or(1.0)));
        pos.push(gt.translation());
    }
    let first_jar = ents.len();
    for (e, s, gt) in &jars {
        ents.push(e);
        nodes.push(MNode { amount: s.amount, capacity: s.capacity, quality: Quality::raw(s.kind), ..Default::default() });
        pos.push(gt.translation());
    }
    let index: HashMap<Entity, usize> = ents.iter().enumerate().map(|(i, e)| (*e, i)).collect();
    // Edges: links, then resonance.
    let mut edges = Vec::new();
    let mut link_edge = Vec::new();
    for (k, l) in links.iter().enumerate() {
        let (Some(&a), Some(&b)) = (index.get(&l.a), index.get(&l.b)) else { continue };
        if l.blocked {
            continue;
        }
        edges.push(MEdge {
            a,
            b,
            throughput: l.spec.throughput,
            loss: (l.spec.loss_per_m * l.len).min(0.9),
            one_way: l.spec.mode == LinkMode::Channel,
            noise: 0.0,
        });
        link_edge.push(k);
    }
    resonance.0.clear();
    let n_links = edges.len();
    let mut res_pairs = Vec::new();
    // (node index, role, frequency, def) of resonators, same order as nodes.
    let resonators: Vec<(usize, MagicRole, u8, &MagicDef)> = devices
        .iter()
        .filter_map(|(e, n, _, _, _)| {
            let d = n.def()?;
            matches!(d.role, MagicRole::Transmitter | MagicRole::Receiver).then(|| (index[&e], d.role, n.frequency(), d))
        })
        .collect();
    for &(tx, role, f, def) in &resonators {
        if role != MagicRole::Transmitter {
            continue;
        }
        let near = |j: usize| pos[j].distance(pos[tx]) <= def.range;
        let rx: Vec<usize> = resonators.iter().filter(|r| r.1 == MagicRole::Receiver && r.2 == f && near(r.0)).map(|r| r.0).collect();
        // Other transmitters on the same frequency interfere.
        let rivals = resonators.iter().filter(|r| r.1 == MagicRole::Transmitter && r.2 == f && r.0 != tx && near(r.0)).count();
        for &j in &rx {
            let d = pos[j].distance(pos[tx]);
            edges.push(MEdge {
                a: tx,
                b: j,
                throughput: def.throughput / rx.len() as f32,
                loss: (0.02 * d).min(0.8),
                one_way: true,
                noise: 0.03 + 0.08 * rivals as f32,
            });
            res_pairs.push((ents[tx], ents[j]));
        }
    }
    let r = network::step(&mut nodes, &edges, dt);
    // Write back.
    let mut i = 0;
    for (e, mut n, _, _, _) in &mut devices {
        if n.def().is_none() {
            continue;
        }
        let m = &nodes[i];
        n.amount = m.amount;
        n.quality = m.quality;
        n.refused = edges.iter().zip(&r.refused).find(|(x, why)| x.b == i && why.is_some()).and_then(|(_, w)| *w);
        if n.def().is_some_and(|d| d.role == MagicRole::Engine)
            && let Ok(mut k) = kblocks.get_mut(e)
        {
            let want = m.consume * dt;
            let got = if want > 0.0 { r.consumed[i] / want } else { 0.0 };
            // Power follows the aether it gets and how stable it is.
            let scale = (got * n.quality.stability).clamp(0.0, 1.0);
            n.output = scale;
            let powered = scale > 0.05;
            if k.powered != powered || (k.power_scale - scale).abs() > 0.1 {
                k.powered = powered;
                k.power_scale = (scale * 10.0).round() / 10.0;
                kgrid.dirty = true;
            }
        }
        i += 1;
    }
    for (k, (_, mut s, _)) in jars.iter_mut().enumerate() {
        s.amount = nodes[first_jar + k].amount;
    }
    for (k, mut l) in links.iter_mut().enumerate() {
        match link_edge.iter().position(|x| *x == k) {
            Some(ei) => {
                l.flow = r.flow[ei] / dt;
                l.refused = r.refused[ei];
            }
            None => {
                l.flow = 0.0;
            }
        }
    }
    for (k, (a, b)) in res_pairs.into_iter().enumerate() {
        resonance.0.push((a, b, r.flow[n_links + k] / dt));
    }
}

/// Rune channels need a clear line of sight (checked twice a second).
fn check_sight(time: Res<Time>, mut acc: Local<f32>, spatial: SpatialQuery, ends: Query<&GlobalTransform>, mut links: Query<&mut MagicLink>) {
    *acc -= time.delta_secs();
    if *acc > 0.0 {
        return;
    }
    *acc = 0.5;
    for mut l in &mut links {
        if l.spec.mode != LinkMode::Channel {
            continue;
        }
        let (Ok(a), Ok(b)) = (ends.get(l.a), ends.get(l.b)) else { continue };
        let (pa, pb) = (a.translation(), b.translation());
        let d = pb - pa;
        let Ok(dir) = Dir3::new(d) else { continue };
        let filter = SpatialQueryFilter::from_mask([Layer::Terrain]);
        // Start/end half a block away so the devices' own cells don't count.
        let blocked = spatial.cast_ray(pa + *dir * 0.5, dir, (d.length() - 1.0).max(0.0), true, &filter).is_some();
        if l.blocked != blocked {
            l.blocked = blocked;
        }
    }
}

#[derive(Resource)]
struct LinkAssets {
    mesh: Handle<Mesh>,
    crystal: Handle<StandardMaterial>,
    beam: Handle<StandardMaterial>,
    blocked: Handle<StandardMaterial>,
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(LinkAssets {
        mesh: meshes.add(Cylinder::new(1.0, 1.0)),
        crystal: mats.add(StandardMaterial { base_color: Color::srgba(0.7, 0.5, 1.0, 0.85), emissive: LinearRgba::rgb(0.6, 0.3, 1.2), alpha_mode: AlphaMode::Blend, ..default() }),
        beam: mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.85, 0.4, 0.5), emissive: LinearRgba::rgb(3.0, 2.2, 0.6), unlit: true, alpha_mode: AlphaMode::Add, ..default() }),
        blocked: mats.add(StandardMaterial { base_color: Color::srgba(0.6, 0.2, 0.2, 0.4), unlit: true, alpha_mode: AlphaMode::Blend, ..default() }),
    });
}

pub fn spawn_link(commands: &mut Commands, a: Entity, b: Entity, item: &str) -> Option<Entity> {
    let Use::MagicLink(spec) = ItemId::from_key(item)?.def().use_ else {
        return None;
    };
    Some(
        commands
            .spawn((
                Name::new(format!("Magic link {item}")),
                MagicLink { a, b, item: item.to_string(), spec, len: 0.0, flow: 0.0, blocked: false, refused: None },
                Transform::default(),
                Visibility::default(),
            ))
            .id(),
    )
}

#[allow(clippy::type_complexity)]
fn draw_links(
    mut commands: Commands,
    assets: Option<Res<LinkAssets>>,
    ends: Query<&GlobalTransform>,
    mut links: Query<(Entity, &mut MagicLink, &mut Transform, Option<&MeshMaterial3d<StandardMaterial>>)>,
    resonance: Res<Resonance>,
    time: Res<Time>,
    mut gizmos: Gizmos,
) {
    let Some(a) = assets else { return };
    for (e, mut l, mut t, mat) in &mut links {
        let (Ok(pa), Ok(pb)) = (ends.get(l.a), ends.get(l.b)) else {
            commands.entity(e).despawn();
            continue;
        };
        let (pa, pb) = (pa.translation(), pb.translation());
        let d = pb - pa;
        l.len = d.length();
        let thick = if l.spec.mode == LinkMode::Conductor { 0.05 } else { 0.02 };
        *t = Transform::from_translation(pa + d / 2.0)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y)))
            .with_scale(Vec3::new(thick, l.len.max(0.01), thick));
        let want = match (l.spec.mode, l.blocked) {
            (LinkMode::Channel, true) => a.blocked.clone(),
            (LinkMode::Channel, false) => a.beam.clone(),
            _ => a.crystal.clone(),
        };
        if mat.is_none_or(|m| m.0 != want) {
            commands.entity(e).insert((Mesh3d(a.mesh.clone()), MeshMaterial3d(want), NotShadowCaster));
        }
    }
    // Resonance: pulses travelling from transmitter to receiver.
    let phase = time.elapsed_secs() % 1.0;
    for (tx, rx, flow) in &resonance.0 {
        let (Ok(pa), Ok(pb)) = (ends.get(*tx), ends.get(*rx)) else { continue };
        let (pa, pb) = (pa.translation(), pb.translation());
        let c = if *flow > 0.0 { Color::srgb(0.95, 0.6, 1.0) } else { Color::srgba(0.95, 0.6, 1.0, 0.25) };
        for k in 0..4 {
            let s = (phase + k as f32 * 0.25) % 1.0;
            gizmos.sphere(Isometry3d::from_translation(pa.lerp(pb, s) + Vec3::Y * (s * (1.0 - s) * 2.0)), 0.08, c);
        }
    }
}

#[derive(Resource, Default)]
pub struct LinkTool {
    pub pending: Option<Entity>,
}

/// Cost in conductor items (the chisel is not used up).
pub fn link_cost(spec: &LinkSpec, len: f32) -> u32 {
    match spec.mode {
        LinkMode::Conductor => (len / 3.0).ceil().max(1.0) as u32,
        LinkMode::Channel => 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn link_tool(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    target: Res<EntityTarget>,
    devices: Query<(), Or<(With<MagicNode>, With<AetherStore>)>>,
    ends: Query<&GlobalTransform>,
    links: Query<(Entity, &MagicLink)>,
    mut state: ResMut<LinkTool>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
    mut gizmos: Gizmos,
) {
    let ActiveTool::MagicLink(item) = *tool else {
        state.pending = None;
        return;
    };
    let Use::MagicLink(spec) = item.def().use_ else { return };
    let aimed = target.entity.filter(|e| devices.contains(*e));
    if let Some(p) = state.pending.and_then(|p| ends.get(p).ok()).map(|g| g.translation()) {
        let to = if target.entity.is_some() { target.point } else { p };
        gizmos.line(p, to, if to.distance(p) <= spec.max_len { Color::srgb(0.8, 0.6, 1.0) } else { Color::srgb(1.0, 0.2, 0.2) });
    }
    if mouse.just_pressed(MouseButton::Right) {
        state.pending = None;
        let Some(dev) = aimed else { return };
        let mut back = 0u32;
        for (le, l) in &links {
            if (l.a == dev || l.b == dev) && l.item == item.def().key {
                back += link_cost(&l.spec, l.len);
                commands.entity(le).despawn();
            }
        }
        if back > 0
            && let Ok(mut inv) = player.single_mut()
        {
            inv.add(item, back);
        }
        notify.write(Notify::info("Связи сняты"));
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(dev) = aimed else {
        notify.write(Notify::warn("Связь подключается к магическому устройству"));
        return;
    };
    let Some(first) = state.pending else {
        state.pending = Some(dev);
        notify.write(Notify::info(match spec.mode {
            LinkMode::Conductor => "Выберите второе устройство",
            LinkMode::Channel => "Канал пойдёт от этого устройства — выберите приёмник",
        }));
        return;
    };
    state.pending = None;
    if first == dev || links.iter().any(|(_, l)| (l.a == first && l.b == dev) || (l.a == dev && l.b == first)) {
        return;
    }
    let (Ok(pa), Ok(pb)) = (ends.get(first), ends.get(dev)) else { return };
    let len = pa.translation().distance(pb.translation());
    if len > spec.max_len {
        notify.write(Notify::warn(format!("Слишком далеко: {len:.1} м (до {:.0} м)", spec.max_len)));
        return;
    }
    let cost = link_cost(&spec, len);
    if rules.consume_items && cost > 0 {
        let Ok(mut inv) = player.single_mut() else { return };
        if !inv.remove(item, cost) {
            notify.write(Notify::warn(format!("Нужно проводника: {cost}")));
            return;
        }
    }
    spawn_link(&mut commands, first, dev, &item.def().key);
    notify.write(Notify::ok(match spec.mode {
        LinkMode::Conductor => format!("Проводник проложен: {len:.1} м"),
        LinkMode::Channel => format!("Рунический канал: {len:.1} м"),
    }));
}

/// RMB with a rune: inscribe it on the aimed magical device.
fn rune_tool(
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    target: Res<EntityTarget>,
    mut nodes: Query<&mut MagicNode>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let ActiveTool::Rune(item) = *tool else { return };
    let Use::Rune(kind) = item.def().use_ else { return };
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Some(mut n) = target.entity.and_then(|e| nodes.get_mut(e).ok()) else {
        notify.write(Notify::warn("Руну наносят на магическое устройство"));
        return;
    };
    if n.rune(kind).is_some() {
        notify.write(Notify::warn("Такая руна уже нанесена"));
        return;
    }
    if n.runes.len() >= MAX_RUNES {
        notify.write(Notify::warn("Нет места: не больше трёх рун"));
        return;
    }
    if rules.consume_items {
        let Ok(mut inv) = player.single_mut() else { return };
        if !inv.remove(item, 1) {
            return;
        }
    }
    n.runes.push((kind, 0));
    notify.write(Notify::ok(format!("Нанесена руна «{}»: {}", kind.label(), rune_value(kind, 0))));
}

pub fn rune_value(k: RuneKind, p: u8) -> String {
    match k {
        RuneKind::Filter => format!("только {}", AetherKind::ALL[p as usize % 5].label()),
        RuneKind::Limit => format!("{} %", (p as u32 + 1) * 25),
        RuneKind::Direction => if p % 2 == 0 { "только отдаёт".into() } else { "только принимает".into() },
        RuneKind::Link => format!("частота {}", p + 1),
    }
}

fn rune_steps(k: RuneKind) -> u8 {
    match k {
        RuneKind::Filter => 5,
        RuneKind::Limit => 4,
        RuneKind::Direction => 2,
        RuneKind::Link => 8,
    }
}

/// Mechanics mode on a magical device: LMB / RMB change the first / second
/// rune's value.
fn mechanics_runes(mut reader: MessageReader<MechanicsUse>, mut nodes: Query<&mut MagicNode>, kin: Query<&KineticBlock>, mut notify: MessageWriter<Notify>) {
    for u in reader.read() {
        let Ok(mut n) = nodes.get_mut(u.entity) else { continue };
        // The magic engine is switched by `kinetics` (it is a motor).
        if kin.get(u.entity).is_ok_and(|k| k.def().is_some_and(|d| d.role != crate::kinetics::network::Role::Static)) {
            continue;
        }
        let slot = if u.secondary { 1 } else { 0 };
        let Some((k, p)) = n.runes.get_mut(slot) else {
            notify.write(Notify::info("Нет руны в этой ячейке: нанесите руну (ПКМ с руной в руке)"));
            continue;
        };
        *p = (*p + 1) % rune_steps(*k);
        notify.write(Notify::info(format!("Руна «{}»: {}", k.label(), rune_value(*k, *p))));
    }
}

/// Collecting a device gives back its runes too.
fn collect_runes(mut reader: MessageReader<CollectProp>, nodes: Query<&MagicNode>, rules: Res<ModeRules>, mut player: Query<&mut Inventory, With<Player>>) {
    for CollectProp(e) in reader.read() {
        let Ok(n) = nodes.get(*e) else { continue };
        if !rules.consume_items {
            continue;
        }
        let Ok(mut inv) = player.single_mut() else { continue };
        for (k, _) in &n.runes {
            let key = match k {
                RuneKind::Filter => "rune_filter",
                RuneKind::Limit => "rune_limit",
                RuneKind::Direction => "rune_direction",
                RuneKind::Link => "rune_link",
            };
            inv.add(ItemId::k(key), 1);
        }
    }
}

/// Lines for engineering panels: (lines, diagnostics).
pub fn describe(world: &World, e: Entity) -> Option<(Vec<String>, Vec<String>)> {
    let mut lines = Vec::new();
    let mut diag = Vec::new();
    if let Some(n) = world.get::<MagicNode>(e) {
        let def = n.def()?;
        lines.push(format!(
            "Эфир: {:.1} / {:.0} · {} · стабильность {:.0}% · искажение {:.0}%",
            n.amount, def.capacity, n.quality.kind.label(), n.quality.stability * 100.0, n.quality.distortion * 100.0
        ));
        if !n.runes.is_empty() {
            let r: Vec<String> = n.runes.iter().map(|(k, p)| format!("{} ({})", k.label(), rune_value(*k, *p))).collect();
            lines.push(format!("Руны: {}", r.join(", ")));
        }
        match def.role {
            MagicRole::Transmitter | MagicRole::Receiver => lines.push(format!("Резонанс: частота {}", n.frequency())),
            MagicRole::Engine => {
                lines.push(format!("Мощность от эфира: {:.0}%", n.output * 100.0));
                if n.output < 0.05 {
                    diag.push("Двигателю не хватает подходящего эфира".into());
                }
            }
            MagicRole::Stabilizer => {
                let p = world.get::<ElectricState>(e).map(|s| s.satisfaction).unwrap_or(0.0);
                if p < 0.5 {
                    diag.push("Стабилизатор без электроэнергии не работает".into());
                }
            }
            _ => {}
        }
        if let Some(why) = n.refused {
            diag.push(format!("Не принимает: {}", why.label()));
        }
        if n.quality.distortion > 0.5 && n.amount > 0.1 {
            diag.push("Сильно искажённая магия: нестабильно".into());
        }
    }
    let mut q = world.try_query::<&MagicLink>()?;
    let mine: Vec<&MagicLink> = q.iter(world).filter(|l| l.a == e || l.b == e).collect();
    if !mine.is_empty() {
        let flow: f32 = mine.iter().map(|l| l.flow).sum();
        lines.push(format!("Магических связей: {} · поток {:.2}/с", mine.len(), flow));
        if mine.iter().any(|l| l.blocked) {
            diag.push("Рунический канал прерван: нет прямой видимости".into());
        }
    }
    if lines.is_empty() { None } else { Some((lines, diag)) }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
enum End {
    Prop(u32),
    Kinetic(IVec3),
}

#[derive(Serialize, Deserialize)]
struct SavedNode {
    cell: IVec3,
    amount: f32,
    kind: AetherKind,
    stability: f32,
    distortion: f32,
    runes: Vec<(RuneKind, u8)>,
}

#[derive(Serialize, Deserialize)]
struct SavedLink {
    a: End,
    b: End,
    item: String,
}

#[derive(Serialize, Deserialize, Default)]
struct SavedArcana {
    nodes: Vec<SavedNode>,
    links: Vec<SavedLink>,
}

fn end_of(world: &World, ids: &SaveEntityIds, e: Entity) -> Option<End> {
    if let Some(k) = world.get::<KineticBlock>(e) {
        return Some(End::Kinetic(k.cell));
    }
    ids.0.get(&e).map(|id| End::Prop(*id))
}

fn capture(world: &mut World) -> serde_json::Value {
    let ids = world.get_resource::<SaveEntityIds>().cloned().unwrap_or_default();
    let mut q = world.query::<(&MagicNode, &KineticBlock)>();
    let nodes = q
        .iter(world)
        .map(|(n, k)| SavedNode { cell: k.cell, amount: n.amount, kind: n.quality.kind, stability: n.quality.stability, distortion: n.quality.distortion, runes: n.runes.clone() })
        .collect();
    let mut lq = world.query::<&MagicLink>();
    let links = lq
        .iter(world)
        .filter_map(|l| Some(SavedLink { a: end_of(world, &ids, l.a)?, b: end_of(world, &ids, l.b)?, item: l.item.clone() }))
        .collect();
    serde_json::to_value(SavedArcana { nodes, links }).unwrap_or_default()
}

fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let mut q = world.query_filtered::<Entity, With<MagicLink>>();
    let old: Vec<Entity> = q.iter(world).collect();
    for e in old {
        world.entity_mut(e).despawn();
    }
    let s: SavedArcana = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut kq = world.query::<(Entity, &KineticBlock)>();
    let cells: HashMap<IVec3, Entity> = kq.iter(world).map(|(e, k)| (k.cell, e)).collect();
    // Nodes may not have their MagicNode yet (attached next frame): insert.
    for n in &s.nodes {
        let Some(&e) = cells.get(&n.cell) else { continue };
        let key = world.get::<KineticBlock>(e).map(|k| k.key.clone()).unwrap_or_default();
        world.entity_mut(e).insert(MagicNode {
            key,
            amount: n.amount,
            quality: Quality { kind: n.kind, stability: n.stability, distortion: n.distortion },
            runes: n.runes.clone(),
            output: 0.0,
            refused: None,
        });
    }
    let ids = world.get_resource::<LoadedEntityIds>().cloned().unwrap_or_default();
    let resolve = |end: End| match end {
        End::Prop(id) => ids.0.get(&id).copied(),
        End::Kinetic(c) => cells.get(&c).copied(),
    };
    let mut commands = world.commands();
    for l in &s.links {
        if let (Some(a), Some(b)) = (resolve(l.a), resolve(l.b)) {
            spawn_link(&mut commands, a, b, &l.item);
        }
    }
    world.flush();
}

/// A spark rising from a magical device (purely visual).
#[derive(Component)]
struct Spark {
    vel: Vec3,
    life: f32,
}

#[derive(Resource)]
struct SparkAssets {
    mesh: Handle<Mesh>,
    by_kind: Vec<Handle<StandardMaterial>>,
}

/// Upper bound on live sparks (cheap even on weak GPUs).
const MAX_SPARKS: usize = 240;

fn setup_sparks(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let by_kind = AetherKind::ALL
        .iter()
        .map(|k| {
            let [r, g, b] = k.rgb();
            mats.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(r * 4.0, g * 4.0, b * 4.0)), unlit: true, ..default() })
        })
        .collect();
    commands.insert_resource(SparkAssets { mesh: meshes.add(Cuboid::new(0.05, 0.05, 0.05)), by_kind });
}

fn sparks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<SparkAssets>>,
    devices: Query<(&MagicNode, &GlobalTransform)>,
    mut live: Query<(Entity, &mut Spark, &mut Transform)>,
    mut acc: Local<f32>,
) {
    let Some(a) = assets else { return };
    let dt = time.delta_secs();
    let mut count = 0;
    for (e, mut s, mut t) in &mut live {
        s.life -= dt;
        if s.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        count += 1;
        t.translation += s.vel * dt;
        s.vel.y += 0.15 * dt;
        t.scale = Vec3::splat((s.life / 1.6).clamp(0.0, 1.0));
    }
    *acc += dt;
    if *acc < 0.1 {
        return;
    }
    *acc = 0.0;
    let seed = (time.elapsed_secs() * 1000.0) as u32;
    for (i, (n, gt)) in devices.iter().enumerate() {
        let Some(def) = n.def() else { continue };
        let fill = n.amount / def.capacity.max(1e-3);
        if fill < 0.05 || count >= MAX_SPARKS {
            continue;
        }
        // Pseudo-random from time and index (no RNG dependency).
        let h = |k: u32| ((seed.wrapping_mul(2654435761).wrapping_add(i as u32 * 97 + k * 31)) % 1000) as f32 / 1000.0;
        if h(0) > 0.25 + fill * 0.5 {
            continue;
        }
        let p = gt.translation() + Vec3::new(h(1) - 0.5, 0.3 + h(2) * 0.3, h(3) - 0.5) * 0.7;
        let kind = AetherKind::ALL.iter().position(|k| *k == n.quality.kind).unwrap_or(0);
        commands.spawn((
            Spark { vel: Vec3::new(h(4) - 0.5, 0.4 + h(5) * 0.4, h(6) - 0.5) * 0.4, life: 1.0 + h(7) * 0.8 },
            Mesh3d(a.mesh.clone()),
            MeshMaterial3d(a.by_kind[kind].clone()),
            Transform::from_translation(p),
            NotShadowCaster,
        ));
        count += 1;
    }
}

pub struct ArcanaPlugin;

impl Plugin for ArcanaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Resonance>()
            .init_resource::<LinkTool>()
            .add_systems(Startup, (setup_assets, setup_sparks))
            .add_save_hook(SaveHook { key: "arcana", capture, restore })
            .add_systems(
                Update,
                (attach_nodes, check_sight, draw_links, mechanics_runes, collect_runes, sparks).run_if(crate::core::states::in_game),
            )
            .add_systems(Update, (link_tool, rune_tool).run_if(gameplay_active))
            .add_systems(
                FixedUpdate,
                step_magic.after(MachinerySet::Power).run_if(crate::core::states::in_game),
            );
    }
}
