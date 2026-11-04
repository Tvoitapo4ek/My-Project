//! Main menu, pause (Esc) and game settings: graphics, sound, controls.
//!
//! * The game starts in the main menu over a slowly circling view of the
//!   proving ground: «Играть», «Настройки», «Выход».
//! * Esc in the game pauses it (the world stops): «Продолжить»,
//!   «Настройки», «В главное меню», «Выход».
//! * Settings have three tabs; every row is ‹ value ›, keys are rebound by
//!   clicking them and pressing a new key. They apply at once and are kept
//!   in `settings.ron` next to the game.

use bevy::{
    app::AppExit,
    audio::{GlobalVolume, Volume},
    input::keyboard::KeyboardInput,
    pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    post_process::bloom::Bloom,
    prelude::*,
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};
use serde::{Deserialize, Serialize};

use crate::{
    core::Ui,
    player::camera::MainCamera,
    ui::{FRAME, font, rivets},
};

const ACCENT: Color = Color::srgb(0.95, 0.76, 0.33);
const TEXT: Color = Color::srgb(0.92, 0.92, 0.9);
const DIM: Color = Color::srgb(0.6, 0.6, 0.62);
const PANEL: Color = Color::srgba(0.08, 0.09, 0.11, 0.94);
const BUTTON: Color = Color::srgb(0.2, 0.21, 0.24);
const BUTTON_HOVER: Color = Color::srgb(0.3, 0.31, 0.35);

pub const RESOLUTIONS: [(u32, u32); 5] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)];
pub const FPS_LIMITS: [u32; 5] = [0, 30, 60, 120, 144];
pub const VIEW: [f32; 3] = [250.0, 500.0, 800.0];

/// Keys that can be bound (name shown in the settings).
pub const KEYS: [(KeyCode, &str); 44] = [
    (KeyCode::KeyA, "A"), (KeyCode::KeyB, "B"), (KeyCode::KeyC, "C"), (KeyCode::KeyD, "D"), (KeyCode::KeyE, "E"), (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"), (KeyCode::KeyH, "H"), (KeyCode::KeyI, "I"), (KeyCode::KeyJ, "J"), (KeyCode::KeyK, "K"), (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"), (KeyCode::KeyN, "N"), (KeyCode::KeyO, "O"), (KeyCode::KeyP, "P"), (KeyCode::KeyQ, "Q"), (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"), (KeyCode::KeyT, "T"), (KeyCode::KeyU, "U"), (KeyCode::KeyV, "V"), (KeyCode::KeyW, "W"), (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"), (KeyCode::KeyZ, "Z"), (KeyCode::Space, "Пробел"), (KeyCode::ShiftLeft, "Shift"), (KeyCode::ControlLeft, "Ctrl"),
    (KeyCode::AltLeft, "Alt"), (KeyCode::Tab, "Tab"), (KeyCode::CapsLock, "Caps"), (KeyCode::ArrowUp, "↑"), (KeyCode::ArrowDown, "↓"),
    (KeyCode::ArrowLeft, "←"), (KeyCode::ArrowRight, "→"), (KeyCode::F1, "F1"), (KeyCode::F2, "F2"), (KeyCode::F4, "F4"), (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"), (KeyCode::F7, "F7"), (KeyCode::F8, "F8"), (KeyCode::F9, "F9"),
];

pub fn key_name(k: KeyCode) -> &'static str {
    KEYS.iter().find(|(c, _)| *c == k).map(|(_, n)| *n).unwrap_or("?")
}

fn key_of(name: &str) -> Option<KeyCode> {
    KEYS.iter().find(|(_, n)| *n == name).map(|(c, _)| *c)
}

/// Rebindable actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Crouch,
    Sprint,
    Inventory,
    View,
}

pub const ACTIONS: [(Action, &str); 9] = [
    (Action::Forward, "Вперёд"),
    (Action::Back, "Назад"),
    (Action::Left, "Влево"),
    (Action::Right, "Вправо"),
    (Action::Jump, "Прыжок / вверх"),
    (Action::Crouch, "Присесть / вниз"),
    (Action::Sprint, "Бег"),
    (Action::Inventory, "Инвентарь"),
    (Action::View, "Вид камеры"),
];

