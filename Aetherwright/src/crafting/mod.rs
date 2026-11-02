//! Production: recipes, hand/workbench crafting UI, machines.

pub mod recipes;
pub mod stations;

use bevy::{picking::pointer::PointerButton, prelude::*};

use crate::{
    core::{events::Notify, rules::ModeRules, states::InputFocus},
    items::{ItemId, ItemStack, inventory::Inventory},
    machinery::{
        components::{Part, PartKind},
        connections::OpenStation,
    },
    player::Player,
    tools::manipulator::MechanicsUse,
    ui::{inventory::CraftingDock, theme},
};
use recipes::{RECIPES, Station, machine_recipe};
use stations::{CrateStorage, Machine};

pub const WORKBENCH_RANGE: f32 = 4.0;

/// Station the inventory screen was opened from (if any).
#[derive(Resource, Default, Debug)]
pub struct StationContext(pub Option<Entity>);

#[derive(Component)]
struct RecipeRow(usize);

#[derive(Component)]
enum StationButton {
    Load,
    Collect,
}

pub fn workbench_near(player: Vec3, parts: &Query<(&Transform, &Part)>) -> bool {
    parts.iter().any(|(t, p)| {
        p.kind == PartKind::Workbench && t.translation.distance(player) < WORKBENCH_RANGE
    })
}

fn open_station(
    mut reader: MessageReader<OpenStation>,
    mut ctx: ResMut<StationContext>,
    mut next: ResMut<NextState<InputFocus>>,
) {
    for o in reader.read() {
        ctx.0 = Some(o.entity);
        next.set(InputFocus::Inventory);
    }
}

/// Mechanics click on a crate empties it into the inventory.
fn crate_collect(
    mut reader: MessageReader<MechanicsUse>,
    mut crates: Query<&mut CrateStorage>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let Ok(mut inv) = player.single_mut() else {
        return;
    };
    for u in reader.read() {
        let Ok(mut c) = crates.get_mut(u.entity) else {
            continue;
        };
        let mut n = 0;
        for slot in c.0.slots.iter_mut() {
            if let Some(s) = *slot {
                let left = inv.add(s.item, s.count);
                n += s.count - left;
                *slot = (left > 0).then(|| ItemStack::new(s.item, left));
            }
        }
        notify.write(Notify::ok(format!("Из ящика забрано предметов: {n}")));
    }
}

fn clear_context(mut ctx: ResMut<StationContext>) {
    ctx.0 = None;
}

