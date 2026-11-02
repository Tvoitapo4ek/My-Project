use glam::Vec2;
use vx_core::{BlockDef, Face, Registry, inventory::Stack};

use crate::{Color, DrawList, FontAtlas, Rect, Texture};

/// Клавиши, важные интерфейсу. Бэкенд ввода (winit, SDL, …) сам решает,
/// какие физические клавиши им соответствуют.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiKey {
    Escape,
    Inventory,
    Enter,
    Debug,
}

/// Ввод за кадр в логических пикселях.
#[derive(Clone, Debug, Default)]
pub struct UiInput {
    pub screen: Vec2,
    pub mouse: Vec2,
    /// Нажато в этом кадре: [левая, правая].
    pub pressed: [bool; 2],
    /// Зажато сейчас: [левая, правая].
    pub down: [bool; 2],
    pub keys: Vec<UiKey>,
    pub scroll: f32,
}

impl UiInput {
    pub fn key(&self, key: UiKey) -> bool {
        self.keys.contains(&key)
    }
}

/// Цвета и размеры. Поменять стиль = поменять тему.
#[derive(Clone, Debug)]
pub struct Theme {
    pub text: Color,
    pub text_dim: Color,
    pub shadow: Color,
    pub accent: Color,
    pub panel: Color,
    pub border: Color,
    pub button: Color,
    pub button_hover: Color,
    pub slot: Color,
    pub slot_hover: Color,
    pub dim: Color,
    pub font_size: f32,
    pub button_height: f32,
    pub slot_size: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            text: [1.0, 1.0, 1.0, 1.0],
            text_dim: [0.75, 0.75, 0.78, 1.0],
            shadow: [0.0, 0.0, 0.0, 0.55],
            accent: [1.0, 0.85, 0.3, 1.0],
            panel: [0.12, 0.12, 0.14, 0.88],
            border: [0.0, 0.0, 0.0, 0.9],
            button: [0.32, 0.33, 0.36, 0.95],
            button_hover: [0.45, 0.5, 0.62, 0.98],
            slot: [0.2, 0.2, 0.22, 0.85],
            slot_hover: [0.4, 0.4, 0.44, 0.9],
            dim: [0.0, 0.0, 0.0, 0.5],
            font_size: 16.0,
            button_height: 36.0,
            slot_size: 40.0,
        }
    }
}

/// Состояние виджетов между кадрами (например, какой ползунок тянут).
#[derive(Default)]
pub struct Memory {
    active: Option<u64>,
}

/// Отклик слота инвентаря.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlotResponse {
    pub hovered: bool,
    pub primary: bool,
    pub secondary: bool,
}

/// Контекст immediate-mode интерфейса: виджеты рисуют себя в `draw`
/// и сразу возвращают результат взаимодействия.
pub struct Ui<'a> {
    pub draw: DrawList,
    pub input: &'a UiInput,
    pub font: &'a FontAtlas,
    pub theme: &'a Theme,
    memory: &'a mut Memory,
    /// Мышь уже «поглощена» виджетом в этом кадре.
    consumed: bool,
}

