//! Logic and automation: wires, buttons, levers, timers, pulse generators,
//! logic gates and flasher lights. On / off signals, no decay.
//!
//! * **Neighbours, like redstone**: a working source powers the blocks on
//!   all six sides (gates and pulse generators only the block in front of
//!   them). A powered block that takes signals (clutch, gearshift, speed
//!   controller, suspension, headlight, bearing, flasher…) gets signal 1.
//! * **Wire buses** (like Ender IO conduits): one block carries up to eight
//!   wires of different colours. RMB with a wire places a bus with the
//!   chosen colour (Shift + wheel), RMB on a bus adds that colour, LMB with
//!   the wrench takes it out. The same colour in neighbouring buses is one
//!   line; colours never mix, so one row of buses carries eight separate
//!   lines. A colour touches the blocks around it only **where its line
//!   ends** (a bus with at most one neighbour of that colour): lines run
//!   past machines without touching them and connect where they end. A line
//!   is on while a working source stands next to one of its ends; then it
//!   powers every block next to its ends.
//! * **Gates** (wrench: И / ИЛИ / НЕ / RS-триггер) read their left, right
//!   and back sides and output at the front, one frame later (so loops do
//!   not hang). RS: left sets, right resets.
//! * **Timer** gives a short pulse every «Интервал» seconds; the **pulse
//!   generator** turns the front of a signal into a short pulse; the
//!   **flasher** lights up on a pulse and blinks while held on.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::{
    content::Mech,
    core::toast,
    grid::{DIRS, VoxelGrid},
    interact::UseBlock,
    inventory::Inventory,
    mech::{BlockEntity, MechSet, Params, Signal, side_axis},
    style::{BoxCache, LayerNode, Layer, Materials, Part, part, spawn_parts},
};

/// Wire colours (name, sRGB).
pub const COLORS: [(&str, [f32; 3]); 8] = [
    ("Красный", [0.85, 0.16, 0.12]),
    ("Синий", [0.16, 0.36, 0.9]),
    ("Зелёный", [0.2, 0.75, 0.25]),
    ("Жёлтый", [0.95, 0.82, 0.18]),
    ("Белый", [0.9, 0.9, 0.88]),
    ("Оранжевый", [0.95, 0.5, 0.12]),
    ("Фиолетовый", [0.6, 0.25, 0.85]),
    ("Голубой", [0.25, 0.8, 0.92]),
];
pub const WIRE_MATS: [&str; 8] = ["wire_0", "wire_1", "wire_2", "wire_3", "wire_4", "wire_5", "wire_6", "wire_7"];
pub const WIRE_MATS_ON: [&str; 8] = ["wire_0_on", "wire_1_on", "wire_2_on", "wire_3_on", "wire_4_on", "wire_5_on", "wire_6_on", "wire_7_on"];

/// How long a button stays pressed, a timer / pulse generator pulse lasts.
pub const BUTTON: f32 = 0.8;
pub const PULSE: f32 = 0.3;
/// A flasher held on longer than this starts to blink (period).
pub const BLINK: f32 = 0.6;

/// Colour for newly laid wires.
#[derive(Resource, Default)]
pub struct WireColor(pub usize);

/// State of a logic block (and of a wire: `input` = powered).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct LogicState {
    /// Output this frame.
    pub out: bool,
    /// Powered from any side this frame.
    pub input: bool,
    /// Lever position / RS memory / last input (edges).
    pub memory: bool,
    /// Timer phase, button / pulse time left, flasher on-time.
    pub t: f32,
}

// ------------------------------------------------------------ pure parts

/// A wire bus: the colours it carries (bits 0..7).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bus(pub u8);

/// Colours of a bus that are on this frame.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BusPower(pub u8);

pub fn colours(mask: u8) -> impl Iterator<Item = usize> {
    (0..8).filter(move |k| mask & (1 << k) != 0)
}

/// Is colour `k` of the bus at `c` an end of its line (at most one
/// neighbouring bus carries it)?
pub fn is_end(buses: &HashMap<IVec3, u8>, c: IVec3, k: usize) -> bool {
    DIRS.iter().filter(|d| buses.get(&(c + **d)).is_some_and(|m| m & (1 << k) != 0)).count() <= 1
}

