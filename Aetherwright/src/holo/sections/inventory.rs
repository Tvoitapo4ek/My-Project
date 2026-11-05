//! Inventory section: spatial slot grid with icons and counts, category
//! filters, search, sorting, quick slots and an item panel with actions
//! (move, split, gather, to quick slot, use / wear / install, drop).
//! In creative a catalog of every item is available.

use bevy::prelude::*;

use super::{
    FieldEdited, HoloAction, HoloSet, action_button, heading, item_icon, rule, text_field,
};
use crate::{
    content::Category,
    core::{events::Notify, states::GameMode},
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo, section_active,
        style::{self, CYAN, DIM, LINE, LINE_DIM, SELECTED, WHITE},
    },
    items::{
        ItemId, ItemStack, Use, all_items,
        drops::SpawnDrop,
        equipment::{Equipment, SlotRef},
        inventory::{HOTBAR, Inventory},
    },
    player::Player,
    survival::{damage::Health, needs::Vitals},
    tools::manipulator::Manipulator,
    world::materials::BlockMaterials,
};

#[derive(Resource, Debug, Default)]
pub struct InvUi {
    pub filter: Option<Category>,
    pub search: String,
    pub selected: Option<usize>,
    /// Slot whose stack is being moved (next slot click places it).
    pub moving: Option<usize>,
    pub catalog: bool,
}

/// Slot node: item shown (for in-place filter dimming) and inventory index.
#[derive(Component)]
struct SlotNode(Option<ItemId>, Option<usize>);

/// UI node of inventory slot `i` (tests, tutorials).
pub fn slot_entity(world: &mut World, i: usize) -> Option<Entity> {
    let mut q = world.query::<(Entity, &SlotNode)>();
    q.iter(world).find(|(_, s)| s.1 == Some(i)).map(|(e, _)| e)
}

fn matches(ui: &InvUi, item: Option<ItemId>) -> bool {
    let Some(item) = item else {
        return ui.filter.is_none() && ui.search.is_empty();
    };
    let d = item.def();
    ui.filter.is_none_or(|c| c == d.category)
        && (ui.search.is_empty() || d.name.to_lowercase().contains(&ui.search.to_lowercase()))
}

fn with_inv(w: &mut World, f: impl FnOnce(&mut Inventory, &mut InvUi)) {
    let mut q = w.query_filtered::<&mut Inventory, With<Player>>();
    let Some(mut inv) = q.iter_mut(w).next().map(|i| i.clone()) else {
        return;
    };
    let mut ui = std::mem::take(&mut *w.resource_mut::<InvUi>());
    f(&mut inv, &mut ui);
    *w.resource_mut::<InvUi>() = ui;
    if let Some(mut i) = q.iter_mut(w).next()
        && *i != inv
    {
        *i = inv;
    }
}

fn click_slot(i: usize) -> HoloAction {
    HoloAction::new(move |w| {
        with_inv(w, |inv, ui| match ui.moving.take() {
            Some(from) => {
                inv.move_stack(from, i);
                ui.selected = Some(i);
            }
            None => ui.selected = if ui.selected == Some(i) { None } else { Some(i) },
        })
    })
}

