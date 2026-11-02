//! Map section: a holographic terrain projection built from the real world.
//! Only explored areas are drawn; landmarks (aether sources, anomalies,
//! constructions, spawn) and user markers float above it. Drag rotates and
//! tilts, MMB drag / arrows pan, wheel / ± zoom, a click in «marker» mode
//! drops a marker where the pointer ray meets the terrain plane.
//!
//! Multi-level maps (caves, islands above) need world streaming (T4) and
//! the world's vertical structure (concept open question); the projection
//! is built from a window + height range so levels can be added later.

use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use serde::{Deserialize, Serialize};

use super::{HoloAction, HoloSet, LiveText, action_button, heading, rule, text_field, FieldEdited};
use crate::{
    core::{events::Notify, resources::SpawnPoint},
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo, section_active,
        panels::{HoloRay, PanelSlot},
        projection::{DragView, HoloMats, HoloViews, Turntable, ViewState, holo_group, holo_material, holo_mesh},
        style::{self, AMBER, CYAN, DIM, VIOLET, WHITE},
    },
    machinery::components::Part,
    magic::{anomalies::Anomaly, sources::AetherSources},
    player::Player,
    save::world_save::{SaveHook, SaveHookExt},
    world::{chunks::VoxelWorld, materials::BlockMaterials, voxel::BlockId},
};

/// Explored cells (4×4 blocks), revealed around the player.
#[derive(Resource, Debug, Default, Clone)]
pub struct ExploredMap {
    pub seen: HashSet<IVec2>,
    /// Total cells in the world (for the percentage).
    pub total: usize,
}

pub const CELL: i32 = 4;
pub const REVEAL_RADIUS: f32 = 36.0;

