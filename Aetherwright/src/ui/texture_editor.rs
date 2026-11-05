//! In-game texture editor (F7).
//!
//! Paints directly into the block texture used by the world, so every block
//! with that texture updates while you draw. "Сохранить" writes the PNG to
//! `assets/textures/blocks/<name>.png` (picked up on next start and by
//! external editors; hot reload keeps both in sync).
//!
//! Tools: pencil (B), eraser — transparent (E), fill (G), eyedropper (I);
//! Ctrl+Z undo, Ctrl+S save, F7 / Esc close. LMB paints, RMB picks a colour.

use bevy::{picking::pointer::PointerButton, prelude::*};

use super::theme;
use crate::{
    content::registry,
    core::{events::Notify, states::InputFocus},
    world::materials::{BlockMaterials, write_png},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Pencil,
    Eraser,
    Fill,
    Picker,
}

impl Tool {
    const ALL: [(Tool, &'static str, KeyCode); 4] = [
        (Tool::Pencil, "Карандаш [B]", KeyCode::KeyB),
        (Tool::Eraser, "Ластик [E]", KeyCode::KeyE),
        (Tool::Fill, "Заливка [G]", KeyCode::KeyG),
        (Tool::Picker, "Пипетка [I]", KeyCode::KeyI),
    ];
}

/// Editing session state.
#[derive(Resource, Debug, Default)]
pub struct TextureEditor {
    pub texture: usize,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
    pub color: [u8; 4],
    pub tool: Tool,
    undo: Vec<Vec<[u8; 4]>>,
    painting: bool,
    /// Pixels changed since the last refresh of the canvas.
    dirty: bool,
    /// Textures edited but not saved.
    pub unsaved: Vec<usize>,
}

impl TextureEditor {
    fn push_undo(&mut self) {
        self.undo.push(self.pixels.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    /// Applies the current tool at a pixel. Returns true if pixels changed.
    pub fn apply(&mut self, i: usize) -> bool {
        if i >= self.pixels.len() {
            return false;
        }
        match self.tool {
            Tool::Pencil | Tool::Eraser => {
                let c = if self.tool == Tool::Eraser {
                    [0, 0, 0, 0]
                } else {
                    self.color
                };
                let changed = self.pixels[i] != c;
                self.pixels[i] = c;
                changed
            }
            Tool::Picker => {
                self.color = self.pixels[i];
                false
            }
            Tool::Fill => self.flood(i),
        }
    }

    /// 4-neighbour flood fill of the region with the same colour.
    fn flood(&mut self, start: usize) -> bool {
        let target = self.pixels[start];
        if target == self.color {
            return false;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            if self.pixels[i] != target {
                continue;
            }
            self.pixels[i] = self.color;
            let (x, y) = (i as i32 % w, i as i32 / w);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && nx < w && ny < h {
                    stack.push((nx + ny * w) as usize);
                }
            }
        }
        true
    }

    fn bytes(&self) -> Vec<u8> {
        self.pixels.iter().flatten().copied().collect()
    }
}

#[derive(Component)]
struct EditorScreen;
#[derive(Component)]
struct Cell(usize);
#[derive(Component)]
struct TexButton(usize);
#[derive(Component)]
struct ToolButton(Tool);
#[derive(Component)]
struct Swatch([u8; 4]);
#[derive(Component)]
struct CurrentColor;
#[derive(Component)]
struct StatusText;
#[derive(Component, Clone, Copy)]
enum Action {
    Save,
    Undo,
    Revert,
    Channel(usize, i16),
}

const CANVAS: f32 = 384.0;

fn to_color(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<State<InputFocus>>,
    mut next: ResMut<NextState<InputFocus>>,
) {
    if keys.just_pressed(KeyCode::F7) {
        next.set(if *focus.get() == InputFocus::TextureEditor {
            InputFocus::Gameplay
        } else {
            InputFocus::TextureEditor
        });
    }
}

/// Loads a texture's current pixels (whatever the world shows right now).
fn load(ed: &mut TextureEditor, index: usize, mats: &BlockMaterials, images: &Assets<Image>) {
    let Some(img) = mats.images.get(index).and_then(|h| images.get(h)) else {
        return;
    };
    let size = img.size();
    let data = img.data.clone().unwrap_or_default();
    ed.texture = index;
    ed.width = size.x;
    ed.height = size.y;
    ed.pixels = data
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    ed.undo.clear();
    ed.dirty = true;
}

fn spawn_screen(world: &mut World) {
    // Load the texture first so the canvas can be built with its size.
    world.resource_scope(|world, mut ed: Mut<TextureEditor>| {
        let mats = world.resource::<BlockMaterials>();
        let images = world.resource::<Assets<Image>>();
        let idx = ed.texture;
        load(&mut ed, idx, mats, images);
        if ed.color[3] == 0 {
            ed.color = [200, 200, 200, 255];
        }
    });
    let (w, h, img) = {
        let ed = world.resource::<TextureEditor>();
        (
            ed.width.max(1),
            ed.height.max(1),
            world
                .resource::<BlockMaterials>()
                .images
                .get(ed.texture)
                .cloned()
                .unwrap_or_default(),
        )
    };
    let cell = (CANVAS / w.max(h) as f32).floor();
    let mut commands = world.commands();
    commands
        .spawn((
            EditorScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(12),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.04, 0.55)),
            GlobalZIndex(70),
        ))
        .with_children(|root| {
            // Texture list.
            root.spawn(theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(2), max_height: percent(85), overflow: Overflow::scroll_y(), width: px(180), ..default() }))
                .with_children(|p| {
                    p.spawn((Text::new("ТЕКСТУРЫ"), theme::font(15.0), TextColor(theme::ACCENT)));
                    for (i, t) in registry().textures.iter().enumerate() {
                        p.spawn((TexButton(i), Node { padding: UiRect::axes(px(4), px(1)), ..default() }, BackgroundColor(Color::NONE)))
                            .observe(on_texture_click)
                            .with_children(|b| {
                                b.spawn((Text::new(t.name.clone()), theme::font(12.0), TextColor(theme::TEXT), Pickable::IGNORE));
                            });
                    }
                });
            // Canvas.
            root.spawn(theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(6), align_items: AlignItems::Center, ..default() })).with_children(|p| {
                p.spawn((StatusText, Text::new(""), theme::font(13.0), TextColor(theme::TEXT)));
                p.spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::px(w as u16, cell),
                    grid_template_rows: RepeatedGridTrack::px(h as u16, cell),
                    border: UiRect::all(px(1)),
                    ..default()
                })
                .insert(BorderColor::all(theme::PANEL_BORDER))
                .with_children(|g| {
                    for i in 0..(w * h) as usize {
                        g.spawn((Cell(i), Node::default(), BackgroundColor(Color::NONE), Outline::new(px(0.5), px(0), Color::srgba(1.0, 1.0, 1.0, 0.05))))
                            .observe(on_cell_press)
                            .observe(on_cell_over);
                    }
                });
                // Live tiled preview (the same image the world uses).
                p.spawn(Node { column_gap: px(0), ..default() }).with_children(|r| {
                    for _ in 0..4 {
                        r.spawn((ImageNode::new(img.clone()), Node { width: px(48), height: px(48), ..default() }));
                    }
                });
            });
            // Tools, colour, actions.
            root.spawn(theme::panel(Node { flex_direction: FlexDirection::Column, row_gap: px(6), width: px(240), ..default() })).with_children(|p| {
                p.spawn((Text::new("ИНСТРУМЕНТЫ"), theme::font(15.0), TextColor(theme::ACCENT)));
                for (tool, label, _) in Tool::ALL {
                    button(p, label, ToolButton(tool)).observe(on_tool_click);
                }
                p.spawn((Text::new("ЦВЕТ"), theme::font(15.0), TextColor(theme::ACCENT)));
                p.spawn((CurrentColor, Node { width: px(220), height: px(26), border: UiRect::all(px(1)), ..default() }, BorderColor::all(theme::PANEL_BORDER), BackgroundColor(Color::WHITE)));
                for (ch, name) in ["R", "G", "B", "A"].iter().enumerate() {
                    p.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        r.spawn((Text::new(*name), theme::font(13.0), TextColor(theme::TEXT), Node { width: px(16), ..default() }));
                        for d in [-32i16, -8, 8, 32] {
                            button(r, &format!("{d:+}"), Action::Channel(ch, d)).observe(on_action);
                        }
                    });
                }
                p.spawn((Text::new("ПАЛИТРА"), theme::font(13.0), TextColor(theme::TEXT_DIM)));
                p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(3), row_gap: px(3), width: px(230), ..default() }).with_children(|g| {
                    for c in BASE_PALETTE {
                        g.spawn((Swatch(c), Node { width: px(22), height: px(22), border: UiRect::all(px(1)), ..default() }, BackgroundColor(to_color(c)), BorderColor::all(theme::PANEL_BORDER)))
                            .observe(on_swatch_click);
                    }
                });
                p.spawn((Text::new("ФАЙЛ"), theme::font(15.0), TextColor(theme::ACCENT)));
                button(p, "Сохранить PNG [Ctrl+S]", Action::Save).observe(on_action);
                button(p, "Отменить [Ctrl+Z]", Action::Undo).observe(on_action);
                button(p, "Вернуть сохранённую", Action::Revert).observe(on_action);
                p.spawn((
                    Text::new("ЛКМ — рисовать, ПКМ — взять цвет.\nМир обновляется сразу; файл —\nassets/textures/blocks/<имя>.png"),
                    theme::font(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
        });
}

