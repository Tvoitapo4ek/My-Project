//! Device sections and the shared frame around them.
//!
//! A section is a plugin that registers a `SectionInfo` and builds its UI
//! when `BuildSection` names it. The frame (navigation, header with the
//! «home» button, detail panel) is built here, so a new section — or a new
//! device for another archetype — only adds content.
//!
//! UI is a pure function of section state + game data: anything that
//! changes the picture sets `HoloNav::rebuild`; values that change every
//! frame use `LiveText` instead of rebuilding.

pub mod blueprints;
pub mod character;
pub mod craft;
pub mod equipment;
pub mod home;
pub mod inventory;
pub mod journal;
pub mod map;
pub mod settings;

use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};

use super::{
    BuildSection, HoloNav, HoloSections,
    device::{DeviceState, HoloDevice},
    panels::{HoloEntities, HoloScroll},
    style::{self, CYAN, DIM, LINE, LINE_DIM, SELECTED, WHITE},
};
use crate::{core::states::TypingFocus, player::Player};

/// Ordering: the frame is rebuilt first, then sections fill it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum HoloSet {
    Frame,
    Build,
}

/// Body of the centre panel (sections spawn into it).
#[derive(Component)]
pub struct ContentBody;

/// Body of the right panel.
#[derive(Component)]
pub struct DetailBody;

/// Text recomputed from the world a few times per second.
#[derive(Component, Clone, Copy)]
pub struct LiveText(pub fn(&mut World) -> String);

/// A clickable node that runs an action with full world access. Keeps
/// section code short: `button(...).observe(act(|w| ...))` is not needed —
/// sections attach `HoloAction` and the shared observer runs it.
#[derive(Component, Clone)]
pub struct HoloAction(pub std::sync::Arc<dyn Fn(&mut World) + Send + Sync>);

impl HoloAction {
    pub fn new(f: impl Fn(&mut World) + Send + Sync + 'static) -> Self {
        Self(std::sync::Arc::new(f))
    }
}

fn on_action_click(mut ev: On<PointerClick>, q: Query<&HoloAction>, mut commands: Commands) {
    if ev.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    if let Ok(a) = q.get(ev.event_target()) {
        ev.propagate(false);
        let f = a.0.clone();
        commands.queue(move |w: &mut World| {
            // Clicking elsewhere ends typing.
            w.resource_mut::<ActiveField>().0 = None;
            w.resource_mut::<TypingFocus>().0 = false;
            f(w);
            if let Some(mut nav) = w.get_resource_mut::<HoloNav>() {
                nav.rebuild = true;
            }
        });
    }
}

/// Spawns a holographic button with a label and an action.
pub fn action_button(
    p: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    size: f32,
    node: Node,
    selected: bool,
    action: HoloAction,
) -> Entity {
    let mut e = p.spawn((style::button(node), action));
    if selected {
        e.insert((BackgroundColor(SELECTED), BorderColor::all(LINE)));
    }
    e.with_children(|b| {
        b.spawn(style::label(text, size, WHITE));
    });
    e.id()
}

/// Item icon: the block texture for block items, otherwise a colour tile
/// with the first letter of the name.
pub fn item_icon(
    p: &mut ChildSpawnerCommands,
    item: crate::items::ItemId,
    size: f32,
    blocks: Option<&crate::world::materials::BlockMaterials>,
) {
    let tex = item.block().and_then(|b| {
        let t = b.def().tex[1];
        blocks.and_then(|m| m.images.get(t.0 as usize).cloned())
    });
    let node = Node {
        width: px(size),
        height: px(size),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        ..default()
    };
    match tex {
        Some(img) => {
            p.spawn((node, ImageNode::new(img), BorderColor::all(LINE_DIM), Pickable::IGNORE));
        }
        None => {
            let c = item.color().to_srgba();
            p.spawn((
                node,
                BackgroundColor(Color::srgba(c.red, c.green, c.blue, 0.85)),
                BorderColor::all(LINE_DIM),
                Pickable::IGNORE,
            ))
            .with_children(|t| {
                let first: String = item.def().name.chars().take(1).collect();
                t.spawn(style::label(first, size * 0.5, Color::srgba(0.0, 0.0, 0.0, 0.75)));
            });
        }
    }
}

