//! Interface, kept as simple as possible:
//! * HUD — crosshair, hotbar (9), held item name, toasts;
//! * inventory screen (E) — 6 × 9 inventory + hotbar; a button switches
//!   to the catalogue of all items (categories on the left, scrolling grid,
//!   search field at the bottom) and back. Minecraft-style clicks: LMB
//!   takes / puts / swaps a stack, RMB splits or puts one; clicking an item
//!   in the catalogue takes a full stack, Shift+click sends it to the
//!   hotbar; clicking the catalogue with a carried stack deletes it.

use bevy::{
    input::keyboard::{Key, KeyboardInput},
    picking::pointer::PointerButton,
    prelude::*,
};

use crate::{
    content::{ItemId, registry},
    core::{Toast, Ui},
    icons::ItemIcons,
    inventory::{HOTBAR, Inventory, Stack},
    player::Player,
};

const BG: Color = Color::srgba(0.08, 0.09, 0.11, 0.92);
const SLOT_BG: Color = Color::srgba(0.18, 0.19, 0.22, 0.95);
const SLOT_HOVER: Color = Color::srgba(0.28, 0.3, 0.34, 0.95);
const BORDER: Color = Color::srgb(0.32, 0.33, 0.37);
/// Window frames: forged steel with brass rivets in the corners.
pub const FRAME: Color = Color::srgb(0.227, 0.243, 0.278);
pub const RIVET: Color = Color::srgb(0.831, 0.659, 0.31);
const RIVET_DARK: Color = Color::srgb(0.45, 0.32, 0.12);
const ACCENT: Color = Color::srgb(0.95, 0.76, 0.33);
const TEXT: Color = Color::srgb(0.92, 0.92, 0.9);
const DIM: Color = Color::srgb(0.6, 0.6, 0.62);

/// Brass rivets in the four corners of a framed window.
pub fn rivets(p: &mut ChildSpawnerCommands) {
    for (l, t) in [(true, true), (false, true), (true, false), (false, false)] {
        let mut n = Node { position_type: PositionType::Absolute, width: px(8), height: px(8), border: UiRect::all(px(1)), ..default() };
        if l { n.left = px(-5) } else { n.right = px(-5) }
        if t { n.top = px(-5) } else { n.bottom = px(-5) }
        p.spawn((n, BackgroundColor(RIVET), BorderColor::all(RIVET_DARK), Pickable::IGNORE));
    }
}

pub fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

/// The UI font (embedded: the Bevy default font has no Cyrillic).
#[derive(Resource, Clone)]
pub struct UiFont {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
}

