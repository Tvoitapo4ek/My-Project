//! Logic and automation: wires, buttons, levers, timers, pulse generators,
//! logic gates and flasher lights. On / off signals, no decay.
//!
//! * **Neighbours, like redstone**: a working source powers the blocks on
//!   all six sides (gates and pulse generators only the block in front of
//!   them). A powered block that takes signals (clutch, gearshift, speed
//!   controller, suspension, headlight, bearing, flasher…) gets signal 1.
//! * **Wires** lie on any face of a block (several on one block, on
//!   different faces). Wires of the same colour in neighbouring cells join
//!   into one line, also around a block's edge; different colours never
//!   join, so lines can run side by side. A line is on when any of its
//!   wires touches a working source; then it powers every block its wires
//!   touch. Colour: Shift + wheel with a wire in hand (for new wires), LMB
//!   with the wrench on a laid wire (next colour).
//! * **Gates** (wrench: И / ИЛИ / НЕ / RS-триггер) read their left, right
//!   and back sides and output at the front, one frame later (so loops do
//!   not hang). RS: left sets, right resets.
//! * **Timer** gives a short pulse every «Интервал» seconds; the **pulse
//!   generator** turns the front of a signal into a short pulse; the
//!   **flasher** lights up on a pulse and blinks while held on.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::{
    content::{BlockId, Mech},
    core::toast,
    grid::{DIRS, VoxelGrid},
    interact::UseBlock,
    inventory::Inventory,
    mech::{BlockEntity, MechSet, Params, Signal, side_axis},
    models,
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

/// A wire: colour and the direction of the block it lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wire {
    pub color: u8,
    pub support: IVec3,
}

/// Two wires join: same colour, and face neighbours or around the edge of
/// the block they both lie on.
pub fn wires_join(a: IVec3, wa: Wire, b: IVec3, wb: Wire) -> bool {
    if wa.color != wb.color {
        return false;
    }
    let d = (b - a).abs();
    match d.x + d.y + d.z {
        1 => true,
        2 => d.max_element() == 1 && a + wa.support == b + wb.support,
        _ => false,
    }
}

/// Cells a working source powers: `front` for one-way outputs.
pub fn targets(cell: IVec3, front: Option<IVec3>) -> Vec<IVec3> {
    match front {
        Some(f) => vec![cell + f],
        None => DIRS.iter().map(|d| cell + *d).collect(),
    }
}

