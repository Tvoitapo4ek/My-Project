//! Fluids: tanks, pipes, pumps, the filling port and the diesel bucket.
//!
//! * Unit: millibucket (mB); a bucket is 1000 mB, a tank block holds
//!   16 000 mB. Fluids: water and diesel.
//! * **Tanks** next to each other join into one tank when they form a
//!   line 1×N or a column of 1×1, 2×2 or 3×3 blocks of any height; the
//!   fluid settles to the bottom and its level shows through the glass.
//! * **Pipes** (and cased pipes, tanks, ports, engines) connected face to
//!   face form a pipe network. Inside a network the port pours into tanks
//!   and engines draw fuel by themselves; moving fluid from one network to
//!   another needs a **pump** (flow = |RPM| × 8 mB/s, direction with the
//!   wrench). A pump with an end open to the water outside fills / empties
//!   ballast tanks.
//! * **Port** — RMB with the diesel bucket pours 1000 mB in (creative:
//!   the bucket never runs dry).
//!
//! Simulation runs per grid, so it works the same in the world and inside
//! physical contraptions (a submarine's ballast, a car's fuel tank).

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    content::{Mech, Tool},
    core::toast,
    grid::{DIRS, VoxelGrid},
    interact::{Clicks, Target},
    inventory::Inventory,
    mech::{BlockEntity, MechSet, Params, Spin},
    models,
    style::{BoxCache, LayerNode, Materials, Part, part, spawn_parts},
};

pub const BUCKET: f32 = 1000.0;
pub const TANK: f32 = 16_000.0;
pub const PORT: f32 = 1000.0;
pub const ENGINE: f32 = 1000.0;
/// Pump flow per RPM (mB/s).
pub const PUMP_RATE: f32 = 8.0;
/// A port empties into the tanks of its network this fast (mB/s).
const PORT_RATE: f32 = 2000.0;
/// Engines refill their own small tank this fast (mB/s).
const DRAW_RATE: f32 = 250.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fluid {
    Water,
    Diesel,
}

impl Fluid {
    pub fn name(self) -> &'static str {
        match self {
            Fluid::Water => "вода",
            Fluid::Diesel => "дизель",
        }
    }
    /// kg per mB: a 16 000 mB tank block of water weighs as much as the
    /// water its 1 m³ displaces (1000 kg).
    pub fn kg_per_mb(self) -> f32 {
        match self {
            Fluid::Water => 1.0 / 16.0,
            Fluid::Diesel => 0.85 / 16.0,
        }
    }
    pub fn material(self) -> &'static str {
        match self {
            Fluid::Water => "water",
            Fluid::Diesel => "diesel",
        }
    }
}

/// Fluid held by a tank block, a port or an engine.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct FluidStore {
    pub fluid: Option<Fluid>,
    pub amount: f32,
    pub cap: f32,
}

impl FluidStore {
    pub fn new(cap: f32) -> Self {
        Self { fluid: None, amount: 0.0, cap }
    }
    /// Room for `f` (0 if it holds another fluid).
    pub fn room(&self, f: Fluid) -> f32 {
        if self.fluid.is_none_or(|x| x == f) { (self.cap - self.amount).max(0.0) } else { 0.0 }
    }
    /// Adds up to `n`; returns what went in.
    pub fn add(&mut self, f: Fluid, n: f32) -> f32 {
        let n = n.min(self.room(f)).max(0.0);
        if n > 0.0 {
            self.fluid = Some(f);
            self.amount += n;
        }
        n
    }
    /// Takes up to `n` of `f`; returns what came out.
    pub fn take(&mut self, f: Fluid, n: f32) -> f32 {
        if self.fluid != Some(f) {
            return 0.0;
        }
        let n = n.min(self.amount).max(0.0);
        self.amount -= n;
        if self.amount <= 1e-3 {
            self.amount = 0.0;
            self.fluid = None;
        }
        n
    }
}

// ------------------------------------------------------------ network (pure)