fn load_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let regular = fonts.add(Font::from_bytes(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").to_vec()));
    let bold = fonts.add(Font::from_bytes(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf").to_vec()));
    commands.insert_resource(UiFont { regular, bold });
}

/// Every new text gets the UI font (bold for titles).
fn apply_font(font: Option<Res<UiFont>>, mut q: Query<&mut TextFont, Added<TextFont>>) {
    let Some(font) = font else { return };
    for mut t in &mut q {
        let big = matches!(t.font_size, bevy::text::FontSize::Px(px) if px >= 18.0);
        t.font = bevy::text::FontSource::Handle(if big { font.bold.clone() } else { font.regular.clone() });
    }
}

/// Where a slot widget gets its stack from.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSrc {
    Inv(usize),
    Catalog(ItemId),
}

#[derive(Component)]
struct SlotIcon;
#[derive(Component)]
struct SlotCount;
#[derive(Component)]
struct HotbarSlot(usize);
#[derive(Component)]
struct HeldName;
#[derive(Component)]
struct ToastText(f32);
#[derive(Component)]
struct ToastPlate;

/// The stack on the mouse cursor (inventory screen).
#[derive(Resource, Default)]
pub struct Carried(pub Option<Stack>);

/// Catalogue view state.
#[derive(Resource, Default)]
pub struct Catalog {
    pub open: bool,
    pub category: Option<String>,
    pub search: String,
    pub typing: bool,
    pub hovered: Option<ItemId>,
    dirty: bool,
}

impl Catalog {
    /// Rebuild the catalogue view next frame.
    pub fn dirty_now(&mut self) {
        self.dirty = true;
    }
}

fn slot_widget(p: &mut ChildSpawnerCommands, src: SlotSrc, size: f32) {
    p.spawn((
        src,
        Node { width: px(size), height: px(size), border: UiRect::all(px(2)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(SLOT_BG),
        BorderColor::all(BORDER),
    ))
    .observe(on_slot_click)
    .observe(|ev: On<PointerOver>, mut q: Query<(&SlotSrc, &mut BackgroundColor)>, mut cat: ResMut<Catalog>, inv: Query<&Inventory, With<Player>>| {
        if let Ok((src, mut bg)) = q.get_mut(ev.entity) {
            bg.0 = SLOT_HOVER;
            cat.hovered = match *src {
                SlotSrc::Catalog(i) => Some(i),
                SlotSrc::Inv(i) => inv.single().ok().and_then(|v| v.slots[i]).map(|s| s.item),
            };
        }
    })
    .observe(|ev: On<PointerOut>, mut q: Query<&mut BackgroundColor, With<SlotSrc>>, mut cat: ResMut<Catalog>| {
        if let Ok(mut bg) = q.get_mut(ev.entity) {
            bg.0 = SLOT_BG;
            cat.hovered = None;
        }
    })
    .with_children(|s| {
        s.spawn((SlotIcon, ImageNode { color: Color::NONE, ..default() }, Node { width: px(size - 8.0), height: px(size - 8.0), ..default() }, Pickable::IGNORE));
        s.spawn((SlotCount, Text::new(""), font(13.0), TextColor(TEXT), Node { position_type: PositionType::Absolute, right: px(3), bottom: px(1), ..default() }, Pickable::IGNORE));
    });
}

fn on_slot_click(ev: On<PointerClick>, srcs: Query<&SlotSrc>, keys: Res<ButtonInput<KeyCode>>, mut carried: ResMut<Carried>, mut inv: Query<&mut Inventory, With<Player>>) {
    let Ok(src) = srcs.get(ev.entity) else { return };
    let Ok(mut inv) = inv.single_mut() else { return };
    let right = ev.button == PointerButton::Secondary;
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    match *src {
        SlotSrc::Inv(i) => {
            if shift && carried.0.is_none() {
                // Shift+click: wear it / take it off; hotbar ⇄ inventory.
                let wear = inv.slots[i].and_then(|s| crate::inventory::equip_slot(s.item)).map(|k| crate::inventory::EQUIP_START + k).filter(|j| *j != i && inv.slots[*j].is_none());
                let range = if let Some(j) = wear {
                    j..j + 1
                } else if i >= crate::inventory::EQUIP_START {
                    0..crate::inventory::SLOTS
                } else if i < HOTBAR {
                    HOTBAR..crate::inventory::SLOTS
                } else {
                    0..HOTBAR
                };
                if let Some(s) = inv.slots[i]
                    && let Some(j) = range.into_iter().find(|j| inv.slots[*j].is_none())
                {
                    inv.slots[j] = Some(s);
                    inv.slots[i] = None;
                }
            } else {
                inv.click(i, &mut carried.0, right);
            }
        }
        SlotSrc::Catalog(item) => {
            if carried.0.is_some() {
                carried.0 = None;
            } else if shift {
                let s = Stack::full(item);
                if let Some(j) = (0..HOTBAR).find(|j| inv.slots[*j].is_none()) {
                    inv.slots[j] = Some(s);
                } else {
                    let sel = inv.selected;
                    inv.slots[sel] = Some(s);
                }
            } else {
                carried.0 = Some(Stack::full(item));
            }
        }
    }
}

fn icon(icons: &ItemIcons, item: ItemId) -> ImageNode {
    ImageNode { image: icons.image.clone(), rect: Some(icons.rect(item.0 as usize)), ..default() }
}

/// Fill slot widgets from the data.
fn refresh_slots(
    inv: Query<&Inventory, With<Player>>,
    icons: Option<Res<ItemIcons>>,
    slots: Query<(&SlotSrc, &Children, Option<&HotbarSlot>, &mut BorderColor)>,
    mut images: Query<&mut ImageNode, With<SlotIcon>>,
    mut counts: Query<&mut Text, With<SlotCount>>,
) {
    let (Ok(inv), Some(icons)) = (inv.single(), icons) else { return };
    for (src, children, hot, mut border) in slots {
        let stack = match *src {
            SlotSrc::Inv(i) => inv.slots.get(i).copied().flatten(),
            SlotSrc::Catalog(item) => Some(Stack { item, count: 1 }),
        };
        if let Some(h) = hot {
            let want = BorderColor::all(if h.0 == inv.selected { ACCENT } else { BORDER });
            if *border != want {
                *border = want;
            }
        }
        for c in children.iter() {
            if let Ok(mut img) = images.get_mut(c) {
                match stack {
                    Some(s) => {
                        let want = icon(&icons, s.item);
                        if img.rect != want.rect || img.color != Color::WHITE {
                            *img = want;
                        }
                    }
                    None if img.color != Color::NONE => img.color = Color::NONE,
                    None => {}
                }
            }
            if let Ok(mut t) = counts.get_mut(c) {
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

fn spawn_hud(mut commands: Commands) {
    // Crosshair.
    commands.spawn((Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(2), height: px(16), margin: UiRect::new(px(-1), px(0), px(-8), px(0)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)), Pickable::IGNORE));
    commands.spawn((Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(16), height: px(2), margin: UiRect::new(px(-8), px(0), px(-1), px(0)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)), Pickable::IGNORE));
    // Hotbar + held item name.
    commands
        .spawn((Node { position_type: PositionType::Absolute, bottom: px(8), width: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(6), ..default() }, Pickable::IGNORE))
        .with_children(|c| {
            c.spawn((HeldName, Text::new(""), font(16.0), TextColor(TEXT), TextShadow::default(), Pickable::IGNORE));
            c.spawn((Node { column_gap: px(2), padding: UiRect::all(px(3)), border: UiRect::all(px(2)), ..default() }, BackgroundColor(BG), BorderColor::all(FRAME), Pickable::IGNORE)).with_children(|row| {
                for i in 0..HOTBAR {
                    row.spawn((HotbarSlot(i), SlotSrc::Inv(i), Node { width: px(52), height: px(52), border: UiRect::all(px(2)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(SLOT_BG), BorderColor::all(BORDER), Pickable::IGNORE))
                        .with_children(|s| {
                            s.spawn((SlotIcon, ImageNode { color: Color::NONE, ..default() }, Node { width: px(44), height: px(44), ..default() }, Pickable::IGNORE));
                            s.spawn((SlotCount, Text::new(""), font(13.0), TextColor(TEXT), Node { position_type: PositionType::Absolute, right: px(3), bottom: px(1), ..default() }, Pickable::IGNORE));
                        });
                }
            });
        });
    // Toasts.
    // Toasts sit on a dark plate so they read against a bright sky.
    commands
        .spawn((Node { position_type: PositionType::Absolute, top: px(54), width: percent(100), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE))
        .with_children(|c| {
            c.spawn((ToastPlate, Node { padding: UiRect::axes(px(14), px(6)), border: UiRect::all(px(2)), ..default() }, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE), Pickable::IGNORE)).with_children(|p| {
                p.spawn((ToastText(0.0), Text::new(""), font(17.0), TextColor(TEXT), TextLayout::justify(Justify::Center), Pickable::IGNORE));
            });
        });
}

fn held_name(inv: Query<&Inventory, With<Player>>, shape: Res<crate::interact::ShapeMode>, mut q: Query<&mut Text, With<HeldName>>) {
    let (Ok(inv), Ok(mut t)) = (inv.single(), q.single_mut()) else { return };
    let s = inv
        .held()
        .map(|s| {
            let name = s.item.def().name.clone();
            // A building block shows its shape (Shift + wheel).
            match s.item.block() {
                Some(b) if shape.0 > 0 && crate::interact::shapeable(b) => format!("{name} — {}", crate::content::SHAPES[shape.0].2),
                _ => name,
            }
        })
        .unwrap_or_default();
    if t.0 != s {
        t.0 = s;
    }
}

fn toasts(time: Res<Time>, mut msgs: MessageReader<Toast>, mut q: Query<(&mut Text, &mut ToastText, &mut TextColor)>, mut plate: Query<(&mut BackgroundColor, &mut BorderColor), With<ToastPlate>>) {
    let Ok((mut t, mut life, mut col)) = q.single_mut() else { return };
    if let Some(m) = msgs.read().last() {
        t.0 = m.0.clone();
        life.0 = 3.0;
    }
    life.0 -= time.delta_secs();
    let a = life.0.clamp(0.0, 1.0);
    col.0 = TEXT.with_alpha(a);
    if let Ok((mut bg, mut border)) = plate.single_mut() {
        bg.0 = BG.with_alpha(0.85 * a);
        *border = BorderColor::all(FRAME.with_alpha(a));
    }
}

// ------------------------------------------------------------ inventory screen

#[derive(Component)]
struct Screen;
#[derive(Component)]
struct InvView;
#[derive(Component)]
struct CatView;
#[derive(Component)]
struct CatGrid;
#[derive(Component)]
struct SearchText;
#[derive(Component)]
struct CursorIcon;
#[derive(Component)]
struct HoverName;
#[derive(Component)]
struct ToggleLabel;

fn toggle_inventory(keys: Res<ButtonInput<KeyCode>>, ui: Res<State<Ui>>, mut next: ResMut<NextState<Ui>>, cat: Res<Catalog>, held: Res<crate::tools::guns::Held>, s: Res<crate::menu::GameSettings>) {
    // E rotates an object held by the physgun.
    if !keys.just_pressed(s.key(crate::menu::Action::Inventory)) || held.body.is_some() {
        return;
    }
    match ui.get() {
        Ui::Game => next.set(Ui::Inventory),
        Ui::Inventory if !cat.typing => next.set(Ui::Game),
        _ => {}
    }
}

fn button(p: &mut ChildSpawnerCommands, label: &str, marker: impl Bundle) -> Entity {
    p.spawn((Node { padding: UiRect::axes(px(12), px(6)), border: UiRect::all(px(2)), ..default() }, BackgroundColor(SLOT_BG), BorderColor::all(BORDER), marker))
        .with_children(|b| {
            b.spawn((Text::new(label), font(15.0), TextColor(TEXT), Pickable::IGNORE));
        })
        .id()
}

fn open_screen(mut commands: Commands, mut cat: ResMut<Catalog>) {
    cat.dirty = true;
    cat.typing = false;
    commands
        .spawn((Screen, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35))))
        .observe(|_: On<PointerClick>, mut carried: ResMut<Carried>, mut cat: ResMut<Catalog>| {
            // Clicking empty space: stop typing; creative drops vanish.
            cat.typing = false;
            let _ = &mut carried;
        })
        .with_children(|root| {
            root.spawn((Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(14)), row_gap: px(10), border: UiRect::all(px(2)), ..default() }, BackgroundColor(BG), BorderColor::all(FRAME))).with_children(|panel| {
                rivets(panel);
                // Header: title of the hovered item + view switch.
                panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: px(20), ..default() }).with_children(|h| {
                    h.spawn((HoverName, Text::new("Инвентарь"), font(18.0), TextColor(ACCENT)));
                    let b = button(h, "Все предметы", ());
                    h.commands().entity(b).insert(ToggleLabel).observe(|_: On<PointerClick>, mut cat: ResMut<Catalog>| {
                        cat.open = !cat.open;
                        cat.dirty = true;
                    });
                });
                // Inventory view: equipment (head, body, legs, feet) | 6 × 9.
                panel.spawn((InvView, Node { column_gap: px(12), ..default() })).with_children(|v| {
                    v.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|eq| {
                        for (k, name) in crate::inventory::EQUIP_NAMES.iter().enumerate() {
                            slot_widget(eq, SlotSrc::Inv(crate::inventory::EQUIP_START + k), 48.0);
                            eq.spawn((Text::new(*name), font(11.0), TextColor(DIM), Pickable::IGNORE));
                        }
                    });
                    v.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|g| {
                        for r in 0..crate::inventory::ROWS {
                            g.spawn(Node { column_gap: px(2), ..default() }).with_children(|row| {
                                for c in 0..9 {
                                    slot_widget(row, SlotSrc::Inv(HOTBAR + r * 9 + c), 48.0);
                                }
                            });
                        }
                    });
                });
                // Catalogue view: categories | grid, search below.
                panel.spawn((CatView, Node { flex_direction: FlexDirection::Column, row_gap: px(6), display: Display::None, ..default() })).with_children(|v| {
                    v.spawn(Node { column_gap: px(10), ..default() }).with_children(|r| {
                        r.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), width: px(150), ..default() }).with_children(|cats| {
                            let mut list = vec![(None, "Все".to_string())];
                            list.extend(registry().categories.iter().map(|c| (Some(c.key.clone()), c.name.clone())));
                            for (key, name) in list {
                                let b = button(cats, &name, ());
                                cats.commands().entity(b).observe(move |_: On<PointerClick>, mut cat: ResMut<Catalog>| {
                                    cat.category = key.clone();
                                    cat.dirty = true;
                                });
                            }
                        });
                        r.spawn((CatGrid, Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_content: AlignContent::FlexStart, width: px(9.0 * 50.0 + 8.0), height: px(6.0 * 50.0), column_gap: px(2), row_gap: px(2), overflow: Overflow::scroll_y(), ..default() }, ScrollPosition::default()))
                            .observe(|ev: On<PointerScroll>, mut q: Query<(&mut ScrollPosition, &ComputedNode)>| {
                                if let Ok((mut pos, node)) = q.get_mut(ev.entity) {
                                    let step = match ev.unit {
                                        bevy::input::mouse::MouseScrollUnit::Line => ev.y * 50.0,
                                        bevy::input::mouse::MouseScrollUnit::Pixel => ev.y,
                                    };
                                    let max = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
                                    pos.0.y = (pos.0.y - step).clamp(0.0, max);
                                }
                            });
                    });
                    // Search field.
                    v.spawn((Node { padding: UiRect::axes(px(10), px(6)), border: UiRect::all(px(2)), ..default() }, BackgroundColor(SLOT_BG), BorderColor::all(BORDER)))
                        .observe(|mut ev: On<PointerClick>, mut cat: ResMut<Catalog>| {
                            cat.typing = true;
                            ev.propagate(false);
                        })
                        .with_children(|s| {
                            s.spawn((SearchText, Text::new(""), font(15.0), TextColor(DIM), Pickable::IGNORE));
                        });
                });
                // Hotbar row (both views).
                panel.spawn(Node { column_gap: px(2), margin: UiRect::top(px(6)), ..default() }).with_children(|row| {
                    for i in 0..HOTBAR {
                        slot_widget(row, SlotSrc::Inv(i), 48.0);
                    }
                });
            });
        });
    commands.spawn((CursorIcon, ImageNode { color: Color::NONE, ..default() }, Node { position_type: PositionType::Absolute, width: px(40), height: px(40), ..default() }, GlobalZIndex(50), Pickable::IGNORE));
    // Item tooltip (follows the cursor).
    commands
        .spawn((Tooltip, Node { position_type: PositionType::Absolute, flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::axes(px(10), px(7)), border: UiRect::all(px(2)), max_width: px(340), display: Display::None, ..default() }, BackgroundColor(Color::srgba(0.06, 0.07, 0.09, 0.96)), BorderColor::all(FRAME), GlobalZIndex(60), Pickable::IGNORE))
        .with_children(|t| {
            t.spawn((TooltipTitle, Text::new(""), font(16.0), TextColor(ACCENT), Pickable::IGNORE));
            t.spawn((TooltipBody, Text::new(""), font(14.0), TextColor(TEXT), Pickable::IGNORE));
            t.spawn((TooltipHint, Text::new(""), font(12.0), TextColor(DIM), Pickable::IGNORE));
        });
}

