//! Journal section: researcher's notes grouped by category. Entries come
//! from `content/journal.ron` (+ mods) and open gradually — by scanning,
//! by obtaining items, or from the start. Each entry has a picture
//! (texture, item icon or line schematic), a description, hypotheses and
//! the player's own notes.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{FieldEdited, HoloAction, HoloSet, action_button, heading, item_icon, rule, scroll_column, text_field};
use crate::{
    content::{JournalCategory, JournalDef, JournalImage, JournalUnlock, registry},
    core::events::Notify,
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo, section_active,
        style::{self, CYAN, DIM, LINE, LINE_DIM, VIOLET, WHITE},
    },
    items::{ItemId, inventory::Inventory},
    player::Player,
    save::world_save::{SaveHook, SaveHookExt},
    tools::scanning::Scanned,
    world::materials::BlockMaterials,
};

#[derive(Resource, Debug, Default, Clone)]
pub struct JournalState {
    pub unlocked: BTreeSet<String>,
    pub notes: BTreeMap<String, Vec<String>>,
    /// Opened but not looked at yet.
    pub fresh: BTreeSet<String>,
}

#[derive(Resource, Debug)]
pub struct JournalUi {
    pub category: JournalCategory,
    pub selected: Option<String>,
}

impl Default for JournalUi {
    fn default() -> Self {
        Self { category: JournalCategory::Creatures, selected: None }
    }
}

impl JournalState {
    pub fn unlock(&mut self, key: &str) -> bool {
        if self.unlocked.insert(key.to_string()) {
            self.fresh.insert(key.to_string());
            true
        } else {
            false
        }
    }
    fn with_always(mut self) -> Self {
        for e in &registry().journal {
            if e.unlock == JournalUnlock::Always {
                self.unlocked.insert(e.key.clone());
            }
        }
        self
    }
}

fn unlock_by_scan(mut scans: MessageReader<Scanned>, mut state: ResMut<JournalState>, mut notify: MessageWriter<Notify>) {
    for s in scans.read() {
        for e in &registry().journal {
            let hit = match &e.unlock {
                JournalUnlock::ScanBlock(k) => s.block.as_deref() == Some(k.as_str()),
                JournalUnlock::ScanObject(t) => s.title.contains(t.as_str()),
                _ => false,
            };
            if hit && state.unlock(&e.key) {
                notify.write(Notify::ok(format!("Журнал: новая запись «{}»", e.title)));
            }
        }
    }
}

fn unlock_by_items(
    inv: Query<&Inventory, (With<Player>, Changed<Inventory>)>,
    mut state: ResMut<JournalState>,
    mut notify: MessageWriter<Notify>,
) {
    let Ok(inv) = inv.single() else {
        return;
    };
    for e in &registry().journal {
        if let JournalUnlock::Obtain(k) = &e.unlock
            && ItemId::from_key(k).is_some_and(|i| inv.count(i) > 0)
            && state.unlock(&e.key)
        {
            notify.write(Notify::ok(format!("Журнал: новая запись «{}»", e.title)));
        }
    }
}

/// Draws an entry picture into a square box.
fn picture(p: &mut ChildSpawnerCommands, img: &JournalImage, size: f32, blocks: Option<&BlockMaterials>) {
    match img {
        JournalImage::None => {}
        JournalImage::Texture(name) => {
            let tex = registry().textures.iter().position(|t| &t.name == name);
            if let Some(h) = tex.and_then(|t| blocks.and_then(|b| b.images.get(t).cloned())) {
                p.spawn((
                    Node { width: px(size), height: px(size), border: UiRect::all(px(1)), ..default() },
                    ImageNode::new(h),
                    BorderColor::all(LINE),
                    Pickable::IGNORE,
                ));
            }
        }
        JournalImage::Item(k) => {
            if let Some(i) = ItemId::from_key(k) {
                item_icon(p, i, size, blocks);
            }
        }
        JournalImage::Scheme(lines) => {
            p.spawn((
                Node { width: px(size), height: px(size), border: UiRect::all(px(1)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.08, 0.12, 0.6)),
                BorderColor::all(LINE_DIM),
                Pickable::IGNORE,
            ))
            .with_children(|b| {
                for &(x0, y0, x1, y1) in lines {
                    let a = Vec2::new(x0, y0) * size;
                    let c = Vec2::new(x1, y1) * size;
                    let len = a.distance(c);
                    let mid = (a + c) / 2.0;
                    let ang = (c - a).to_angle();
                    b.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(mid.x - len / 2.0),
                            top: px(mid.y - 1.0),
                            width: px(len),
                            height: px(2),
                            ..default()
                        },
                        UiTransform { rotation: Rot2::radians(ang), ..default() },
                        BackgroundColor(style::CYAN),
                        Pickable::IGNORE,
                    ));
                }
            });
        }
    }
}