const BASE_PALETTE: [[u8; 4]; 24] = [
    [0, 0, 0, 255],
    [40, 40, 44, 255],
    [90, 90, 96, 255],
    [140, 140, 146, 255],
    [200, 200, 204, 255],
    [255, 255, 255, 255],
    [121, 88, 62, 255],
    [176, 132, 82, 255],
    [102, 74, 48, 255],
    [214, 196, 148, 255],
    [92, 138, 64, 255],
    [48, 92, 46, 255],
    [70, 118, 58, 255],
    [130, 124, 118, 255],
    [196, 150, 120, 255],
    [206, 120, 70, 255],
    [60, 110, 170, 255],
    [52, 107, 158, 184],
    [70, 190, 220, 255],
    [180, 60, 140, 255],
    [120, 230, 210, 255],
    [255, 200, 120, 255],
    [190, 40, 60, 255],
    [0, 0, 0, 0],
];

fn button<'a, B: Bundle>(
    p: &'a mut ChildSpawnerCommands,
    label: &str,
    marker: B,
) -> EntityCommands<'a> {
    let mut e = p.spawn((
        marker,
        Node {
            padding: UiRect::axes(px(6), px(2)),
            border: UiRect::all(px(1)),
            ..default()
        },
        BorderColor::all(theme::PANEL_BORDER),
        BackgroundColor(theme::PANEL_BG),
    ));
    e.with_children(|b| {
        b.spawn((
            Text::new(label),
            theme::font(12.0),
            TextColor(theme::TEXT),
            Pickable::IGNORE,
        ));
    });
    e
}

