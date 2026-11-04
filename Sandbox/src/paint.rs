//! Texture editor (Pause → «Текстуры»): paint any 32×32 texture pixel by
//! pixel and see it on the blocks immediately.
//!
//! * left — every texture of the game; middle — the canvas; right —
//!   current colour, palette (style colours + the texture's own), tools,
//!   3×3 tiling preview;
//! * LMB paints with the tool (B brush, F fill, I eyedropper, X eraser),
//!   RMB always picks a colour; `[` / `]` darker / lighter;
//!   Ctrl+Z undo, Ctrl+S save, Esc back.
//! * Every stroke goes straight into the block atlas and the model
//!   materials; «Сохранить» writes `assets/textures/<name>.png`.

use bevy::{
    image::{ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    ui::RelativeCursorPosition,
};

use crate::{
    content::registry,
    core::{Toast, Ui},
    grid::render::{BlockAtlas, TILE, load_texture, pixel_image},
    style::Materials,
    ui::{FRAME, font, rivets},
};

const N: usize = TILE as usize;
const CELL: f32 = 14.0;
const UNDO: usize = 64;
const PANEL: Color = Color::srgba(0.08, 0.09, 0.11, 0.95);
const BUTTON: Color = Color::srgb(0.18, 0.19, 0.22);
const ON: Color = Color::srgb(0.42, 0.33, 0.14);
const ACCENT: Color = Color::srgb(0.95, 0.76, 0.33);
const TEXT: Color = Color::srgb(0.92, 0.92, 0.9);
const DIM: Color = Color::srgb(0.6, 0.6, 0.62);

/// Steel, brass, copper, wood, rubber, glow — the house palette.
const STYLE: [[u8; 4]; 16] = [
    [0x1c, 0x1e, 0x22, 255],
    [0x3a, 0x3e, 0x47, 255],
    [0x5d, 0x63, 0x6e, 255],
    [0x8c, 0x93, 0x9e, 255],
    [0xc9, 0xcd, 0xd3, 255],
    [0x6b, 0x4a, 0x1c, 255],
    [0xa8, 0x7a, 0x2e, 255],
    [0xd4, 0xa8, 0x4f, 255],
    [0x7a, 0x3e, 0x24, 255],
    [0xb8, 0x6b, 0x3c, 255],
    [0x5a, 0x3d, 0x26, 255],
    [0x8f, 0x6a, 0x42, 255],
    [0x24, 0x22, 0x21, 255],
    [0xc0, 0x3a, 0x2b, 255],
    [0x3f, 0xa7, 0xd6, 255],
    [0xf2, 0xe6, 0xc8, 255],
];
const SWATCHES: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Brush,
    Fill,
    Pick,
    Eraser,
}

/// The texture being edited.
#[derive(Resource)]
pub struct Paint {
    pub name: String,
    /// RGBA8, TILE×TILE.
    pub px: Vec<u8>,
    pub color: [u8; 4],
    pub tool: Tool,
    pub undo: Vec<Vec<u8>>,
    /// Changed since the last save.
    pub unsaved: bool,
    /// Pixels changed this frame: push them to the game.
    pub changed: bool,
    pub palette: Vec<[u8; 4]>,
    canvas: Handle<Image>,
}

impl Default for Paint {
    fn default() -> Self {
        Self { name: String::new(), px: vec![0; N * N * 4], color: STYLE[7], tool: Tool::Brush, undo: Vec::new(), unsaved: false, changed: false, palette: STYLE.to_vec(), canvas: Handle::default() }
    }
}