/// Section heading inside a panel.
pub fn heading(p: &mut ChildSpawnerCommands, text: &str) {
    p.spawn(style::label(text.to_uppercase(), 15.0, CYAN));
}

/// A thin separator line.
pub fn rule(p: &mut ChildSpawnerCommands) {
    p.spawn((
        Node { height: px(1), width: percent(100), margin: UiRect::vertical(px(4)), ..default() },
        BackgroundColor(LINE_DIM),
        Pickable::IGNORE,
    ));
}

/// Scrollable column (wheel scrolls it).
pub fn scroll_column(node: Node) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(),
            ..node
        },
        ScrollPosition::default(),
        HoloScroll,
    )
}

// --- Text fields -----------------------------------------------------------

/// Editable single-line text. Click to type, Enter to confirm, Esc to stop.
#[derive(Component, Debug, Clone)]
pub struct TextField {
    pub id: &'static str,
    pub value: String,
    pub placeholder: &'static str,
}

#[derive(Component)]
struct TextFieldLabel;

/// Field being typed into.
#[derive(Resource, Default, Debug)]
pub struct ActiveField(pub Option<Entity>);

/// Field content changed (every keystroke) or was confirmed (Enter).
#[derive(Message, Debug, Clone)]
pub struct FieldEdited {
    pub id: &'static str,
    pub value: String,
    pub submitted: bool,
}

pub fn text_field(
    p: &mut ChildSpawnerCommands,
    id: &'static str,
    value: &str,
    placeholder: &'static str,
    node: Node,
) {
    p.spawn((
        Node {
            padding: UiRect::axes(px(8), px(4)),
            border: UiRect::all(px(1)),
            min_width: px(160),
            ..node
        },
        BackgroundColor(Color::srgba(0.0, 0.05, 0.08, 0.8)),
        BorderColor::all(LINE_DIM),
        style::HoverGlow,
        TextField { id, value: value.to_string(), placeholder },
    ))
    .observe(on_field_click)
    .with_children(|f| {
        let (text, color) = if value.is_empty() { (placeholder.to_string(), DIM) } else { (value.to_string(), WHITE) };
        f.spawn((style::label(text, 14.0, color), TextFieldLabel));
    });
}

fn on_field_click(
    ev: On<PointerClick>,
    fields: Query<(), With<TextField>>,
    mut active: ResMut<ActiveField>,
    mut typing: ResMut<TypingFocus>,
) {
    if fields.contains(ev.event_target()) {
        active.0 = Some(ev.event_target());
        typing.0 = true;
    }
}