/// Cells a working source powers: `front` for one-way outputs.
pub fn targets(cell: IVec3, front: Option<IVec3>) -> Vec<IVec3> {
    match front {
        Some(f) => vec![cell + f],
        None => DIRS.iter().map(|d| cell + *d).collect(),
    }
}

/// Spreads power. Returns the powered colours of every bus and, for every
/// powered block cell, the sides it is powered from (bits of `DIRS`).
pub fn spread(buses: &HashMap<IVec3, u8>, sources: &[(IVec3, Option<IVec3>)]) -> (HashMap<IVec3, u8>, HashMap<IVec3, u8>) {
    let mut on: HashMap<IVec3, u8> = HashMap::new();
    let mut blocks: HashMap<IVec3, u8> = HashMap::new();
    let hit = |blocks: &mut HashMap<IVec3, u8>, target: IVec3, from: IVec3| {
        let side = DIRS.iter().position(|d| *d == from - target).unwrap_or(0);
        *blocks.entry(target).or_default() |= 1 << side;
    };
    let mut q = VecDeque::new();
    for &(c, front) in sources {
        for t in targets(c, front) {
            match buses.get(&t) {
                // A source feeds the colours that end next to it.
                Some(&m) => {
                    for k in colours(m) {
                        if is_end(buses, t, k) && on.get(&t).is_none_or(|x| x & (1 << k) == 0) {
                            *on.entry(t).or_default() |= 1 << k;
                            q.push_back((t, k));
                        }
                    }
                }
                None => hit(&mut blocks, t, c),
            }
        }
    }
    while let Some((c, k)) = q.pop_front() {
        for d in DIRS {
            let o = c + d;
            if buses.get(&o).is_some_and(|m| m & (1 << k) != 0) && on.get(&o).is_none_or(|x| x & (1 << k) == 0) {
                *on.entry(o).or_default() |= 1 << k;
                q.push_back((o, k));
            }
        }
    }
    for (&c, &m) in &on {
        for k in colours(m) {
            if !is_end(buses, c, k) {
                continue;
            }
            for d in DIRS {
                let t = c + d;
                if !buses.contains_key(&t) {
                    hit(&mut blocks, t, c);
                }
            }
        }
    }
    (on, blocks)
}

/// Gate modes (wrench «Режим»).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    And,
    Or,
    Not,
    Rs,
}

impl Gate {
    pub fn of(v: f32) -> Gate {
        match v as i32 {
            0 => Gate::And,
            1 => Gate::Or,
            2 => Gate::Not,
            _ => Gate::Rs,
        }
    }
    /// (output, memory) from the left, right and back inputs.
    pub fn eval(self, left: bool, right: bool, back: bool, memory: bool) -> (bool, bool) {
        match self {
            Gate::And => (left && right, memory),
            Gate::Or => (left || right || back, memory),
            Gate::Not => (!(left || right || back), memory),
            Gate::Rs => {
                let m = if right { false } else if left { true } else { memory };
                (m, m)
            }
        }
    }
}

/// Flasher lamp: lit on a short signal, blinks while held on.
pub fn flasher_lit(powered: bool, on_for: f32) -> bool {
    powered && (on_for < BLINK || (on_for / (BLINK * 0.5)) as i32 % 2 == 0)
}

// ------------------------------------------------------------ systems

pub const LOGIC_MECHS: [Mech; 11] = [Mech::Wire, Mech::Button, Mech::Lever, Mech::Timer, Mech::PulseGenerator, Mech::LogicGate, Mech::Flasher, Mech::ThresholdSensor, Mech::RadioTransmitter, Mech::RadioReceiver, Mech::Lamp];

fn is_logic(m: Option<Mech>) -> bool {
    m.is_some_and(|m| LOGIC_MECHS.contains(&m))
}

/// Radio channels: on this frame (`on`, read by receivers) and being
/// collected from the transmitters for the next one.
#[derive(Resource, Debug, Default)]
pub struct Radio {
    pub on: HashSet<u8>,
    next: HashSet<u8>,
}

fn add_state(mut commands: Commands, color: Res<WireColor>, mut added: Query<(Entity, &BlockEntity, &mut Params), (Added<BlockEntity>, Without<LogicState>)>) {
    for (e, b, mut p) in &mut added {
        if !is_logic(b.mech()) {
            continue;
        }
        commands.entity(e).insert(LogicState::default());
        if b.mech() == Some(Mech::Wire) {
            // A loaded bus gets its colours from the save right after.
            commands.entity(e).insert((Bus(1 << color.0), BusPower::default()));
        }
        let _ = &mut p;
    }
}