impl Paint {
    pub fn select(&mut self, name: &str) {
        self.name = name.to_string();
        self.px = load_texture(name);
        self.undo.clear();
        self.unsaved = false;
        self.changed = true;
        self.palette = palette(&self.px);
    }
    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * N + x) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
    pub fn set(&mut self, x: usize, y: usize, c: [u8; 4]) {
        let i = (y * N + x) * 4;
        if self.px[i..i + 4] != c {
            self.px[i..i + 4].copy_from_slice(&c);
            self.changed = true;
            self.unsaved = true;
        }
    }
    pub fn snapshot(&mut self) {
        self.undo.push(self.px.clone());
        if self.undo.len() > UNDO {
            self.undo.remove(0);
        }
    }
    pub fn undo(&mut self) -> bool {
        let Some(p) = self.undo.pop() else { return false };
        self.px = p;
        self.changed = true;
        self.unsaved = true;
        true
    }
    /// Flood fill of the area of the same colour (4-neighbours).
    pub fn fill(&mut self, x: usize, y: usize, c: [u8; 4]) {
        let from = self.get(x, y);
        if from == c {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((x, y)) = stack.pop() {
            if self.get(x, y) != from {
                continue;
            }
            self.set(x, y, c);
            if x > 0 { stack.push((x - 1, y)) }
            if y > 0 { stack.push((x, y - 1)) }
            if x + 1 < N { stack.push((x + 1, y)) }
            if y + 1 < N { stack.push((x, y + 1)) }
        }
    }
    /// One stroke of the current tool at a pixel (LMB).
    pub fn apply(&mut self, x: usize, y: usize) {
        match self.tool {
            Tool::Brush => self.set(x, y, self.color),
            Tool::Eraser => self.set(x, y, [0, 0, 0, 0]),
            Tool::Fill => self.fill(x, y, self.color),
            Tool::Pick => self.color = self.get(x, y),
        }
    }
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        image::save_buffer(path, &self.px, TILE, TILE, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())
    }
}

/// The style colours followed by the texture's most used own colours.
fn palette(px: &[u8]) -> Vec<[u8; 4]> {
    let mut count: Vec<([u8; 4], u32)> = Vec::new();
    for p in px.chunks(4) {
        let c = [p[0], p[1], p[2], p[3]];
        if c[3] == 0 || STYLE.contains(&c) {
            continue;
        }
        match count.iter_mut().find(|(k, _)| *k == c) {
            Some((_, n)) => *n += 1,
            None => count.push((c, 1)),
        }
    }
    count.sort_by(|a, b| b.1.cmp(&a.1));
    STYLE.iter().copied().chain(count.into_iter().map(|(c, _)| c)).take(SWATCHES).collect()
}

/// Lighter (`k` > 1) or darker colour.
pub fn shade(c: [u8; 4], k: f32) -> [u8; 4] {
    let f = |v: u8| ((v as f32 * k).round().clamp(0.0, 255.0)) as u8;
    let c2 = [f(c[0]), f(c[1]), f(c[2]), c[3]];
    // Black stays black when lightening otherwise.
    if k > 1.0 && c2[..3] == c[..3] { [c[0].saturating_add(8), c[1].saturating_add(8), c[2].saturating_add(8), c[3]] } else { c2 }
}

/// All editable textures: block atlas + model materials.
pub fn texture_names() -> Vec<String> {
    let mut v: Vec<String> = registry().textures.clone();
    for (m, _, _) in crate::style::MATERIALS {
        if !v.iter().any(|t| t == m) {
            v.push(m.to_string());
        }
    }
    v.sort();
    v
}

#[derive(Component)]
struct PaintRoot;
#[derive(Component)]
struct Canvas;
#[derive(Component)]
struct TexButton(String);
#[derive(Component)]
struct Swatch(usize);
#[derive(Component)]
struct Current;
#[derive(Component, Clone, Copy)]
enum Act {
    Tool(Tool),
    Darker,
    Lighter,
    Undo,
    Save,
    Revert,
}
#[derive(Component)]
struct Status;

fn btn(p: &mut ChildSpawnerCommands, label: &str, w: f32, extra: impl Bundle) {
    p.spawn((extra, Node { width: px(w), height: px(24), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() }, BackgroundColor(BUTTON), BorderColor::all(FRAME)))
        .observe(on_click)
        .with_children(|b| {
            b.spawn((Text::new(label), font(12.0), TextColor(TEXT), Pickable::IGNORE));
        });
}

