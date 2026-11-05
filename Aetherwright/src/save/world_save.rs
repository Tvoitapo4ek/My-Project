//! Saving and loading the game (F5 / F9).
//!
//! Save: snapshot → JSON → written atomically (temp file + rename, the
//! previous save kept as `.bak`).
//! Load: parse + validate first; on any error the current game is left
//! untouched and the player is told why. On success the world is reset,
//! terrain regenerated from the saved seed, saved chunks patched in before
//! meshing, then objects, joints, drops and the player are restored.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use avian3d::prelude::*;
use bevy::prelude::*;

use super::serialization::*;
use crate::{
    core::{
        events::Notify,
        resources::WorldClock,
        states::{AppState, GameMode},
    },
    crafting::stations::{CrateStorage, Machine, SavedStorage},
    items::{
        ItemStack,
        drops::{ItemDrop, SpawnDrop},
        inventory::Inventory,
    },
    machinery::components::{Engine, PartAssets, PartKind, PowerCell, spawn_part},
    magic::{sources::AetherStore, spells::AetherReserve},
    physics::{
        Frozen, Prop,
        joints::{Connection, spawn_connection},
        rigid_bodies::{loose_block_mesh, spawn_loose_block},
    },
    player::{Player, camera::CameraRig, movement::CharacterState},
    survival::{creatures::Creature, damage::Health, needs::Vitals},
    tools::manipulator::Manipulator,
    world::{
        Chunk, ChunkEntities, GenerationFinished, LoadProgress, MeshingStarted,
        PendingChunkPatches,
        chunks::{ChunkData, VoxelWorld},
        materials::BlockMaterials,
        voxel::BlockId,
    },
};