/// Everything in the settings window (saved as `settings.ron`).
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub fullscreen: bool,
    /// Share of the screen resolution the 3D view is drawn at (0.5..1):
    /// lighter on Retina / 4K screens; the interface stays sharp.
    pub render_scale: f32,
    pub resolution: usize,
    pub vsync: bool,
    pub fps_limit: usize,
    pub shadows: bool,
    pub ssao: bool,
    pub bloom: bool,
    pub view: usize,
    pub master: f32,
    pub effects: f32,
    pub motors: f32,
    pub interface: f32,
    pub sensitivity: f32,
    pub invert_y: bool,
    pub fov: f32,
    /// Key names, in `ACTIONS` order.
    pub keys: Vec<String>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            render_scale: 1.0,
            resolution: 0,
            vsync: true,
            fps_limit: 0,
            shadows: true,
            // Contact shadows cost a lot on laptops: off unless asked for.
            ssao: false,
            bloom: true,
            view: 2,
            master: 1.0,
            effects: 1.0,
            motors: 1.0,
            interface: 1.0,
            sensitivity: 1.0,
            invert_y: false,
            fov: 70.0,
            keys: ["W", "S", "A", "D", "Пробел", "Shift", "Ctrl", "E", "F5"].iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl GameSettings {
    pub fn key(&self, a: Action) -> KeyCode {
        let i = ACTIONS.iter().position(|(x, _)| *x == a).unwrap_or(0);
        self.keys.get(i).and_then(|n| key_of(n)).or_else(|| key_of(&GameSettings::default().keys[i])).unwrap_or(KeyCode::KeyW)
    }
    pub fn load() -> Self {
        std::fs::read_to_string(FILE).ok().and_then(|t| ron::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(t) = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()) {
            let _ = std::fs::write(FILE, t);
        }
    }
}

const FILE: &str = "settings.ron";

// ------------------------------------------------------------ rows

/// A settings row: what it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Fullscreen,
    Resolution,
    Vsync,
    FpsLimit,
    Shadows,
    Ssao,
    Bloom,
    View,
    RenderScale,
    Master,
    Effects,
    Motors,
    Interface,
    Sensitivity,
    InvertY,
    Fov,
}

const GRAPHICS: [(Setting, &str); 9] = [
    (Setting::Fullscreen, "Режим"),
    (Setting::RenderScale, "Масштаб рендера"),
    (Setting::Resolution, "Разрешение"),
    (Setting::Vsync, "Вертикальная синхр."),
    (Setting::FpsLimit, "Ограничение FPS"),
    (Setting::Shadows, "Тени"),
    (Setting::Ssao, "Контактные тени (SSAO)"),
    (Setting::Bloom, "Свечение (bloom)"),
    (Setting::View, "Дальность прорисовки"),
];
const SOUND: [(Setting, &str); 4] = [(Setting::Master, "Общая громкость"), (Setting::Effects, "Эффекты"), (Setting::Motors, "Моторы и двигатели"), (Setting::Interface, "Интерфейс")];
const CONTROLS: [(Setting, &str); 3] = [(Setting::Sensitivity, "Чувствительность мыши"), (Setting::InvertY, "Инверсия мыши по Y"), (Setting::Fov, "Поле зрения")];

fn on_off(b: bool) -> String {
    if b { "Вкл".into() } else { "Выкл".into() }
}