fn open(mut commands: Commands, mut paint: ResMut<Paint>, mut images: ResMut<Assets<Image>>, mut time: ResMut<Time<Virtual>>) {
    time.pause();
    if paint.name.is_empty() {
        paint.select("steel");
    }
    let mut img = pixel_image(TILE, TILE, paint.px.clone());
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    paint.canvas = images.add(img);
    paint.changed = true;
    let canvas = paint.canvas.clone();
    let names = texture_names();
    commands
        .spawn((PaintRoot, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35))))
        .with_children(|root| {
            root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::all(px(16)), border: UiRect::all(px(2)), ..default() }, BackgroundColor(PANEL), BorderColor::all(FRAME)))
                .with_children(|w| {
                    rivets(w);
                    w.spawn((Status, Text::new(""), font(18.0), TextColor(ACCENT)));
                    w.spawn(Node { column_gap: px(14), ..default() }).with_children(|row| {
                        // Textures: two columns.
                        row.spawn(Node { flex_direction: FlexDirection::Column, flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(3), height: px(N as f32 * CELL), align_content: AlignContent::FlexStart, ..default() })
                            .with_children(|l| {
                                for n in &names {
                                    btn(l, n, 112.0, TexButton(n.clone()));
                                }
                            });
                        // Canvas.
                        row.spawn((Canvas, RelativeCursorPosition::default(), ImageNode::new(canvas.clone()), Node { width: px(N as f32 * CELL), height: px(N as f32 * CELL), border: UiRect::all(px(2)), ..default() }, BorderColor::all(FRAME)));
                        // Colour and tools.
                        row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), width: px(176), ..default() }).with_children(|r| {
                            r.spawn((Current, Node { width: px(176), height: px(28), border: UiRect::all(px(2)), ..default() }, BackgroundColor(Color::WHITE), BorderColor::all(FRAME)));
                            r.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(2), row_gap: px(2), ..default() }).with_children(|g| {
                                for i in 0..SWATCHES {
                                    g.spawn((Swatch(i), Node { width: px(20), height: px(20), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::NONE), BorderColor::all(FRAME))).observe(on_click);
                                }
                            });
                            r.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|t| {
                                for (label, a) in [("Кисть B", Act::Tool(Tool::Brush)), ("Заливка F", Act::Tool(Tool::Fill)), ("Пипетка I", Act::Tool(Tool::Pick)), ("Ластик X", Act::Tool(Tool::Eraser)), ("Темнее [", Act::Darker), ("Светлее ]", Act::Lighter), ("Отмена", Act::Undo), ("Сброс", Act::Revert)] {
                                    btn(t, label, 86.0, a);
                                }
                                btn(t, "Сохранить Ctrl+S", 176.0, Act::Save);
                            });
                            // 3×3 tiling preview: seams show at once.
                            r.spawn(Node { flex_wrap: FlexWrap::Wrap, width: px(96), margin: UiRect::top(px(4)), ..default() }).with_children(|t| {
                                for _ in 0..9 {
                                    t.spawn((ImageNode::new(canvas.clone()), Node { width: px(32), height: px(32), ..default() }));
                                }
                            });
                        });
                    });
                    w.spawn((Text::new("ЛКМ — инструмент · ПКМ — пипетка · Ctrl+Z — отмена · Esc — назад"), font(12.0), TextColor(DIM)));
                });
        });
}

fn on_click(ev: On<PointerClick>, texs: Query<&TexButton>, swatches: Query<&Swatch>, acts: Query<&Act>, mut paint: ResMut<Paint>, mut toasts: MessageWriter<Toast>) {
    let e = ev.entity;
    if let Ok(t) = texs.get(e) {
        paint.select(&t.0);
    } else if let Ok(s) = swatches.get(e) {
        if let Some(c) = paint.palette.get(s.0).copied() {
            paint.color = c;
            if paint.tool == Tool::Eraser || paint.tool == Tool::Pick {
                paint.tool = Tool::Brush;
            }
        }
    } else if let Ok(a) = acts.get(e) {
        act(&mut paint, *a, &mut toasts);
    }
}