pub fn default_save_path() -> PathBuf {
    std::env::var("AETHERWRIGHT_SAVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("saves"))
        .join("world.json")
}

/// The save being restored (set by `start_load`, consumed after the world
/// is ready again).
#[derive(Resource)]
pub struct PendingLoad(pub SaveFile);

// ------------------------------------------------------------- capture ----

pub fn capture(world: &mut World) -> SaveFile {
    let voxels = world.resource::<VoxelWorld>();
    let meta = WorldMeta {
        seed: voxels.seed,
        chunk_size: voxels.chunk_size,
        size_chunks: voxels.size_chunks.to_array(),
        sea_level: voxels.sea_level,
    };
    // Palette = registry order, so runtime ids are palette indices.
    let block_palette: Vec<String> = crate::content::registry()
        .blocks
        .iter()
        .map(|b| b.key.clone())
        .collect();
    let mut modified: Vec<IVec3> = voxels.modified.iter().copied().collect();
    modified.sort_by_key(|c| (c.x, c.y, c.z));
    let chunks = modified
        .iter()
        .filter_map(|c| {
            voxels.chunks.get(c).map(|d| ChunkSave {
                coord: c.to_array(),
                rle: d.to_rle(),
            })
        })
        .collect();

    let rig = world.resource::<CameraRig>().clone();
    let mut pq = world.query_filtered::<(
        &Position,
        &CharacterState,
        &Health,
        &Vitals,
        &AetherReserve,
        &Inventory,
        &Manipulator,
    ), With<Player>>();
    let player = pq
        .iter(world)
        .next()
        .map(|(p, s, h, v, a, inv, m)| PlayerSave {
            position: p.0,
            yaw: rig.yaw,
            pitch: rig.pitch,
            health: h.hp,
            hunger: v.hunger,
            thirst: v.thirst,
            body_temp: v.body_temp,
            aether: a.amount,
            inventory: inv.to_saved(),
            selected: inv.selected,
            manipulator: m.clone(),
            flying: s.flying,
        })
        .expect("player exists in game");

    let mut ids: HashMap<Entity, u32> = HashMap::new();
    let mut props = Vec::new();
    let mut q = world.query::<(
        Entity,
        &Prop,
        &Position,
        &Rotation,
        Option<&LinearVelocity>,
        Option<&AngularVelocity>,
        Has<Frozen>,
        (
            Option<&Engine>,
            Option<&PowerCell>,
            Option<&AetherStore>,
            Option<&Machine>,
            Option<&CrateStorage>,
        ),
    )>();
    for (e, prop, p, r, lv, av, frozen, (engine, cell, store, machine, crate_s)) in q.iter(world) {
        let id = props.len() as u32;
        ids.insert(e, id);
        let storage = machine
            .map(|m| SavedStorage {
                slots: vec![
                    m.input.map(|s| s.to_saved()),
                    m.output.map(|s| s.to_saved()),
                ],
                fuel_ops: m.fuel_ops,
                progress: m.progress,
            })
            .or_else(|| {
                crate_s.map(|c| SavedStorage {
                    slots: c.0.to_saved(),
                    ..default()
                })
            });
        props.push(PropSave {
            id,
            prop: prop.clone(),
            position: p.0,
            rotation: r.0,
            linear_velocity: lv.map(|v| v.0).unwrap_or_default(),
            angular_velocity: av.map(|v| v.0).unwrap_or_default(),
            frozen,
            state: PartState {
                engine: engine.map(|e| (e.on, e.throttle, e.reverse)),
                energy: cell.map(|c| c.energy),
                aether: store.map(|s| (s.amount, s.kind)),
                storage,
            },
        });
    }
    let mut cq = world.query::<(Entity, &Chunk)>();
    let chunk_of: HashMap<Entity, IVec3> = cq.iter(world).map(|(e, c)| (e, c.coord)).collect();
    let body_ref = |e: Entity| {
        ids.get(&e)
            .map(|id| BodyRef::Prop(*id))
            .or_else(|| chunk_of.get(&e).map(|c| BodyRef::Chunk(c.to_array())))
    };
    let mut jq = world.query::<&Connection>();
    let joints = jq
        .iter(world)
        .filter_map(|c| {
            Some(JointSave {
                a: body_ref(c.a)?,
                b: body_ref(c.b)?,
                kind: c.kind,
                frame_a: c.frame_a,
                frame_b: c.frame_b,
            })
        })
        .collect();
    let mut dq = world.query::<(&ItemDrop, &Position)>();
    let drops = dq
        .iter(world)
        .map(|(d, p)| DropSave {
            item: d.stack.to_saved(),
            position: p.0,
        })
        .collect();

    SaveFile {
        format: FORMAT.into(),
        version: SAVE_VERSION,
        world: meta,
        block_palette,
        game_mode: *world.resource::<State<GameMode>>().get(),
        time_of_day: world.resource::<WorldClock>().time_of_day,
        gravity: world.resource::<Gravity>().0.length(),
        player,
        chunks,
        props,
        joints,
        drops,
        extensions: {
            world.insert_resource(SaveEntityIds(ids.clone()));
            let ext = capture_extensions(world);
            world.remove_resource::<SaveEntityIds>();
            ext
        },
    }
}

/// During capture: entity → prop id, so extensions can refer to bodies.
#[derive(Resource, Default, Debug, Clone)]
pub struct SaveEntityIds(pub HashMap<Entity, u32>);

/// During restore: prop id → new entity.
#[derive(Resource, Default, Debug, Clone)]
pub struct LoadedEntityIds(pub HashMap<u32, Entity>);

/// A module's piece of the save file (see `SaveFile::extensions`).
#[derive(Clone, Copy)]
pub struct SaveHook {
    pub key: &'static str,
    pub capture: fn(&mut World) -> serde_json::Value,
    /// Called after loading; `None` when the save has no data for the key
    /// (older save) — reset to defaults then.
    pub restore: fn(&mut World, Option<&serde_json::Value>),
}

/// Registered save extensions. Modules add theirs in `Plugin::build`.
#[derive(Resource, Default, Clone)]
pub struct SaveHooks(pub Vec<SaveHook>);

pub trait SaveHookExt {
    fn add_save_hook(&mut self, hook: SaveHook) -> &mut Self;
}

impl SaveHookExt for App {
    fn add_save_hook(&mut self, hook: SaveHook) -> &mut Self {
        self.init_resource::<SaveHooks>();
        self.world_mut().resource_mut::<SaveHooks>().0.push(hook);
        self
    }
}

fn capture_extensions(world: &mut World) -> std::collections::BTreeMap<String, serde_json::Value> {
    let hooks = world.get_resource::<SaveHooks>().cloned().unwrap_or_default();
    hooks.0.iter().map(|h| (h.key.to_string(), (h.capture)(world))).collect()
}

fn restore_extensions(world: &mut World, ext: &std::collections::BTreeMap<String, serde_json::Value>) {
    let hooks = world.get_resource::<SaveHooks>().cloned().unwrap_or_default();
    for h in &hooks.0 {
        (h.restore)(world, ext.get(h.key));
    }
}

/// Writes atomically; keeps the previous file as `.bak`.
pub fn write_save(path: &Path, save: &SaveFile) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, to_json(save))?;
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    std::fs::rename(&tmp, path)
}

pub fn read_save(path: &Path) -> Result<SaveFile, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|e| LoadError::Io(e.to_string()))?;
    parse(&text)
}

pub fn save_to(world: &mut World, path: &Path) -> Result<usize, String> {
    let save = capture(world);
    let n = save.props.len();
    write_save(path, &save).map_err(|e| e.to_string())?;
    Ok(n)
}