fn type_into_field(
    mut keys: MessageReader<KeyboardInput>,
    mut active: ResMut<ActiveField>,
    mut typing: ResMut<TypingFocus>,
    mut fields: Query<(&mut TextField, &Children, &mut BorderColor)>,
    mut labels: Query<(&mut Text, &mut TextColor), With<TextFieldLabel>>,
    mut edited: MessageWriter<FieldEdited>,
    time: Res<Time>,
) {
    let Some(e) = active.0 else {
        keys.clear();
        return;
    };
    let Ok((mut field, children, mut border)) = fields.get_mut(e) else {
        active.0 = None;
        typing.0 = false;
        return;
    };
    if !typing.0 {
        active.0 = None;
        return;
    }
    let mut changed = false;
    let mut done = None;
    for k in keys.read() {
        if !k.state.is_pressed() {
            continue;
        }
        match &k.logical_key {
            Key::Character(s) => {
                if field.value.chars().count() < 64 {
                    field.value.push_str(s);
                    changed = true;
                }
            }
            Key::Space => {
                field.value.push(' ');
                changed = true;
            }
            Key::Backspace => {
                field.value.pop();
                changed = true;
            }
            Key::Enter => done = Some(true),
            Key::Escape => done = Some(false),
            _ => {}
        }
    }
    let blink = (time.elapsed_secs() * 2.0) as i32 % 2 == 0;
    *border = BorderColor::all(if blink { LINE } else { LINE_DIM });
    for c in children.iter() {
        if let Ok((mut t, mut col)) = labels.get_mut(c) {
            t.0 = format!("{}▏", field.value);
            col.0 = WHITE;
        }
    }
    if changed {
        edited.write(FieldEdited { id: field.id, value: field.value.clone(), submitted: false });
    }
    if let Some(submit) = done {
        if submit {
            edited.write(FieldEdited { id: field.id, value: field.value.clone(), submitted: true });
        }
        active.0 = None;
        typing.0 = false;
        for c in children.iter() {
            if let Ok((mut t, mut col)) = labels.get_mut(c) {
                if field.value.is_empty() {
                    t.0 = field.placeholder.to_string();
                    col.0 = DIM;
                } else {
                    t.0 = field.value.clone();
                }
            }
        }
    }
}

// --- Frame -----------------------------------------------------------------

#[derive(Component)]
pub struct NavButton(pub &'static str);

fn rebuild_frame(
    mut commands: Commands,
    mut nav: ResMut<HoloNav>,
    state: Res<DeviceState>,
    ents: Option<Res<HoloEntities>>,
    sections: Res<HoloSections>,
    device: Query<&HoloDevice, With<Player>>,
    children: Query<&Children>,
    mut active: ResMut<ActiveField>,
    typing: Res<TypingFocus>,
    mut msg: MessageWriter<BuildSection>,
) {
    let Some(ents) = ents else {
        return;
    };
    // Never rebuild under the user's fingers: wait until typing ends.
    if !nav.rebuild || *state == DeviceState::Closed || typing.0 {
        return;
    }
    nav.rebuild = false;
    active.0 = None;
    for root in [ents.nav, ents.content, ents.detail, ents.projection] {
        if let Ok(ch) = children.get(root) {
            for c in ch.iter() {
                commands.entity(c).despawn();
            }
        }
    }
    let current = nav.current;
    let info = sections.0.iter().find(|s| s.id == current).cloned();
    let device_name = device.single().map(|d| d.name).unwrap_or("Проектор");

    // Navigation panel.
    commands.entity(ents.nav).with_children(|p| {
        p.spawn((
            style::frame(Node {
                flex_direction: FlexDirection::Column,
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(14)),
                row_gap: px(8),
                ..default()
            }),
        ))
        .with_children(|f| {
            f.spawn(style::label("◆ ПРОЕКТОР", 22.0, CYAN));
            f.spawn(style::label(device_name, 13.0, DIM));
            rule(f);
            for s in &sections.0 {
                let id = s.id;
                let sel = id == current;
                let mut b = f.spawn((
                    style::button(Node {
                        width: percent(100),
                        justify_content: JustifyContent::FlexStart,
                        column_gap: px(10),
                        padding: UiRect::axes(px(10), px(8)),
                        ..default()
                    }),
                    NavButton(id),
                    HoloAction::new(move |w| w.resource_mut::<HoloNav>().go(id)),
                ));
                if sel {
                    b.insert((BackgroundColor(SELECTED), BorderColor::all(LINE)));
                }
                b.with_children(|b| {
                    b.spawn(style::label(s.glyph, 18.0, if sel { WHITE } else { CYAN }));
                    b.spawn(style::label(s.title, 17.0, WHITE));
                });
            }
            f.spawn(Node { flex_grow: 1.0, ..default() });
            rule(f);
            f.spawn(style::label(
                "ЛКМ — выбор · колесо — прокрутка\nПКМ (держать) — осмотреться\nWASD — медленный шаг\nTab / Esc — закрыть",
                12.0,
                DIM,
            ));
        });
    });

    // Centre panel: header + body.
    let mut body = Entity::PLACEHOLDER;
    commands.entity(ents.content).with_children(|p| {
        p.spawn(style::frame(Node {
            flex_direction: FlexDirection::Column,
            width: percent(100),
            height: percent(100),
            padding: UiRect::all(px(14)),
            row_gap: px(8),
            ..default()
        }))
        .with_children(|f| {
            f.spawn(Node { column_gap: px(12), align_items: AlignItems::Center, ..default() })
                .with_children(|h| {
                    if current != "home" {
                        action_button(
                            h,
                            "⌂ Главная",
                            15.0,
                            Node::default(),
                            false,
                            HoloAction::new(|w| w.resource_mut::<HoloNav>().go("home")),
                        );
                    }
                    if let Some(i) = &info {
                        h.spawn(style::label(format!("{}  {}", i.glyph, i.title.to_uppercase()), 22.0, WHITE));
                    }
                    h.spawn(Node { flex_grow: 1.0, ..default() });
                    h.spawn((style::label("", 13.0, DIM), LiveText(clock_line)));
                });
            rule(f);
            body = f
                .spawn((
                    ContentBody,
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        min_height: px(0),
                        ..default()
                    },
                ))
                .id();
        });
    });

    // Right panel.
    let mut detail = Entity::PLACEHOLDER;
    commands.entity(ents.detail).with_children(|p| {
        detail = p
            .spawn((
                DetailBody,
                style::frame(Node {
                    flex_direction: FlexDirection::Column,
                    width: percent(100),
                    height: percent(100),
                    padding: UiRect::all(px(14)),
                    row_gap: px(8),
                    ..default()
                }),
            ))
            .id();
    });

    msg.write(BuildSection { id: current, content: body, detail, projection: ents.projection });
}