#[allow(clippy::too_many_arguments)]
fn fill_dock(
    mut commands: Commands,
    docks: Query<Entity, Added<CraftingDock>>,
    ctx: Res<StationContext>,
    rules: Res<ModeRules>,
    player: Query<(&Transform, &Inventory), With<Player>>,
    parts: Query<(&Transform, &Part)>,
    machines: Query<&Machine>,
) {
    let Ok(dock) = docks.single() else { return };
    let Ok((pt, inv)) = player.single() else {
        return;
    };
    let bench = workbench_near(pt.translation, &parts) || !rules.consume_items;
    commands.entity(dock).with_children(|p| {
        if let Some(st) = ctx.0
            && let Ok(m) = machines.get(st)
        {
            p.spawn((
                Text::new("СТАНЦИЯ"),
                theme::font(16.0),
                TextColor(theme::ACCENT),
            ));
            let item = |s: Option<ItemStack>| {
                s.map(|s| format!("{} ×{}", s.item.def().name, s.count))
                    .unwrap_or("—".into())
            };
            p.spawn((
                Text::new(format!(
                    "Вход: {}\nВыход: {}\nТопливо: {}  ·  вал {:.1} рад/с",
                    item(m.input),
                    item(m.output),
                    m.fuel_ops,
                    m.drive_speed
                )),
                theme::font(12.0),
                TextColor(theme::TEXT),
            ));
            for (b, label) in [
                (StationButton::Load, "Загрузить сырьё / уголь"),
                (StationButton::Collect, "Забрать результат"),
            ] {
                p.spawn((
                    b,
                    Node {
                        padding: UiRect::all(px(4)),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BorderColor::all(theme::ACCENT),
                    BackgroundColor(theme::PANEL_BG),
                ))
                .observe(on_station_button)
                .with_children(|b| {
                    b.spawn((
                        Text::new(label),
                        theme::font(12.0),
                        TextColor(theme::TEXT),
                        Pickable::IGNORE,
                    ));
                });
            }
        }
        p.spawn((
            Text::new("СОЗДАНИЕ"),
            theme::font(16.0),
            TextColor(theme::ACCENT),
        ));
        p.spawn((
            Text::new(if bench {
                "Верстак рядом: доступны все рецепты"
            } else {
                "Нет верстака рядом: только ручные рецепты"
            }),
            theme::font(11.0),
            TextColor(theme::TEXT_DIM),
        ));
        for (i, r) in RECIPES.iter().enumerate() {
            if !matches!(r.station, Station::Hand | Station::Workbench) {
                continue;
            }
            let available = r.station == Station::Hand || bench;
            let afford = r.can_afford(inv) || !rules.consume_items;
            let color = if available && afford {
                theme::OK
            } else if available {
                theme::TEXT_DIM
            } else {
                Color::srgba(0.6, 0.6, 0.6, 0.4)
            };
            let inputs: Vec<String> = r
                .inputs
                .iter()
                .map(|(k, n)| format!("{} {n}", ItemId::k(k).def().name))
                .collect();
            p.spawn((
                RecipeRow(i),
                Node {
                    padding: UiRect::all(px(3)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BorderColor::all(theme::PANEL_BORDER),
            ))
            .observe(on_recipe_click)
            .with_children(|row| {
                row.spawn((
                    Text::new(format!(
                        "{} ×{}  ←  {}  [{}]",
                        r.output_item().def().name,
                        r.output.1,
                        inputs.join(", "),
                        r.station.label()
                    )),
                    theme::font(11.0),
                    TextColor(color),
                    Pickable::IGNORE,
                ));
            });
        }
        p.spawn((
            Text::new("Плавильня и дробилка: Механизмы-режим ЛКМ по станции."),
            theme::font(11.0),
            TextColor(theme::TEXT_DIM),
        ));
    });
}

fn on_recipe_click(
    ev: On<PointerClick>,
    rows: Query<&RecipeRow>,
    rules: Res<ModeRules>,
    mut player: Query<(&Transform, &mut Inventory), With<Player>>,
    parts: Query<(&Transform, &Part), Without<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    if ev.button != PointerButton::Primary {
        return;
    }
    let Ok(RecipeRow(i)) = rows.get(ev.event_target()) else {
        return;
    };
    let r = &RECIPES[*i];
    let Ok((pt, mut inv)) = player.single_mut() else {
        return;
    };
    let bench = parts.iter().any(|(t, p)| {
        p.kind == PartKind::Workbench && t.translation.distance(pt.translation) < WORKBENCH_RANGE
    });
    if r.station == Station::Workbench && !bench && rules.consume_items {
        notify.write(Notify::warn("Нужен верстак рядом"));
        return;
    }
    let ok = if rules.consume_items {
        r.craft(&mut inv)
    } else {
        inv.add(r.output_item(), r.output.1) == 0
    };
    if ok {
        notify.write(Notify::ok(format!(
            "Создано: {} ×{}",
            r.output_item().def().name,
            r.output.1
        )));
    } else {
        notify.write(Notify::warn("Не хватает материалов или места"));
    }
}

fn on_station_button(
    ev: On<PointerClick>,
    buttons: Query<&StationButton>,
    ctx: Res<StationContext>,
    mut machines: Query<(&Part, &mut Machine)>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let Ok(b) = buttons.get(ev.event_target()) else {
        return;
    };
    let Some(st) = ctx.0 else { return };
    let Ok((part, mut m)) = machines.get_mut(st) else {
        return;
    };
    let Ok(mut inv) = player.single_mut() else {
        return;
    };
    match b {
        StationButton::Load => load_station(part.kind, &mut m, &mut inv, &mut notify),
        StationButton::Collect => {
            if let Some(o) = m.output.take() {
                let left = inv.add(o.item, o.count);
                m.output = (left > 0).then(|| ItemStack::new(o.item, left));
            }
        }
    }
}

/// Moves one suitable input (and coal for a smelter) from the inventory.
pub fn load_station(
    kind: PartKind,
    m: &mut Machine,
    inv: &mut Inventory,
    notify: &mut MessageWriter<Notify>,
) {
    let station = if kind == PartKind::Smelter {
        Station::Smelter
    } else {
        Station::Grinder
    };
    if m.input.is_none()
        && let Some(item) = inv
            .slots
            .iter()
            .flatten()
            .map(|s| s.item)
            .find(|i| machine_recipe(station, *i).is_some())
    {
        inv.remove(item, 1);
        m.input = Some(ItemStack::new(item, 1));
    }
    if station == Station::Smelter && m.fuel_ops == 0 && inv.remove(ItemId::k("coal"), 1) {
        m.fuel_ops += 4;
    }
    if m.input.is_none() {
        notify.write(Notify::warn("Нет подходящего сырья"));
    }
}

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StationContext>()
            .add_plugins(stations::StationsPlugin)
            .add_systems(Update, (open_station, crate_collect, fill_dock))
            .add_systems(OnExit(InputFocus::Inventory), clear_context);
    }
}