impl ExploredMap {
    pub fn is_seen(&self, x: i32, z: i32) -> bool {
        self.seen.contains(&IVec2::new(x.div_euclid(CELL), z.div_euclid(CELL)))
    }
    pub fn percent(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.seen.len() as f32 * 100.0 / self.total as f32 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub position: Vec3,
    pub label: String,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct MapMarkers(pub Vec<Marker>);

#[derive(Resource, Debug, Default)]
pub struct MapUi {
    pub placing: bool,
    pub selected: Option<usize>,
    /// View state the projection was built for (rebuild when it changes).
    built: Option<(f32, Vec2)>,
}

/// How the current projection maps to the world.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MapFrame {
    pub center: Vec2,
    pub radius: f32,
    /// metres of projection per block
    pub scale: f32,
    pub base_y: f32,
    pub root: Option<Entity>,
}

/// World column under the pointer.
#[derive(Resource, Debug, Default)]
pub struct MapHover(pub Option<IVec2>);

#[derive(Component)]
struct MapRoot;

fn reveal(
    time: Res<Time>,
    mut acc: Local<f32>,
    world: Option<Res<VoxelWorld>>,
    player: Query<&Transform, With<Player>>,
    mut map: ResMut<ExploredMap>,
) {
    *acc -= time.delta_secs();
    if *acc > 0.0 {
        return;
    }
    *acc = 0.5;
    let (Some(world), Ok(t)) = (world, player.single()) else {
        return;
    };
    let size = world.size_blocks();
    map.total = ((size.x / CELL) * (size.z / CELL)) as usize;
    let p = t.translation;
    let r = (REVEAL_RADIUS / CELL as f32).ceil() as i32;
    let c = IVec2::new((p.x as i32).div_euclid(CELL), (p.z as i32).div_euclid(CELL));
    for dz in -r..=r {
        for dx in -r..=r {
            let cell = c + IVec2::new(dx, dz);
            let centre = (cell.as_vec2() + 0.5) * CELL as f32;
            if cell.x < 0 || cell.y < 0 || cell.x >= size.x / CELL || cell.y >= size.z / CELL {
                continue;
            }
            if centre.distance(Vec2::new(p.x, p.z)) <= REVEAL_RADIUS && !map.seen.contains(&cell) {
                map.seen.insert(cell);
            }
        }
    }
}

/// Average colour of each block texture (top face), cached.
fn block_colors(blocks: &BlockMaterials, images: &Assets<Image>, cache: &mut HashMap<u16, [f32; 3]>, b: BlockId) -> [f32; 3] {
    let tex = b.def().tex[0].0;
    *cache.entry(tex).or_insert_with(|| {
        let Some(img) = blocks.images.get(tex as usize).and_then(|h| images.get(h)) else {
            return [0.5, 0.6, 0.6];
        };
        let Some(data) = &img.data else {
            return [0.5, 0.6, 0.6];
        };
        let (mut r, mut g, mut bl, mut n) = (0.0, 0.0, 0.0, 0.0);
        for px in data.chunks_exact(4) {
            if px[3] > 10 {
                r += px[0] as f32;
                g += px[1] as f32;
                bl += px[2] as f32;
                n += 1.0;
            }
        }
        if n == 0.0 { [0.5, 0.6, 0.6] } else { [r / n / 255.0, g / n / 255.0, bl / n / 255.0] }
    })
}

/// Terrain mesh of the explored part of a window.
#[allow(clippy::too_many_arguments)]
fn terrain_mesh(
    world: &VoxelWorld,
    explored: &ExploredMap,
    blocks: &BlockMaterials,
    images: &Assets<Image>,
    cache: &mut HashMap<u16, [f32; 3]>,
    center: Vec2,
    radius: f32,
    scale: f32,
) -> (Mesh, f32) {
    let n = 56;
    let step = ((radius * 2.0) / n as f32).max(1.0);
    let size = world.size_blocks();
    let mut cols = vec![None; n * n];
    let mut min_y = i32::MAX;
    for gz in 0..n {
        for gx in 0..n {
            let x = (center.x - radius + (gx as f32 + 0.5) * step) as i32;
            let z = (center.y - radius + (gz as f32 + 0.5) * step) as i32;
            if x < 0 || z < 0 || x >= size.x || z >= size.z || !explored.is_seen(x, z) {
                continue;
            }
            if let Some(y) = world.surface_y(x, z) {
                let b = world.get(IVec3::new(x, y, z));
                let c = block_colors(blocks, images, cache, b);
                let a = if b.is_liquid() { 0.55 } else { 0.85 };
                cols[gz * n + gx] = Some((y, [c[0], c[1], c[2], a]));
                min_y = min_y.min(y);
            }
        }
    }
    if min_y == i32::MAX {
        min_y = 0;
    }
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let cell = step * scale;
    let half = n as f32 * cell / 2.0;
    let quad = |pos: &mut Vec<[f32; 3]>, col: &mut Vec<[f32; 4]>, idx: &mut Vec<u32>, v: [Vec3; 4], c: [f32; 4]| {
        let i = pos.len() as u32;
        for p in v {
            pos.push(p.to_array());
            col.push(c);
        }
        idx.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    };
    let h = |y: i32| (y - min_y + 1) as f32 * scale * 1.3;
    for gz in 0..n {
        for gx in 0..n {
            let Some((y, c)) = cols[gz * n + gx] else { continue };
            let x0 = gx as f32 * cell - half;
            let z0 = gz as f32 * cell - half;
            let top = h(y);
            quad(&mut pos, &mut col, &mut idx, [
                Vec3::new(x0, top, z0), Vec3::new(x0, top, z0 + cell), Vec3::new(x0 + cell, top, z0 + cell), Vec3::new(x0 + cell, top, z0),
            ], c);
            // Walls towards lower or unexplored neighbours give the relief depth.
            let side = [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6, c[3] * 0.8];
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (gx as i32 + dx, gz as i32 + dz);
                let other = if nx < 0 || nz < 0 || nx >= n as i32 || nz >= n as i32 {
                    None
                } else {
                    cols[nz as usize * n + nx as usize]
                };
                let low = other.map(|o| h(o.0)).unwrap_or(0.0);
                if low >= top {
                    continue;
                }
                let (a, b) = match (dx, dz) {
                    (1, _) => (Vec2::new(x0 + cell, z0 + cell), Vec2::new(x0 + cell, z0)),
                    (-1, _) => (Vec2::new(x0, z0), Vec2::new(x0, z0 + cell)),
                    (_, 1) => (Vec2::new(x0, z0 + cell), Vec2::new(x0 + cell, z0 + cell)),
                    _ => (Vec2::new(x0 + cell, z0), Vec2::new(x0, z0)),
                };
                quad(&mut pos, &mut col, &mut idx, [
                    Vec3::new(a.x, top, a.y), Vec3::new(a.x, low, a.y), Vec3::new(b.x, low, b.y), Vec3::new(b.x, top, b.y),
                ], side);
            }
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    (mesh, min_y as f32)
}

/// Landmarks: (position, label, kind colour index).
fn landmarks(w: &mut World) -> Vec<(Vec3, String, u8)> {
    let mut out = Vec::new();
    if let Some(s) = w.get_resource::<SpawnPoint>() {
        out.push((s.0, "Точка появления".into(), 0));
    }
    if let Some(src) = w.get_resource::<AetherSources>() {
        let mut seen = HashSet::new();
        for (p, kind, _) in &src.0 {
            let key = (p.as_ivec3() / 10).xz();
            if seen.insert(key) {
                out.push((*p, format!("Источник: {}", kind.label()), 1));
            }
        }
    }
    let mut q = w.query::<(&Anomaly, &Transform)>();
    for (a, t) in q.iter(w) {
        out.push((t.translation, format!("Аномалия ({})", a.kind.label()), 2));
    }
    let mut seen = HashSet::new();
    let mut q = w.query::<(&Part, &Transform)>();
    for (_, t) in q.iter(w) {
        if seen.insert((t.translation.as_ivec3() / 8).xz()) {
            out.push((t.translation, "Постройка".into(), 3));
        }
    }
    out
}

fn view_radius(v: &ViewState) -> f32 {
    (56.0 / v.zoom).clamp(16.0, 160.0)
}

fn build(world: &mut World) {
    let msgs: Vec<BuildSection> = world
        .resource_mut::<Messages<BuildSection>>()
        .iter_current_update_messages()
        .filter(|m| m.id == "map")
        .copied()
        .collect();
    let Some(m) = msgs.last().copied() else {
        return;
    };
    let player = {
        let mut q = world.query_filtered::<&Transform, With<Player>>();
        q.iter(world).next().map(|t| t.translation).unwrap_or_default()
    };
    let view = *world.resource_mut::<HoloViews>().get("map");
    let radius = view_radius(&view);
    let center = Vec2::new(player.x, player.z) + view.pan * radius * 2.0;
    let scale = 0.6 / (radius * 2.0);
    world.resource_mut::<MapUi>().built = Some((view.zoom, view.pan));
    let marks = landmarks(world);
    let markers = world.resource::<MapMarkers>().0.clone();
    let ui_selected = world.resource::<MapUi>().selected;
    let placing = world.resource::<MapUi>().placing;

    // Terrain.
    let mut cache = world.remove_resource::<ColorCache>().unwrap_or_default();
    let mesh = {
        let (Some(vw), Some(bm), Some(images)) = (
            world.get_resource::<VoxelWorld>(),
            world.get_resource::<BlockMaterials>(),
            world.get_resource::<Assets<Image>>(),
        ) else {
            world.insert_resource(cache);
            return;
        };
        terrain_mesh(vw, world.resource::<ExploredMap>(), bm, images, &mut cache.0, center, radius, scale)
    };
    world.insert_resource(cache);
    let (mesh, min_y) = mesh;
    let base_h = |y: f32| (y - min_y + 1.0) * scale * 1.3;
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(holo_material(Color::srgba(1.0, 1.0, 1.0, 0.9), false));
    let mats = world.resource::<HoloMats>().clone();
    let explored = world.resource::<ExploredMap>().clone();
    let inside = |p: Vec3| (Vec2::new(p.x, p.z) - center).abs().max_element() < radius;
    let local = |p: Vec3| Vec3::new((p.x - center.x) * scale, base_h(p.y), (p.z - center.y) * scale);

    let (pin, beacon, arrow) = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        (
            meshes.add(Sphere::new(0.008).mesh().ico(1).unwrap()),
            meshes.add(Cuboid::new(0.0015, 0.05, 0.0015)),
            meshes.add(Cone { radius: 0.012, height: 0.03 }),
        )
    };
    let mut commands = world.commands();
    let root = commands
        .spawn((holo_group(Transform::from_xyz(0.0, -0.12, 0.0)), Turntable { view: "map", base_scale: 1.0, tilt: true }, MapRoot, ChildOf(m.projection)))
        .id();
    commands.spawn((holo_mesh(mesh, mat, Transform::default()), ChildOf(root)));
    // Player arrow.
    commands.spawn((
        holo_mesh(arrow, mats.bright.clone(), Transform::from_translation(local(player) + Vec3::Y * 0.03).with_rotation(Quat::from_rotation_x(std::f32::consts::PI))),
        ChildOf(root),
    ));
    for (p, _, kind) in &marks {
        if !inside(*p) || !explored.is_seen(p.x as i32, p.z as i32) {
            continue;
        }
        let mat = match kind {
            1 => mats.violet.clone(),
            2 => mats.highlight.clone(),
            3 => mats.amber.clone(),
            _ => mats.line.clone(),
        };
        commands.spawn((holo_mesh(pin.clone(), mat.clone(), Transform::from_translation(local(*p) + Vec3::Y * 0.05)), ChildOf(root)));
        commands.spawn((holo_mesh(beacon.clone(), mat, Transform::from_translation(local(*p) + Vec3::Y * 0.025)), ChildOf(root)));
    }
    for (i, mk) in markers.iter().enumerate() {
        if !inside(mk.position) {
            continue;
        }
        let mat = if ui_selected == Some(i) { mats.highlight.clone() } else { mats.bright.clone() };
        commands.spawn((holo_mesh(pin.clone(), mat.clone(), Transform::from_translation(local(mk.position) + Vec3::Y * 0.07).with_scale(Vec3::splat(1.4))), ChildOf(root)));
        commands.spawn((holo_mesh(beacon.clone(), mat, Transform::from_translation(local(mk.position) + Vec3::Y * 0.035).with_scale(Vec3::new(1.0, 1.4, 1.0))), ChildOf(root)));
    }
    world.insert_resource(MapFrame { center, radius, scale, base_y: base_h(min_y + 4.0), root: Some(root) });

    // Panels.
    let mut commands = world.commands();
    commands.entity(m.content).with_children(|p| {
        p.spawn(Node { column_gap: px(6), flex_wrap: FlexWrap::Wrap, row_gap: px(6), ..default() }).with_children(|r| {
            let nudge = |d: Vec2| HoloAction::new(move |w| {
                let v = w.resource_mut::<HoloViews>().get("map").pan;
                w.resource_mut::<HoloViews>().get("map").pan = v + d;
            });
            action_button(r, "←", 15.0, Node::default(), false, nudge(Vec2::new(-0.15, 0.0)));
            action_button(r, "↑", 15.0, Node::default(), false, nudge(Vec2::new(0.0, -0.15)));
            action_button(r, "↓", 15.0, Node::default(), false, nudge(Vec2::new(0.0, 0.15)));
            action_button(r, "→", 15.0, Node::default(), false, nudge(Vec2::new(0.15, 0.0)));
            action_button(r, "+", 15.0, Node::default(), false, HoloAction::new(|w| {
                let v = w.resource_mut::<HoloViews>().get("map").zoom;
                w.resource_mut::<HoloViews>().get("map").zoom = (v * 1.4).min(4.0);
            }));
            action_button(r, "−", 15.0, Node::default(), false, HoloAction::new(|w| {
                let v = w.resource_mut::<HoloViews>().get("map").zoom;
                w.resource_mut::<HoloViews>().get("map").zoom = (v / 1.4).max(0.4);
            }));
            action_button(r, "◀ поворот", 14.0, Node::default(), false, HoloAction::new(|w| w.resource_mut::<HoloViews>().get("map").yaw -= 0.4));
            action_button(r, "поворот ▶", 14.0, Node::default(), false, HoloAction::new(|w| w.resource_mut::<HoloViews>().get("map").yaw += 0.4));
            action_button(r, "◎ к себе", 14.0, Node::default(), false, HoloAction::new(|w| {
                let v = w.resource_mut::<HoloViews>();
                let mut v = v;
                let s = v.get("map");
                s.pan = Vec2::ZERO;
                s.yaw = 0.0;
            }));
            action_button(r, if placing { "Метка: укажите место" } else { "Поставить метку" }, 14.0, Node::default(), placing, HoloAction::new(|w| {
                let mut ui = w.resource_mut::<MapUi>();
                ui.placing = !ui.placing;
            }));
            action_button(r, "Метка здесь", 14.0, Node::default(), false, HoloAction::new(|w| {
                let mut q = w.query_filtered::<&Transform, With<Player>>();
                if let Some(t) = q.iter(w).next().map(|t| t.translation) {
                    add_marker(w, t);
                }
            }));
        });
        p.spawn((
            Node { flex_grow: 1.0, align_items: AlignItems::FlexEnd, justify_content: JustifyContent::Center, ..default() },
            DragView("map"),
            MapViewport,
        ))
        .observe(on_map_click)
        .with_children(|c| {
            c.spawn(style::label(
                "ЛКМ-перетаскивание — поворот/наклон · СКМ — сдвиг · колесо — масштаб · уровень: поверхность (подземные уровни — после T4)",
                11.0,
                DIM,
            ));
        });
        p.spawn(Node { column_gap: px(14), ..default() }).with_children(|l| {
            for (txt, c) in [("▲ вы", WHITE), ("● источник эфира", VIOLET), ("● аномалия", WHITE), ("● постройка", AMBER), ("● ваши метки", CYAN)] {
                l.spawn(style::label(txt, 12.0, c));
            }
        });
    });
    commands.entity(m.detail).with_children(|p| {
        heading(p, "Область");
        p.spawn((style::label("", 13.0, WHITE), LiveText(hover_info)));
        p.spawn((style::label("", 12.0, DIM), LiveText(|w| format!("Исследовано: {:.1}%", w.resource::<ExploredMap>().percent()))));
        rule(p);
        heading(p, "Метки");
        if markers.is_empty() {
            p.spawn(style::label("Меток нет", 13.0, DIM));
        }
        p.spawn(super::scroll_column(Node { max_height: px(250), row_gap: px(4), ..default() })).with_children(|list| {
            for (i, mk) in markers.iter().enumerate() {
                list.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
                    action_button(r, format!("{} ({:.0}, {:.0})", mk.label, mk.position.x, mk.position.z), 12.0, Node { flex_grow: 1.0, ..default() }, ui_selected == Some(i), HoloAction::new(move |w| {
                        let mut ui = w.resource_mut::<MapUi>();
                        ui.selected = if ui.selected == Some(i) { None } else { Some(i) };
                    }));
                    action_button(r, "✕", 12.0, Node::default(), false, HoloAction::new(move |w| {
                        let mut m = w.resource_mut::<MapMarkers>();
                        if i < m.0.len() {
                            m.0.remove(i);
                        }
                        w.resource_mut::<MapUi>().selected = None;
                    }));
                });
            }
        });
        if let Some(i) = ui_selected
            && let Some(mk) = markers.get(i)
        {
            p.spawn(style::label("Название метки:", 12.0, DIM));
            text_field(p, "map_marker_name", &mk.label, "Название", Node { width: percent(100), ..default() });
        }
        rule(p);
        heading(p, "Ориентиры рядом");
        for (pos, label, _) in marks.iter().filter(|(p, _, _)| explored.is_seen(p.x as i32, p.z as i32)).take(6) {
            p.spawn(style::label(format!("{label}: {:.0} м", pos.distance(player)), 12.0, DIM));
        }
    });
}

