use glam::Vec2;

use super::{Action, Nav, Screen, Settings};
use crate::{Column, Rect, Ui};

const WIDTH: f32 = 320.0;

pub fn main_menu(ui: &mut Ui, nav: &mut Nav, actions: &mut Vec<Action>) {
    let screen = ui.screen();
    let top = screen.h * 0.22;
    let title = Rect::new(0.0, top, screen.w, 64.0);
    ui.text_centered(title, "VOXEL", 64.0, ui.theme.accent);
    let sub = Rect::new(0.0, top + 70.0, screen.w, 20.0);
    ui.text_centered(sub, "воксельный движок на Rust", 18.0, ui.theme.text_dim);

    let h = ui.theme.button_height;
    let mut col = Column::centered(screen, top + 130.0, WIDTH, 12.0);
    if ui.button(col.next(h), "Играть") {
        nav.go(Screen::Playing);
    }
    if ui.button(col.next(h), "Настройки") {
        nav.go(Screen::Settings);
    }
    if ui.button(col.next(h), "Выход") {
        actions.push(Action::Quit);
    }
    let hint = "WASD — ходьба · E — инвентарь · G — собрать корабль · Esc — меню";
    ui.text(Vec2::new(10.0, screen.h - 24.0), hint, 14.0, ui.theme.text_dim);
}

pub fn pause(ui: &mut Ui, nav: &mut Nav, actions: &mut Vec<Action>) {
    let screen = ui.screen();
    ui.fill(screen, ui.theme.dim);
    let top = screen.h * 0.25;
    ui.text_centered(Rect::new(0.0, top, screen.w, 32.0), "Пауза", 32.0, ui.theme.text);
    let h = ui.theme.button_height;
    let mut col = Column::centered(screen, top + 60.0, WIDTH, 12.0);
    if ui.button(col.next(h), "Вернуться к игре") {
        nav.go(Screen::Playing);
    }
    if ui.button(col.next(h), "Настройки") {
        nav.go(Screen::Settings);
    }
    if ui.button(col.next(h), "Главное меню") {
        nav.go(Screen::MainMenu);
    }
    if ui.button(col.next(h), "Выход из игры") {
        actions.push(Action::Quit);
    }
}

pub fn settings(ui: &mut Ui, nav: &mut Nav, s: &mut Settings, in_game: bool) {
    let screen = ui.screen();
    if in_game {
        ui.fill(screen, ui.theme.dim);
    }
    let h = ui.theme.button_height;
    let w = WIDTH + 80.0;
    let panel = screen.centered(w + 40.0, 60.0 + 6.0 * (h + 12.0) + 20.0);
    ui.panel(panel);
    ui.text_centered(Rect::new(panel.x, panel.y + 14.0, panel.w, 24.0), "Настройки", 24.0, ui.theme.text);

    let mut col = Column::centered(panel, panel.y + 60.0, w, 12.0);
    ui.slider(col.next(h), &format!("Чувствительность мыши: {:.2}", s.sensitivity), &mut s.sensitivity, 0.1..=3.0);
    ui.slider(col.next(h), &format!("Поле зрения: {:.0}°", s.fov), &mut s.fov, 50.0..=110.0);
    ui.slider(col.next(h), &format!("Масштаб интерфейса: {:.2}×", s.ui_scale), &mut s.ui_scale, 0.5..=2.5);
    ui.toggle(col.next(h), "Творческий режим", &mut s.creative);
    ui.toggle(col.next(h), "Показывать FPS", &mut s.show_fps);
    col.next(0.0);
    if ui.button(col.next(h), "Готово") {
        nav.screen = nav.back;
    }
}