/// Spreads power. Returns the powered wire cells and, for every powered
/// block cell, the sides it is powered from (bits of `DIRS`).
pub fn spread(wires: &HashMap<IVec3, Wire>, sources: &[(IVec3, Option<IVec3>)]) -> (HashSet<IVec3>, HashMap<IVec3, u8>) {
    let mut on_wires = HashSet::new();
    let mut blocks: HashMap<IVec3, u8> = HashMap::new();
    let hit = |blocks: &mut HashMap<IVec3, u8>, target: IVec3, from: IVec3| {
        let side = DIRS.iter().position(|d| *d == from - target).unwrap_or(0);
        *blocks.entry(target).or_default() |= 1 << side;
    };
    let mut q = VecDeque::new();
    for &(c, front) in sources {
        for t in targets(c, front) {
            if wires.contains_key(&t) {
                if on_wires.insert(t) {
                    q.push_back(t);
                }
            } else {
                hit(&mut blocks, t, c);
            }
        }
    }
    let near: Vec<IVec3> = {
        let mut v: Vec<IVec3> = DIRS.to_vec();
        for a in DIRS {
            for b in DIRS {
                if a.dot(b) == 0 && (a + b) != IVec3::ZERO && !v.contains(&(a + b)) {
                    v.push(a + b);
                }
            }
        }
        v
    };
    while let Some(c) = q.pop_front() {
        let w = wires[&c];
        for d in &near {
            let o = c + *d;
            if let Some(wo) = wires.get(&o)
                && !on_wires.contains(&o)
                && wires_join(c, w, o, *wo)
            {
                on_wires.insert(o);
                q.push_back(o);
            }
        }
    }
    for &c in &on_wires {
        for d in DIRS {
            let t = c + d;
            if !wires.contains_key(&t) {
                hit(&mut blocks, t, c);
            }
        }
    }
    (on_wires, blocks)
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

fn is_logic(m: Option<Mech>) -> bool {
    matches!(m, Some(Mech::Wire | Mech::Button | Mech::Lever | Mech::Timer | Mech::PulseGenerator | Mech::LogicGate | Mech::Flasher))
}

fn add_state(mut commands: Commands, color: Res<WireColor>, mut added: Query<(Entity, &BlockEntity, &mut Params), (Added<BlockEntity>, Without<LogicState>)>) {
    for (e, b, mut p) in &mut added {
        if !is_logic(b.mech()) {
            continue;
        }
        commands.entity(e).insert(LogicState::default());
        if b.mech() == Some(Mech::Wire)
            && let Some((i, _)) = b.id.def().param("color")
            && p.0[i] == 0.0
        {
            p.0[i] = color.0 as f32;
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
fn run(time: Res<Time>, grids: Query<&VoxelGrid>, mut blocks: Query<(&BlockEntity, &Params, &mut LogicState)>, mut signals: Query<&mut Signal, Without<LogicState>>) {
    let dt = time.delta_secs();
    for grid in &grids {
        if grid.entities.is_empty() {
            continue;
        }
        // 1. Outputs of this frame (from last frame's inputs).
        let mut wires = HashMap::new();
        let mut sources = Vec::new();
        for (&c, &e) in &grid.entities {
            let Ok((b, p, mut s)) = blocks.get_mut(e) else { continue };
            let front = Some(b.dir());
            let out = match b.mech() {
                Some(Mech::Wire) => {
                    wires.insert(c, Wire { color: p.get(b.id, "color") as u8, support: b.dir() });
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
                _ => continue,
            };
            s.out = out.0;
            if out.0 {
                sources.push((c, out.1));
            }
        }
        // 2. Power.
        let (on_wires, powered) = spread(&wires, &sources);
        // 3. Inputs → next outputs; signals to the blocks that take them.
        for (&c, &e) in &grid.entities {
            let from = powered.get(&c).copied().unwrap_or(0);
            if let Ok((b, p, mut s)) = blocks.get_mut(e) {
                let side = |d: IVec3| DIRS.iter().position(|x| *x == d).is_some_and(|i| from & (1 << i) != 0);
                match b.mech() {
                    Some(Mech::Wire) => s.input = on_wires.contains(&c),
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

/// Model of a wire for its connections (block-local arm directions).
pub fn wire_parts(arms: &[Vec3], color: usize, on: bool) -> Vec<Part> {
    let mat = if on { WIRE_MATS_ON[color % 8] } else { WIRE_MATS[color % 8] };
    // Local +Y points into the block the wire lies on.
    let mut v = vec![part([0.18, 0.04, 0.18], mat, [0.0, 0.47, 0.0])];
    let arms: Vec<Vec3> = if arms.is_empty() { vec![Vec3::X, Vec3::NEG_X] } else { arms.to_vec() };
    for a in arms {
        let r = Quat::from_rotation_arc(Vec3::Z, a);
        v.push(part([0.1, 0.04, 0.5], mat, (a * 0.25 + Vec3::Y * 0.47).to_array()).rot(r));
    }
    v
}

/// What a wire's model was built for.
#[derive(Component, Debug, Clone, PartialEq)]
struct WireLook(Vec<IVec3>, usize, bool);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn wire_looks(
    mut commands: Commands,
    grids: Query<&VoxelGrid>,
    wires: Query<(Entity, &BlockEntity, &Params, &LogicState, Option<&WireLook>, Option<&Children>)>,
    nodes: Query<(), With<LayerNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    let wire_id = BlockId::k("wire");
    for (e, b, p, s, look, children) in &wires {
        if b.id != wire_id {
            continue;
        }
        let Ok(grid) = grids.get(b.grid) else { continue };
        let color = p.get(b.id, "color") as usize;
        let me = Wire { color: color as u8, support: b.dir() };
        // Arms towards joined wires and the blocks it powers on its plane.
        let rot = models::block_rot(b.id.def().placement, b.facing).inverse();
        let mut arms: Vec<IVec3> = Vec::new();
        for (c, cell) in neighbours(grid, b.cell) {
            let d = c - b.cell;
            let dir = if cell.id == wire_id {
                let Some(o) = grid.entities.get(&c).and_then(|o| wires.get(*o).ok()) else { continue };
                let w = Wire { color: o.2.get(o.1.id, "color") as u8, support: o.1.dir() };
                if !wires_join(b.cell, me, c, w) {
                    continue;
                }
                // Up a wall: run towards the wall the other wire lies on.
                if d.abs().element_sum() == 1 && d.dot(me.support) != 0 { w.support } else { d - me.support * d.dot(me.support) }
            } else if d.abs().element_sum() == 1 && d.dot(me.support) == 0 && is_logic(cell.id.def().mech) || (d.abs().element_sum() == 1 && d.dot(me.support) == 0 && cell.id.def().mech.is_some_and(takes_signal)) {
                d
            } else {
                continue;
            };
            let l = (rot * dir.as_vec3()).round().as_ivec3();
            let l = IVec3::new(l.x, 0, l.z);
            if l != IVec3::ZERO && !arms.contains(&l) {
                arms.push(l);
            }
        }
        arms.sort_by_key(|a| (a.x, a.z));
        let want = WireLook(arms.clone(), color, s.input);
        if look == Some(&want) {
            continue;
        }
        for c in children.into_iter().flatten() {
            if nodes.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        let arm_v: Vec<Vec3> = arms.iter().map(|a| a.as_vec3().normalize()).collect();
        spawn_parts(&mut commands, e, &wire_parts(&arm_v, color, s.input), &mut meshes, &mut cache, &mats, false);
        commands.entity(e).insert(want);
    }
}

/// Face and edge neighbours of a cell that hold a block.
fn neighbours(grid: &VoxelGrid, c: IVec3) -> Vec<(IVec3, crate::grid::Cell)> {
    let mut out = Vec::new();
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let d = IVec3::new(x, y, z);
                let n = d.abs().element_sum();
                if n == 0 || n == 3 {
                    continue;
                }
                let cell = grid.get(c + d);
                if !cell.is_air() {
                    out.push((c + d, cell));
                }
            }
        }
    }
    out
}

/// Blocks that react to a signal.
pub fn takes_signal(m: Mech) -> bool {
    matches!(m, Mech::Clutch | Mech::Gearshift | Mech::SpeedController | Mech::Suspension | Mech::Headlight | Mech::Bearing | Mech::Flasher | Mech::LogicGate | Mech::PulseGenerator)
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
            _ => continue,
        };
        commands.spawn((Lamp { on, off: "seg_off", lit: None }, Mesh3d(cache.get(&mut meshes, size)), MeshMaterial3d(mats.get("seg_off")), Transform::from_translation(at), bevy::light::NotShadowCaster, ChildOf(e)));
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

/// Logic runs after the control panels, before the machines.
pub struct LogicPlugin;

impl Plugin for LogicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WireColor>()
            .add_systems(Update, (add_state, use_switches, run).chain().after(crate::signal::SignalSet).before(MechSet))
            .add_systems(Update, (pick_color, add_lamps, wire_looks, animate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(color: u8, support: IVec3) -> Wire {
        Wire { color, support }
    }

    #[test]
    fn wires_join_by_colour_and_around_edges() {
        let floor = IVec3::NEG_Y;
        assert!(wires_join(IVec3::ZERO, wire(0, floor), IVec3::X, wire(0, floor)));
        assert!(!wires_join(IVec3::ZERO, wire(0, floor), IVec3::X, wire(1, floor)), "colours never join");
        // Over the edge of one block: on its top and on its side.
        let top = IVec3::new(0, 1, 0);
        let side = IVec3::new(1, 0, 0);
        assert!(wires_join(top, wire(2, IVec3::NEG_Y), side, wire(2, IVec3::NEG_X)));
        // Diagonal on the floor (no shared block): no.
        assert!(!wires_join(IVec3::ZERO, wire(2, floor), IVec3::new(1, 0, 1), wire(2, floor)));
    }

    #[test]
    fn power_runs_along_one_colour() {
        // Lever at x=0; red line x=1..4 on the floor; a blue wire at x=5.
        let mut w = HashMap::new();
        for x in 1..=4 {
            w.insert(IVec3::new(x, 0, 0), wire(0, IVec3::NEG_Y));
        }
        w.insert(IVec3::new(5, 0, 0), wire(1, IVec3::NEG_Y));
        let (on, blocks) = spread(&w, &[(IVec3::ZERO, None)]);
        assert_eq!(on.len(), 4);
        assert!(!on.contains(&IVec3::new(5, 0, 0)), "blue stays off");
        // The red line powers the blocks it touches (the floor under it, the
        // cell above its end, …) but not through the blue wire.
        assert!(blocks.contains_key(&IVec3::new(4, -1, 0)));
        assert!(blocks.contains_key(&IVec3::new(4, 1, 0)));
        // One-way output: a gate at x=10 facing +X powers only x=11.
        let (_, b) = spread(&HashMap::new(), &[(IVec3::new(10, 0, 0), Some(IVec3::X))]);
        assert_eq!(b.keys().copied().collect::<Vec<_>>(), vec![IVec3::new(11, 0, 0)]);
        // …and the side it came from: −X.
        assert_eq!(b[&IVec3::new(11, 0, 0)], 1 << 1);
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