/// RMB with a wire on a bus: add the chosen colour.
fn add_colour(mut used: MessageReader<UseBlock>, grids: Query<&VoxelGrid>, mut buses: Query<&mut Bus>, color: Res<WireColor>, mut toasts: MessageWriter<crate::core::Toast>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    for u in used.read() {
        if u.mech != Mech::Wire {
            continue;
        }
        let Some(&e) = grids.get(u.grid).ok().and_then(|g| g.entities.get(&u.cell)) else { continue };
        let Ok(mut b) = buses.get_mut(e) else { continue };
        let bit = 1u8 << color.0;
        if b.0 & bit != 0 {
            toasts.write(toast(format!("{} уже есть в этой шине", COLORS[color.0].0)));
        } else {
            b.0 |= bit;
            toasts.write(toast(format!("В шину добавлен {} провод ({} из 8)", COLORS[color.0].0.to_lowercase(), b.0.count_ones())));
            sounds.write(crate::sound::sfx("place"));
        }
    }
}

/// RMB on a lever / button.
fn use_switches(mut used: MessageReader<UseBlock>, grids: Query<&VoxelGrid>, mut states: Query<&mut LogicState>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    for u in used.read() {
        if !matches!(u.mech, Mech::Lever | Mech::Button) {
            continue;
        }
        let Some(&e) = grids.get(u.grid).ok().and_then(|g| g.entities.get(&u.cell)) else { continue };
        let Ok(mut s) = states.get_mut(e) else { continue };
        if u.mech == Mech::Lever {
            s.memory = !s.memory;
        } else {
            s.t = BUTTON;
        }
        sounds.write(crate::sound::sfx("toggle"));
    }
}

