//! The worlds screen (main menu → «Играть»): saved worlds, newest first —
//! «Играть», «Удалить» (asks again) — and a new world by name.
//! Pause → «Сохранить»; leaving to the menu or quitting saves too, and
//! the game saves itself every 5 minutes (`save`).
//!
//! Console: `world new <имя>`, `world load <имя>`, `world save`, `worlds`.

use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};

use crate::{
    core::{Ui, toast},
    save,
    ui::{FRAME, font, rivets},
};

const PANEL: Color = Color::srgba(0.08, 0.09, 0.11, 0.94);
const BUTTON: Color = Color::srgb(0.18, 0.19, 0.22);
const HOVER: Color = Color::srgb(0.26, 0.27, 0.31);
const DANGER: Color = Color::srgb(0.45, 0.16, 0.12);
const ACCENT: Color = Color::srgb(0.95, 0.76, 0.33);
const TEXT: Color = Color::srgb(0.92, 0.92, 0.9);
const DIM: Color = Color::srgb(0.6, 0.6, 0.62);

/// Something to do with the world that needs the whole `World`.
#[derive(Message, Debug, Clone)]
pub enum WorldAction {
    New(String),
    Load(String),
    Save,
    Delete(String),
}

#[derive(Resource, Default)]
struct Form {
    name: String,
    /// The world whose «Удалить» was pressed once.
    confirm: Option<String>,
    dirty: bool,
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct List;
#[derive(Component)]
struct NameText;
#[derive(Component, Clone)]
enum Btn {
    Play(String),
    Delete(String),
    Create,
    Back,
}

fn button(p: &mut ChildSpawnerCommands, label: &str, w: f32, b: Btn, color: Color) {
    p.spawn((b, Node { width: px(w), height: px(32), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() }, BackgroundColor(color), BorderColor::all(FRAME)))
        .observe(on_click)
        .observe(move |ev: On<PointerOver>, mut q: Query<&mut BackgroundColor>| {
            if let Ok(mut c) = q.get_mut(ev.entity) {
                c.0 = HOVER;
            }
        })
        .observe(move |ev: On<PointerOut>, mut q: Query<&mut BackgroundColor>| {
            if let Ok(mut c) = q.get_mut(ev.entity) {
                c.0 = color;
            }
        })
        .with_children(|b| {
            b.spawn((Text::new(label), font(15.0), TextColor(TEXT), Pickable::IGNORE));
        });
}

fn ago(t: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(t);
    let d = now.saturating_sub(t);
    match d {
        0..60 => "только что".into(),
        60..3600 => format!("{} мин назад", d / 60),
        3600..86400 => format!("{} ч назад", d / 3600),
        _ => format!("{} дн назад", d / 86400),
    }
}

fn open(mut commands: Commands, mut form: ResMut<Form>) {
    form.confirm = None;
    form.dirty = true;
    commands
        .spawn((Root, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), padding: UiRect::left(px(80)), align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.35))))
        .with_children(|root| {
            root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::all(px(18)), border: UiRect::all(px(2)), width: px(460), ..default() }, BackgroundColor(PANEL), BorderColor::all(FRAME))).with_children(|p| {
                rivets(p);
                p.spawn((Text::new("Миры"), font(26.0), TextColor(ACCENT)));
                p.spawn((List, Node { flex_direction: FlexDirection::Column, row_gap: px(6), max_height: px(330), overflow: Overflow::clip_y(), ..default() }));
                p.spawn((Text::new("Новый мир"), font(14.0), TextColor(DIM), Node { margin: UiRect::top(px(6)), ..default() }));
                p.spawn(Node { column_gap: px(8), ..default() }).with_children(|r| {
                    r.spawn((Node { width: px(300), height: px(32), padding: UiRect::horizontal(px(8)), align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgb(0.12, 0.13, 0.15)), BorderColor::all(FRAME))).with_children(|f| {
                        f.spawn((NameText, Text::new(""), font(15.0), TextColor(TEXT)));
                    });
                    button(r, "Создать", 112.0, Btn::Create, Color::srgb(0.42, 0.33, 0.14));
                });
                button(p, "Назад", 424.0, Btn::Back, BUTTON);
            });
        });
}

fn close(mut commands: Commands, q: Query<Entity, With<Root>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn refresh(mut commands: Commands, mut form: ResMut<Form>, lists: Query<Entity, With<List>>, mut name: Query<&mut Text, With<NameText>>, time: Res<Time<Real>>) {
    for mut t in &mut name {
        let caret = if (time.elapsed_secs() * 2.0) as i32 % 2 == 0 { "▏" } else { " " };
        t.0 = format!("{}{caret}", form.name);
    }
    if !form.dirty {
        return;
    }
    form.dirty = false;
    let worlds = save::list();
    for list in &lists {
        commands.entity(list).despawn_children();
        commands.entity(list).with_children(|l| {
            if worlds.is_empty() {
                l.spawn((Text::new("Пока нет миров — создайте первый."), font(14.0), TextColor(DIM)));
            }
            for (w, t) in &worlds {
                l.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                    r.spawn(Node { width: px(220), flex_direction: FlexDirection::Column, ..default() }).with_children(|c| {
                        c.spawn((Text::new(w.clone()), font(16.0), TextColor(TEXT)));
                        c.spawn((Text::new(ago(*t)), font(12.0), TextColor(DIM)));
                    });
                    button(r, "Играть", 112.0, Btn::Play(w.clone()), BUTTON);
                    let sure = form.confirm.as_ref() == Some(w);
                    button(r, if sure { "Точно?" } else { "Удалить" }, 84.0, Btn::Delete(w.clone()), if sure { DANGER } else { BUTTON });
                });
            }
        });
    }
}

