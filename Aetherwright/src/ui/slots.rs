//! Item slot widget shared by the hotbar and the inventory screen.
//! Visuals are refreshed from the data every frame (cheap: < 100 slots);
//! clicks are handled by picking observers and only work while the
//! inventory screen owns the cursor.

use bevy::{picking::pointer::PointerButton, prelude::*};

use super::theme;
use crate::{
    core::states::InputFocus,
    items::{ItemId, ItemStack, all_items, inventory::Inventory},
    player::Player,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSource {
    /// Player inventory slot index.
    Player(usize),
    /// Creative catalog entry (infinite source).
    Catalog(usize),
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SlotView {
    pub source: SlotSource,
    pub hotbar: bool,
}

#[derive(Component)]
struct SlotIcon;

#[derive(Component)]
struct SlotLabel;

#[derive(Component)]
struct SlotCount;

/// The stack carried by the mouse cursor in the inventory screen.
#[derive(Resource, Default, Debug)]
pub struct CursorStack(pub Option<ItemStack>);

fn short_name(item: ItemId) -> String {
    let name = item.def().name.as_str();
    let first = name.split_whitespace().next().unwrap_or(name);
    first.chars().take(7).collect()
}

pub fn spawn_slot(parent: &mut ChildSpawnerCommands, source: SlotSource, hotbar: bool) {
    parent
        .spawn((
            SlotView { source, hotbar },
            Node {
                width: px(56),
                height: px(56),
                border: UiRect::all(px(1)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(3)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
            BorderColor::all(theme::PANEL_BORDER),
        ))
        .observe(on_slot_click)
        .with_children(|s| {
            s.spawn((
                SlotIcon,
                Node {
                    width: px(30),
                    height: px(30),
                    ..default()
                },
                ImageNode { color: Color::NONE, ..default() },
                BackgroundColor(Color::NONE),
                Pickable::IGNORE,
            ));
            s.spawn((
                SlotLabel,
                Text::new(""),
                theme::font(10.0),
                TextColor(theme::TEXT),
                Pickable::IGNORE,
            ));
            s.spawn((
                SlotCount,
                Text::new(""),
                theme::font(11.0),
                TextColor(theme::ACCENT),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(3),
                    top: px(1),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });
}

fn on_slot_click(
    ev: On<PointerClick>,
    views: Query<&SlotView>,
    focus: Res<State<InputFocus>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<CursorStack>,
    mut player: Query<&mut Inventory, With<Player>>,
) {
    if *focus.get() != InputFocus::Inventory {
        return;
    }
    let Ok(view) = views.get(ev.event_target()) else {
        return;
    };
    let Ok(mut inv) = player.single_mut() else {
        return;
    };
    match view.source {
        SlotSource::Player(i) => {
            let shift = keys.pressed(KeyCode::ShiftLeft);
            match ev.button {
                PointerButton::Primary if shift && cursor.0.is_none() => {
                    // Quick move between hotbar and backpack.
                    if let Some(stack) = inv.slots[i].take() {
                        let range: Vec<usize> = if i < crate::items::inventory::HOTBAR {
                            (crate::items::inventory::HOTBAR..inv.slots.len()).collect()
                        } else {
                            (0..crate::items::inventory::HOTBAR).collect()
                        };
                        let mut left = Some(stack);
                        for j in range {
                            if left.is_none() {
                                break;
                            }
                            if inv.slots[j].is_none_or(|s| s.item == stack.item) {
                                inv.click_slot(j, &mut left);
                            }
                        }
                        inv.slots[i] = left;
                    }
                }
                PointerButton::Primary => inv.click_slot(i, &mut cursor.0),
                PointerButton::Secondary => inv.right_click_slot(i, &mut cursor.0),
                _ => {}
            }
        }
        SlotSource::Catalog(k) => {
            let item = ItemId(k as u16);
            let n = if ev.button == PointerButton::Secondary {
                1
            } else {
                item.def().max_stack
            };
            cursor.0 = Some(ItemStack::new(item, n));
        }
    }
}

/// Shows an item's pixel icon (or its colour before icons exist).
pub fn set_icon(item: Option<ItemId>, bg: &mut BackgroundColor, img: &mut ImageNode) {
    let pic = item.and_then(crate::items::icons::icon);
    let (want_bg, want_col) = match (item, &pic) {
        (Some(_), Some(_)) => (Color::NONE, Color::WHITE),
        (Some(i), None) => (i.color(), Color::NONE),
        (None, _) => (Color::NONE, Color::NONE),
    };
    if bg.0 != want_bg {
        bg.0 = want_bg;
    }
    if img.color != want_col {
        img.color = want_col;
    }
    if let Some(h) = pic {
        if img.image != h {
            img.image = h;
        }
    }
}

fn refresh_slots(
    player: Query<&Inventory, With<Player>>,
    views: Query<(&SlotView, &Children, &mut BorderColor, &mut BackgroundColor)>,
    mut icons: Query<(&mut BackgroundColor, &mut ImageNode), (With<SlotIcon>, Without<SlotView>)>,
    mut labels: Query<&mut Text, (With<SlotLabel>, Without<SlotCount>)>,
    mut counts: Query<&mut Text, (With<SlotCount>, Without<SlotLabel>)>,
) {
    let Ok(inv) = player.single() else { return };
    for (view, children, mut border, mut bg) in views {
        let stack = match view.source {
            SlotSource::Player(i) => inv.slots.get(i).copied().flatten(),
            SlotSource::Catalog(k) => {
                (k < all_items().len()).then(|| ItemStack::new(ItemId(k as u16), 0))
            }
        };
        let selected =
            view.hotbar && matches!(view.source, SlotSource::Player(i) if i == inv.selected);
        let want_border = if selected {
            BorderColor::all(theme::ACCENT)
        } else {
            BorderColor::all(theme::PANEL_BORDER)
        };
        if *border != want_border {
            *border = want_border;
        }
        bg.0 = if selected {
            Color::srgba(0.1, 0.3, 0.4, 0.7)
        } else {
            theme::PANEL_BG
        };
        for child in children.iter() {
            if let Ok((mut bg, mut img)) = icons.get_mut(child) {
                set_icon(stack.map(|s| s.item), &mut bg, &mut img);
            }
            if let Ok(mut t) = labels.get_mut(child) {
                let s = stack.map(|s| short_name(s.item)).unwrap_or_default();
                if t.0 != s {
                    t.0 = s;
                }
            }
            if let Ok(mut t) = counts.get_mut(child) {
                let s = match stack {
                    Some(s) if s.count > 1 => s.count.to_string(),
                    _ => String::new(),
                };
                if t.0 != s {
                    t.0 = s;
                }
            }
        }
    }
}

pub struct SlotsPlugin;

impl Plugin for SlotsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorStack>()
            .add_systems(Update, refresh_slots);
    }
}
