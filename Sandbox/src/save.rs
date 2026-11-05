//! Saving and loading worlds.
//!
//! A world is `saves/<name>/world.ron` in your folder (`paths`): every grid
//! (the world, each physical machine, each moving part of a bearing or
//! piston) with its blocks, glue, chains and the state of its block
//! entities; the player (position, look, inventory) and the time of day.
//!
//! * Blocks are stored per chunk as runs over a palette of block keys, so a
//!   removed mod block is just skipped on load.
//! * The state of a block entity is written by **hooks**: every module with
//!   state registers one (`SaveHooks::add`) — a name, «write» and «read».
//!   Settings (`Params`) are saved by key for every block.
//! * Machines keep their pose and velocity; moving parts are hinged to
//!   their bearing / piston again.
//!
//! The format has a version; an older world is read with defaults for
//! anything new.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    content::{BlockId, registry},
    contraption::Contraption,
    grid::{CHUNK, Cell, VoxelGrid, WorldGrid},
    inventory::{Inventory, Stack},
    mech::{BlockEntity, Params},
    player::{Body, Player},
    sky::Clock,
    tools::glue::Glue,
};

pub const VERSION: u32 = 1;
pub const DIR: &str = "saves";

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorldSave {
    pub version: u32,
    pub name: String,
    /// What made the world (only the proving ground for now).
    #[serde(default)]
    pub generator: String,
    #[serde(default)]
    pub seed: u64,
    /// Seconds since 1970 of the last save.
    #[serde(default)]
    pub saved_at: u64,
    pub clock: (f32, f32, bool),
    pub player: PlayerSave,
    pub grids: Vec<GridSave>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PlayerSave {
    pub feet: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub flying: bool,
    pub selected: usize,
    /// Slot → (item key, count).
    pub slots: Vec<Option<(String, u32)>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum GridKind {
    World,
    /// A physical machine.
    Body { pos: Vec3, rot: Quat, vel: Vec3, ang: Vec3, #[serde(default)] frozen: bool },
    /// The moving part of a bearing / piston: index of the base grid, the
    /// mount's cell there, its value when the part was made.
    Mounted { base: usize, mount: IVec3, start: f32, pos: Vec3, rot: Quat },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GridSave {
    pub kind: GridKind,
    pub palette: Vec<String>,
    /// Chunk key → runs of (palette index + 1, 0 = air; facing; length).
    pub chunks: Vec<(IVec3, Vec<(u16, u8, u16)>)>,
    #[serde(default)]
    pub glue: Vec<(IVec3, IVec3)>,
    #[serde(default)]
    pub chains: Vec<crate::chains::ChainLink>,
    #[serde(default)]
    pub blocks: Vec<BlockSave>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct BlockSave {
    pub at: IVec3,
    #[serde(default)]
    pub params: Vec<(String, f32)>,
    /// Hook name → its data (RON).
    #[serde(default)]
    pub data: Vec<(String, String)>,
}

// ------------------------------------------------------------ hooks

/// Grid entity → its index in the save (to store references between grids).
pub struct SaveCtx {
    pub grid_index: HashMap<Entity, usize>,
}

/// Index in the save → the new grid entity.
pub struct LoadCtx {
    pub grids: Vec<Entity>,
}

pub type SaveFn = fn(&World, Entity, &SaveCtx) -> Option<String>;
pub type LoadFn = fn(&mut World, Entity, &str, &LoadCtx);

#[derive(Resource, Default, Clone)]
pub struct SaveHooks(pub Vec<(&'static str, SaveFn, LoadFn)>);

impl SaveHooks {
    pub fn add(&mut self, name: &'static str, save: SaveFn, load: LoadFn) {
        self.0.push((name, save, load));
    }
}

/// RON of a value (hooks use it for their data).
pub fn to_ron<T: Serialize>(v: &T) -> Option<String> {
    ron::to_string(v).ok()
}

pub fn from_ron<T: for<'a> Deserialize<'a>>(s: &str) -> Option<T> {
    ron::from_str(s).ok()
}

// ------------------------------------------------------------ grids

fn encode(grid: &VoxelGrid) -> (Vec<String>, Vec<(IVec3, Vec<(u16, u8, u16)>)>) {
    let mut palette: Vec<String> = Vec::new();
    let mut index: HashMap<BlockId, u16> = HashMap::new();
    let mut chunks = Vec::new();
    let mut keys: Vec<&IVec3> = grid.chunks.keys().collect();
    keys.sort_by_key(|k| (k.y, k.z, k.x));
    for k in keys {
        let ch = &grid.chunks[k];
        if ch.count == 0 {
            continue;
        }
        let mut runs: Vec<(u16, u8, u16)> = Vec::new();
        for cell in ch.cells.iter() {
            let p = if cell.is_air() {
                0
            } else {
                *index.entry(cell.id).or_insert_with(|| {
                    palette.push(cell.id.def().key.clone());
                    palette.len() as u16
                })
            };
            let f = if cell.is_air() { 0 } else { cell.facing };
            match runs.last_mut() {
                Some((lp, lf, n)) if *lp == p && *lf == f && *n < u16::MAX => *n += 1,
                _ => runs.push((p, f, 1)),
            }
        }
        chunks.push((*k, runs));
    }
    (palette, chunks)
}

/// Fills `grid` from saved chunks; unknown blocks are skipped.
pub fn decode(grid: &mut VoxelGrid, palette: &[String], chunks: &[(IVec3, Vec<(u16, u8, u16)>)]) -> usize {
    let ids: Vec<Option<BlockId>> = palette.iter().map(|k| registry().block(k)).collect();
    let mut missing = 0;
    for (k, runs) in chunks {
        let mut i: i32 = 0;
        for &(p, f, n) in runs {
            for _ in 0..n {
                if p > 0 {
                    match ids.get(p as usize - 1).copied().flatten() {
                        Some(id) => {
                            let l = IVec3::new(i % CHUNK, i / (CHUNK * CHUNK), (i / CHUNK) % CHUNK);
                            grid.set(*k * CHUNK + l, Cell::new(id, f));
                        }
                        None => missing += 1,
                    }
                }
                i += 1;
            }
        }
    }
    missing
}

fn grid_save(world: &World, e: Entity, kind: GridKind, ctx: &SaveCtx, hooks: &SaveHooks) -> Option<GridSave> {
    let grid = world.get::<VoxelGrid>(e)?;
    let (palette, chunks) = encode(grid);
    let mut blocks = Vec::new();
    let mut cells: Vec<(&IVec3, &Entity)> = grid.entities.iter().collect();
    cells.sort_by_key(|(c, _)| (c.y, c.z, c.x));
    for (c, be) in cells {
        let Some(b) = world.get::<BlockEntity>(*be) else { continue };
        let def = b.id.def();
        let params: Vec<(String, f32)> = world.get::<Params>(*be).map(|p| def.params.iter().zip(&p.0).map(|(d, v)| (d.key.clone(), *v)).collect()).unwrap_or_default();
        let data: Vec<(String, String)> = hooks.0.iter().filter_map(|(n, s, _)| s(world, *be, ctx).map(|d| (n.to_string(), d))).collect();
        if !params.is_empty() || !data.is_empty() {
            blocks.push(BlockSave { at: *c, params, data });
        }
    }
    Some(GridSave {
        kind,
        palette,
        chunks,
        glue: world.get::<Glue>(e).map(|g| g.0.clone()).unwrap_or_default(),
        chains: world.get::<crate::chains::Chains>(e).map(|c| c.0.clone()).unwrap_or_default(),
        blocks,
    })
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The whole world as a save (pure read).
pub fn snapshot(world: &mut World, name: &str) -> WorldSave {
    let world_e = world.resource::<crate::world::WorldEntity>().0;
    // Grids: the world first, then machines, then moving parts (they point
    // at their base by index).
    let mut order = vec![world_e];
    let mut q = world.query_filtered::<Entity, (With<VoxelGrid>, With<Contraption>, Without<WorldGrid>, Without<crate::ponder::PonderGrid>)>();
    let mut bodies: Vec<Entity> = q.iter(world).collect();
    bodies.sort_by_key(|e| world.get::<crate::mounts::MountedOn>(*e).is_some());
    order.extend(bodies);
    let ctx = SaveCtx { grid_index: order.iter().enumerate().map(|(i, e)| (*e, i)).collect() };
    let hooks = world.resource::<SaveHooks>().clone();
    let mut grids = Vec::new();
    for &e in &order {
        let t = world.get::<GlobalTransform>(e).map(|g| g.compute_transform()).unwrap_or_default();
        let kind = if e == world_e {
            GridKind::World
        } else if let Some(m) = world.get::<crate::mounts::MountedOn>(e) {
            let Some(b) = world.get::<BlockEntity>(m.mount).copied() else { continue };
            let Some(mount) = world.get::<crate::mounts::Mount>(m.mount) else { continue };
            let (Some(&base), Some(sub)) = (ctx.grid_index.get(&b.grid), mount.sub) else { continue };
            GridKind::Mounted { base, mount: b.cell, start: sub.start, pos: t.translation, rot: t.rotation }
        } else {
            GridKind::Body {
                pos: t.translation,
                rot: t.rotation,
                vel: world.get::<LinearVelocity>(e).map(|v| v.0).unwrap_or_default(),
                ang: world.get::<AngularVelocity>(e).map(|v| v.0).unwrap_or_default(),
                frozen: world.get::<crate::tools::guns::Frozen>(e).is_some(),
            }
        };
        if let Some(g) = grid_save(world, e, kind, &ctx, &hooks) {
            grids.push(g);
        }
    }
    let mut q = world.query_filtered::<(&Position, &Body, &Inventory), With<Player>>();
    let player = q
        .iter(world)
        .next()
        .map(|(p, b, inv)| PlayerSave {
            feet: p.0 - Vec3::Y * (b.height() * 0.5 + 0.02),
            yaw: b.yaw,
            pitch: b.pitch,
            flying: b.flying,
            selected: inv.selected,
            slots: inv.slots.iter().map(|s| s.map(|s| (s.item.def().key.clone(), s.count))).collect(),
        })
        .unwrap_or_default();
    let c = world.resource::<Clock>();
    WorldSave { version: VERSION, name: name.to_string(), generator: "proving_ground".into(), seed: 0, saved_at: now(), clock: (c.hours, c.speed, c.paused), player, grids }
}

// ------------------------------------------------------------ files

pub fn name_ok(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.chars().count() <= 32 && !n.contains(['/', '\\', ':', '.']) && !n.starts_with(' ')
}

pub fn path_of(name: &str) -> std::path::PathBuf {
    std::path::Path::new(DIR).join(name.trim()).join("world.ron")
}

pub fn write(save: &WorldSave) -> Result<std::path::PathBuf, String> {
    if !name_ok(&save.name) {
        return Err("имя: до 32 знаков, без / \\ : .".into());
    }
    let path = path_of(&save.name);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = ron::ser::to_string_pretty(save, ron::ser::PrettyConfig::default().compact_arrays(true)).map_err(|e| e.to_string())?;
    // Write next to it and swap: a crash mid-save keeps the old world.
    let tmp = path.with_extension("ron.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn read(name: &str) -> Result<WorldSave, String> {
    let text = std::fs::read_to_string(path_of(name)).map_err(|_| format!("нет мира «{name}»"))?;
    ron::from_str(&text).map_err(|e| format!("мир «{name}» повреждён: {e}"))
}

/// Saved worlds, newest first: (name, saved at).
pub fn list() -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = std::fs::read_dir(DIR)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().join("world.ron").exists())
                .filter_map(|e| {
                    let name = e.file_name().to_str()?.to_string();
                    let t = e.path().join("world.ron").metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                    Some((name, t))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

pub fn delete(name: &str) -> Result<(), String> {
    if !name_ok(name) {
        return Err("нет такого мира".into());
    }
    std::fs::remove_dir_all(std::path::Path::new(DIR).join(name.trim())).map_err(|e| e.to_string())
}

// ------------------------------------------------------------ world

/// The world being played (None — the menu's background polygon).
#[derive(Resource, Default)]
pub struct CurrentWorld {
    pub name: Option<String>,
    /// Seconds of play since the last save.
    pub since_save: f32,
}

/// Removes every grid (world, machines, moving parts, their joints) and
/// puts a fresh world grid in place.
fn reset(world: &mut World, grid: VoxelGrid) -> Entity {
    let mut q = world.query_filtered::<Entity, (With<VoxelGrid>, Without<crate::ponder::PonderGrid>)>();
    let grids: Vec<Entity> = q.iter(world).collect();
    for e in grids {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    let mut joints = world.query_filtered::<Entity, Or<(With<RevoluteJoint>, With<PrismaticJoint>)>>();
    let js: Vec<Entity> = joints.iter(world).collect();
    for j in js {
        world.entity_mut(j).despawn();
    }
    let e = world.spawn((Name::new("World grid"), WorldGrid, grid, Transform::default(), Visibility::default(), RigidBody::Static)).id();
    world.insert_resource(crate::world::WorldEntity(e));
    world.insert_resource(crate::logic::Radio::default());
    world.insert_resource(crate::tools::guns::Held::default());
    e
}

/// Starts a new world (the proving ground) under `name` and saves it.
pub fn new_world(world: &mut World, name: &str) -> Result<(), String> {
    if !name_ok(name) {
        return Err("имя: до 32 знаков, без / \\ : .".into());
    }
    if path_of(name).exists() {
        return Err(format!("мир «{}» уже есть", name.trim()));
    }
    reset(world, crate::world::generate());
    place_player(world, &PlayerSave { feet: crate::world::SPAWN, slots: Vec::new(), ..default() }, true);
    *world.resource_mut::<Clock>() = Clock::default();
    world.insert_resource(CurrentWorld { name: Some(name.trim().to_string()), since_save: 0.0 });
    let s = snapshot(world, name.trim());
    write(&s).map(|_| ())
}

fn place_player(world: &mut World, p: &PlayerSave, fresh: bool) {
    let mut q = world.query_filtered::<(&mut Position, &mut LinearVelocity, &mut Body, &mut Inventory, &mut Transform), With<Player>>();
    for (mut pos, mut vel, mut b, mut inv, mut t) in q.iter_mut(world) {
        pos.0 = p.feet + Vec3::Y * (b.height() * 0.5 + 0.02);
        t.translation = pos.0;
        vel.0 = Vec3::ZERO;
        b.yaw = p.yaw;
        b.pitch = p.pitch;
        b.flying = p.flying;
        b.safe = None;
        if fresh {
            *inv = crate::inventory::starting();
        } else {
            let mut slots: Vec<Option<Stack>> = p.slots.iter().map(|s| s.as_ref().and_then(|(k, n)| registry().item(k).map(|item| Stack { item, count: *n }))).collect();
            slots.resize(inv.slots.len(), None);
            inv.slots = slots;
            inv.selected = p.selected.min(crate::inventory::HOTBAR - 1);
        }
    }
    // Off any seat.
    let mut seated = world.query_filtered::<Entity, (With<Player>, With<crate::player::Seated>)>();
    let es: Vec<Entity> = seated.iter(world).collect();
    for e in es {
        world.entity_mut(e).remove::<crate::player::Seated>();
    }
}

/// State to apply once the loaded block entities exist.
#[derive(Resource, Default)]
struct Pending {
    blocks: Vec<(Entity, IVec3, BlockSave)>,
    mounts: Vec<(Entity, IVec3, Entity, f32)>,
    grids: Vec<Entity>,
    frames: u32,
}

/// Loads a saved world in place of the current one.
pub fn load_world(world: &mut World, name: &str) -> Result<(), String> {
    let save = read(name)?;
    let mut grids: Vec<Entity> = Vec::new();
    let mut pending = Pending::default();
    let mut missing = 0;
    for (i, g) in save.grids.iter().enumerate() {
        let mut vg = VoxelGrid::default();
        missing += decode(&mut vg, &g.palette, &g.chunks);
        let e = match &g.kind {
            GridKind::World => reset(world, vg),
            GridKind::Body { pos, rot, vel, ang, frozen } => {
                let cells: Vec<IVec3> = vg.cells().into_iter().map(|(c, _)| c).collect();
                let mass = crate::assembly::mass_of(&vg, &cells);
                let mut e = world.spawn((Name::new("Contraption"), vg, Transform::from_translation(*pos).with_rotation(*rot), Visibility::default(), Contraption { mass }, RigidBody::Dynamic, LinearVelocity(*vel), AngularVelocity(*ang), LinearDamping(0.05), AngularDamping(0.3)));
                if *frozen {
                    e.insert((crate::tools::guns::Frozen, RigidBody::Static));
                }
                e.id()
            }
            GridKind::Mounted { base, mount, start, pos, rot } => {
                let cells: Vec<IVec3> = vg.cells().into_iter().map(|(c, _)| c).collect();
                let mass = crate::assembly::mass_of(&vg, &cells);
                let e = world.spawn((Name::new("Moving part"), vg, Transform::from_translation(*pos).with_rotation(*rot), Visibility::default(), Contraption { mass }, RigidBody::Dynamic, LinearDamping(0.05), AngularDamping(0.3))).id();
                if let Some(b) = grids.get(*base) {
                    pending.mounts.push((*b, *mount, e, *start));
                }
                e
            }
        };
        if i == 0 && g.kind != GridKind::World {
            reset(world, VoxelGrid::default());
        }
        world.entity_mut(e).insert((Glue(g.glue.clone()), crate::chains::Chains(g.chains.clone())));
        for b in &g.blocks {
            pending.blocks.push((e, b.at, b.clone()));
        }
        grids.push(e);
    }
    if missing > 0 {
        warn!("load {name}: {missing} blocks of unknown kinds skipped");
    }
    pending.grids = grids;
    world.insert_resource(pending);
    place_player(world, &save.player, false);
    let (hours, speed, paused) = save.clock;
    *world.resource_mut::<Clock>() = Clock { hours, speed, paused };
    world.insert_resource(CurrentWorld { name: Some(save.name.clone()), since_save: 0.0 });
    Ok(())
}

/// Applies saved settings and states as the block entities appear.
fn apply_pending(world: &mut World) {
    let Some(mut p) = world.remove_resource::<Pending>() else { return };
    p.frames += 1;
    let hooks = world.resource::<SaveHooks>().clone();
    let ctx = LoadCtx { grids: p.grids.clone() };
    let mut left = Vec::new();
    for (g, at, b) in std::mem::take(&mut p.blocks) {
        let e = world.get::<VoxelGrid>(g).and_then(|grid| grid.entities.get(&at).copied());
        // Wait until the block entity and the components modules add to it exist.
        let Some(e) = e.filter(|_| p.frames >= 3) else {
            left.push((g, at, b));
            continue;
        };
        if let (Some(be), Some(mut params)) = (world.get::<BlockEntity>(e).copied(), world.get_mut::<Params>(e)) {
            for (k, v) in &b.params {
                if let Some((i, _)) = be.id.def().param(k)
                    && let Some(slot) = params.0.get_mut(i)
                {
                    *slot = *v;
                }
            }
        }
        for (name, data) in &b.data {
            if let Some((_, _, load)) = hooks.0.iter().find(|(n, ..)| n == name) {
                load(world, e, data, &ctx);
            }
        }
    }
    p.blocks = left;
    let mut mounts_left = Vec::new();
    for (base, cell, sub, start) in std::mem::take(&mut p.mounts) {
        let e = world.get::<VoxelGrid>(base).and_then(|grid| grid.entities.get(&cell).copied());
        match e.filter(|e| p.frames >= 3 && world.get::<crate::mounts::Mount>(*e).is_some()) {
            Some(e) => crate::mounts::reattach(world, e, sub, start),
            None => mounts_left.push((base, cell, sub, start)),
        }
    }
    p.mounts = mounts_left;
    if (!p.blocks.is_empty() || !p.mounts.is_empty()) && p.frames < 120 {
        world.insert_resource(p);
    }
}

/// Saves the current world (if one is being played).
pub fn save_current(world: &mut World) -> Option<Result<std::path::PathBuf, String>> {
    let name = world.resource::<CurrentWorld>().name.clone()?;
    let s = snapshot(world, &name);
    let r = write(&s);
    world.resource_mut::<CurrentWorld>().since_save = 0.0;
    Some(r)
}

/// Every 5 minutes of play.
pub const AUTOSAVE: f32 = 300.0;

fn autosave(world: &mut World) {
    if *world.resource::<State<crate::core::Ui>>().get() != crate::core::Ui::Game {
        return;
    }
    let dt = world.resource::<Time<Real>>().delta_secs();
    let due = {
        let mut c = world.resource_mut::<CurrentWorld>();
        c.since_save += dt;
        c.name.is_some() && c.since_save >= AUTOSAVE
    };
    if due && let Some(Err(e)) = save_current(world) {
        world.write_message(crate::core::toast(format!("Автосохранение не удалось: {e}")));
    }
}

/// Leaving the game (window closed / «Выход»): save first.
fn save_on_exit(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<AppExit>>) {
    let exiting = cursor.read(world.resource::<Messages<AppExit>>()).next().is_some();
    if exiting {
        let _ = save_current(world);
    }
}

// ------------------------------------------------------------ hooks of base modules

fn hook_fluid_save(w: &World, e: Entity, _: &SaveCtx) -> Option<String> {
    let s = w.get::<crate::fluids::FluidStore>(e)?;
    (s.amount > 0.0).then(|| to_ron(&(s.fluid, s.amount))).flatten()
}
fn hook_fluid_load(w: &mut World, e: Entity, d: &str, _: &LoadCtx) {
    if let Some((f, a)) = from_ron::<(Option<crate::fluids::Fluid>, f32)>(d)
        && let Some(mut s) = w.get_mut::<crate::fluids::FluidStore>(e)
    {
        s.fluid = f;
        s.amount = a.min(s.cap.max(a));
    }
}
fn hook_logic_save(w: &World, e: Entity, _: &SaveCtx) -> Option<String> {
    let s = w.get::<crate::logic::LogicState>(e)?;
    (s.memory || s.out || s.t != 0.0).then(|| to_ron(&(s.out, s.memory, s.t))).flatten()
}
fn hook_logic_load(w: &mut World, e: Entity, d: &str, _: &LoadCtx) {
    if let Some((out, memory, t)) = from_ron::<(bool, bool, f32)>(d)
        && let Some(mut s) = w.get_mut::<crate::logic::LogicState>(e)
    {
        s.out = out;
        s.memory = memory;
        s.t = t;
    }
}
fn hook_link_save(w: &World, e: Entity, ctx: &SaveCtx) -> Option<String> {
    let p = w.get::<crate::signal::Linked>(e)?.0?;
    to_ron(&(*ctx.grid_index.get(&p.grid)?, p.cell))
}
fn hook_link_load(w: &mut World, e: Entity, d: &str, ctx: &LoadCtx) {
    if let Some((g, cell)) = from_ron::<(usize, IVec3)>(d)
        && let Some(&grid) = ctx.grids.get(g)
    {
        w.entity_mut(e).insert(crate::signal::Linked(Some(crate::signal::PanelRef { grid, cell })));
    }
}
fn hook_light_save(w: &World, e: Entity, _: &SaveCtx) -> Option<String> {
    w.get::<crate::lights::Headlight>(e).filter(|h| h.on).map(|_| "on".into())
}
fn hook_light_load(w: &mut World, e: Entity, _: &str, _: &LoadCtx) {
    if let Some(mut h) = w.get_mut::<crate::lights::Headlight>(e) {
        h.on = true;
    }
}
fn hook_mount_save(w: &World, e: Entity, _: &SaveCtx) -> Option<String> {
    let m = w.get::<crate::mounts::Mount>(e)?;
    (m.value != 0.0).then(|| to_ron(&m.value)).flatten()
}
fn hook_mount_load(w: &mut World, e: Entity, d: &str, _: &LoadCtx) {
    if let Some(v) = from_ron::<f32>(d)
        && let Some(mut m) = w.get_mut::<crate::mounts::Mount>(e)
    {
        m.value = v;
    }
}
fn hook_fly_save(w: &World, e: Entity, _: &SaveCtx) -> Option<String> {
    let f = w.get::<crate::mech::Flywheel>(e)?;
    (f.energy > 0.0).then(|| to_ron(&f.energy)).flatten()
}
fn hook_fly_load(w: &mut World, e: Entity, d: &str, _: &LoadCtx) {
    if let Some(v) = from_ron::<f32>(d)
        && let Some(mut f) = w.get_mut::<crate::mech::Flywheel>(e)
    {
        f.energy = v;
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveHooks>();
        let mut hooks = app.world_mut().resource_mut::<SaveHooks>();
        hooks.add("fluid", hook_fluid_save, hook_fluid_load);
        hooks.add("logic", hook_logic_save, hook_logic_load);
        hooks.add("link", hook_link_save, hook_link_load);
        hooks.add("light", hook_light_save, hook_light_load);
        hooks.add("mount", hook_mount_save, hook_mount_load);
        hooks.add("flywheel", hook_fly_save, hook_fly_load);
        app.init_resource::<CurrentWorld>().add_systems(Update, (apply_pending, autosave)).add_systems(Last, save_on_exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_round_trip() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-1, 5, 17), Cell::new(BlockId::k("stone"), 3));
        g.set(IVec3::new(0, 0, 0), Cell::new(BlockId::k("gearbox"), 4));
        for x in 0..40 {
            g.set(IVec3::new(x, -1, 2), Cell::new(BlockId::k("concrete"), 2));
        }
        let (palette, chunks) = encode(&g);
        assert_eq!(palette.len(), 3);
        let mut back = VoxelGrid::default();
        assert_eq!(decode(&mut back, &palette, &chunks), 0);
        let mut a = g.cells();
        let mut b = back.cells();
        a.sort_by_key(|(c, _)| (c.x, c.y, c.z));
        b.sort_by_key(|(c, _)| (c.x, c.y, c.z));
        assert_eq!(a, b);
        assert_eq!(back.by_mech.get(&crate::content::Mech::Gearbox).map(|s| s.len()), Some(1), "the index is rebuilt");
        // A whole floor chunk is a single run.
        assert!(chunks.iter().all(|(_, r)| r.len() < 200));
        // Unknown blocks (a removed mod) are skipped, not fatal.
        let mut odd = VoxelGrid::default();
        assert_eq!(decode(&mut odd, &["no_such_block".into()], &[(IVec3::ZERO, vec![(1, 2, 3)])]), 3);
        assert!(odd.cells().is_empty());
    }

    #[test]
    fn names_and_format() {
        assert!(name_ok("Мой мир 2") && !name_ok("") && !name_ok("../x") && !name_ok("a.b"));
        let s = WorldSave { version: VERSION, name: "t".into(), grids: vec![GridSave { kind: GridKind::Body { pos: Vec3::ONE, rot: Quat::IDENTITY, vel: Vec3::ZERO, ang: Vec3::ZERO, frozen: false }, palette: vec!["stone".into()], chunks: vec![], glue: vec![], chains: vec![], blocks: vec![] }], ..default() };
        let text = ron::to_string(&s).unwrap();
        let back: WorldSave = ron::from_str(&text).unwrap();
        assert_eq!(back.grids[0].kind, s.grids[0].kind);
        // An older file without the newer fields still loads.
        let old: WorldSave = ron::from_str("(version: 1, name: \"o\", clock: (10.0, 0.02, false), player: (feet: (0.0, 1.0, 0.0), yaw: 0.0, pitch: 0.0, flying: false, selected: 0, slots: []), grids: [])").unwrap();
        assert_eq!(old.generator, "");
    }
}