fn on_click(ev: On<PointerClick>, btns: Query<&Btn>, mut form: ResMut<Form>, mut actions: MessageWriter<WorldAction>, mut next: ResMut<NextState<Ui>>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    let Ok(b) = btns.get(ev.entity) else { return };
    sounds.write(crate::sound::sfx("ui_click"));
    match b.clone() {
        Btn::Play(w) => {
            actions.write(WorldAction::Load(w));
        }
        Btn::Delete(w) => {
            if form.confirm.as_ref() == Some(&w) {
                actions.write(WorldAction::Delete(w));
                form.confirm = None;
            } else {
                form.confirm = Some(w);
            }
            form.dirty = true;
        }
        Btn::Create => {
            let n = form.name.trim().to_string();
            actions.write(WorldAction::New(n));
        }
        Btn::Back => next.set(Ui::Menu),
    }
}

fn type_name(mut keys: MessageReader<KeyboardInput>, mut form: ResMut<Form>, mut actions: MessageWriter<WorldAction>) {
    for k in keys.read() {
        if !k.state.is_pressed() {
            continue;
        }
        match &k.logical_key {
            Key::Character(s) if form.name.chars().count() < 32 => form.name.push_str(s),
            Key::Space if !form.name.is_empty() => form.name.push(' '),
            Key::Backspace => {
                form.name.pop();
            }
            Key::Enter => {
                actions.write(WorldAction::New(form.name.trim().to_string()));
            }
            _ => {}
        }
    }
}

/// Runs the actions (they need the whole world).
fn act(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<WorldAction>>) {
    let actions: Vec<WorldAction> = cursor.read(world.resource::<Messages<WorldAction>>()).cloned().collect();
    for a in actions {
        let (msg, play) = match a {
            WorldAction::New(n) => match save::new_world(world, &n) {
                Ok(()) => {
                    world.resource_mut::<Form>().name.clear();
                    (format!("Новый мир «{}»", n.trim()), true)
                }
                Err(e) => (e, false),
            },
            WorldAction::Load(n) => match save::load_world(world, &n) {
                Ok(()) => (format!("Мир «{n}» загружен"), true),
                Err(e) => (e, false),
            },
            WorldAction::Save => match save::save_current(world) {
                Some(Ok(_)) => ("Мир сохранён".into(), false),
                Some(Err(e)) => (format!("Не удалось сохранить: {e}"), false),
                None => ("Мир не выбран: главное меню → «Играть» → новый мир".into(), false),
            },
            WorldAction::Delete(n) => {
                let r = save::delete(&n);
                world.resource_mut::<Form>().dirty = true;
                (r.map(|_| format!("Мир «{n}» удалён")).unwrap_or_else(|e| e), false)
            }
        };
        world.write_message(toast(msg));
        if play {
            world.resource_mut::<NextState<Ui>>().set(Ui::Game);
        }
    }
}

/// Console: `world …`.
pub fn command(world: &mut World, parts: &[&str]) -> String {
    match parts {
        ["worlds"] => {
            let l = save::list();
            if l.is_empty() { "Миров нет".into() } else { l.into_iter().map(|(n, _)| n).collect::<Vec<_>>().join(", ") }
        }
        ["world", "save"] => match save::save_current(world) {
            Some(Ok(p)) => format!("Сохранено: {}", p.display()),
            Some(Err(e)) => e,
            None => "Мир не выбран (world new <имя>)".into(),
        },
        ["world", "new", name @ ..] => {
            let n = name.join(" ");
            save::new_world(world, &n).map(|_| format!("Новый мир «{n}»")).unwrap_or_else(|e| e)
        }
        ["world", "load", name @ ..] => {
            let n = name.join(" ");
            save::load_world(world, &n).map(|_| format!("Мир «{n}» загружен")).unwrap_or_else(|e| e)
        }
        _ => "world new|load <имя>, world save, worlds".into(),
    }
}

/// Going back to the main menu saves the world.
fn save_on_menu(world: &mut World) {
    if let Some(Err(e)) = save::save_current(world) {
        world.write_message(toast(format!("Не удалось сохранить: {e}")));
    }
}

pub struct WorldsPlugin;

impl Plugin for WorldsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Form>()
            .add_message::<WorldAction>()
            .add_systems(OnEnter(Ui::Worlds), open)
            .add_systems(OnExit(Ui::Worlds), close)
            .add_systems(OnEnter(Ui::Menu), save_on_menu)
            .add_systems(Update, ((type_name, refresh).chain().run_if(in_state(Ui::Worlds)), act));
    }
}
