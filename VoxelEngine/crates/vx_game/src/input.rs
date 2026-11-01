use std::collections::HashSet;

use glam::Vec2;
use winit::{event::MouseButton, keyboard::KeyCode};

/// Состояние ввода за кадр: что зажато и что нажато именно в этом кадре.
#[derive(Default)]
pub struct Input {
    held: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    buttons_held: HashSet<MouseButton>,
    buttons_pressed: HashSet<MouseButton>,
    pub mouse_delta: Vec2,
    pub scroll: f32,
}

impl Input {
    pub fn key(&mut self, key: KeyCode, down: bool) {
        if down {
            if self.held.insert(key) {
                self.pressed.insert(key);
            }
        } else {
            self.held.remove(&key);
        }
    }

    pub fn button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.buttons_held.insert(button);
            self.buttons_pressed.insert(button);
        } else {
            self.buttons_held.remove(&button);
        }
    }

    pub fn held(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }
    pub fn clicked(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }
    pub fn button_held(&self, button: MouseButton) -> bool {
        self.buttons_held.contains(&button)
    }

    /// Вызывать в конце кадра.
    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.buttons_pressed.clear();
        self.mouse_delta = Vec2::ZERO;
        self.scroll = 0.0;
    }

    /// Окно потеряло фокус — отпустить всё.
    pub fn clear(&mut self) {
        *self = Input::default();
    }
}