/// Text of a setting's value.
pub fn value_text(s: &GameSettings, k: Setting) -> String {
    match k {
        Setting::Fullscreen => if s.fullscreen { "Полный экран" } else { "Окно" }.into(),
        Setting::Resolution => {
            let (w, h) = RESOLUTIONS[s.resolution.min(RESOLUTIONS.len() - 1)];
            format!("{w}×{h}")
        }
        Setting::Vsync => on_off(s.vsync),
        Setting::FpsLimit => match FPS_LIMITS[s.fps_limit.min(FPS_LIMITS.len() - 1)] {
            0 => "Нет".into(),
            n => format!("{n}"),
        },
        Setting::Shadows => on_off(s.shadows),
        Setting::Ssao => on_off(s.ssao),
        Setting::Bloom => on_off(s.bloom),
        Setting::View => ["Близко", "Средне", "Далеко"][s.view.min(2)].into(),
        Setting::RenderScale => format!("{:.0}%", s.render_scale * 100.0),
        Setting::Master => format!("{:.0}%", s.master * 100.0),
        Setting::Effects => format!("{:.0}%", s.effects * 100.0),
        Setting::Motors => format!("{:.0}%", s.motors * 100.0),
        Setting::Interface => format!("{:.0}%", s.interface * 100.0),
        Setting::Sensitivity => format!("{:.2}", s.sensitivity),
        Setting::InvertY => on_off(s.invert_y),
        Setting::Fov => format!("{:.0}°", s.fov),
    }
}

/// ‹ / › on a setting.
pub fn step(s: &mut GameSettings, k: Setting, dir: i32) {
    let cycle = |v: usize, n: usize| (v as i32 + dir).rem_euclid(n as i32) as usize;
    let vol = |v: f32| ((v + dir as f32 * 0.1) * 10.0).round().clamp(0.0, 10.0) / 10.0;
    match k {
        Setting::Fullscreen => s.fullscreen = !s.fullscreen,
        Setting::Resolution => s.resolution = cycle(s.resolution, RESOLUTIONS.len()),
        Setting::Vsync => s.vsync = !s.vsync,
        Setting::FpsLimit => s.fps_limit = cycle(s.fps_limit, FPS_LIMITS.len()),
        Setting::Shadows => s.shadows = !s.shadows,
        Setting::Ssao => s.ssao = !s.ssao,
        Setting::Bloom => s.bloom = !s.bloom,
        Setting::View => s.view = cycle(s.view, VIEW.len()),
        Setting::RenderScale => s.render_scale = ((s.render_scale + dir as f32 * 0.1) * 10.0).round().clamp(5.0, 10.0) / 10.0,
        Setting::Master => s.master = vol(s.master),
        Setting::Effects => s.effects = vol(s.effects),
        Setting::Motors => s.motors = vol(s.motors),
        Setting::Interface => s.interface = vol(s.interface),
        Setting::Sensitivity => s.sensitivity = ((s.sensitivity + dir as f32 * 0.25) * 4.0).round().clamp(1.0, 16.0) / 4.0,
        Setting::InvertY => s.invert_y = !s.invert_y,
        Setting::Fov => s.fov = (s.fov + dir as f32 * 5.0).clamp(55.0, 110.0),
    }
}

// ------------------------------------------------------------ screens

#[derive(Component)]
struct Screen;
#[derive(Component, Clone, Copy)]
enum MenuButton {
    Play,
    Resume,
    Options,
    ToMenu,
    Quit,
    Done,
    Tab(usize),
}
#[derive(Component, Clone, Copy)]
struct StepButton(Setting, i32);
#[derive(Component, Clone, Copy)]
struct ValueText(Setting);
#[derive(Component, Clone, Copy)]
struct KeyButton(usize);
#[derive(Component, Clone, Copy)]
struct KeyText(usize);
#[derive(Component)]
struct TabPage(usize);

/// Where «Готово» in the settings goes back to, the open tab, the key
/// being rebound.
#[derive(Resource, Default)]
pub struct MenuState {
    pub back: Option<Ui>,
    pub tab: usize,
    pub rebinding: Option<usize>,
}

fn button(p: &mut ChildSpawnerCommands, label: &str, b: MenuButton, width: f32) {
    p.spawn((b, Node { width: px(width), height: px(40), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(2)), ..default() }, BackgroundColor(BUTTON), BorderColor::all(FRAME)))
        .observe(on_button)
        .observe(|ev: On<PointerOver>, mut q: Query<&mut BackgroundColor>| {
            if let Ok(mut c) = q.get_mut(ev.entity) {
                c.0 = BUTTON_HOVER;
            }
        })
        .observe(|ev: On<PointerOut>, mut q: Query<&mut BackgroundColor>| {
            if let Ok(mut c) = q.get_mut(ev.entity) {
                c.0 = BUTTON;
            }
        })
        .with_children(|b| {
            b.spawn((Text::new(label), font(17.0), TextColor(TEXT), Pickable::IGNORE));
        });
}