#[derive(Resource, Default)]
struct ColorCache(HashMap<u16, [f32; 3]>);

#[derive(Component)]
struct MapViewport;

fn add_marker(w: &mut World, p: Vec3) {
    let mut m = w.resource_mut::<MapMarkers>();
    let n = m.0.len() + 1;
    m.0.push(Marker { position: p, label: format!("Метка {n}") });
    w.write_message(Notify::ok(format!("Метка {n} поставлена")));
}

/// Ray from the pointer onto the terrain plane → world column.
fn ray_to_world(ray: Ray3d, frame: &MapFrame, gt: &GlobalTransform) -> Option<Vec2> {
    let inv = gt.affine().inverse();
    let o = inv.transform_point3(ray.origin);
    let d = inv.transform_vector3(*ray.direction);
    if d.y.abs() < 1e-6 {
        return None;
    }
    let t = (frame.base_y - o.y) / d.y;
    if t <= 0.0 {
        return None;
    }
    let hit = o + d * t;
    let p = frame.center + Vec2::new(hit.x, hit.z) / frame.scale;
    ((p - frame.center).abs().max_element() < frame.radius).then_some(p)
}

fn track_hover(
    ray: Res<HoloRay>,
    frame: Res<MapFrame>,
    roots: Query<&GlobalTransform, With<MapRoot>>,
    mut hover: ResMut<MapHover>,
) {
    hover.0 = None;
    if ray.hit.is_none_or(|h| h.0 != PanelSlot::Content) {
        return;
    }
    let (Some(r), Some(root)) = (ray.ray, frame.root) else {
        return;
    };
    if let Ok(gt) = roots.get(root) {
        hover.0 = ray_to_world(r, &frame, gt).map(|p| p.as_ivec2());
    }
}

