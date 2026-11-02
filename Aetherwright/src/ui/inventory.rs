//! Inventory screen (I / Tab): backpack grid, hotbar row, the stack held by
//! the cursor, item tooltip and — in creative — a catalog of every item.
//! The crafting panel (`crafting` module) docks into `CraftingDock`.

use bevy::prelude::*;

use super::{
    slots::{CursorStack, SlotSource, SlotView, spawn_slot},
    theme,
};
use crate::{
    core::states::{GameMode, InputFocus, gameplay_active},
    items::{
        ItemStack, all_items,
        inventory::{HOTBAR, Inventory, SLOTS},
    },
    player::Player,
};

#[derive(Component)]
struct InventoryScreen;

#[derive(Component)]
struct CursorIcon;

#[derive(Component)]
struct Tooltip;

/// Container other modules (crafting) fill with their panel.
#[derive(Component)]
pub struct CraftingDock;

fn open_inventory(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<InputFocus>>) {
    if keys.just_pressed(KeyCode::KeyI) || keys.just_pressed(KeyCode::Tab) {
        next.set(InputFocus::Inventory);
    }
}

fn close_inventory(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<InputFocus>>) {
    if keys.just_pressed(KeyCode::KeyI)
        || keys.just_pressed(KeyCode::Tab)
        || keys.just_pressed(KeyCode::Escape)
    {
        next.set(InputFocus::Gameplay);
    }
}

fn spawn_screen(mut commands: Commands, mode: Res<State<GameMode>>) {
    let creative = *mode.get() == GameMode::Creative;
    commands
        .spawn((
            InventoryScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(16),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.04, 0.5)),
            GlobalZIndex(50),
        ))
        .with_children(|root| {
            // Left: crafting dock (filled by the crafting module).
            root.spawn((CraftingDock, theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(4), width: px(300), max_height: percent(85), overflow: Overflow::scroll_y(), ..default() })));
            // Centre: inventory.
            root.spawn(theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() })).with_children(|p| {
                p.spawn((Text::new("ИНВЕНТАРЬ"), theme::font(18.0), TextColor(theme::ACCENT)));
                for row in 0..(SLOTS - HOTBAR) / HOTBAR {
                    p.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
                        for c in 0..HOTBAR {
                            spawn_slot(r, SlotSource::Player(HOTBAR + row * HOTBAR + c), false);
                        }
                    });
                }
                p.spawn((Text::new("Быстрые слоты"), theme::font(13.0), TextColor(theme::TEXT_DIM)));
                p.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
                    for c in 0..HOTBAR {
                        spawn_slot(r, SlotSource::Player(c), true);
                    }
                });
                p.spawn((Tooltip, Text::new(""), theme::font(13.0), TextColor(theme::TEXT), Node { max_width: px(520), ..default() }));
                p.spawn((
                    Text::new("ЛКМ — взять/положить · ПКМ — половина/по одному · Shift+ЛКМ — переложить · I/Tab — закрыть"),
                    theme::font(12.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
            if creative {
                root.spawn(theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(4), max_width: px(310), ..default() })).with_children(|p| {
                    p.spawn((Text::new("КАТАЛОГ (креатив)"), theme::font(16.0), TextColor(theme::MAGIC)));
                    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|g| {
                        for k in 0..all_items().len() {
                            spawn_slot(g, SlotSource::Catalog(k), false);
                        }
                    });
                });
            }
        });
    commands.spawn((
        InventoryScreen,
        CursorIcon,
        Node {
            position_type: PositionType::Absolute,
            width: px(26),
            height: px(26),
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
        GlobalZIndex(60),
        Pickable::IGNORE,
    ));
}

/// Returns the carried stack to the inventory when the screen closes.
fn despawn_screen(
    mut commands: Commands,
    q: Query<Entity, With<InventoryScreen>>,
    mut cursor: ResMut<CursorStack>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut drops: MessageWriter<crate::items::drops::SpawnDrop>,
    eye: Res<crate::core::resources::EyeRay>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    if let Some(stack) = cursor.0.take()
        && let Ok(mut inv) = player.single_mut()
    {
        let left = inv.add(stack.item, stack.count);
        if left > 0 {
            drops.write(crate::items::drops::SpawnDrop {
                stack: ItemStack::new(stack.item, left),
                position: eye.origin + eye.direction,
                velocity: eye.direction * 3.0,
            });
        }
    }
}

fn update_cursor_and_tooltip(
    windows: Query<&Window>,
    cursor: Res<CursorStack>,
    mut icon: Query<(&mut Node, &mut BackgroundColor, &mut BorderColor), With<CursorIcon>>,
    hovered: Query<(&SlotView, &bevy::picking::hover::Hovered)>,
    player: Query<&Inventory, With<Player>>,
    mut tooltip: Query<&mut Text, With<Tooltip>>,
) {
    let Ok(window) = windows.single() else { return };
    if let Ok((mut node, mut bg, mut border)) = icon.single_mut() {
        if let Some(pos) = window.cursor_position() {
            node.left = px(pos.x + 8.0);
            node.top = px(pos.y + 8.0);
        }
        bg.0 = cursor.0.map(|s| s.item.color()).unwrap_or(Color::NONE);
        *border = if cursor.0.is_some() {
            BorderColor::all(theme::ACCENT)
        } else {
            BorderColor::all(Color::NONE)
        };
    }
    let Ok(inv) = player.single() else { return };
    let mut text = String::new();
    for (view, hovered) in &hovered {
        if !hovered.get() {
            continue;
        }
        let item = match view.source {
            SlotSource::Player(i) => inv.slots.get(i).copied().flatten().map(|s| s.item),
            SlotSource::Catalog(k) => Some(crate::items::ItemId(k as u16)),
        };
        if let Some(item) = item {
            let d = item.def();
            text = format!(
                "{} · {} · стопка {}\n{}",
                d.name,
                d.category.label(),
                d.max_stack,
                d.info
            );
        }
    }
    if let Some(stack) = cursor.0 {
        text = format!(
            "В руке: {} ×{}\n{}",
            stack.item.def().name,
            stack.count,
            text
        );
    }
    for mut t in &mut tooltip {
        if t.0 != text {
            t.0 = text.clone();
        }
    }
}

/// Adds `Hovered` tracking to slots (needed for tooltips).
fn track_hover(mut commands: Commands, added: Query<Entity, Added<SlotView>>) {
    for e in &added {
        commands
            .entity(e)
            .insert(bevy::picking::hover::Hovered::default());
    }
}

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, open_inventory.run_if(gameplay_active))
            .add_systems(
                Update,
                (close_inventory, update_cursor_and_tooltip)
                    .run_if(in_state(InputFocus::Inventory)),
            )
            .add_systems(Update, track_hover)
            .add_systems(OnEnter(InputFocus::Inventory), spawn_screen)
            .add_systems(OnExit(InputFocus::Inventory), despawn_screen);
    }
}