fn unlock_hint(e: &JournalDef) -> String {
    match &e.unlock {
        JournalUnlock::Always => String::new(),
        JournalUnlock::ScanBlock(k) => {
            let name = crate::world::voxel::BlockId::from_key(k).map(|b| b.def().name.clone()).unwrap_or_else(|| k.clone());
            format!("Отсканируйте: {name}")
        }
        JournalUnlock::ScanObject(t) => format!("Отсканируйте объект «{t}»"),
        JournalUnlock::Obtain(k) => match ItemId::from_key(k) {
            Some(i) => format!("Получите: {}", i.def().name),
            None => "Ещё не найдено".into(),
        },
    }
}

fn build(
    mut msgs: MessageReader<BuildSection>,
    mut commands: Commands,
    ui: Res<JournalUi>,
    mut state: ResMut<JournalState>,
    blocks: Option<Res<BlockMaterials>>,
) {
    let Some(m) = msgs.read().filter(|m| m.id == "journal").last().copied() else {
        return;
    };
    let blocks = blocks.as_deref();
    let entries: Vec<&JournalDef> = registry().journal.iter().filter(|e| e.category == ui.category).collect();
    commands.entity(m.content).with_children(|p| {
        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(5), row_gap: px(5), ..default() }).with_children(|r| {
            for c in JournalCategory::ALL {
                let total = registry().journal.iter().filter(|e| e.category == c).count();
                let open = registry().journal.iter().filter(|e| e.category == c && state.unlocked.contains(&e.key)).count();
                let fresh = registry().journal.iter().any(|e| e.category == c && state.fresh.contains(&e.key));
                action_button(r, format!("{}{} {open}/{total}", if fresh { "● " } else { "" }, c.label()), 13.0, Node::default(), ui.category == c, HoloAction::new(move |w| {
                    let mut ui = w.resource_mut::<JournalUi>();
                    ui.category = c;
                    ui.selected = None;
                }));
            }
        });
        rule(p);
        p.spawn(Node { flex_grow: 1.0, column_gap: px(14), min_height: px(0), ..default() }).with_children(|row| {
            // Entry list.
            row.spawn(scroll_column(Node { width: px(300), row_gap: px(4), ..default() })).with_children(|list| {
                if entries.is_empty() {
                    list.spawn(style::label("Записей нет", 13.0, DIM));
                }
                for e in &entries {
                    let key = e.key.clone();
                    let open = state.unlocked.contains(&e.key);
                    let title = if open { e.title.clone() } else { "??? — не изучено".into() };
                    let mark = if state.fresh.contains(&e.key) { "● " } else { "" };
                    action_button(list, format!("{mark}{title}"), 14.0, Node { width: percent(100), justify_content: JustifyContent::FlexStart, ..default() }, ui.selected.as_deref() == Some(e.key.as_str()), HoloAction::new(move |w| {
                        w.resource_mut::<JournalUi>().selected = Some(key.clone());
                    }));
                }
            });
            // Entry page.
            row.spawn(scroll_column(Node { flex_grow: 1.0, row_gap: px(8), ..default() })).with_children(|page| {
                let Some(e) = ui.selected.as_ref().and_then(|k| entries.iter().find(|e| &e.key == k)) else {
                    page.spawn(style::label("Выберите запись слева.", 14.0, DIM));
                    return;
                };
                if !state.unlocked.contains(&e.key) {
                    page.spawn(style::label("Запись ещё не открыта.", 16.0, WHITE));
                    page.spawn(style::label(unlock_hint(e), 14.0, CYAN));
                    return;
                }
                page.spawn(Node { column_gap: px(14), ..default() }).with_children(|h| {
                    picture(h, &e.image, 120.0, blocks);
                    h.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 1.0, ..default() }).with_children(|t| {
                        t.spawn(style::label(e.title.clone(), 20.0, WHITE));
                        t.spawn(style::label(e.category.label(), 13.0, CYAN));
                    });
                });
                page.spawn(style::label(e.text.clone(), 15.0, WHITE));
                if !e.hypothesis.is_empty() {
                    page.spawn(style::label(format!("Гипотеза: {}", e.hypothesis), 14.0, VIOLET));
                }
            });
        });
    });

    // Notes panel.
    commands.entity(m.detail).with_children(|p| {
        heading(p, "Заметки");
        let Some(key) = ui.selected.clone().filter(|k| state.unlocked.contains(k)) else {
            p.spawn(style::label("Откройте запись, чтобы вести заметки.", 13.0, DIM));
            return;
        };
        let notes = state.notes.get(&key).cloned().unwrap_or_default();
        p.spawn(scroll_column(Node { max_height: px(380), row_gap: px(4), ..default() })).with_children(|list| {
            if notes.is_empty() {
                list.spawn(style::label("Заметок нет", 13.0, DIM));
            }
            for (i, n) in notes.iter().enumerate() {
                list.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
                    r.spawn((style::label(format!("— {n}"), 13.0, WHITE), Node { flex_grow: 1.0, ..default() }));
                    let k = key.clone();
                    action_button(r, "✕", 12.0, Node::default(), false, HoloAction::new(move |w| {
                        if let Some(v) = w.resource_mut::<JournalState>().notes.get_mut(&k)
                            && i < v.len()
                        {
                            v.remove(i);
                        }
                    }));
                });
            }
        });
        p.spawn(style::label("Новая заметка (Enter — сохранить):", 12.0, DIM));
        text_field(p, "journal_note", "", "нажмите, чтобы писать…", Node { width: percent(100), ..default() });
    });
    if let Some(k) = &ui.selected {
        // Seen now; no change detection (would trigger another rebuild).
        state.bypass_change_detection().fresh.remove(k);
    }
}