#[allow(clippy::type_complexity)]
fn run(time: Res<Time>, grids: Query<&VoxelGrid>, mut blocks: Query<(&BlockEntity, &Params, &mut LogicState, Option<&Signal>)>, mut signals: Query<&mut Signal, Without<LogicState>>, mut radio: ResMut<Radio>, mut bus: Query<(&Bus, &mut BusPower)>) {
    let _t = crate::profile::scope("логика");
    let dt = time.delta_secs();
    // Channels on last frame (receivers), collected anew this frame.
    radio.on = std::mem::take(&mut radio.next);
    for grid in &grids {
        if grid.entities.is_empty() {
            continue;
        }
        // 1. Outputs of this frame (from last frame's inputs).
        let logic: Vec<(IVec3, Entity)> = grid.entities_of(&LOGIC_MECHS).collect();
        if logic.is_empty() {
            continue;
        }
        let mut wires = HashMap::new();
        let mut sources = Vec::new();
        for &(c, e) in &logic {
            let Ok((b, p, mut s, _)) = blocks.get_mut(e) else { continue };
            let front = Some(b.dir());
            let out = match b.mech() {
                Some(Mech::Wire) => {
                    if let Ok((m, _)) = bus.get(e) {
                        wires.insert(c, m.0);
                    }
                    continue;
                }
                Some(Mech::Lever) => (s.memory, None),
                Some(Mech::Button) => {
                    s.t = (s.t - dt).max(0.0);
                    (s.t > 0.0, None)
                }
                Some(Mech::Timer) => {
                    let interval = p.get(b.id, "interval").max(0.1);
                    s.t += dt;
                    if s.t >= interval {
                        s.t -= interval;
                    }
                    (s.t < PULSE, None)
                }
                Some(Mech::PulseGenerator | Mech::LogicGate) => (s.out, front),
                // Set by `telemetry` from its reading.
                Some(Mech::ThresholdSensor) => (s.memory, None),
                Some(Mech::RadioReceiver) => (radio.on.contains(&(p.get(b.id, "channel") as u8)), None),
                _ => continue,
            };
            s.out = out.0;
            if out.0 {
                sources.push((c, out.1));
            }
        }
        // 2. Power.
        let (on_wires, powered) = spread(&wires, &sources);
        for &(c, e) in &logic {
            if let Ok((_, mut pw)) = bus.get_mut(e) {
                let want = BusPower(on_wires.get(&c).copied().unwrap_or(0));
                if *pw != want {
                    *pw = want;
                }
            }
        }
        // 3. Inputs → next outputs; signals to the blocks that take them
        // (logic blocks, and any block next to power).
        let own: HashSet<IVec3> = logic.iter().map(|(c, _)| *c).collect();
        let mut visit = logic;
        visit.extend(powered.keys().filter(|c| !own.contains(*c)).filter_map(|c| grid.entities.get(c).map(|e| (*c, *e))));
        for (c, e) in visit {
            let from = powered.get(&c).copied().unwrap_or(0);
            if let Ok((b, p, mut s, sig)) = blocks.get_mut(e) {
                let side = |d: IVec3| DIRS.iter().position(|x| *x == d).is_some_and(|i| from & (1 << i) != 0);
                match b.mech() {
                    Some(Mech::Wire) => s.input = on_wires.get(&c).is_some_and(|m| *m != 0),
                    Some(Mech::LogicGate) => {
                        let (f, x) = (b.dir(), side_axis(b));
                        let (out, mem) = Gate::of(p.get(b.id, "mode")).eval(side(x), side(-x), side(-f), s.memory);
                        s.out = out;
                        s.memory = mem;
                        s.input = from != 0;
                    }
                    Some(Mech::PulseGenerator) => {
                        // Inputs on any side but its output.
                        let input = from & !(1 << DIRS.iter().position(|d| *d == b.dir()).unwrap_or(0)) != 0;
                        if input && !s.memory {
                            s.t = PULSE;
                        }
                        s.memory = input;
                        s.t = (s.t - dt).max(0.0);
                        s.out = s.t > 0.0;
                        s.input = input;
                    }
                    Some(Mech::RadioTransmitter) => {
                        // Powered by logic, or by a control-panel sensor on it.
                        s.input = from != 0 || sig.is_some_and(|x| x.0 != 0.0);
                        s.out = s.input;
                        if s.input {
                            radio.next.insert(p.get(b.id, "channel") as u8);
                        }
                    }
                    Some(Mech::Lamp) => {
                        s.input = from != 0;
                        s.out = s.input;
                    }
                    Some(Mech::ThresholdSensor | Mech::RadioReceiver) => {}
                    Some(Mech::Flasher) => {
                        s.input = from != 0;
                        s.t = if s.input { s.t + dt } else { 0.0 };
                        s.out = flasher_lit(s.input, s.t);
                    }
                    _ => s.input = from != 0,
                }
                continue;
            }
            if from != 0
                && let Ok(mut sig) = signals.get_mut(e)
                && sig.0 == 0.0
            {
                sig.0 = 1.0;
            }
        }
    }
}

/// Shift + wheel with a wire in hand: colour for new wires.
fn pick_color(keys: Res<ButtonInput<KeyCode>>, scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>, players: Query<&Inventory, With<crate::player::Player>>, mut color: ResMut<WireColor>, mut toasts: MessageWriter<crate::core::Toast>) {
    let Ok(inv) = players.single() else { return };
    let wire = inv.held().is_some_and(|s| s.item.def().key == "wire");
    if !wire || !keys.pressed(KeyCode::ShiftLeft) || scroll.delta.y == 0.0 {
        return;
    }
    color.0 = (color.0 as i32 - scroll.delta.y.signum() as i32).rem_euclid(COLORS.len() as i32) as usize;
    toasts.write(toast(format!("Цвет провода: {}", COLORS[color.0].0)));
}

// ------------------------------------------------------------ looks

/// Where colour `k` runs in the cross-section of an arm along `axis`
/// (fixed per axis, so strands line up from bus to bus).
fn strand(axis: IVec3, k: usize) -> Vec3 {
    let (u, v) = match axis.abs() {
        IVec3 { x: 1, .. } => (Vec3::Y, Vec3::Z),
        IVec3 { y: 1, .. } => (Vec3::X, Vec3::Z),
        _ => (Vec3::X, Vec3::Y),
    };
    u * ((k % 4) as f32 - 1.5) * 0.065 + v * ((k / 4) as f32 - 0.5) * 0.065
}