fn despawn_screen(mut commands: Commands, q: Query<Entity, With<EditorScreen>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Pushes edited pixels into the live texture.
pub fn commit(ed: &mut TextureEditor, mats: &BlockMaterials, images: &mut Assets<Image>) {
    if let Some(mut img) = mats.images.get(ed.texture).and_then(|h| images.get_mut(h)) {
        img.data = Some(ed.bytes());
    }
    if !ed.unsaved.contains(&ed.texture) {
        ed.unsaved.push(ed.texture);
    }
    ed.dirty = true;
}

fn on_cell_press(
    ev: On<PointerPress>,
    cells: Query<&Cell>,
    mut ed: ResMut<TextureEditor>,
    mats: Res<BlockMaterials>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok(Cell(i)) = cells.get(ev.event_target()) else {
        return;
    };
    if ev.button == PointerButton::Secondary {
        ed.color = ed.pixels.get(*i).copied().unwrap_or(ed.color);
        ed.dirty = true;
        return;
    }
    if ev.button != PointerButton::Primary {
        return;
    }
    ed.push_undo();
    ed.painting = matches!(ed.tool, Tool::Pencil | Tool::Eraser);
    if ed.apply(*i) {
        commit(&mut ed, &mats, &mut images);
    } else {
        ed.dirty = true;
    }
}

fn on_cell_over(
    ev: On<PointerOver>,
    cells: Query<&Cell>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut ed: ResMut<TextureEditor>,
    mats: Res<BlockMaterials>,
    mut images: ResMut<Assets<Image>>,
) {
    if !ed.painting || !mouse.pressed(MouseButton::Left) {
        return;
    }
    let Ok(Cell(i)) = cells.get(ev.event_target()) else {
        return;
    };
    if ed.apply(*i) {
        commit(&mut ed, &mats, &mut images);
    }
}

fn on_texture_click(
    ev: On<PointerClick>,
    q: Query<&TexButton>,
    mut ed: ResMut<TextureEditor>,
    mut next: ResMut<NextState<InputFocus>>,
) {
    if let Ok(TexButton(i)) = q.get(ev.event_target()) {
        ed.texture = *i;
        // Rebuild the screen (canvas size may differ).
        next.set(InputFocus::Gameplay);
        ed.painting = false;
        REOPEN.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

static REOPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn reopen(mut next: ResMut<NextState<InputFocus>>) {
    if REOPEN.swap(false, std::sync::atomic::Ordering::Relaxed) {
        next.set(InputFocus::TextureEditor);
    }
}

fn on_tool_click(ev: On<PointerClick>, q: Query<&ToolButton>, mut ed: ResMut<TextureEditor>) {
    if let Ok(ToolButton(t)) = q.get(ev.event_target()) {
        ed.tool = *t;
        ed.dirty = true;
    }
}

fn on_swatch_click(ev: On<PointerClick>, q: Query<&Swatch>, mut ed: ResMut<TextureEditor>) {
    if let Ok(Swatch(c)) = q.get(ev.event_target()) {
        ed.color = *c;
        if ed.tool == Tool::Picker || ed.tool == Tool::Eraser {
            ed.tool = Tool::Pencil;
        }
        ed.dirty = true;
    }
}

fn on_action(
    ev: On<PointerClick>,
    q: Query<&Action>,
    mut ed: ResMut<TextureEditor>,
    mats: Res<BlockMaterials>,
    mut images: ResMut<Assets<Image>>,
    mut notify: MessageWriter<Notify>,
) {
    let Ok(a) = q.get(ev.event_target()) else {
        return;
    };
    run_action(*a, &mut ed, &mats, &mut images, &mut notify);
}

fn run_action(
    a: Action,
    ed: &mut TextureEditor,
    mats: &BlockMaterials,
    images: &mut Assets<Image>,
    notify: &mut MessageWriter<Notify>,
) {
    match a {
        Action::Channel(ch, d) => {
            ed.color[ch] = (ed.color[ch] as i16 + d).clamp(0, 255) as u8;
            ed.dirty = true;
        }
        Action::Undo => {
            if let Some(prev) = ed.undo.pop() {
                ed.pixels = prev;
                commit(ed, mats, images);
            }
        }
        Action::Save => match save(ed) {
            Ok(path) => {
                let t = ed.texture;
                ed.unsaved.retain(|x| *x != t);
                notify.write(Notify::ok(format!("Сохранено: {path}")));
            }
            Err(e) => {
                notify.write(Notify::warn(format!("Не удалось сохранить: {e}")));
            }
        },
        Action::Revert => {
            let def = &registry().textures[ed.texture];
            let (w, h, data) = match crate::world::materials::texture_file(&def.name)
                .map(|p| crate::world::materials::read_png(&p))
            {
                Some(Ok(v)) => v,
                _ => (
                    crate::world::materials::TEX_SIZE,
                    crate::world::materials::TEX_SIZE,
                    crate::world::materials::procedural_rgba(def, ed.texture as u32),
                ),
            };
            if (w, h) == (ed.width, ed.height) {
                ed.push_undo();
                ed.pixels = data
                    .chunks_exact(4)
                    .map(|c| [c[0], c[1], c[2], c[3]])
                    .collect();
                commit(ed, mats, images);
                let t = ed.texture;
                ed.unsaved.retain(|x| *x != t);
            }
        }
    }
}

/// Writes the edited texture as PNG; returns the path.
pub fn save(ed: &TextureEditor) -> Result<String, String> {
    let name = &registry().textures[ed.texture].name;
    let path = std::path::Path::new("assets/textures/blocks").join(format!("{name}.png"));
    write_png(&path, ed.width, ed.height, &ed.bytes())?;
    Ok(path.display().to_string())
}

fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut ed: ResMut<TextureEditor>,
    mats: Res<BlockMaterials>,
    mut images: ResMut<Assets<Image>>,
    mut notify: MessageWriter<Notify>,
) {
    if mouse.just_released(MouseButton::Left) {
        ed.painting = false;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::SuperLeft);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        run_action(Action::Undo, &mut ed, &mats, &mut images, &mut notify);
    }
    if ctrl && keys.just_pressed(KeyCode::KeyS) {
        run_action(Action::Save, &mut ed, &mats, &mut images, &mut notify);
    }
    if !ctrl {
        for (tool, _, key) in Tool::ALL {
            if keys.just_pressed(key) {
                ed.tool = tool;
                ed.dirty = true;
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn refresh(
    mut ed: ResMut<TextureEditor>,
    mut cells: Query<
        (&Cell, &mut BackgroundColor),
        (
            Without<CurrentColor>,
            Without<ToolButton>,
            Without<TexButton>,
        ),
    >,
    mut current: Query<
        &mut BackgroundColor,
        (
            With<CurrentColor>,
            Without<Cell>,
            Without<ToolButton>,
            Without<TexButton>,
        ),
    >,
    mut tools: Query<(&ToolButton, &mut BorderColor), Without<TexButton>>,
    mut texs: Query<
        (&TexButton, &mut BackgroundColor),
        (Without<Cell>, Without<CurrentColor>, Without<ToolButton>),
    >,
    mut status: Query<&mut Text, With<StatusText>>,
) {
    if !ed.dirty {
        return;
    }
    ed.dirty = false;
    for (Cell(i), mut bg) in &mut cells {
        if let Some(p) = ed.pixels.get(*i) {
            // Transparent pixels show a faint checker.
            bg.0 = if p[3] == 0 {
                if (i % ed.width as usize + i / ed.width as usize) % 2 == 0 {
                    Color::srgba(1.0, 1.0, 1.0, 0.06)
                } else {
                    Color::srgba(1.0, 1.0, 1.0, 0.02)
                }
            } else {
                to_color(*p)
            };
        }
    }
    for mut c in &mut current {
        c.0 = to_color(ed.color);
    }
    for (ToolButton(t), mut b) in &mut tools {
        *b = BorderColor::all(if *t == ed.tool {
            theme::ACCENT
        } else {
            theme::PANEL_BORDER
        });
    }
    for (TexButton(i), mut bg) in &mut texs {
        bg.0 = if *i == ed.texture {
            Color::srgba(0.2, 0.6, 0.8, 0.35)
        } else if ed.unsaved.contains(i) {
            Color::srgba(0.9, 0.6, 0.2, 0.25)
        } else {
            Color::NONE
        };
    }
    let name = &registry().textures[ed.texture].name;
    let c = ed.color;
    let unsaved = if ed.unsaved.contains(&ed.texture) {
        " · не сохранено"
    } else {
        ""
    };
    for mut t in &mut status {
        t.0 = format!(
            "{name} · {}×{} · цвет #{:02X}{:02X}{:02X}{:02X}{unsaved}",
            ed.width, ed.height, c[0], c[1], c[2], c[3]
        );
    }
}

pub struct TextureEditorPlugin;

impl Plugin for TextureEditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextureEditor>()
            .add_systems(Update, (toggle, reopen))
            .add_systems(
                Update,
                (hotkeys, refresh)
                    .chain()
                    .run_if(in_state(InputFocus::TextureEditor)),
            )
            .add_systems(OnEnter(InputFocus::TextureEditor), spawn_screen)
            .add_systems(OnExit(InputFocus::TextureEditor), despawn_screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> TextureEditor {
        TextureEditor {
            width: 4,
            height: 4,
            pixels: vec![[0, 0, 0, 255]; 16],
            color: [255, 0, 0, 255],
            ..default()
        }
    }

    #[test]
    fn pencil_eraser_picker() {
        let mut e = editor();
        assert!(e.apply(5));
        assert_eq!(e.pixels[5], [255, 0, 0, 255]);
        e.tool = Tool::Eraser;
        e.apply(5);
        assert_eq!(e.pixels[5][3], 0);
        e.tool = Tool::Picker;
        e.apply(0);
        assert_eq!(e.color, [0, 0, 0, 255]);
    }

    #[test]
    fn flood_fill_respects_borders_and_undo() {
        let mut e = editor();
        for i in [1, 5, 9, 13] {
            e.pixels[i] = [9, 9, 9, 255]; // vertical wall at x = 1
        }
        e.push_undo();
        e.tool = Tool::Fill;
        assert!(e.apply(0));
        let red = e.pixels.iter().filter(|p| **p == [255, 0, 0, 255]).count();
        assert_eq!(red, 4, "only the column left of the wall");
        e.pixels = e.undo.pop().unwrap();
        assert!(e.pixels.iter().all(|p| *p != [255, 0, 0, 255]));
    }
}
