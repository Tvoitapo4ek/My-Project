use glam::Vec2;
use vx_core::inventory::HOTBAR;

use super::UiContext;
use crate::{Rect, Ui};

/// HUD поверх мира: прицел, панель быстрого доступа, сообщения, отладка.
pub fn draw(ui: &mut Ui, ctx: &UiContext, debug: bool, crosshair: bool) {
    let screen = ui.screen();
    let c = screen.center();
    if crosshair {
        let (len, w) = (9.0, 2.0);
        for r in [Rect::new(c.x - len, c.y - w / 2.0, 2.0 * len, w), Rect::new(c.x - w / 2.0, c.y - len, w, 2.0 * len)]
        {
            ui.fill(Rect::new(r.x + 1.0, r.y + 1.0, r.w, r.h), ui.theme.shadow);
            ui.fill(r, [1.0, 1.0, 1.0, 0.9]);
        }
    }

    // Панель быстрого доступа.
    let s = ui.theme.slot_size;
    let bar = Rect::new(c.x - s * HOTBAR as f32 / 2.0, screen.h - s - 8.0, s * HOTBAR as f32, s);
    for i in 0..HOTBAR {
        let rect = Rect::new(bar.x + i as f32 * s, bar.y, s, s);
        ui.slot(rect, ctx.inventory.slots[i], ctx.reg, i == ctx.inventory.selected);
    }
    if let Some(stack) = ctx.inventory.selected_stack() {
        let name = ctx.reg.get(stack.block).title();
        ui.text_centered(Rect::new(0.0, bar.y - 28.0, screen.w, 20.0), name, 16.0, ui.theme.text);
    }

    // Сообщения — над панелью слева.
    let size = ui.theme.font_size;
    for (i, msg) in ctx.hud.messages.iter().rev().enumerate() {
        let y = bar.y - 40.0 - (i as f32 + 1.0) * (size + 6.0);
        let w = ui.text_width(msg, size);
        ui.fill(Rect::new(6.0, y - 2.0, w + 12.0, size + 6.0), ui.theme.dim);
        ui.text(Vec2::new(12.0, y), msg, size, ui.theme.text);
    }

    if ctx.settings.show_fps && ctx.hud.fps > 0.0 {
        let fps = format!("{:.0} FPS", ctx.hud.fps);
        let w = ui.text_width(&fps, size);
        ui.text(Vec2::new(screen.w - w - 10.0, 8.0), &fps, size, ui.theme.text_dim);
    }
    if debug {
        for (i, line) in ctx.hud.debug.iter().enumerate() {
            let y = 8.0 + i as f32 * (size + 4.0);
            let w = ui.text_width(line, size);
            ui.fill(Rect::new(4.0, y - 1.0, w + 8.0, size + 4.0), ui.theme.dim);
            ui.text(Vec2::new(8.0, y), line, size, ui.theme.text);
        }
    }
}