/// Model of a bus: a dark hub, and per arm the strands of the colours that
/// go that way (lit while on).
pub fn bus_parts(mask: u8, power: u8, arms: &[(IVec3, u8)]) -> Vec<Part> {
    let mut v = vec![part([0.3, 0.3, 0.3], "steel", [0.0, 0.0, 0.0])];
    for k in colours(mask) {
        let mat = if power & (1 << k) != 0 { WIRE_MATS_ON[k] } else { WIRE_MATS[k] };
        // A stub on the hub shows every colour the bus carries.
        v.push(part([0.06, 0.06, 0.06], mat, (Vec3::Y * 0.16 + strand(IVec3::Y, k)).to_array()));
    }
    for &(d, m) in arms {
        let a = d.as_vec3();
        let size = Vec3::splat(0.05) + a.abs() * 0.45;
        for k in colours(m) {
            let mat = if power & (1 << k) != 0 { WIRE_MATS_ON[k] } else { WIRE_MATS[k] };
            v.push(part(size.to_array(), mat, (a * 0.27 + strand(d, k)).to_array()));
        }
        // A brass collar where the arm leaves the hub.
        v.push(part((Vec3::splat(0.24) - a.abs() * 0.18).to_array(), "brass", (a * 0.17).to_array()));
    }
    v
}

