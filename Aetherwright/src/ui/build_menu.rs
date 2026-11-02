//! Build menu (B): every buildable block and part with what the player has.
//! Clicking an entry puts it into the selected hotbar slot (survival: moved
//! from the backpack; creative: a fresh stack).

use bevy::{picking::pointer::PointerButton, prelude::*};

use super::theme;
use crate::{
    core::states::{GameMode, InputFocus, gameplay_active},
    items::{Category, ItemId, ItemStack, all_items, inventory::Inventory},
    player::Player,
};

#[derive(Component)]
struct BuildMenuScreen;

#[derive(Component)]
struct BuildEntry(ItemId);

#[derive(Component)]
struct BuildEntryText(ItemId);

fn buildable(item: ItemId) -> bool {
    matches!(
        item.def().category,
        Category::Block | Category::Part | Category::Station
    )
}

fn open(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<InputFocus>>) {
    if keys.just_pressed(KeyCode::KeyB) {
        next.set(InputFocus::BuildMenu);
    }
}

fn close(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<InputFocus>>) {
    if keys.just_pressed(KeyCode::KeyB) || keys.just_pressed(KeyCode::Escape) {
        next.set(InputFocus::Gameplay);
    }
}

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            BuildMenuScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.04, 0.45)),
            GlobalZIndex(50),
        ))
        .with_children(|root| {
            root.spawn(theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(6), width: px(760), ..default() }))
                .with_children(|p| {
                    p.spawn((Text::new("СТРОИТЕЛЬСТВО"), theme::font(20.0), TextColor(theme::ACCENT)));
                    p.spawn((
                        Text::new("Клик — в выбранный слот. Блоки ставятся по сетке, детали — свободно (R поворот, Shift+R наклон). Соединяйте детали гравизахватом."),
                        theme::font(13.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                    for (title, cats) in [
                        ("Архитектурные блоки", &[Category::Block][..]),
                        ("Функциональные детали", &[Category::Part][..]),
                        ("Станции", &[Category::Station][..]),
                    ] {
                        p.spawn((Text::new(title), theme::font(15.0), TextColor(theme::MAGIC)));
                        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|g| {
                            for (k, def) in all_items().iter().enumerate() {
                                let item = ItemId(k as u16);
                                if !cats.contains(&def.category) || !buildable(item) {
                                    continue;
                                }
                                g.spawn((
                                    BuildEntry(item),
                                    Node { width: px(176), padding: UiRect::all(px(5)), column_gap: px(6), border: UiRect::all(px(1)), align_items: AlignItems::Center, ..default() },
                                    BackgroundColor(theme::PANEL_BG),
                                    BorderColor::all(theme::PANEL_BORDER),
                                ))
                                .observe(on_click)
                                .with_children(|e| {
                                    e.spawn((Node { width: px(14), height: px(14), ..default() }, BackgroundColor(item.color()), Pickable::IGNORE));
                                    e.spawn((BuildEntryText(item), Text::new(def.name.clone()), theme::font(12.0), TextColor(theme::TEXT), Pickable::IGNORE));
                                });
                            }
                        });
                    }
                });
        });
}

fn on_click(
    ev: On<PointerClick>,
    entries: Query<&BuildEntry>,
    mode: Res<State<GameMode>>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut next: ResMut<NextState<InputFocus>>,
    mut notify: MessageWriter<crate::core::events::Notify>,
) {
    if ev.button != PointerButton::Primary {
        return;
    }
    let Ok(BuildEntry(item)) = entries.get(ev.event_target()) else {
        return;
    };
    let Ok(mut inv) = player.single_mut() else {
        return;
    };
    let sel = inv.selected;
    if *mode.get() == GameMode::Creative {
        let old = inv.slots[sel].replace(ItemStack::new(*item, item.def().max_stack));
        if let Some(old) = old {
            inv.add(old.item, old.count);
        }
    } else if let Some(i) = inv
        .slots
        .iter()
        .position(|s| s.is_some_and(|s| s.item == *item))
    {
        inv.slots.swap(i, sel);
    } else {
        notify.write(crate::core::events::Notify::warn(format!(
            "Нет в инвентаре: {} — создайте на верстаке",
            item.def().name
        )));
        return;
    }
    next.set(InputFocus::Gameplay);
}

fn refresh(
    player: Query<&Inventory, With<Player>>,
    mode: Res<State<GameMode>>,
    mut q: Query<(&BuildEntryText, &mut Text)>,
) {
    let Ok(inv) = player.single() else { return };
    let creative = *mode.get() == GameMode::Creative;
    for (BuildEntryText(item), mut t) in &mut q {
        let s = if creative {
            format!("{} ∞", item.def().name)
        } else {
            format!("{} ×{}", item.def().name, inv.count(*item))
        };
        if t.0 != s {
            t.0 = s;
        }
    }
}

fn despawn(mut commands: Commands, q: Query<Entity, With<BuildMenuScreen>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

pub struct BuildMenuPlugin;

impl Plugin for BuildMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, open.run_if(gameplay_active))
            .add_systems(
                Update,
                (close, refresh).run_if(in_state(InputFocus::BuildMenu)),
            )
            .add_systems(OnEnter(InputFocus::BuildMenu), spawn)
            .add_systems(OnExit(InputFocus::BuildMenu), despawn);
    }
}