fn close_screen(mut commands: Commands, q: Query<Entity, Or<(With<Screen>, With<CursorIcon>, With<Tooltip>)>>, mut carried: ResMut<Carried>, mut inv: Query<&mut Inventory, With<Player>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
    if let (Some(s), Ok(mut inv)) = (carried.0.take(), inv.single_mut()) {
        inv.add(s);
    }
}

fn matches(item: ItemId, cat: &Catalog) -> bool {
    let d = item.def();
    !d.hidden && cat.category.as_ref().is_none_or(|c| *c == d.category) && (cat.search.is_empty() || d.name.to_lowercase().contains(&cat.search.to_lowercase()) || d.key.contains(&cat.search.to_lowercase()))
}

#[allow(clippy::too_many_arguments)]
fn update_screen(
    mut commands: Commands,
    mut cat: ResMut<Catalog>,
    mut views: ParamSet<(Query<&mut Node, With<InvView>>, Query<&mut Node, With<CatView>>)>,
    grid: Query<Entity, With<CatGrid>>,
    mut labels: Query<&Children, With<ToggleLabel>>,
    mut texts: ParamSet<(Query<&mut Text>, Query<(&mut Text, &mut TextColor), With<SearchText>>, Query<&mut Text, With<HoverName>>)>,
) {
    if cat.dirty {
        cat.dirty = false;
        let open = cat.open;
        for mut n in views.p0().iter_mut() {
            n.display = if open { Display::None } else { Display::Flex };
        }
        for mut n in views.p1().iter_mut() {
            n.display = if open { Display::Flex } else { Display::None };
        }
        if let Ok(children) = labels.single_mut() {
            for c in children.iter() {
                if let Ok(mut t) = texts.p0().get_mut(c) {
                    t.0 = if open { "Инвентарь".into() } else { "Все предметы".into() };
                }
            }
        }
        if let Ok(g) = grid.single() {
            commands.entity(g).despawn_children();
            let items: Vec<ItemId> = (0..registry().items.len() as u16).map(ItemId).filter(|i| matches(*i, &cat)).collect();
            commands.entity(g).with_children(|p| {
                for i in items {
                    slot_widget(p, SlotSrc::Catalog(i), 48.0);
                }
            });
        }
    }
    let hovered = cat.hovered.map(|i| i.def().name.clone()).unwrap_or_else(|| if cat.open { "Все предметы".into() } else { "Инвентарь".into() });
    if let Ok(mut t) = texts.p2().single_mut()
        && t.0 != hovered
    {
        t.0 = hovered;
    }
    if let Ok((mut t, mut c)) = texts.p1().single_mut() {
        let s = if cat.search.is_empty() && !cat.typing { "Поиск…".to_string() } else { format!("{}{}", cat.search, if cat.typing { "▏" } else { "" }) };
        if t.0 != s {
            t.0 = s;
        }
        c.0 = if cat.typing || !cat.search.is_empty() { TEXT } else { DIM };
    }
}