/// How a block takes part in a pipe network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Pipe,
    Tank,
    Port,
    /// Takes fuel on every side but its output (`front`).
    Engine { front: IVec3 },
    /// Moves fluid from one end to the other (`d` = facing); a boundary
    /// between two networks.
    Pump { d: IVec3 },
    /// An open valve: a pipe along its axis only.
    Valve { axis: IVec3 },
}

/// Every mechanism that takes part in pipe networks.
pub const FLUID_MECHS: [Mech; 8] = [Mech::Pipe, Mech::CasedPipe, Mech::FluidTank, Mech::FluidPort, Mech::CreativeTank, Mech::DieselEngine, Mech::Pump, Mech::Valve];

pub fn role_of(b: &BlockEntity) -> Option<Role> {
    Some(match b.mech()? {
        Mech::Pipe | Mech::CasedPipe => Role::Pipe,
        Mech::FluidTank => Role::Tank,
        // An endless tank works like a port that never empties.
        Mech::FluidPort | Mech::CreativeTank => Role::Port,
        Mech::DieselEngine => Role::Engine { front: crate::grid::facing_dir(b.facing) },
        Mech::Pump => Role::Pump { d: b.dir() },
        // Shut: no part of any network.
        Mech::Valve if b.id.def().key.ends_with("_closed") => return None,
        Mech::Valve => Role::Valve { axis: b.dir() },
        _ => return None,
    })
}

/// Does fluid pass through this side (`dir` from the block's cell)?
pub fn opens(role: Role, dir: IVec3) -> bool {
    match role {
        Role::Pipe | Role::Tank | Role::Port => true,
        Role::Engine { front } => dir != front,
        Role::Pump { d } => dir == d || dir == -d,
        Role::Valve { axis } => dir == axis || dir == -axis,
    }
}

/// Pipe networks: component index of every non-pump node.
pub fn components(nodes: &HashMap<IVec3, Role>) -> HashMap<IVec3, usize> {
    let mut comp = HashMap::new();
    let mut n = 0;
    for (&start, &r) in nodes {
        if comp.contains_key(&start) || matches!(r, Role::Pump { .. }) {
            continue;
        }
        let mut q = VecDeque::from([start]);
        comp.insert(start, n);
        while let Some(c) = q.pop_front() {
            let rc = nodes[&c];
            for d in DIRS {
                let o = c + d;
                let Some(&ro) = nodes.get(&o) else { continue };
                if comp.contains_key(&o) || matches!(ro, Role::Pump { .. }) || !opens(rc, d) || !opens(ro, -d) {
                    continue;
                }
                comp.insert(o, n);
                q.push_back(o);
            }
        }
        n += 1;
    }
    comp
}

/// A set of tank cells that forms one tank: a full box that is a line
/// (1×1×N along any axis) or a column with a square 1×1, 2×2 or 3×3
/// footprint and any height.
pub fn joins(cells: &[IVec3]) -> bool {
    if cells.is_empty() {
        return false;
    }
    let lo = cells.iter().fold(cells[0], |a, c| a.min(*c));
    let hi = cells.iter().fold(cells[0], |a, c| a.max(*c));
    let s = hi - lo + IVec3::ONE;
    let full = (s.x * s.y * s.z) as usize == cells.len();
    let line = [s.x, s.y, s.z].iter().filter(|v| **v == 1).count() >= 2;
    let column = s.x == s.z && s.x <= 3;
    full && (line || column)
}

/// Tanks: connected tank cells that join, otherwise every cell alone.
pub fn tank_groups(tanks: &HashSet<IVec3>) -> Vec<Vec<IVec3>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut sorted: Vec<IVec3> = tanks.iter().copied().collect();
    sorted.sort_by_key(|c| (c.y, c.x, c.z));
    for start in sorted {
        if seen.contains(&start) {
            continue;
        }
        let mut group = vec![start];
        let mut q = VecDeque::from([start]);
        seen.insert(start);
        while let Some(c) = q.pop_front() {
            for d in DIRS {
                let o = c + d;
                if tanks.contains(&o) && seen.insert(o) {
                    group.push(o);
                    q.push_back(o);
                }
            }
        }
        group.sort_by_key(|c| (c.y, c.x, c.z));
        if joins(&group) {
            out.push(group);
        } else {
            out.extend(group.into_iter().map(|c| vec![c]));
        }
    }
    out
}