fn add_note(mut ev: MessageReader<FieldEdited>, ui: Res<JournalUi>, mut state: ResMut<JournalState>, mut nav: ResMut<HoloNav>) {
    for e in ev.read().filter(|e| e.id == "journal_note" && e.submitted) {
        let text = e.value.trim();
        if let (Some(k), false) = (&ui.selected, text.is_empty()) {
            state.notes.entry(k.clone()).or_default().push(text.to_string());
            nav.rebuild = true;
        }
    }
}

fn watch(state: Res<JournalState>, mut nav: ResMut<HoloNav>) {
    if state.is_changed() && !state.is_added() {
        nav.rebuild = true;
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SavedJournal {
    unlocked: Vec<String>,
    notes: BTreeMap<String, Vec<String>>,
}

fn capture(w: &mut World) -> serde_json::Value {
    let s = w.resource::<JournalState>();
    serde_json::to_value(SavedJournal { unlocked: s.unlocked.iter().cloned().collect(), notes: s.notes.clone() }).unwrap_or_default()
}

fn restore(w: &mut World, v: Option<&serde_json::Value>) {
    let s: SavedJournal = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    *w.resource_mut::<JournalState>() = JournalState {
        unlocked: s.unlocked.into_iter().collect(),
        notes: s.notes,
        fresh: BTreeSet::new(),
    }
    .with_always();
}

pub struct JournalSection;

impl Plugin for JournalSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>()
            .insert_resource(JournalState::default().with_always())
            .init_resource::<JournalUi>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo { id: "journal", title: "Журнал", glyph: "≡", order: 40 });
        app.add_save_hook(SaveHook { key: "journal", capture, restore })
            .add_systems(Update, (unlock_by_scan, unlock_by_items).run_if(crate::core::states::in_game))
            .add_systems(Update, build.in_set(HoloSet::Build))
            .add_systems(Update, (add_note, watch).before(HoloSet::Frame).run_if(section_active("journal")));
    }
}