/// What a bus's model was built for.
#[derive(Component, Debug, Clone, PartialEq)]
struct BusLook(u8, u8, Vec<(IVec3, u8)>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn wire_looks(
    mut commands: Commands,
    grids: Query<&VoxelGrid>,
    wires: Query<(Entity, &BlockEntity, &Bus, &BusPower, Option<&BusLook>, Option<&Children>)>,
    all: Query<&BlockEntity>,
    nodes: Query<(), With<LayerNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (e, b, bus, power, look, children) in &wires {
        let Ok(grid) = grids.get(b.grid) else { continue };
        let neighbour = |c: IVec3| grid.entities.get(&c).and_then(|o| wires.get(*o).ok()).map(|w| w.2.0);
        let mut map: HashMap<IVec3, u8> = HashMap::new();
        map.insert(b.cell, bus.0);
        for d in DIRS {
            if let Some(m) = neighbour(b.cell + d) {
                map.insert(b.cell + d, m);
            }
        }
        // Arms: shared colours towards buses; colours ending here towards
        // the logic blocks and machines next to it.
        let mut arms: Vec<(IVec3, u8)> = Vec::new();
        for d in DIRS {
            let o = b.cell + d;
            let m = match map.get(&o) {
                Some(m) => m & bus.0,
                None => {
                    let takes = grid.entities.get(&o).and_then(|x| all.get(*x).ok()).and_then(|x| x.mech()).is_some_and(|m| is_logic(Some(m)) || takes_signal(m));
                    if !takes {
                        continue;
                    }
                    colours(bus.0).filter(|k| is_end(&map, b.cell, *k)).fold(0u8, |a, k| a | (1 << k))
                }
            };
            if m != 0 {
                arms.push((d, m));
            }
        }
        let want = BusLook(bus.0, power.0, arms.clone());
        if look == Some(&want) {
            continue;
        }
        for c in children.into_iter().flatten() {
            if nodes.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        spawn_parts(&mut commands, e, &bus_parts(bus.0, power.0, &arms), &mut meshes, &mut cache, &mats, false);
        commands.entity(e).insert(want);
    }
}

fn hook_save(w: &World, e: Entity, _: &crate::save::SaveCtx) -> Option<String> {
    w.get::<Bus>(e).map(|b| b.0.to_string())
}

fn hook_load(w: &mut World, e: Entity, d: &str, _: &crate::save::LoadCtx) {
    if let (Ok(m), Some(mut b)) = (d.parse::<u8>(), w.get_mut::<Bus>(e)) {
        b.0 = m;
    }
}

/// Blocks that react to a signal.
pub fn takes_signal(m: Mech) -> bool {
    matches!(m, Mech::Clutch | Mech::Gearshift | Mech::SpeedController | Mech::Suspension | Mech::Headlight | Mech::Bearing | Mech::Flasher | Mech::LogicGate | Mech::PulseGenerator | Mech::SlidingDoor | Mech::Lamp | Mech::RadioTransmitter | Mech::Valve)
}

/// A lamp on a logic block (lit with its output).
#[derive(Component)]
struct Lamp {
    on: &'static str,
    off: &'static str,
    lit: Option<bool>,
}

/// The flasher's light.
#[derive(Component)]
struct FlashLight;

fn add_lamps(mut commands: Commands, added: Query<(Entity, &BlockEntity), Added<LogicState>>, mut meshes: ResMut<Assets<Mesh>>, mut cache: ResMut<BoxCache>, mats: Option<Res<Materials>>) {
    let Some(mats) = mats else { return };
    for (e, b) in &added {
        let (size, at, on) = match b.mech() {
            Some(Mech::Timer) => (Vec3::new(0.14, 0.14, 0.04), Vec3::new(0.3, 0.32, 0.45), "glow_green"),
            Some(Mech::PulseGenerator) => (Vec3::new(0.16, 0.08, 0.06), Vec3::new(0.0, -0.14, 0.44), "glow_amber"),
            Some(Mech::LogicGate) => (Vec3::new(0.16, 0.08, 0.06), Vec3::new(0.0, -0.2, 0.44), "glow_red"),
            Some(Mech::Flasher) => (Vec3::new(0.26, 0.26, 0.26), Vec3::new(0.0, -0.14, 0.0), "glow_amber"),
            Some(Mech::Lamp) => (Vec3::new(0.4, 0.4, 0.4), Vec3::new(0.0, -0.08, 0.0), "glow_lamp"),
            Some(Mech::RadioReceiver) => (Vec3::new(0.12, 0.12, 0.06), Vec3::new(0.0, 0.1, 0.46), "glow_green"),
            Some(Mech::RadioTransmitter) => (Vec3::new(0.12, 0.12, 0.06), Vec3::new(0.0, 0.1, 0.46), "glow_red"),
            Some(Mech::ThresholdSensor) => (Vec3::new(0.16, 0.08, 0.16), Vec3::new(0.0, 0.3, 0.0), "glow_cyan"),
            _ => continue,
        };
        commands.spawn((Lamp { on, off: "seg_off", lit: None }, Mesh3d(cache.get(&mut meshes, size)), MeshMaterial3d(mats.get("seg_off")), Transform::from_translation(at), bevy::light::NotShadowCaster, ChildOf(e)));
        if b.mech() == Some(Mech::Lamp) {
            commands.spawn((FlashLight, PointLight { color: Color::srgb(1.0, 0.86, 0.62), intensity: 40_000.0, range: 12.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 0.0, 0.0), Visibility::Hidden, ChildOf(e)));
        }
        if b.mech() == Some(Mech::Flasher) {
            commands.spawn((FlashLight, PointLight { color: Color::srgb(1.0, 0.62, 0.2), intensity: 25_000.0, range: 10.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, -0.1, 0.0), Visibility::Hidden, ChildOf(e)));
        }
    }
}

#[allow(clippy::type_complexity)]
fn animate(
    blocks: Query<(&BlockEntity, &LogicState, &Params, &Children)>,
    mut lamps: Query<(&mut Lamp, &mut MeshMaterial3d<StandardMaterial>)>,
    mut lights: Query<&mut Visibility, With<FlashLight>>,
    mut nodes: Query<(&LayerNode, &mut Transform)>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (b, s, p, children) in &blocks {
        for c in children.iter() {
            if let Ok((mut l, mut m)) = lamps.get_mut(c)
                && l.lit != Some(s.out)
            {
                l.lit = Some(s.out);
                m.0 = mats.get(if s.out { l.on } else { l.off });
            }
            if let Ok(mut v) = lights.get_mut(c) {
                v.set_if_neq(if s.out { Visibility::Inherited } else { Visibility::Hidden });
            }
            if let Ok((l, mut t)) = nodes.get_mut(c)
                && l.0 == Layer::Mount
            {
                match b.mech() {
                    // Pivot at the base plate (local +Y = the wall).
                    Some(Mech::Lever) => {
                        let r = Quat::from_rotation_x(if s.memory { 0.6 } else { -0.6 });
                        let pivot = Vec3::Y * 0.4;
                        *t = Transform::from_translation(pivot - r * pivot).with_rotation(r);
                    }
                    Some(Mech::Button) => t.translation = Vec3::Y * if s.out { 0.06 } else { 0.0 },
                    Some(Mech::Timer) => {
                        let k = s.t / p.get(b.id, "interval").max(0.1);
                        t.rotation = Quat::from_rotation_z(-k * std::f32::consts::TAU);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Logic: after the control panels, before the machines (signals are set).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogicSystems;

/// Logic runs after the control panels, before the machines.
pub struct LogicPlugin;

impl Plugin for LogicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WireColor>()
            .init_resource::<Radio>()
            .init_resource::<crate::save::SaveHooks>()
            .add_systems(Update, (add_state, add_colour.after(crate::interact::InteractSet), use_switches, run).chain().in_set(LogicSystems).after(crate::signal::SignalSet).before(MechSet))
            .add_systems(Update, (pick_color, add_lamps, wire_looks, animate));
        app.world_mut().resource_mut::<crate::save::SaveHooks>().add("bus", hook_save, hook_load);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buses_carry_colours_apart() {
        // x=1 red, x=2..4 red + blue, x=5..6 blue. Red ends at 1 and 4,
        // blue at 2 and 6. A lever next to x=1.
        let (red, blue) = (1u8, 2u8);
        let mut b = HashMap::new();
        b.insert(IVec3::new(1, 0, 0), red);
        for x in 2..=4 {
            b.insert(IVec3::new(x, 0, 0), red | blue);
        }
        b.insert(IVec3::new(5, 0, 0), blue);
        b.insert(IVec3::new(6, 0, 0), blue);
        assert!(is_end(&b, IVec3::new(1, 0, 0), 0) && is_end(&b, IVec3::new(4, 0, 0), 0));
        assert!(!is_end(&b, IVec3::new(4, 0, 0), 1), "blue goes on through x=4");
        let (on, blocks) = spread(&b, &[(IVec3::ZERO, None)]);
        assert!(on.values().all(|m| *m == red) && on.len() == 4, "only red is fed: {on:?}");
        // Red powers what is next to its ends, not what it passes by.
        assert!(blocks.contains_key(&IVec3::new(4, 1, 0)) && blocks.contains_key(&IVec3::new(1, 1, 0)));
        assert!(!blocks.contains_key(&IVec3::new(3, 1, 0)), "the middle of the line touches nothing");
        // Blue fed at its far end: on from 6 back to 2, powers next to x=2.
        let (on2, blocks2) = spread(&b, &[(IVec3::new(7, 0, 0), None)]);
        assert!(on2.values().all(|m| *m == blue) && on2.len() == 5, "{on2:?}");
        assert!(blocks2.contains_key(&IVec3::new(2, 1, 0)));
        assert!(!blocks2.contains_key(&IVec3::new(4, 1, 0)));
        // One-way output: a gate at x=10 facing +X powers only x=11, from −X.
        let (_, g) = spread(&HashMap::new(), &[(IVec3::new(10, 0, 0), Some(IVec3::X))]);
        assert_eq!(g.keys().copied().collect::<Vec<_>>(), vec![IVec3::new(11, 0, 0)]);
        assert_eq!(g[&IVec3::new(11, 0, 0)], 1 << 1);
    }

    #[test]
    fn gates() {
        assert_eq!(Gate::And.eval(true, true, false, false).0, true);
        assert_eq!(Gate::And.eval(true, false, false, false).0, false);
        assert_eq!(Gate::Or.eval(false, false, true, false).0, true);
        assert_eq!(Gate::Not.eval(false, false, false, false).0, true);
        assert_eq!(Gate::Not.eval(false, true, false, false).0, false);
        // RS: set, hold, reset, hold.
        let (o, m) = Gate::Rs.eval(true, false, false, false);
        assert!(o && m);
        assert!(Gate::Rs.eval(false, false, false, m).0);
        let (o, m) = Gate::Rs.eval(false, true, false, m);
        assert!(!o && !m);
        assert!(!Gate::Rs.eval(false, false, false, m).0);
    }

    #[test]
    fn flasher_blinks_while_held() {
        assert!(flasher_lit(true, 0.1), "a pulse lights it");
        assert!(!flasher_lit(false, 0.1));
        let lit: Vec<bool> = (0..8).map(|i| flasher_lit(true, BLINK + i as f32 * 0.15)).collect();
        assert!(lit.contains(&true) && lit.contains(&false), "held on: blinks");
    }
}