fn on_map_click(_ev: On<PointerClick>, mut commands: Commands) {
    commands.queue(|w: &mut World| {
        if !w.resource::<MapUi>().placing {
            return;
        }
        let Some(c) = w.resource::<MapHover>().0 else {
            return;
        };
        let y = w.get_resource::<VoxelWorld>().and_then(|v| v.surface_y(c.x, c.y)).unwrap_or(0) as f32 + 1.0;
        add_marker(w, Vec3::new(c.x as f32 + 0.5, y, c.y as f32 + 0.5));
        w.resource_mut::<MapUi>().placing = false;
        w.resource_mut::<HoloNav>().rebuild = true;
    });
}

fn hover_info(w: &mut World) -> String {
    let Some(c) = w.resource::<MapHover>().0 else {
        return "Наведите на карту".into();
    };
    if !w.resource::<ExploredMap>().is_seen(c.x, c.y) {
        return format!("({}, {}) — не исследовано", c.x, c.y);
    }
    let Some(vw) = w.get_resource::<VoxelWorld>() else {
        return String::new();
    };
    let Some(y) = vw.surface_y(c.x, c.y) else {
        return "пустота".into();
    };
    let b = vw.get(IVec3::new(c.x, y, c.y));
    let mut s = format!("({}, {}) · высота {}\nПоверхность: {}", c.x, c.y, y, b.def().name);
    if let Some(src) = w.get_resource::<AetherSources>() {
        let f = src.sample(Vec3::new(c.x as f32, y as f32 + 1.0, c.y as f32));
        if f.strength > 0.05 {
            s.push_str(&format!("\nЭфир: {:.1} ({})", f.strength, f.kind.label()));
        }
    }
    s
}