fn framed(root: &mut ChildSpawnerCommands, width: f32, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::all(px(18)), border: UiRect::all(px(2)), width: px(width), align_items: AlignItems::Center, ..default() }, BackgroundColor(PANEL), BorderColor::all(FRAME)))
        .with_children(|p| {
            rivets(p);
            f(p);
        });
}

fn open_menu(mut commands: Commands) {
    commands
        .spawn((Screen, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), padding: UiRect::left(px(80)), align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.35))))
        .with_children(|root| {
            framed(root, 340.0, |p| {
                p.spawn((Text::new("SANDBOX"), font(40.0), TextColor(ACCENT)));
                p.spawn((Text::new("инженерный полигон"), font(15.0), TextColor(DIM), Node { margin: UiRect::bottom(px(12)), ..default() }));
                button(p, "Играть", MenuButton::Play, 260.0);
                button(p, "Настройки", MenuButton::Options, 260.0);
                button(p, "Выход", MenuButton::Quit, 260.0);
            });
        });
}

fn open_pause(mut commands: Commands, mut time: ResMut<Time<Virtual>>) {
    time.pause();
    commands
        .spawn((Screen, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45))))
        .with_children(|root| {
            framed(root, 320.0, |p| {
                p.spawn((Text::new("Пауза"), font(26.0), TextColor(ACCENT), Node { margin: UiRect::bottom(px(8)), ..default() }));
                button(p, "Продолжить", MenuButton::Resume, 250.0);
                button(p, "Настройки", MenuButton::Options, 250.0);
                button(p, "В главное меню", MenuButton::ToMenu, 250.0);
                button(p, "Выход", MenuButton::Quit, 250.0);
            });
        });
}

fn row_label(row: &mut ChildSpawnerCommands, label: &str) {
    row.spawn((Text::new(label), font(15.0), TextColor(TEXT), Node { width: px(250), ..default() }));
}

fn open_options(mut commands: Commands, state: Res<MenuState>) {
    commands
        .spawn((Screen, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45))))
        .with_children(|root| {
            framed(root, 560.0, |p| {
                p.spawn((Text::new("Настройки"), font(24.0), TextColor(ACCENT)));
                p.spawn(Node { column_gap: px(8), margin: UiRect::vertical(px(4)), ..default() }).with_children(|tabs| {
                    for (i, t) in ["Графика", "Звук", "Управление"].iter().enumerate() {
                        button(tabs, t, MenuButton::Tab(i), 150.0);
                    }
                });
                let rows = |p: &mut ChildSpawnerCommands, list: &[(Setting, &str)]| {
                    for (k, label) in list {
                        p.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                            row_label(row, label);
                            for dir in [-1, 0, 1] {
                                if dir == 0 {
                                    row.spawn((ValueText(*k), Text::new(""), font(15.0), TextColor(ACCENT), Node { width: px(150), justify_content: JustifyContent::Center, ..default() }, TextLayout::justify(Justify::Center)));
                                    continue;
                                }
                                row.spawn((StepButton(*k, dir), Node { width: px(30), height: px(26), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() }, BackgroundColor(BUTTON), BorderColor::all(Color::srgb(0.4, 0.4, 0.44))))
                                    .observe(on_step)
                                    .with_children(|b| {
                                        b.spawn((Text::new(if dir < 0 { "‹" } else { "›" }), font(17.0), TextColor(TEXT), Pickable::IGNORE));
                                    });
                            }
                        });
                    }
                };
                for (i, list) in [&GRAPHICS[..], &SOUND[..], &CONTROLS[..]].iter().enumerate() {
                    p.spawn((TabPage(i), Node { flex_direction: FlexDirection::Column, row_gap: px(6), display: if i == state.tab { Display::Flex } else { Display::None }, ..default() })).with_children(|page| {
                        rows(page, list);
                        if i == 2 {
                            page.spawn((Text::new("Клавиши (щёлкните и нажмите новую)"), font(14.0), TextColor(DIM), Node { margin: UiRect::top(px(6)), ..default() }));
                            for (j, (_, label)) in ACTIONS.iter().enumerate() {
                                page.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                    row_label(row, label);
                                    row.spawn((KeyButton(j), Node { width: px(226), height: px(26), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() }, BackgroundColor(BUTTON), BorderColor::all(Color::srgb(0.4, 0.4, 0.44))))
                                        .observe(on_key)
                                        .with_children(|b| {
                                            b.spawn((KeyText(j), Text::new(""), font(15.0), TextColor(ACCENT), Pickable::IGNORE));
                                        });
                                });
                            }
                        }
                    });
                }
                p.spawn(Node { margin: UiRect::top(px(8)), ..default() }).with_children(|b| button(b, "Готово", MenuButton::Done, 200.0));
            });
        });
}