/// Spreads `total` over a tank's cells: the bottom layer fills first.
/// `cells` sorted by height; returns the amount per cell (same order).
pub fn settle(cells: &[IVec3], total: f32, cap: f32) -> Vec<f32> {
    let mut left = total.max(0.0);
    let mut out = vec![0.0; cells.len()];
    let mut i = 0;
    while i < cells.len() {
        let y = cells[i].y;
        let j = cells[i..].iter().position(|c| c.y != y).map(|k| i + k).unwrap_or(cells.len());
        let n = (j - i) as f32;
        let layer = left.min(cap * n);
        for v in &mut out[i..j] {
            *v = layer / n;
        }
        left -= layer;
        i = j;
    }
    out
}

// ------------------------------------------------------------ simulation

/// A tank (all its cells), a port or an engine during one step.
struct Unit {
    cells: Vec<Entity>,
    positions: Vec<IVec3>,
    store: FluidStore,
    kind: Role,
    ballast: bool,
}

#[allow(clippy::type_complexity)]
pub fn simulate(time: Res<Time>, water: Res<crate::water::Water>, grids: Query<(&VoxelGrid, &GlobalTransform)>, blocks: Query<(&BlockEntity, &Spin, &Params)>, mut stores: Query<&mut FluidStore>) {
    let _t = crate::profile::scope("жидкости");
    let dt = time.delta_secs();
    for (grid, gt) in &grids {
        let mut nodes: HashMap<IVec3, Role> = HashMap::new();
        let mut ent: HashMap<IVec3, Entity> = HashMap::new();
        for (c, e) in grid.entities_of(&FLUID_MECHS) {
            if let Ok((b, ..)) = blocks.get(e)
                && let Some(r) = role_of(b)
            {
                nodes.insert(c, r);
                ent.insert(c, e);
            }
        }
        if nodes.is_empty() {
            continue;
        }
        // Endless tanks refill themselves.
        for (c, e) in &ent {
            if let Ok((b, _, p)) = blocks.get(*e)
                && b.mech() == Some(Mech::CreativeTank)
                && let Ok(mut s) = stores.get_mut(*e)
            {
                let f = if p.get(b.id, "fluid") >= 0.5 { Fluid::Diesel } else { Fluid::Water };
                let full = FluidStore { fluid: Some(f), amount: TANK, cap: TANK };
                if *s != full {
                    *s = full;
                }
                let _ = c;
            }
        }
        let comp = components(&nodes);
        // Units: tanks (joined), ports, engines.
        let tanks: HashSet<IVec3> = nodes.iter().filter(|(_, r)| **r == Role::Tank).map(|(c, _)| *c).collect();
        let mut units: Vec<Unit> = Vec::new();
        for g in tank_groups(&tanks) {
            let es: Vec<Entity> = g.iter().map(|c| ent[c]).collect();
            let mut st = FluidStore::new(TANK * g.len() as f32);
            for e in &es {
                if let Ok(s) = stores.get(*e)
                    && let Some(f) = s.fluid
                {
                    st.fluid.get_or_insert(f);
                    if st.fluid == Some(f) {
                        st.amount += s.amount;
                    }
                }
            }
            let ballast = es.iter().any(|e| blocks.get(*e).is_ok_and(|(b, _, p)| p.get(b.id, "mode") >= 0.5));
            units.push(Unit { cells: es, positions: g, store: st, kind: Role::Tank, ballast });
        }
        for (c, r) in &nodes {
            if matches!(r, Role::Port | Role::Engine { .. })
                && let Ok(s) = stores.get(ent[c])
            {
                units.push(Unit { cells: vec![ent[c]], positions: vec![*c], store: *s, kind: *r, ballast: false });
            }
        }
        let comp_of = |u: &Unit| comp.get(&u.positions[0]).copied();
        // Ports pour into the tanks of their network.
        for i in 0..units.len() {
            if units[i].kind != Role::Port || units[i].store.amount <= 0.0 {
                continue;
            }
            let Some(f) = units[i].store.fluid else { continue };
            let c = comp_of(&units[i]);
            let mut budget = (PORT_RATE * dt).min(units[i].store.amount);
            for j in 0..units.len() {
                if units[j].kind == Role::Tank && comp_of(&units[j]) == c && budget > 0.0 {
                    let moved = units[j].store.add(f, budget);
                    units[i].store.take(f, moved);
                    budget -= moved;
                }
            }
        }
        // Engines draw diesel from tanks and ports of their network.
        for i in 0..units.len() {
            if !matches!(units[i].kind, Role::Engine { .. }) {
                continue;
            }
            let c = comp_of(&units[i]);
            let mut want = (DRAW_RATE * dt).min(units[i].store.room(Fluid::Diesel));
            for j in 0..units.len() {
                if want <= 0.0 || i == j || !matches!(units[j].kind, Role::Tank | Role::Port) || comp_of(&units[j]) != c {
                    continue;
                }
                let got = units[j].store.take(Fluid::Diesel, want);
                units[i].store.add(Fluid::Diesel, got);
                want -= got;
            }
        }
        // Pumps move fluid between networks (or to / from open water).
        for (&c, &r) in &nodes {
            let Role::Pump { d } = r else { continue };
            let Ok((b, spin, p)) = blocks.get(ent[&c]) else { continue };
            let back = p.get(b.id, "direction") >= 0.5;
            let (from, to) = if back { (c + d, c - d) } else { (c - d, c + d) };
            let mut budget = spin.rpm.abs() * PUMP_RATE * dt;
            if budget <= 0.0 {
                continue;
            }
            let open = |cell: IVec3| !nodes.contains_key(&cell) && grid.get(cell).is_air() && water.depth(gt.transform_point(cell.as_vec3() + Vec3::splat(0.5))) > 0.0;
            let net = |cell: IVec3, dir: IVec3| nodes.get(&cell).filter(|ro| !matches!(ro, Role::Pump { .. }) && opens(**ro, dir)).and_then(|_| comp.get(&cell).copied());
            let src = net(from, if back { -d } else { d });
            let dst = net(to, if back { d } else { -d });
            match (src, dst) {
                (Some(a), Some(bn)) if a != bn => {
                    for i in 0..units.len() {
                        if budget <= 0.0 || comp_of(&units[i]) != Some(a) || !matches!(units[i].kind, Role::Tank | Role::Port) {
                            continue;
                        }
                        let Some(f) = units[i].store.fluid else { continue };
                        for j in 0..units.len() {
                            if budget <= 0.0 || comp_of(&units[j]) != Some(bn) || !matches!(units[j].kind, Role::Tank | Role::Engine { .. }) {
                                continue;
                            }
                            let n = units[i].store.amount.min(budget).min(units[j].store.room(f));
                            let moved = units[i].store.take(f, n);
                            units[j].store.add(f, moved);
                            budget -= moved;
                        }
                    }
                }
                (None, Some(bn)) if open(from) => {
                    // Sea water into ballast tanks.
                    for u in units.iter_mut().filter(|u| u.ballast && comp.get(&u.positions[0]) == Some(&bn)) {
                        budget -= u.store.add(Fluid::Water, budget);
                    }
                }
                (Some(a), None) if open(to) => {
                    for u in units.iter_mut().filter(|u| u.kind == Role::Tank && comp.get(&u.positions[0]) == Some(&a)) {
                        budget -= u.store.take(Fluid::Water, budget);
                    }
                }
                _ => {}
            }
        }
        // Write back (tanks settle to the bottom).
        for u in &units {
            if u.kind == Role::Tank {
                let amounts = settle(&u.positions, u.store.amount, TANK);
                for (e, a) in u.cells.iter().zip(amounts) {
                    if let Ok(mut s) = stores.get_mut(*e) {
                        let new = FluidStore { fluid: if a > 0.0 { u.store.fluid } else { None }, amount: a, cap: TANK };
                        if *s != new {
                            *s = new;
                        }
                    }
                }
            } else if let Ok(mut s) = stores.get_mut(u.cells[0])
                && *s != u.store
            {
                *s = u.store;
            }
        }
    }
}