fn spawn_slot_node(
    p: &mut ChildSpawnerCommands,
    stack: Option<ItemStack>,
    selected: bool,
    moving: bool,
    action: HoloAction,
    blocks: Option<&BlockMaterials>,
    index: Option<usize>,
) {
    let border = if moving { style::AMBER } else if selected { LINE } else { LINE_DIM };
    p.spawn((
        Node {
            width: px(62),
            height: px(62),
            border: UiRect::all(px(if selected || moving { 2 } else { 1 })),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(if selected { SELECTED } else { Color::srgba(0.02, 0.1, 0.14, 0.75) }),
        BorderColor::all(border),
        style::HoverGlow,
        SlotNode(stack.map(|s| s.item), index),
        action,
    ))
    .with_children(|s| {
        if let Some(st) = stack {
            item_icon(s, st.item, 32.0, blocks);
            let name: String = st.item.def().name.split_whitespace().next().unwrap_or("").chars().take(8).collect();
            s.spawn(style::label(name, 10.0, DIM));
            if st.count > 1 {
                s.spawn((
                    style::label(st.count.to_string(), 13.0, CYAN),
                    Node { position_type: PositionType::Absolute, right: px(3), top: px(1), ..default() },
                ));
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn build(
    mut msgs: MessageReader<BuildSection>,
    mut commands: Commands,
    ui: Res<InvUi>,
    mode: Res<State<GameMode>>,
    player: Query<(&Inventory, Option<&Equipment>), With<Player>>,
    blocks: Option<Res<BlockMaterials>>,
) {
    let Some(m) = msgs.read().filter(|m| m.id == "inventory").last().copied() else {
        return;
    };
    let Ok((inv, _eq)) = player.single() else {
        return;
    };
    let blocks = blocks.as_deref();
    let creative = *mode.get() == GameMode::Creative;
    commands.entity(m.content).with_children(|p| {
        // Filters.
        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(5), row_gap: px(5), ..default() })
            .with_children(|r| {
                action_button(r, "Все", 13.0, Node::default(), ui.filter.is_none(), HoloAction::new(|w| w.resource_mut::<InvUi>().filter = None));
                for c in Category::ALL {
                    action_button(r, c.label(), 13.0, Node::default(), ui.filter == Some(c), HoloAction::new(move |w| {
                        let mut ui = w.resource_mut::<InvUi>();
                        ui.filter = if ui.filter == Some(c) { None } else { Some(c) };
                    }));
                }
            });
        p.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
            text_field(r, "inv_search", &ui.search, "Поиск по названию…", Node { width: px(260), ..default() });
            action_button(r, "Сортировать", 14.0, Node::default(), false, HoloAction::new(|w| with_inv(w, |inv, ui| {
                inv.sort_backpack();
                ui.selected = None;
                ui.moving = None;
            })));
            if creative {
                action_button(r, if ui.catalog { "Каталог ✓" } else { "Каталог" }, 14.0, Node::default(), ui.catalog, HoloAction::new(|w| {
                    let mut ui = w.resource_mut::<InvUi>();
                    ui.catalog = !ui.catalog;
                }));
            }
            let hint = if ui.moving.is_some() { "Выберите слот, куда переложить" } else { "Выберите предмет" };
            r.spawn(style::label(hint, 13.0, if ui.moving.is_some() { style::AMBER } else { DIM }));
        });
        rule(p);
        if creative && ui.catalog {
            p.spawn(super::scroll_column(Node { flex_grow: 1.0, min_height: px(0), ..default() })).with_children(|c| {
                c.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|g| {
                    for (k, _) in all_items().iter().enumerate() {
                        let item = ItemId(k as u16);
                        let stack = ItemStack::new(item, item.def().max_stack);
                        spawn_slot_node(g, Some(stack), false, false, HoloAction::new(move |w| with_inv(w, |inv, _| {
                            inv.add(item, item.def().max_stack);
                        })), blocks, None);
                    }
                });
            });
            return;
        }
        // Backpack rows, then the quick slots.
        let rows = (inv.slots.len() - HOTBAR) / HOTBAR;
        for row in 0..rows {
            p.spawn(Node { column_gap: px(6), ..default() }).with_children(|r| {
                for c in 0..HOTBAR {
                    let i = HOTBAR + row * HOTBAR + c;
                    spawn_slot_node(r, inv.slots[i], ui.selected == Some(i), ui.moving == Some(i), click_slot(i), blocks, Some(i));
                }
            });
        }
        p.spawn(style::label("Быстрые слоты (1–9)", 13.0, DIM));
        p.spawn(Node { column_gap: px(6), ..default() }).with_children(|r| {
            for i in 0..HOTBAR {
                spawn_slot_node(r, inv.slots[i], ui.selected == Some(i), ui.moving == Some(i), click_slot(i), blocks, Some(i));
            }
        });
    });

    // Item panel.
    commands.entity(m.detail).with_children(|p| {
        let Some(sel) = ui.selected else {
            heading(p, "Предмет");
            p.spawn(style::label("Выберите слот, чтобы увидеть описание и действия.", 13.0, DIM));
            return;
        };
        let Some(st) = inv.slots.get(sel).copied().flatten() else {
            heading(p, "Пустой слот");
            p.spawn(style::label(format!("Слот {}", sel + 1), 13.0, DIM));
            return;
        };
        let d = st.item.def();
        p.spawn(Node { column_gap: px(10), align_items: AlignItems::Center, ..default() }).with_children(|h| {
            item_icon(h, st.item, 48.0, blocks);
            h.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                t.spawn(style::label(d.name.clone(), 18.0, WHITE));
                t.spawn(style::label(d.category.label(), 13.0, CYAN));
            });
        });
        p.spawn(style::label(format!("Количество: {} / {}  ·  масса {:.1} кг", st.count, d.max_stack, d.mass * st.count as f32), 13.0, DIM));
        if !d.info.is_empty() {
            p.spawn(style::label(d.info.clone(), 14.0, WHITE));
        }
        rule(p);
        let wide = || Node { width: percent(100), ..default() };
        action_button(p, "Переместить…", 14.0, wide(), ui.moving.is_some(), HoloAction::new(move |w| {
            w.resource_mut::<InvUi>().moving = Some(sel);
        }));
        action_button(p, "Разделить пополам", 14.0, wide(), false, HoloAction::new(move |w| with_inv(w, |inv, _| {
            inv.split(sel);
        })));
        action_button(p, "Собрать все такие", 14.0, wide(), false, HoloAction::new(move |w| with_inv(w, |inv, _| inv.gather(sel))));
        match &d.use_ {
            Use::Eat { .. } => {
                action_button(p, "Съесть / выпить", 14.0, wide(), false, HoloAction::new(move |w| consume(w, sel)));
            }
            Use::Wear(_) => {
                action_button(p, "Надеть", 14.0, wide(), false, HoloAction::new(move |w| wear(w, sel)));
            }
            Use::Module(_) => {
                action_button(p, "Установить в манипулятор", 14.0, wide(), false, HoloAction::new(move |w| install(w, sel)));
            }
            _ => {}
        }
        p.spawn(style::label("В быстрый слот:", 13.0, DIM));
        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|r| {
            for k in 0..HOTBAR {
                action_button(r, (k + 1).to_string(), 13.0, Node { width: px(34), ..default() }, sel == k, HoloAction::new(move |w| with_inv(w, |inv, ui| {
                    inv.move_stack(sel, k);
                    ui.selected = Some(k);
                })));
            }
        });
        action_button(p, "Выбросить", 14.0, Node { width: percent(100), margin: UiRect::top(px(6)), ..default() }, false, HoloAction::new(move |w| drop_stack(w, sel)));
    });
}

