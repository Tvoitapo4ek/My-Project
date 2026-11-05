//! Blueprints section: the library of saved constructions with a rotating
//! 3D model, components and resource list, versions, copies, renaming and
//! loading into the building clipboard (then «Выделение» mode, V — paste).

use std::collections::BTreeMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use super::{FieldEdited, HoloAction, HoloSet, action_button, heading, rule, scroll_column, text_field};
pub use crate::building::library::BlueprintLibrary;
use crate::{
    building::{blueprints::Blueprint, selection::Selection},
    core::events::Notify,
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo, section_active,
        projection::{DragView, HoloMats, Turntable, holo_group, holo_material, holo_mesh},
        style::{self, AMBER, CYAN, DIM, WHITE},
    },
    items::inventory::Inventory,
    player::Player,
};

#[derive(Resource, Debug, Default)]
pub struct BlueprintUi {
    pub selected: Option<usize>,
    /// Chosen version number (None = latest).
    pub version: Option<u32>,
}

impl BlueprintUi {
    fn current<'a>(&self, lib: &'a BlueprintLibrary) -> Option<(usize, &'a crate::building::library::BlueprintVersion)> {
        let i = self.selected?;
        let e = lib.entries.get(i)?;
        let v = match self.version {
            Some(n) => e.versions.iter().find(|v| v.version == n).or(e.latest()),
            None => e.latest(),
        }?;
        Some((i, v))
    }
}

/// One mesh of coloured cubes for the blueprint's blocks (capped).
fn model_mesh(bp: &Blueprint) -> Mesh {
    let mut pos = Vec::new();
    let mut col = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([1.0, 0.0, 0.0], [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]]),
        ([-1.0, 0.0, 0.0], [[0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]),
        ([0.0, 1.0, 0.0], [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]]),
        ([0.0, -1.0, 0.0], [[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]]),
        ([0.0, 0.0, 1.0], [[1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0], [0.0, 0.0, 1.0]]),
        ([0.0, 0.0, -1.0], [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]]),
    ];
    let filled: std::collections::HashSet<IVec3> = bp.blocks.iter().map(|(p, _)| *p).collect();
    for (p, b) in bp.blocks.iter().take(6000) {
        let c = crate::items::item_for_block(*b).map(|i| i.color().to_srgba()).unwrap_or(Srgba::gray(0.6));
        for (k, (n, quad)) in faces.iter().enumerate() {
            let nb = *p + IVec3::new(n[0] as i32, n[1] as i32, n[2] as i32);
            if filled.contains(&nb) {
                continue;
            }
            let shade = [1.0, 0.8, 1.15, 0.6, 0.9, 0.9][k];
            let i = pos.len() as u32;
            for v in quad {
                pos.push([p.x as f32 + v[0], p.y as f32 + v[1], p.z as f32 + v[2]]);
                col.push([c.red * shade, c.green * shade, c.blue * shade, 0.8]);
            }
            idx.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
        }
    }
    let n = pos.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

fn clipboard(w: &World) -> Option<Blueprint> {
    w.resource::<Selection>().clipboard.clone().filter(|c| !c.is_empty())
}