/// Zoom / pan changed by drag or wheel: rebuild the projection (debounced).
fn watch_view(time: Res<Time>, mut wait: Local<f32>, mut views: ResMut<HoloViews>, ui: Res<MapUi>, mut nav: ResMut<HoloNav>, markers: Res<MapMarkers>) {
    let v = *views.get("map");
    if markers.is_changed() {
        nav.rebuild = true;
    }
    if ui.built.is_some_and(|(z, p)| z != v.zoom || p != v.pan) {
        *wait += time.delta_secs();
        if *wait > 0.25 {
            *wait = 0.0;
            nav.rebuild = true;
        }
    }
}

fn rename_marker(mut ev: MessageReader<FieldEdited>, ui: Res<MapUi>, mut markers: ResMut<MapMarkers>) {
    for e in ev.read().filter(|e| e.id == "map_marker_name" && e.submitted) {
        if let Some(m) = ui.selected.and_then(|i| markers.0.get_mut(i)) {
            m.label = e.value.trim().to_string();
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SavedMap {
    explored: Vec<(i32, i32)>,
    markers: Vec<Marker>,
}

fn capture(w: &mut World) -> serde_json::Value {
    let mut explored: Vec<(i32, i32)> = w.resource::<ExploredMap>().seen.iter().map(|c| (c.x, c.y)).collect();
    explored.sort_unstable();
    serde_json::to_value(SavedMap { explored, markers: w.resource::<MapMarkers>().0.clone() }).unwrap_or_default()
}

fn restore(w: &mut World, v: Option<&serde_json::Value>) {
    let s: SavedMap = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    w.resource_mut::<ExploredMap>().seen = s.explored.into_iter().map(|(x, z)| IVec2::new(x, z)).collect();
    w.resource_mut::<MapMarkers>().0 = s.markers;
}

pub struct MapSection;

impl Plugin for MapSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>()
            .init_resource::<ExploredMap>()
            .init_resource::<MapMarkers>()
            .init_resource::<MapUi>()
            .init_resource::<MapFrame>()
            .init_resource::<MapHover>()
            .init_resource::<HoloViews>();
        app.world_mut().resource_mut::<HoloViews>().0.insert("map", ViewState { yaw: 0.0, pitch: 0.75, zoom: 1.0, pan: Vec2::ZERO, spin: false });
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo { id: "map", title: "Карта", glyph: "◈", order: 30 });
        app.add_save_hook(SaveHook { key: "map", capture, restore })
            .add_systems(Update, reveal.run_if(crate::core::states::in_game))
            .add_systems(Update, build.in_set(HoloSet::Build))
            .add_systems(
                Update,
                (rename_marker, track_hover, watch_view).before(HoloSet::Frame).run_if(section_active("map")),
            );
    }
}