fn type_search(mut keys: MessageReader<KeyboardInput>, mut cat: ResMut<Catalog>) {
    if !cat.typing || !cat.open {
        keys.clear();
        return;
    }
    let mut changed = false;
    for k in keys.read() {
        if !k.state.is_pressed() {
            continue;
        }
        match &k.logical_key {
            Key::Character(s) => {
                cat.search.push_str(s);
                changed = true;
            }
            Key::Space => {
                cat.search.push(' ');
                changed = true;
            }
            Key::Backspace => {
                cat.search.pop();
                changed = true;
            }
            Key::Enter => cat.typing = false,
            _ => {}
        }
    }
    if changed {
        cat.dirty = true;
    }
}

#[derive(Component)]
struct Tooltip;
#[derive(Component)]
struct TooltipTitle;
#[derive(Component)]
struct TooltipBody;
#[derive(Component)]
struct TooltipHint;

/// Text of an item's tooltip: (title, body, hint). Shift: the long text.
pub fn tooltip_text(item: ItemId, long: bool, shape: usize) -> (String, String, String) {
    let def = item.def();
    let title = match item.block() {
        Some(b) if shape > 0 && crate::interact::shapeable(b) => format!("{} — {}", def.name, crate::content::SHAPES[shape].2),
        _ => def.name.clone(),
    };
    let (short, full) = registry().describe(item).cloned().unwrap_or_default();
    let mut hint = Vec::new();
    if !long && full != short {
        hint.push("Shift — подробнее");
    }
    if crate::ponder::has_scene(item) {
        hint.push("удерживайте W — как работает");
    }
    (title, if long { full } else { short }, hint.join(" · "))
}