fn close(mut commands: Commands, q: Query<Entity, With<Screen>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn unpause(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

#[allow(clippy::too_many_arguments)]
fn on_button(ev: On<PointerClick>, buttons: Query<&MenuButton>, ui: Res<State<Ui>>, mut next: ResMut<NextState<Ui>>, mut state: ResMut<MenuState>, mut pages: Query<(&TabPage, &mut Node)>, mut exit: MessageWriter<AppExit>, mut sounds: MessageWriter<crate::sound::Sfx>, settings: Res<GameSettings>) {
    let Ok(b) = buttons.get(ev.entity) else { return };
    sounds.write(crate::sound::sfx("ui_click"));
    match *b {
        MenuButton::Play | MenuButton::Resume => next.set(Ui::Game),
        MenuButton::Options => {
            state.back = Some(*ui.get());
            next.set(Ui::Options);
        }
        MenuButton::ToMenu => next.set(Ui::Menu),
        MenuButton::Quit => {
            exit.write(AppExit::Success);
        }
        MenuButton::Done => {
            settings.save();
            next.set(state.back.take().unwrap_or(Ui::Menu));
        }
        MenuButton::Tab(i) => {
            state.tab = i;
            for (p, mut n) in &mut pages {
                n.display = if p.0 == i { Display::Flex } else { Display::None };
            }
        }
    }
}

fn on_step(ev: On<PointerClick>, buttons: Query<&StepButton>, mut settings: ResMut<GameSettings>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    if let Ok(b) = buttons.get(ev.entity) {
        step(&mut settings, b.0, b.1);
        sounds.write(crate::sound::sfx("ui_click"));
    }
}

fn on_key(ev: On<PointerClick>, buttons: Query<&KeyButton>, mut state: ResMut<MenuState>) {
    if let Ok(b) = buttons.get(ev.entity) {
        state.rebinding = Some(b.0);
    }
}

/// The next key pressed goes to the action being rebound (Esc cancels).
fn rebind(mut keys: MessageReader<KeyboardInput>, mut state: ResMut<MenuState>, mut settings: ResMut<GameSettings>) {
    let Some(i) = state.rebinding else {
        keys.clear();
        return;
    };
    for k in keys.read() {
        if !k.state.is_pressed() {
            continue;
        }
        if k.key_code != KeyCode::Escape
            && KEYS.iter().any(|(c, _)| *c == k.key_code)
            && let Some(slot) = settings.keys.get_mut(i)
        {
            *slot = key_name(k.key_code).to_string();
        }
        state.rebinding = None;
        break;
    }
}

fn refresh(settings: Res<GameSettings>, state: Res<MenuState>, mut values: Query<(&ValueText, &mut Text), Without<KeyText>>, mut keys: Query<(&KeyText, &mut Text), Without<ValueText>>) {
    for (v, mut t) in &mut values {
        let s = value_text(&settings, v.0);
        if t.0 != s {
            t.0 = s;
        }
    }
    for (k, mut t) in &mut keys {
        let s = if state.rebinding == Some(k.0) { "… нажмите клавишу".to_string() } else { settings.keys.get(k.0).cloned().unwrap_or_default() };
        if t.0 != s {
            t.0 = s;
        }
    }
}

/// Esc: game → pause → game; settings → back; other windows → game.
fn escape(keys: Res<ButtonInput<KeyCode>>, ui: Res<State<Ui>>, mut next: ResMut<NextState<Ui>>, mut state: ResMut<MenuState>) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match *ui.get() {
        Ui::Menu => {}
        Ui::Game => next.set(Ui::Pause),
        Ui::Pause => next.set(Ui::Game),
        Ui::Ponder => next.set(Ui::Inventory),
        Ui::Options => {
            if state.rebinding.take().is_none() {
                next.set(state.back.take().unwrap_or(Ui::Menu));
            }
        }
        _ => next.set(Ui::Game),
    }
}

// ------------------------------------------------------------ apply

#[allow(clippy::type_complexity)]
fn apply(
    mut commands: Commands,
    settings: Res<GameSettings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut cams: Query<(Entity, &mut Projection, Has<ScreenSpaceAmbientOcclusion>, Has<Bloom>), With<MainCamera>>,
    mut suns: Query<&mut DirectionalLight>,
    mut volume: ResMut<GlobalVolume>,
    added_cam: Query<(), Added<MainCamera>>,
) {
    if !settings.is_changed() && added_cam.is_empty() {
        return;
    }
    if let Ok(mut w) = windows.single_mut() {
        let mode = if settings.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
        if w.mode != mode {
            w.mode = mode;
        }
        let (rw, rh) = RESOLUTIONS[settings.resolution.min(RESOLUTIONS.len() - 1)];
        if !settings.fullscreen && (w.resolution.physical_width(), w.resolution.physical_height()) != (rw, rh) && settings.is_changed() {
            w.resolution.set_physical_resolution(rw, rh);
        }
        let pm = if settings.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
        if w.present_mode != pm {
            w.present_mode = pm;
        }
    }
    for (e, mut proj, ssao, bloom) in &mut cams {
        if let Projection::Perspective(p) = proj.as_mut() {
            p.fov = settings.fov.to_radians();
            p.far = VIEW[settings.view.min(VIEW.len() - 1)];
        }
        match (settings.ssao, ssao) {
            (true, false) => {
                commands.entity(e).insert(ScreenSpaceAmbientOcclusion { quality_level: ScreenSpaceAmbientOcclusionQualityLevel::Medium, ..default() });
            }
            (false, true) => {
                commands.entity(e).remove::<ScreenSpaceAmbientOcclusion>();
            }
            _ => {}
        }
        match (settings.bloom, bloom) {
            (true, false) => {
                commands.entity(e).insert(Bloom { intensity: 0.15, ..Bloom::NATURAL });
            }
            (false, true) => {
                commands.entity(e).remove::<Bloom>();
            }
            _ => {}
        }
    }
    for mut s in &mut suns {
        s.shadow_maps_enabled = settings.shadows;
    }
    volume.volume = Volume::Linear(settings.master);
}

/// The 3D view drawn smaller (render scale < 100 %): its image, and the
/// full-screen node that shows it under the interface.
#[derive(Resource)]
struct Scaled {
    size: UVec2,
    ui_cam: Entity,
    node: Entity,
}

/// Size of the scaled 3D view for a window size.
pub fn scaled_size(window: UVec2, scale: f32) -> UVec2 {
    (window.as_vec2() * scale.clamp(0.5, 1.0)).round().as_uvec2().max(UVec2::ONE)
}

#[allow(clippy::too_many_arguments)]
fn render_scale(
    mut commands: Commands,
    settings: Res<GameSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<(Entity, &mut bevy::camera::RenderTarget), With<MainCamera>>,
    mut images: ResMut<Assets<Image>>,
    scaled: Option<Res<Scaled>>,
) {
    let (Ok(w), Ok((cam, mut target))) = (windows.single(), cams.single_mut()) else { return };
    let win = UVec2::new(w.resolution.physical_width(), w.resolution.physical_height());
    if settings.render_scale >= 0.99 {
        if let Some(s) = scaled {
            *target = bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Primary);
            commands.entity(cam).insert(IsDefaultUiCamera);
            commands.entity(s.ui_cam).despawn();
            commands.entity(s.node).despawn();
            commands.remove_resource::<Scaled>();
        }
        return;
    }
    let size = scaled_size(win, settings.render_scale);
    if scaled.as_ref().is_some_and(|s| s.size == size) {
        return;
    }
    let mut img = Image::new_target_texture(size.x, size.y, bevy::render::render_resource::TextureFormat::Rgba8Unorm, Some(bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb));
    img.sampler = bevy::image::ImageSampler::linear();
    let image = images.add(img);
    *target = bevy::camera::RenderTarget::Image(image.clone().into());
    if let Some(s) = scaled {
        // Same helpers, a new image size.
        commands.entity(s.node).insert(ImageNode::new(image.clone()));
        commands.insert_resource(Scaled { size, ui_cam: s.ui_cam, node: s.node });
        return;
    }
    commands.entity(cam).remove::<IsDefaultUiCamera>();
    let ui_cam = commands.spawn((Name::new("UI camera"), Camera2d, Camera { order: 1, ..default() }, IsDefaultUiCamera)).id();
    let node = commands.spawn((Name::new("Scaled view"), ImageNode::new(image.clone()), Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(-100), Pickable::IGNORE)).id();
    commands.insert_resource(Scaled { size, ui_cam, node });
}