// ---------------------------------------------------------------- load ----

/// Validates the file and, if it is good, starts rebuilding the world.
pub fn start_load(world: &mut World, path: &Path) -> Result<(), LoadError> {
    let save = read_save(path)?;
    // Decode chunk patches up front so a bad chunk aborts before anything changes.
    let size = save.world.chunk_size;
    // Palette index → current block id; blocks of removed mods become air.
    let map: Vec<BlockId> = save
        .block_palette
        .iter()
        .map(|k| {
            BlockId::from_key(k).unwrap_or_else(|| {
                warn!("save: unknown block {k} replaced with air");
                BlockId(0)
            })
        })
        .collect();
    let mut patches = Vec::new();
    for c in &save.chunks {
        let data = ChunkData::from_rle(size, &c.rle, |i| map.get(i as usize).copied())
            .ok_or_else(|| LoadError::Invalid(format!("чанк {:?}", c.coord)))?;
        patches.push((IVec3::from_array(c.coord), data));
    }
    reset_world(world, &save.world);
    world.resource_mut::<PendingChunkPatches>().0 = patches;
    world.insert_resource(PendingLoad(save));
    world
        .resource_mut::<NextState<AppState>>()
        .set(AppState::Loading);
    Ok(())
}

/// Despawns everything world-specific and prepares regeneration.
fn reset_world(world: &mut World, meta: &WorldMeta) {
    let mut doomed: Vec<Entity> = Vec::new();
    doomed.extend(world.resource::<ChunkEntities>().0.values().copied());
    let mut q = world.query_filtered::<Entity, Or<(With<Prop>, With<Connection>, With<ItemDrop>, With<Creature>)>>();
    doomed.extend(q.iter(world));
    for e in doomed {
        if let Ok(em) = world.get_entity_mut(e) {
            em.despawn();
        }
    }
    world.resource_mut::<ChunkEntities>().0.clear();
    world.insert_resource(VoxelWorld::new(
        meta.chunk_size,
        IVec3::from_array(meta.size_chunks),
        meta.seed,
        meta.sea_level,
    ));
    *world.resource_mut::<LoadProgress>() = LoadProgress::default();
    world.resource_mut::<GenerationFinished>().0 = false;
    world.resource_mut::<MeshingStarted>().0 = false;
}