fn save_lib(w: &mut World) {
    let lib = w.resource::<BlueprintLibrary>().clone();
    let msg = match lib.save() {
        Ok(()) => None,
        Err(e) => Some(Notify::warn(format!("Не удалось сохранить чертежи: {e}"))),
    };
    if let Some(m) = msg {
        w.write_message(m);
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    mut msgs: MessageReader<BuildSection>,
    mut commands: Commands,
    ui: Res<BlueprintUi>,
    lib: Res<BlueprintLibrary>,
    sel: Res<Selection>,
    inv: Query<&Inventory, With<Player>>,
    mats: Option<Res<HoloMats>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(m) = msgs.read().filter(|m| m.id == "blueprints").last().copied() else {
        return;
    };
    let has_clip = sel.clipboard.as_ref().is_some_and(|c| !c.is_empty());
    let current = ui.current(&lib);
    commands.entity(m.content).with_children(|p| {
        p.spawn(Node { column_gap: px(8), flex_wrap: FlexWrap::Wrap, row_gap: px(6), ..default() }).with_children(|r| {
            action_button(r, "Сохранить буфер как новый", 14.0, Node::default(), false, HoloAction::new(|w| {
                let Some(bp) = clipboard(w) else {
                    w.write_message(Notify::warn("Буфер пуст: выделите область (режим «Выделение») и нажмите C"));
                    return;
                };
                let i = w.resource_mut::<BlueprintLibrary>().add("Чертёж", &bp);
                *w.resource_mut::<BlueprintUi>() = BlueprintUi { selected: Some(i), version: None };
                save_lib(w);
                w.write_message(Notify::ok("Чертёж сохранён"));
            }));
            if current.is_some() {
                action_button(r, "Новая версия из буфера", 14.0, Node::default(), false, HoloAction::new(|w| {
                    let (Some(bp), Some(i)) = (clipboard(w), w.resource::<BlueprintUi>().selected) else {
                        w.write_message(Notify::warn("Буфер пуст"));
                        return;
                    };
                    let v = w.resource_mut::<BlueprintLibrary>().add_version(i, &bp);
                    w.resource_mut::<BlueprintUi>().version = v;
                    save_lib(w);
                }));
            }
            r.spawn(style::label(if has_clip { "в буфере есть постройка" } else { "буфер пуст" }, 12.0, DIM));
        });
        rule(p);
        p.spawn(Node { flex_grow: 1.0, column_gap: px(10), min_height: px(0), ..default() }).with_children(|row| {
            row.spawn(scroll_column(Node { width: px(290), row_gap: px(4), ..default() })).with_children(|list| {
                if lib.entries.is_empty() {
                    list.spawn(style::label("Библиотека пуста.\nВыделите постройку манипулятором (режим «Выделение»), скопируйте (C) и сохраните.", 13.0, DIM));
                }
                for (i, e) in lib.entries.iter().enumerate() {
                    let label = format!("{}  · v{}", e.name, e.latest().map(|v| v.version).unwrap_or(0));
                    action_button(list, label, 14.0, Node { width: percent(100), justify_content: JustifyContent::FlexStart, ..default() }, ui.selected == Some(i), HoloAction::new(move |w| {
                        *w.resource_mut::<BlueprintUi>() = BlueprintUi { selected: Some(i), version: None };
                    }));
                }
            });
            row.spawn((Node { flex_grow: 1.0, align_items: AlignItems::FlexEnd, justify_content: JustifyContent::Center, ..default() }, DragView("blueprints")))
                .with_children(|c| {
                    c.spawn(style::label(if current.is_some() { "перетаскивание — поворот · колесо — масштаб" } else { "" }, 12.0, DIM));
                });
        });
    });

    commands.entity(m.detail).with_children(|p| {
        let Some((i, ver)) = current else {
            heading(p, "Чертёж");
            p.spawn(style::label("Выберите чертёж в списке.", 13.0, DIM));
            return;
        };
        let entry = &lib.entries[i];
        heading(p, "Чертёж");
        text_field(p, "bp_name", &entry.name, "Название", Node { width: percent(100), ..default() });
        p.spawn(Node { column_gap: px(4), flex_wrap: FlexWrap::Wrap, row_gap: px(4), ..default() }).with_children(|r| {
            for v in &entry.versions {
                let n = v.version;
                action_button(r, format!("v{n}"), 12.0, Node::default(), ver.version == n, HoloAction::new(move |w| w.resource_mut::<BlueprintUi>().version = Some(n)));
            }
        });
        let bp = ver.data.to_blueprint();
        p.spawn(style::label(format!("Размер: {}×{}×{}", bp.size.x, bp.size.y, bp.size.z), 13.0, WHITE));
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for (_, b) in &bp.blocks {
            *counts.entry(b.def().name.clone()).or_default() += 1;
        }
        for part in &bp.parts {
            let name = crate::items::ItemId::from_key(part.kind.key()).map(|i| i.def().name.clone()).unwrap_or_default();
            *counts.entry(name).or_default() += 1;
        }
        p.spawn(style::label("Компоненты:", 12.0, CYAN));
        p.spawn(scroll_column(Node { max_height: px(150), ..default() })).with_children(|l| {
            for (name, n) in &counts {
                l.spawn(style::label(format!("{name} × {n}"), 12.0, WHITE));
            }
            l.spawn(style::label(format!("Соединений: {}", bp.joints.len()), 12.0, DIM));
        });
        p.spawn(style::label("Ресурсы (есть / нужно):", 12.0, CYAN));
        let inv = inv.single().ok();
        p.spawn(scroll_column(Node { max_height: px(110), ..default() })).with_children(|l| {
            for (item, need) in bp.cost() {
                let have = inv.map(|i| i.count(item)).unwrap_or(0);
                l.spawn(style::label(format!("{} {have}/{need}", item.def().name), 12.0, if have >= need { WHITE } else { AMBER }));
            }
        });
        let wide = || Node { width: percent(100), ..default() };
        action_button(p, "Загрузить в буфер", 13.0, wide(), false, HoloAction::new(move |w| {
            w.resource_mut::<Selection>().clipboard = Some(bp.clone());
            w.write_message(Notify::ok("Чертёж в буфере: режим «Выделение», V — вставка"));
        }));
        action_button(p, "Копировать чертёж", 13.0, wide(), false, HoloAction::new(move |w| {
            let c = w.resource_mut::<BlueprintLibrary>().duplicate(i);
            w.resource_mut::<BlueprintUi>().selected = c;
            save_lib(w);
        }));
        let v = ver.version;
        action_button(p, format!("Удалить версию v{v}"), 13.0, wide(), false, HoloAction::new(move |w| {
            w.resource_mut::<BlueprintLibrary>().remove_version(i, v);
            *w.resource_mut::<BlueprintUi>() = BlueprintUi::default();
            save_lib(w);
        }));
    });

    if let (Some((_, ver)), Some(mats)) = (current, mats) {
        let bp = ver.data.to_blueprint();
        let size = bp.size.max(IVec3::ONE).as_vec3();
        let s = 0.3 / size.max_element();
        let root = commands
            .spawn((holo_group(Transform::from_xyz(0.14, -0.08, 0.0)), Turntable { view: "blueprints", base_scale: s, tilt: false }, ChildOf(m.projection)))
            .id();
        let inner = commands.spawn((holo_group(Transform::from_translation(-size / 2.0 * Vec3::new(1.0, 0.0, 1.0))), ChildOf(root))).id();
        let mat = materials.add(holo_material(Color::srgba(1.0, 1.0, 1.0, 0.85), false));
        commands.spawn((holo_mesh(meshes.add(model_mesh(&bp)), mat, Transform::default()), ChildOf(inner)));
        let part_mesh = meshes.add(Cuboid::new(0.8, 0.8, 0.8));
        for part in &bp.parts {
            commands.spawn((holo_mesh(part_mesh.clone(), mats.amber.clone(), Transform::from_translation(part.offset).with_rotation(part.rotation)), ChildOf(inner)));
        }
    }
}

fn rename(mut ev: MessageReader<FieldEdited>, ui: Res<BlueprintUi>, mut lib: ResMut<BlueprintLibrary>, mut nav: ResMut<HoloNav>) {
    for e in ev.read().filter(|e| e.id == "bp_name" && e.submitted) {
        if let Some(i) = ui.selected {
            lib.rename(i, &e.value);
            let _ = lib.save();
            nav.rebuild = true;
        }
    }
}

pub struct BlueprintsSection;

impl Plugin for BlueprintsSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>().init_resource::<BlueprintUi>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo { id: "blueprints", title: "Чертежи", glyph: "▲", order: 50 });
        app.add_systems(Update, build.in_set(HoloSet::Build))
            .add_systems(Update, rename.before(HoloSet::Frame).run_if(section_active("blueprints")));
    }
}