/// Frame limiter (when set): sleeps out the rest of the frame.
fn limit_fps(settings: Res<GameSettings>, mut last: Local<Option<std::time::Instant>>) {
    let fps = FPS_LIMITS[settings.fps_limit.min(FPS_LIMITS.len() - 1)];
    let now = std::time::Instant::now();
    if fps > 0
        && let Some(l) = *last
    {
        let frame = std::time::Duration::from_secs_f64(1.0 / fps as f64);
        let spent = now - l;
        if spent < frame {
            std::thread::sleep(frame - spent);
        }
    }
    *last = Some(std::time::Instant::now());
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameSettings::load())
            .init_resource::<MenuState>()
            .add_systems(OnEnter(Ui::Menu), open_menu)
            .add_systems(OnExit(Ui::Menu), close)
            .add_systems(OnEnter(Ui::Pause), open_pause)
            .add_systems(OnExit(Ui::Pause), (close, unpause))
            .add_systems(OnEnter(Ui::Options), open_options)
            .add_systems(OnExit(Ui::Options), close)
            .add_systems(Update, (escape, apply, render_scale, (rebind, refresh).run_if(in_state(Ui::Options))))
            .add_systems(Last, limit_fps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_step_and_round_trip() {
        let mut s = GameSettings::default();
        step(&mut s, Setting::Fov, 1);
        assert_eq!(s.fov, 75.0);
        for _ in 0..20 {
            step(&mut s, Setting::Master, -1);
        }
        assert_eq!(s.master, 0.0, "clamped");
        step(&mut s, Setting::Resolution, -1);
        assert_eq!(s.resolution, RESOLUTIONS.len() - 1, "wraps");
        assert_eq!(value_text(&s, Setting::Resolution), "3840×2160");
        s.keys[0] = "Z".into();
        assert_eq!(s.key(Action::Forward), KeyCode::KeyZ);
        let text = ron::ser::to_string(&s).unwrap();
        let back: GameSettings = ron::from_str(&text).unwrap();
        assert_eq!(back, s);
        // Missing fields (an older file) take defaults.
        assert_eq!(scaled_size(UVec2::new(2880, 1800), 0.5), UVec2::new(1440, 900), "Retina at 50 %");
        step(&mut s, Setting::RenderScale, -1);
        assert_eq!(value_text(&s, Setting::RenderScale), "90%");
        let old: GameSettings = ron::from_str("(fov: 90.0)").unwrap();
        assert_eq!((old.fov, old.vsync), (90.0, true));
    }
}