fn add_stores(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<FluidStore>)>) {
    for (e, b) in &added {
        let cap = match b.mech() {
            Some(Mech::FluidTank) => TANK,
            Some(Mech::FluidPort) => PORT,
            Some(Mech::CreativeTank) => TANK,
            Some(Mech::DieselEngine) => ENGINE,
            _ => continue,
        };
        commands.entity(e).insert(FluidStore::new(cap));
    }
}

// ------------------------------------------------------------ bucket

fn use_bucket(clicks: Res<Clicks>, target: Res<Target>, players: Query<&Inventory, With<crate::player::Player>>, grids: Query<&VoxelGrid>, blocks: Query<&BlockEntity>, mut stores: Query<&mut FluidStore>, mut msgs: MessageWriter<crate::core::Toast>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    let Ok(inv) = players.single() else { return };
    let fluid = match crate::tools::held_tool(inv) {
        Some(Tool::DieselBucket) => Fluid::Diesel,
        Some(Tool::WaterBucket) => Fluid::Water,
        _ => return,
    };
    if !clicks.right {
        return;
    }
    let Some(a) = target.hit else { return };
    let Some(&e) = grids.get(a.grid).ok().and_then(|g| g.entities.get(&a.cell)) else { return };
    match blocks.get(e).ok().and_then(|b| b.mech()) {
        Some(Mech::FluidPort) => {
            if let Ok(mut s) = stores.get_mut(e) {
                let put = s.add(fluid, BUCKET);
                if put > 0.0 {
                    msgs.write(toast(format!("Залито {put:.0} mB, в порту {:.0} mB ({})", s.amount, fluid.name())));
                    sounds.write(crate::sound::sfx("glue"));
                } else if s.room(fluid) == 0.0 && s.fluid.is_some_and(|f| f != fluid) {
                    msgs.write(toast(format!("В порту другая жидкость ({})", s.fluid.map(|f| f.name()).unwrap_or(""))));
                } else {
                    msgs.write(toast("Порт полон"));
                }
            }
        }
        Some(Mech::FluidTank) => {
            msgs.write(toast("Заливайте через заливной порт"));
        }
        _ => {}
    }
}