fn consume(w: &mut World, sel: usize) {
    let mut q = w.query_filtered::<(&mut Inventory, &mut Vitals, &mut Health), With<Player>>();
    let Some((mut inv, mut v, mut h)) = q.iter_mut(w).next() else {
        return;
    };
    let Some(st) = inv.slots[sel] else {
        return;
    };
    let Use::Eat { food, water } = st.item.def().use_ else {
        return;
    };
    inv.take_one(sel);
    v.hunger = (v.hunger + food).min(100.0);
    v.thirst = (v.thirst + water).min(100.0);
    h.hp = (h.hp + food * 0.2).min(h.max);
    w.write_message(Notify::ok(format!("Съедено: {}", st.item.def().name)));
}

fn wear(w: &mut World, sel: usize) {
    let mut q = w.query_filtered::<(&mut Inventory, &mut Equipment), With<Player>>();
    let Some((mut inv, mut eq)) = q.iter_mut(w).next() else {
        return;
    };
    let Some(slot) = inv.slots[sel].and_then(|s| crate::items::equipment::wear_slot(s.item)) else {
        return;
    };
    // First free slot of that kind, else replace the first one.
    let target = SlotRef::ALL
        .iter()
        .filter(|r| r.slot == slot)
        .find(|r| eq.get(**r).is_none())
        .or_else(|| SlotRef::ALL.iter().find(|r| r.slot == slot))
        .copied();
    let ok = target.is_some_and(|t| eq.equip_from(t, &mut inv, sel));
    w.write_message(if ok { Notify::ok("Надето") } else { Notify::warn("Не удалось надеть: нет места") });
}

fn install(w: &mut World, sel: usize) {
    let mut q = w.query_filtered::<(&mut Inventory, &mut Manipulator), With<Player>>();
    let Some((mut inv, mut m)) = q.iter_mut(w).next() else {
        return;
    };
    let Some(st) = inv.slots[sel] else {
        return;
    };
    let Use::Module(key) = &st.item.def().use_ else {
        return;
    };
    let ok = crate::tools::modules::install(&mut m, key);
    if ok {
        inv.take_one(sel);
    }
    w.write_message(if ok { Notify::ok("Модуль установлен") } else { Notify::warn("Модуль уже установлен") });
}

fn drop_stack(w: &mut World, sel: usize) {
    let eye = *w.resource::<crate::core::resources::EyeRay>();
    let mut q = w.query_filtered::<&mut Inventory, With<Player>>();
    let Some(mut inv) = q.iter_mut(w).next() else {
        return;
    };
    let Some(st) = inv.slots[sel].take() else {
        return;
    };
    let flat = Vec3::new(eye.direction.x, 0.0, eye.direction.z).normalize_or_zero();
    w.write_message(SpawnDrop { stack: st, position: eye.origin + flat * 0.8, velocity: flat * 2.0 + Vec3::Y });
    w.resource_mut::<InvUi>().selected = None;
}

fn on_search(mut ev: MessageReader<FieldEdited>, mut ui: ResMut<InvUi>) {
    for e in ev.read().filter(|e| e.id == "inv_search") {
        ui.search = e.value.clone();
    }
}

/// Dims slots that don't match the filter / search (no rebuild needed).
fn dim_filtered(ui: Res<InvUi>, mut q: Query<(&SlotNode, &mut BackgroundColor, &Children)>, mut texts: Query<&mut TextColor>) {
    for (s, mut bg, children) in &mut q {
        let on = s.0.is_none() || matches(&ui, s.0);
        let a = if on { 0.75 } else { 0.18 };
        bg.0.set_alpha(a);
        for c in children.iter() {
            if let Ok(mut t) = texts.get_mut(c) {
                t.0.set_alpha(if on { 1.0 } else { 0.3 });
            }
        }
    }
}

/// Inventory data changed (pickup, crafting…): redraw.
fn watch_inventory(q: Query<(), (With<Player>, Changed<Inventory>)>, mut nav: ResMut<HoloNav>) {
    if !q.is_empty() {
        nav.rebuild = true;
    }
}

pub struct InventorySection;

impl Plugin for InventorySection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>().init_resource::<InvUi>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo {
            id: "inventory",
            title: "Инвентарь",
            glyph: "■",
            order: 10,
        });
        app.add_systems(Update, build.in_set(HoloSet::Build)).add_systems(
            Update,
            (on_search, dim_filtered, watch_inventory)
                .chain()
                .before(HoloSet::Frame)
                .run_if(section_active("inventory")),
        );
    }
}