fn act(paint: &mut Paint, a: Act, toasts: &mut MessageWriter<Toast>) {
    match a {
        Act::Tool(t) => paint.tool = t,
        Act::Darker => paint.color = shade(paint.color, 0.88),
        Act::Lighter => paint.color = shade(paint.color, 1.12),
        Act::Undo => {
            paint.undo();
        }
        Act::Revert => {
            let n = paint.name.clone();
            paint.snapshot();
            let undo = std::mem::take(&mut paint.undo);
            paint.select(&n);
            paint.undo = undo;
        }
        Act::Save => {
            let path = std::path::PathBuf::from(format!("assets/textures/{}.png", paint.name));
            match paint.save_to(&path) {
                Ok(()) => {
                    paint.unsaved = false;
                    toasts.write(Toast(format!("Сохранено: {}", path.display())));
                }
                Err(e) => {
                    toasts.write(Toast(format!("Не удалось сохранить: {e}")));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    canvas: Query<&RelativeCursorPosition, With<Canvas>>,
    mut paint: ResMut<Paint>,
    mut toasts: MessageWriter<Toast>,
    mut last: Local<Option<(usize, usize)>>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        act(&mut paint, Act::Undo, &mut toasts);
    } else if ctrl && keys.just_pressed(KeyCode::KeyS) {
        act(&mut paint, Act::Save, &mut toasts);
    } else if !ctrl {
        for (k, a) in [(KeyCode::KeyB, Act::Tool(Tool::Brush)), (KeyCode::KeyF, Act::Tool(Tool::Fill)), (KeyCode::KeyI, Act::Tool(Tool::Pick)), (KeyCode::KeyX, Act::Tool(Tool::Eraser)), (KeyCode::BracketLeft, Act::Darker), (KeyCode::BracketRight, Act::Lighter)] {
            if keys.just_pressed(k) {
                act(&mut paint, a, &mut toasts);
            }
        }
    }
    let Ok(rel) = canvas.single() else { return };
    let cell = rel.normalized.filter(|_| rel.cursor_over).map(|p| {
        let f = |v: f32| (((v + 0.5) * N as f32) as usize).min(N - 1);
        (f(p.x), f(p.y))
    });
    let Some((x, y)) = cell else {
        *last = None;
        return;
    };
    if mouse.just_pressed(MouseButton::Right) {
        paint.color = paint.get(x, y);
    }
    if mouse.just_pressed(MouseButton::Left) {
        if paint.tool != Tool::Pick {
            paint.snapshot();
        }
        paint.apply(x, y);
        *last = Some((x, y));
    } else if mouse.pressed(MouseButton::Left) && matches!(paint.tool, Tool::Brush | Tool::Eraser) {
        // Fast strokes: fill the gap since the last frame.
        let (x0, y0) = last.unwrap_or((x, y));
        let steps = (x as i32 - x0 as i32).abs().max((y as i32 - y0 as i32).abs()).max(1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let px = (x0 as f32 + (x as f32 - x0 as f32) * t).round() as usize;
            let py = (y0 as f32 + (y as f32 - y0 as f32) * t).round() as usize;
            paint.apply(px, py);
        }
        *last = Some((x, y));
    } else {
        *last = None;
    }
}

/// Push edited pixels to the canvas, the block atlas and model materials.
fn publish(mut paint: ResMut<Paint>, mut images: ResMut<Assets<Image>>, atlas: Option<Res<BlockAtlas>>, materials: Option<Res<Materials>>, mats: Res<Assets<StandardMaterial>>) {
    if !paint.changed {
        return;
    }
    paint.changed = false;
    if let Some(mut img) = images.get_mut(&paint.canvas) {
        img.data = Some(paint.px.clone());
    }
    if let Some(atlas) = atlas
        && let Some(i) = registry().textures.iter().position(|t| *t == paint.name)
        && let Some(mut img) = images.get_mut(&atlas.image)
    {
        let w = img.width() as usize;
        let cols = w / N;
        let (c, r) = (i % cols, i / cols);
        if let Some(data) = img.data.as_mut() {
            for y in 0..N {
                let dst = ((r * N + y) * w + c * N) * 4;
                data[dst..dst + N * 4].copy_from_slice(&paint.px[y * N * 4..(y + 1) * N * 4]);
            }
        }
    }
    if let Some(m) = materials
        && let Some(h) = m.0.get(&paint.name)
        && let Some(tex) = mats.get(h).and_then(|m| m.base_color_texture.clone())
        && let Some(mut img) = images.get_mut(&tex)
    {
        img.data = Some(paint.px.clone());
    }
}

fn refresh(paint: Res<Paint>, mut status: Query<&mut Text, With<Status>>, mut current: Query<&mut BackgroundColor, (With<Current>, Without<Swatch>, Without<Act>, Without<TexButton>)>, mut swatches: Query<(&Swatch, &mut BackgroundColor), (Without<Act>, Without<TexButton>)>, mut acts: Query<(&Act, &mut BackgroundColor), Without<TexButton>>, mut texs: Query<(&TexButton, &mut BackgroundColor)>) {
    if !paint.is_changed() {
        return;
    }
    let col = |c: [u8; 4]| Color::srgba_u8(c[0], c[1], c[2], c[3]);
    for mut t in &mut status {
        t.0 = format!("Текстура: {}{}", paint.name, if paint.unsaved { " •" } else { "" });
    }
    for mut b in &mut current {
        b.0 = col(paint.color);
    }
    for (s, mut b) in &mut swatches {
        b.0 = paint.palette.get(s.0).map(|c| col(*c)).unwrap_or(Color::NONE);
    }
    for (a, mut b) in &mut acts {
        b.0 = if matches!(a, Act::Tool(t) if *t == paint.tool) { ON } else { BUTTON };
    }
    for (t, mut b) in &mut texs {
        b.0 = if t.0 == paint.name { ON } else { BUTTON };
    }
}

fn close(mut commands: Commands, q: Query<Entity, With<PaintRoot>>, mut time: ResMut<Time<Virtual>>) {
    time.unpause();
    for e in &q {
        commands.entity(e).despawn();
    }
}

pub struct PaintPlugin;

impl Plugin for PaintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Paint>()
            .add_systems(OnEnter(Ui::Paint), open)
            .add_systems(OnExit(Ui::Paint), close)
            .add_systems(Update, (input, publish, refresh).chain().run_if(in_state(Ui::Paint)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_fill_undo() {
        let mut p = Paint { px: vec![0; N * N * 4], ..default() };
        p.px.chunks_mut(4).for_each(|c| c.copy_from_slice(&[10, 10, 10, 255]));
        // A vertical line splits the canvas in two.
        p.snapshot();
        for y in 0..N {
            p.set(5, y, [200, 0, 0, 255]);
        }
        p.snapshot();
        p.fill(0, 0, [0, 200, 0, 255]);
        assert_eq!(p.get(4, 31), [0, 200, 0, 255], "fills its side");
        assert_eq!(p.get(6, 0), [10, 10, 10, 255], "stops at the line");
        assert!(p.undo());
        assert_eq!(p.get(0, 0), [10, 10, 10, 255]);
        assert!(p.undo());
        assert_eq!(p.get(5, 3), [10, 10, 10, 255]);
        assert!(!p.undo());
        p.tool = Tool::Pick;
        p.apply(1, 1);
        assert_eq!(p.color, [10, 10, 10, 255]);
        assert_eq!(shade([100, 100, 100, 255], 0.5), [50, 50, 50, 255]);
        assert_ne!(shade([0, 0, 0, 255], 1.12), [0, 0, 0, 255], "black can be lightened");
    }

    #[test]
    fn every_texture_is_editable() {
        let names = texture_names();
        assert!(names.iter().any(|n| n == "steel") && names.iter().any(|n| n == "player"));
        for n in &names {
            assert!(std::path::Path::new(&format!("assets/textures/{n}.png")).exists(), "{n}");
        }
    }
}