#[allow(clippy::type_complexity)]
fn tooltip(
    windows: Query<&Window>,
    cat: Res<Catalog>,
    carried: Res<Carried>,
    keys: Res<ButtonInput<KeyCode>>,
    shape: Res<crate::interact::ShapeMode>,
    mut tip: Query<&mut Node, With<Tooltip>>,
    mut texts: ParamSet<(Query<&mut Text, With<TooltipTitle>>, Query<&mut Text, With<TooltipBody>>, Query<&mut Text, With<TooltipHint>>)>,
) {
    let (Ok(w), Ok(mut n)) = (windows.single(), tip.single_mut()) else { return };
    let Some(item) = cat.hovered.filter(|_| carried.0.is_none() && !cat.typing) else {
        n.display = Display::None;
        return;
    };
    let long = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let (title, body, hint) = tooltip_text(item, long, shape.0);
    n.display = Display::Flex;
    if let Some(p) = w.cursor_position() {
        // Right of the cursor; to the left near the right edge.
        let x = if p.x > w.width() - 360.0 { p.x - 350.0 } else { p.x + 18.0 };
        n.left = px(x);
        n.top = px((p.y + 14.0).min(w.height() - 160.0));
    }
    for (i, s) in [title, body, hint].into_iter().enumerate() {
        let set = |t: &mut Text| {
            if t.0 != s {
                t.0 = s.clone();
            }
        };
        match i {
            0 => texts.p0().iter_mut().for_each(|mut t| set(&mut t)),
            1 => texts.p1().iter_mut().for_each(|mut t| set(&mut t)),
            _ => texts.p2().iter_mut().for_each(|mut t| set(&mut t)),
        }
    }
}