fn clock_line(w: &mut World) -> String {
    let mode = match w.get_resource::<State<crate::core::states::GameMode>>().map(|s| *s.get()) {
        Some(crate::core::states::GameMode::Creative) => "Креатив",
        _ => "Выживание",
    };
    let time = w
        .get_resource::<crate::core::resources::WorldClock>()
        .map(|c| {
            let m = (c.time_of_day * 24.0 * 60.0) as u32;
            format!("{:02}:{:02}", m / 60, m % 60)
        })
        .unwrap_or_default();
    format!("{mode} · {time}")
}

#[derive(Default)]
struct LiveTimer(f32);

fn refresh_live_text(world: &mut World, mut timer: Local<LiveTimer>) {
    let dt = world.resource::<Time>().delta_secs();
    timer.0 -= dt;
    if timer.0 > 0.0 {
        return;
    }
    timer.0 = 0.2;
    let mut q = world.query::<(Entity, &LiveText)>();
    let list: Vec<(Entity, LiveText)> = q.iter(world).map(|(e, l)| (e, *l)).collect();
    for (e, l) in list {
        let s = (l.0)(world);
        if let Some(mut t) = world.get_mut::<Text>(e)
            && t.0 != s
        {
            t.0 = s;
        }
    }
}


pub struct SectionsPlugin;

impl Plugin for SectionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveField>()
            .add_message::<FieldEdited>()
            .configure_sets(Update, (HoloSet::Frame, HoloSet::Build).chain())
            .add_observer(on_action_click)
            .add_systems(Update, rebuild_frame.in_set(HoloSet::Frame))
            .add_systems(Update, type_into_field.before(HoloSet::Frame))
            .add_systems(Update, refresh_live_text.run_if(super::device_shown))
            .add_plugins((
                home::HomeSection,
                inventory::InventorySection,
                equipment::EquipmentSection,
                map::MapSection,
                journal::JournalSection,
                blueprints::BlueprintsSection,
                craft::CraftSection,
                character::CharacterSection,
                settings::SettingsSection,
            ));
    }
}