fn id(label: &str) -> u64 {
    label.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

impl<'a> Ui<'a> {
    pub fn new(input: &'a UiInput, font: &'a FontAtlas, theme: &'a Theme, memory: &'a mut Memory) -> Self {
        if !input.down[0] {
            memory.active = None;
        }
        Ui { draw: DrawList::default(), input, font, theme, memory, consumed: false }
    }

    pub fn screen(&self) -> Rect {
        Rect::new(0.0, 0.0, self.input.screen.x, self.input.screen.y)
    }

    pub fn hovered(&self, rect: Rect) -> bool {
        rect.contains(self.input.mouse)
    }

    /// Клик левой кнопкой по области (один виджет за кадр).
    pub fn clicked(&mut self, rect: Rect) -> bool {
        let hit = !self.consumed && self.input.pressed[0] && self.hovered(rect);
        self.consumed |= hit;
        hit
    }

    /// Был ли клик где-то вне виджетов.
    pub fn clicked_nowhere(&self) -> bool {
        !self.consumed && (self.input.pressed[0] || self.input.pressed[1])
    }

    pub fn fill(&mut self, rect: Rect, color: Color) {
        self.draw.rect(rect, color);
    }

    /// Панель с рамкой.
    pub fn panel(&mut self, rect: Rect) {
        self.draw.rect(rect.shrink(-2.0), self.theme.border);
        self.draw.rect(rect, self.theme.panel);
    }

    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        self.font.text_width(text, size)
    }

    /// Текст с тенью.
    pub fn text(&mut self, pos: Vec2, text: &str, size: f32, color: Color) {
        let shadow = (size / 12.0).max(1.0);
        self.draw.text(pos + Vec2::splat(shadow), size, self.theme.shadow, text);
        self.draw.text(pos, size, color, text);
    }

    pub fn text_centered(&mut self, rect: Rect, text: &str, size: f32, color: Color) {
        let w = self.text_width(text, size);
        self.text(Vec2::new(rect.x + (rect.w - w) / 2.0, rect.y + (rect.h - size) / 2.0), text, size, color);
    }

    pub fn button(&mut self, rect: Rect, label: &str) -> bool {
        let hover = self.hovered(rect);
        self.draw.rect(rect.shrink(-2.0), self.theme.border);
        self.draw.rect(rect, if hover { self.theme.button_hover } else { self.theme.button });
        let color = if hover { self.theme.accent } else { self.theme.text };
        self.text_centered(rect, label, self.theme.font_size, color);
        self.clicked(rect)
    }

    /// Ползунок. Возвращает `true`, если значение изменилось.
    pub fn slider(&mut self, rect: Rect, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> bool {
        let key = id(label);
        if self.clicked(rect) {
            self.memory.active = Some(key);
        }
        let (lo, hi) = (*range.start(), *range.end());
        let old = *value;
        if self.memory.active == Some(key) {
            let t = ((self.input.mouse.x - rect.x) / rect.w).clamp(0.0, 1.0);
            *value = lo + t * (hi - lo);
        }
        let t = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
        self.draw.rect(rect.shrink(-2.0), self.theme.border);
        self.draw.rect(rect, self.theme.button);
        self.draw.rect(Rect::new(rect.x, rect.y, rect.w * t, rect.h), self.theme.button_hover);
        self.draw.rect(Rect::new(rect.x + rect.w * t - 2.0, rect.y - 2.0, 4.0, rect.h + 4.0), self.theme.accent);
        self.text_centered(rect, label, self.theme.font_size, self.theme.text);
        *value != old
    }

    /// Переключатель «вкл/выкл» в виде кнопки.
    pub fn toggle(&mut self, rect: Rect, label: &str, value: &mut bool) -> bool {
        let text = format!("{label}: {}", if *value { "вкл" } else { "выкл" });
        let changed = self.button(rect, &text);
        if changed {
            *value = !*value;
        }
        changed
    }

    /// Иконка блока — изометрический кубик из текстур граней.
    pub fn block_icon(&mut self, rect: Rect, def: &BlockDef) {
        let s = rect.w.min(rect.h);
        let (t, l, r, cx, q) = (rect.y + (rect.h - s) / 2.0, rect.x, rect.x + s, rect.x + s / 2.0, s / 4.0);
        let b = t + s;
        let p = |x: f32, y: f32| Vec2::new(x, y);
        let shade = |k: f32| [k, k, k, 1.0];
        let tex = |f: Face| Texture::Block(def.faces[f as usize]);
        self.draw.quad([p(l, t + q), p(cx, t), p(r, t + q), p(cx, t + 2.0 * q)], tex(Face::PosY), shade(1.0));
        self.draw.quad([p(l, t + q), p(cx, t + 2.0 * q), p(cx, b), p(l, b - q)], tex(Face::PosZ), shade(0.78));
        self.draw.quad([p(cx, t + 2.0 * q), p(r, t + q), p(r, b - q), p(cx, b)], tex(Face::PosX), shade(0.6));
    }

    /// Стопка предметов: иконка + количество.
    pub fn stack(&mut self, rect: Rect, stack: Stack, reg: &Registry) {
        self.block_icon(rect.shrink(rect.w * 0.12), reg.get(stack.block));
        if stack.count > 1 {
            let text = stack.count.to_string();
            let size = self.theme.font_size * 0.85;
            let w = self.text_width(&text, size);
            self.text(Vec2::new(rect.x + rect.w - w - 2.0, rect.y + rect.h - size - 1.0), &text, size, self.theme.text);
        }
    }

    /// Слот инвентаря. Клики обрабатывает вызывающий.
    pub fn slot(&mut self, rect: Rect, stack: Option<Stack>, reg: &Registry, highlight: bool) -> SlotResponse {
        let hovered = self.hovered(rect);
        self.draw.rect(rect, self.theme.border);
        let inner = rect.shrink(2.0);
        self.draw.rect(inner, if hovered { self.theme.slot_hover } else { self.theme.slot });
        if highlight {
            let a = self.theme.accent;
            for side in [
                Rect::new(rect.x, rect.y, rect.w, 2.0),
                Rect::new(rect.x, rect.y + rect.h - 2.0, rect.w, 2.0),
                Rect::new(rect.x, rect.y, 2.0, rect.h),
                Rect::new(rect.x + rect.w - 2.0, rect.y, 2.0, rect.h),
            ] {
                self.draw.rect(side, a);
            }
        }
        if let Some(s) = stack {
            self.stack(inner, s, reg);
        }
        let free = !self.consumed && hovered;
        let r =
            SlotResponse { hovered, primary: free && self.input.pressed[0], secondary: free && self.input.pressed[1] };
        self.consumed |= r.primary || r.secondary;
        r
    }

    /// Всплывающая подсказка у курсора.
    pub fn tooltip(&mut self, text: &str) {
        let size = self.theme.font_size;
        let w = self.text_width(text, size) + 12.0;
        let mut p = self.input.mouse + Vec2::new(14.0, -size - 12.0);
        p.x = p.x.min(self.input.screen.x - w - 4.0);
        let rect = Rect::new(p.x, p.y, w, size + 8.0);
        self.panel(rect);
        self.text(Vec2::new(rect.x + 6.0, rect.y + 4.0), text, size, self.theme.text);
    }
}

/// Простая вертикальная раскладка: каждый `next()` выдаёт прямоугольник ниже предыдущего.
pub struct Column {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub gap: f32,
}

impl Column {
    /// Колонка шириной `w` по центру `area`, начиная с `y`.
    pub fn centered(area: Rect, y: f32, w: f32, gap: f32) -> Self {
        Column { x: area.x + (area.w - w) / 2.0, y, w, gap }
    }
    pub fn next(&mut self, h: f32) -> Rect {
        let r = Rect::new(self.x, self.y, self.w, h);
        self.y += h + self.gap;
        r
    }
}