/// After the world is rebuilt: player, objects, joints, drops, settings.
pub fn restore_entities(world: &mut World) {
    let Some(PendingLoad(save)) = world.remove_resource::<PendingLoad>() else {
        return;
    };
    world
        .resource_mut::<NextState<GameMode>>()
        .set(save.game_mode);
    world.resource_mut::<WorldClock>().time_of_day = save.time_of_day.rem_euclid(1.0);
    world.resource_mut::<Gravity>().0 = Vec3::NEG_Y * save.gravity.clamp(0.0, 60.0);
    {
        let mut rig = world.resource_mut::<CameraRig>();
        rig.yaw = save.player.yaw;
        rig.pitch = save.player.pitch;
    }
    let mut pq = world.query_filtered::<(
        &mut Position,
        &mut LinearVelocity,
        &mut CharacterState,
        &mut Health,
        &mut Vitals,
        &mut AetherReserve,
        &mut Inventory,
        &mut Manipulator,
    ), With<Player>>();
    if let Some((mut p, mut v, mut s, mut h, mut vit, mut a, mut inv, mut m)) =
        pq.iter_mut(world).next()
    {
        let ps = &save.player;
        p.0 = ps.position;
        v.0 = Vec3::ZERO;
        s.flying = ps.flying;
        h.hp = ps.health.clamp(1.0, h.max);
        vit.hunger = ps.hunger.clamp(0.0, 100.0);
        vit.thirst = ps.thirst.clamp(0.0, 100.0);
        vit.body_temp = ps.body_temp.clamp(30.0, 42.0);
        a.amount = ps.aether.clamp(0.0, a.max);
        *inv = Inventory::from_saved(&ps.inventory, ps.selected);
        *m = ps.manipulator.clone();
    }

    // Objects.
    let assets = world.remove_resource::<PartAssets>();
    let mesh = loose_block_mesh(world);
    let mats = BlockMaterials {
        by_tex: world.resource::<BlockMaterials>().by_tex.clone(),
        images: Vec::new(),
    };
    let mut entities: HashMap<u32, Entity> = HashMap::new();
    {
        let mut commands = world.commands();
        for p in &save.props {
            let t = Transform::from_translation(p.position).with_rotation(p.rotation);
            let e = match &p.prop {
                Prop::Block(k) => BlockId::from_key(k)
                    .map(|b| spawn_loose_block(&mut commands, mesh.clone(), &mats, b, t, false)),
                Prop::Part(k) => match (PartKind::from_key(k), assets.as_ref()) {
                    (Some(kind), Some(a)) => Some(spawn_part(&mut commands, a, kind, t)),
                    _ => None,
                },
            };
            let Some(e) = e else {
                warn!("save: unknown object {:?} skipped", p.prop);
                continue;
            };
            entities.insert(p.id, e);
            commands.entity(e).insert((
                LinearVelocity(p.linear_velocity),
                AngularVelocity(p.angular_velocity),
            ));
            if p.frozen {
                commands.entity(e).insert((RigidBody::Static, Frozen));
            }
        }
    }
    world.flush();
    if let Some(a) = assets {
        world.insert_resource(a);
    }
    // Part state (components exist now).
    for p in &save.props {
        let Some(&e) = entities.get(&p.id) else {
            continue;
        };
        let st = &p.state;
        if let (Some((on, throttle, reverse)), Some(mut en)) =
            (st.engine, world.get_mut::<Engine>(e))
        {
            en.on = on;
            en.throttle = throttle.clamp(0.0, 1.0);
            en.reverse = reverse;
        }
        if let (Some(energy), Some(mut c)) = (st.energy, world.get_mut::<PowerCell>(e)) {
            c.energy = energy.clamp(0.0, c.capacity);
        }
        if let Some((amount, kind)) = st.aether {
            world.entity_mut(e).insert(AetherStore {
                amount,
                capacity: 100.0,
                kind,
            });
        }
        if let Some(s) = &st.storage {
            let stack = |i: usize| {
                s.slots
                    .get(i)
                    .cloned()
                    .flatten()
                    .as_ref()
                    .and_then(ItemStack::from_saved)
            };
            match &p.prop {
                Prop::Part(k) if k == "crate" => {
                    let mut storage = CrateStorage::default();
                    for (i, slot) in storage.0.slots.iter_mut().enumerate() {
                        *slot = stack(i);
                    }
                    world.entity_mut(e).insert(storage);
                }
                _ => {
                    world.entity_mut(e).insert(Machine {
                        input: stack(0),
                        output: stack(1),
                        fuel_ops: s.fuel_ops,
                        progress: s.progress,
                        drive_speed: 0.0,
                    });
                }
            }
        }
    }
    // Joints.
    let chunks = world.resource::<ChunkEntities>().0.clone();
    let resolve = |r: BodyRef| match r {
        BodyRef::Prop(id) => entities.get(&id).copied(),
        BodyRef::Chunk(c) => chunks.get(&IVec3::from_array(c)).copied(),
    };
    {
        let mut commands = world.commands();
        for j in &save.joints {
            if let (Some(a), Some(b)) = (resolve(j.a), resolve(j.b)) {
                spawn_connection(
                    &mut commands,
                    Connection {
                        a,
                        b,
                        kind: j.kind,
                        frame_a: j.frame_a,
                        frame_b: j.frame_b,
                    },
                );
            }
        }
    }
    world.flush();
    // Module data last: bodies, joints and the player exist by now.
    world.insert_resource(LoadedEntityIds(entities.clone()));
    restore_extensions(world, &save.extensions);
    world.remove_resource::<LoadedEntityIds>();
    for d in &save.drops {
        if let Some(stack) = ItemStack::from_saved(&d.item) {
            world.write_message(SpawnDrop {
                stack,
                position: d.position,
                velocity: Vec3::ZERO,
            });
        }
    }
    world.write_message(Notify::ok(format!(
        "Мир загружен: объектов {}, соединений {}",
        save.props.len(),
        save.joints.len()
    )));
}

fn hotkeys(world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let (save, load) = (
        keys.just_pressed(KeyCode::F5),
        keys.just_pressed(KeyCode::F9),
    );
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return;
    }
    let path = default_save_path();
    if save {
        let msg = match save_to(world, &path) {
            Ok(n) => Notify::ok(format!("Сохранено: {} (объектов {n})", path.display())),
            Err(e) => Notify::warn(format!("Ошибка сохранения: {e}")),
        };
        world.write_message(msg);
    }
    if load {
        if let Err(e) = start_load(world, &path) {
            world.write_message(Notify::warn(format!("Загрузка невозможна — {e}")));
        } else {
            world.write_message(Notify::info("Загрузка мира…"));
        }
    }
}

pub struct WorldSavePlugin;

impl Plugin for WorldSavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, hotkeys)
            .add_systems(OnEnter(AppState::InGame), restore_entities);
    }
}