fn cursor_icon(windows: Query<&Window>, carried: Res<Carried>, icons: Option<Res<ItemIcons>>, mut q: Query<(&mut Node, &mut ImageNode), With<CursorIcon>>) {
    let (Ok(w), Ok((mut n, mut img)), Some(icons)) = (windows.single(), q.single_mut(), icons) else { return };
    if let Some(p) = w.cursor_position() {
        n.left = px(p.x - 20.0);
        n.top = px(p.y - 20.0);
    }
    match carried.0 {
        Some(s) => *img = icon(&icons, s.item),
        None => img.color = Color::NONE,
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Carried>()
            .init_resource::<Catalog>()
            .add_systems(PreStartup, load_fonts)
            .add_systems(Startup, spawn_hud)
            .add_systems(PostUpdate, apply_font)
            .add_systems(Update, (toggle_inventory, refresh_slots, held_name, toasts))
            .add_systems(OnEnter(Ui::Inventory), open_screen)
            .add_systems(OnExit(Ui::Inventory), close_screen)
            .add_systems(Update, (type_search, update_screen, cursor_icon, tooltip).chain().run_if(in_state(Ui::Inventory)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_filters() {
        let mut cat = Catalog::default();
        let all = (0..registry().items.len() as u16).filter(|i| matches(ItemId(*i), &cat)).count();
        assert_eq!(all, registry().items.iter().filter(|i| !i.hidden).count());
        cat.category = Some("tools".into());
        assert!((0..registry().items.len() as u16).all(|i| !matches(ItemId(i), &cat) || ItemId(i).tool().is_some()));
        cat.category = None;
        cat.search = "вал".into();
        assert!(matches(ItemId::k("shaft"), &cat) && !matches(ItemId::k("glass"), &cat));
    }
}