// ------------------------------------------------------------ looks

/// Connected sides of a pipe (bits of `DIRS`), to rebuild its model.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeMask(pub u8);

/// Parts of a pipe with arms to the connected sides (block-local dirs).
pub fn pipe_parts(local: &[IVec3], cased: bool) -> Vec<Part> {
    let mut v = Vec::new();
    let dirs: Vec<IVec3> = if local.is_empty() { vec![IVec3::Y, IVec3::NEG_Y] } else { local.to_vec() };
    if cased {
        v.push(part([0.9, 0.9, 0.9], "copper_casing", [0.0, 0.0, 0.0]));
    } else if dirs.len() != 2 || dirs[0] != -dirs[1] {
        v.push(part([0.44, 0.44, 0.44], "copper", [0.0, 0.0, 0.0]));
    }
    for d in dirs {
        let r = Quat::from_rotation_arc(Vec3::Y, d.as_vec3());
        let a = d.as_vec3();
        if !cased {
            v.push(part([0.32, 0.5, 0.32], "copper", (a * 0.25).to_array()).rot(r));
        }
        v.push(part([0.42, 0.08, 0.42], "brass", (a * 0.46).to_array()).rot(r));
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn update_pipes(
    mut commands: Commands,
    grids: Query<&VoxelGrid>,
    pipes: Query<(Entity, &BlockEntity, Option<&PipeMask>, Option<&Children>)>,
    all: Query<&BlockEntity>,
    nodes: Query<(), With<LayerNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (e, b, mask, children) in &pipes {
        let cased = match b.mech() {
            Some(Mech::Pipe) => false,
            Some(Mech::CasedPipe) => true,
            _ => continue,
        };
        let Ok(grid) = grids.get(b.grid) else { continue };
        let mut m = 0u8;
        for (i, d) in DIRS.iter().enumerate() {
            if let Some(o) = grid.entities.get(&(b.cell + *d)).and_then(|o| all.get(*o).ok())
                && let Some(r) = role_of(o)
                && opens(r, -*d)
            {
                m |= 1 << i;
            }
        }
        if mask.is_some_and(|k| k.0 == m) {
            continue;
        }
        let rot = models::block_rot(b.id.def().placement, b.facing).inverse();
        let local: Vec<IVec3> = DIRS.iter().enumerate().filter(|(i, _)| m & (1 << i) != 0).map(|(_, d)| (rot * d.as_vec3()).round().as_ivec3()).collect();
        for c in children.into_iter().flatten() {
            if nodes.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        spawn_parts(&mut commands, e, &pipe_parts(&local, cased), &mut meshes, &mut cache, &mats, true);
        commands.entity(e).insert(PipeMask(m));
    }
}

/// The fluid inside a tank block (scaled with the level).
#[derive(Component)]
struct Level;

#[allow(clippy::type_complexity)]
fn show_levels(
    mut commands: Commands,
    tanks: Query<(Entity, &BlockEntity, &FluidStore, Option<&Children>), Changed<FluidStore>>,
    mut levels: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility), With<Level>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (e, b, s, children) in &tanks {
        if b.mech() != Some(Mech::FluidTank) {
            continue;
        }
        let frac = (s.amount / s.cap.max(1.0)).clamp(0.0, 1.0);
        let h = 0.86 * frac;
        let t = Transform::from_xyz(0.0, -0.43 + h * 0.5, 0.0).with_scale(Vec3::new(1.0, h.max(1e-3), 1.0));
        let mat = mats.get(s.fluid.unwrap_or(Fluid::Water).material());
        let vis = if frac > 0.002 { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(l) = children.and_then(|c| c.iter().find(|c| levels.contains(*c)))
            && let Ok((mut tr, mut m, mut v)) = levels.get_mut(l)
        {
            *tr = t;
            m.0 = mat;
            *v = vis;
        } else {
            commands.spawn((Level, Mesh3d(cache.get(&mut meshes, Vec3::new(0.84, 1.0, 0.84))), MeshMaterial3d(mat), t, vis, ChildOf(e)));
        }
    }
}

// ------------------------------------------------------------ valves

/// The signal a valve saw last frame (its edges open / shut it).
#[derive(Component, Debug, Clone, Copy)]
pub struct ValveSignal(pub bool);

/// Open ⇄ shut: the block itself changes (like doors), facing kept.
pub fn flip_valve(grid: &mut VoxelGrid, c: IVec3) {
    let cell = grid.get(c);
    let key = &cell.id.def().key;
    let other = if key.ends_with("_closed") { "fluid_valve" } else { "fluid_valve_closed" };
    grid.set(c, crate::grid::Cell::new(crate::content::BlockId::k(other), cell.facing));
}

fn valves(mut commands: Commands, mut used: MessageReader<crate::interact::UseBlock>, mut grids: Query<&mut VoxelGrid>, blocks: Query<(Entity, &BlockEntity, &crate::mech::Signal, Option<&ValveSignal>)>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    for u in used.read() {
        if u.mech == Mech::Valve
            && let Ok(mut g) = grids.get_mut(u.grid)
        {
            flip_valve(&mut g, u.cell);
            sounds.write(crate::sound::sfx("toggle"));
        }
    }
    for (e, b, sig, last) in &blocks {
        if b.mech() != Some(Mech::Valve) {
            continue;
        }
        let on = sig.0 != 0.0;
        let Some(last) = last else {
            commands.entity(e).insert(ValveSignal(on));
            continue;
        };
        if on != last.0 {
            commands.entity(e).insert(ValveSignal(on));
            let shut = b.id.def().key.ends_with("_closed");
            // Signal on → shut; signal gone → open again.
            if on != shut
                && let Ok(mut g) = grids.get_mut(b.grid)
            {
                flip_valve(&mut g, b.cell);
            }
        }
    }
}

pub struct FluidsPlugin;

impl Plugin for FluidsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_stores, simulate, show_levels).chain().after(MechSet))
            .add_systems(Update, (use_bucket.after(crate::interact::InteractSet), update_pipes, valves.after(crate::interact::InteractSet).after(crate::logic::LogicSystems).before(MechSet)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tanks_join_into_lines_and_columns() {
        let set = |v: &[IVec3]| v.iter().copied().collect::<HashSet<_>>();
        // 1×3 line along X: one tank of 48 000 mB.
        let line = [IVec3::ZERO, IVec3::X, IVec3::X * 2];
        let g = tank_groups(&set(&line));
        assert_eq!(g.len(), 1);
        assert_eq!(TANK * g[0].len() as f32, 48_000.0);
        // 2×2×3 column: one tank.
        let col: Vec<IVec3> = (0..2).flat_map(|x| (0..3).flat_map(move |y| (0..2).map(move |z| IVec3::new(x, y, z)))).collect();
        assert_eq!(tank_groups(&set(&col)).len(), 1);
        // 3×3×2 column joins too; a 4×4 slab does not (16 separate tanks).
        let c3: Vec<IVec3> = (0..3).flat_map(|x| (0..2).flat_map(move |y| (0..3).map(move |z| IVec3::new(x, y, z)))).collect();
        assert_eq!(tank_groups(&set(&c3)).len(), 1);
        let slab: Vec<IVec3> = (0..4).flat_map(|x| (0..4).map(move |z| IVec3::new(x, 0, z))).collect();
        assert_eq!(tank_groups(&set(&slab)).len(), 16);
        // An L shape is not a tank: each block alone.
        assert_eq!(tank_groups(&set(&[IVec3::ZERO, IVec3::X, IVec3::Y])).len(), 3);
    }

    #[test]
    fn fluid_settles_to_the_bottom() {
        // A 1×1×3 column with 20 000 mB: bottom full, middle 4 000, top empty.
        let col = [IVec3::ZERO, IVec3::Y, IVec3::Y * 2];
        assert_eq!(settle(&col, 20_000.0, TANK), vec![16_000.0, 4_000.0, 0.0]);
        // A 2-wide layer shares it.
        let wide = [IVec3::ZERO, IVec3::X];
        assert_eq!(settle(&wide, 1000.0, TANK), vec![500.0, 500.0]);
    }

    #[test]
    fn stores_keep_one_fluid() {
        let mut s = FluidStore::new(PORT);
        assert_eq!(s.add(Fluid::Diesel, 1500.0), 1000.0, "capped");
        assert_eq!(s.add(Fluid::Water, 10.0), 0.0, "no mixing");
        assert_eq!(s.take(Fluid::Diesel, 400.0), 400.0);
        assert_eq!(s.take(Fluid::Diesel, 1e6), 600.0);
        assert_eq!(s.fluid, None, "empty → any fluid");
    }

    /// Motor → cogwheel → pump (solved by the kinetic network), then the
    /// real simulation moves water from one tank to the other.
    #[test]
    fn pump_driven_by_the_network_moves_fluid() {
        use crate::{
            content::BlockId,
            mech::network::{self, Axis, Node, Role as K},
        };
        use bevy::ecs::system::RunSystemOnce;
        // Kinetics: a source shaft on +Y into a cog (axis Y) meshing with the
        // pump (a cog with its axis along the pipe, X… here Y as well).
        let mut k = HashMap::new();
        k.insert(IVec3::new(1, -1, 0), Node::new(K::Shaft, Axis::of(IVec3::Y)).one_port(IVec3::Y).source(64.0));
        k.insert(IVec3::new(1, 0, 0), Node::new(K::Cog, Axis::of(IVec3::Y)));
        k.insert(IVec3::new(0, 0, 0), Node::new(K::Cog, Axis::of(IVec3::Y)));
        let rpm = network::solve(&k).speed[&IVec3::ZERO];
        assert_eq!(rpm.abs(), 64.0, "the cogwheel turns the pump");
        // Fluids: tank (0,-2) — pipe (0,-1) — pump (0,0) facing +Y — pipe (0,1) — tank (0,2).
        let mut w = World::new();
        let mut t = Time::<()>::default();
        t.advance_by(std::time::Duration::from_secs_f32(0.5));
        w.insert_resource(t);
        w.insert_resource(crate::water::Water::default());
        let g = w.spawn((VoxelGrid::default(), GlobalTransform::default())).id();
        let put = |w: &mut World, y: i32, key: &str, facing: u8, rpm: f32| {
            let id = BlockId::k(key);
            let cell = IVec3::new(0, y, 0);
            let e = w.spawn((BlockEntity { grid: g, cell, id, facing }, Spin { rpm, angle: 0.0 }, Params::defaults(id))).id();
            let mut grid = w.get_mut::<VoxelGrid>(g).unwrap();
            grid.set(cell, crate::grid::Cell::new(id, facing));
            grid.entities.insert(cell, e);
            e
        };
        let a = put(&mut w, -2, "fluid_tank", 2, 0.0);
        put(&mut w, -1, "fluid_pipe", 2, 0.0);
        put(&mut w, 0, "mechanical_pump", 2, rpm);
        put(&mut w, 1, "fluid_pipe", 2, 0.0);
        let b = put(&mut w, 2, "fluid_tank", 2, 0.0);
        let mut sa = FluidStore::new(TANK);
        sa.add(Fluid::Water, 5000.0);
        w.entity_mut(a).insert(sa);
        w.entity_mut(b).insert(FluidStore::new(TANK));
        w.run_system_once(simulate).unwrap();
        // 64 RPM × 8 mB/s for 0.5 s = 256 mB.
        let (fa, fb) = (w.get::<FluidStore>(a).unwrap().amount, w.get::<FluidStore>(b).unwrap().amount);
        assert_eq!((fa, fb), (5000.0 - 256.0, 256.0));
        // Stopped pump: nothing moves.
        let pump = w.get::<VoxelGrid>(g).unwrap().entities[&IVec3::ZERO];
        w.get_mut::<Spin>(pump).unwrap().rpm = 0.0;
        w.run_system_once(simulate).unwrap();
        assert_eq!(w.get::<FluidStore>(b).unwrap().amount, 256.0);
    }

    #[test]
    fn pumps_split_networks() {
        // tank — pipe — pump(+X) — pipe — tank: two networks.
        let mut n = HashMap::new();
        n.insert(IVec3::new(0, 0, 0), Role::Tank);
        n.insert(IVec3::new(1, 0, 0), Role::Pipe);
        n.insert(IVec3::new(2, 0, 0), Role::Pump { d: IVec3::X });
        n.insert(IVec3::new(3, 0, 0), Role::Pipe);
        n.insert(IVec3::new(4, 0, 0), Role::Tank);
        let c = components(&n);
        assert_eq!(c[&IVec3::ZERO], c[&IVec3::X]);
        assert_ne!(c[&IVec3::X], c[&IVec3::new(3, 0, 0)]);
        assert!(!c.contains_key(&IVec3::new(2, 0, 0)));
        // An engine does not take fuel through its output side.
        assert!(!opens(Role::Engine { front: IVec3::Z }, IVec3::Z));
        assert!(opens(Role::Engine { front: IVec3::Z }, IVec3::X));
    }
}
