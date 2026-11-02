use glam::Vec2;

/// Цвет RGBA в sRGB, компоненты 0..1.
pub type Color = [f32; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
    /// Прямоугольник размера `w×h` по центру этого.
    pub fn centered(&self, w: f32, h: f32) -> Rect {
        Rect::new(self.x + (self.w - w) / 2.0, self.y + (self.h - h) / 2.0, w, h)
    }
    pub fn shrink(&self, by: f32) -> Rect {
        Rect::new(self.x + by, self.y + by, self.w - 2.0 * by, self.h - 2.0 * by)
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }
    pub fn min(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
    pub fn max(&self) -> Vec2 {
        Vec2::new(self.x + self.w, self.y + self.h)
    }
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Откуда брать пиксели.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Texture {
    /// Атлас интерфейса (шрифт + белый пиксель для заливки).
    Atlas,
    /// Слой текстурного массива блоков.
    Block(u32),
}

/// Примитив интерфейса. Это весь контракт между экранами и бэкендом.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        rect: Rect,
        color: Color,
    },
    /// Произвольный четырёхугольник (углы по часовой: ЛВ, ПВ, ПН, ЛН).
    Quad {
        points: [Vec2; 4],
        uv: Rect,
        texture: Texture,
        color: Color,
    },
    /// Строка текста; `pos` — левый верхний угол, `size` — высота строки.
    Text {
        pos: Vec2,
        size: f32,
        color: Color,
        text: String,
    },
}

/// Список примитивов в порядке рисования.
#[derive(Default, Debug)]
pub struct DrawList {
    pub shapes: Vec<Shape>,
}

impl DrawList {
    pub fn rect(&mut self, rect: Rect, color: Color) {
        self.shapes.push(Shape::Rect { rect, color });
    }

    pub fn image(&mut self, rect: Rect, texture: Texture, color: Color) {
        let (a, b) = (rect.min(), rect.max());
        let points = [a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)];
        self.quad(points, texture, color);
    }

    pub fn quad(&mut self, points: [Vec2; 4], texture: Texture, color: Color) {
        self.shapes.push(Shape::Quad { points, uv: Rect::new(0.0, 0.0, 1.0, 1.0), texture, color });
    }

    pub fn text(&mut self, pos: Vec2, size: f32, color: Color, text: impl Into<String>) {
        self.shapes.push(Shape::Text { pos, size, color, text: text.into() });
    }
}
