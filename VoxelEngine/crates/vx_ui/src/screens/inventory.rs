use glam::Vec2;
use vx_core::inventory::{HOTBAR, MAX_STACK, Stack};

use super::UiContext;
use crate::{Rect, Ui};

const PAD: f32 = 12.0;
const TITLE: f32 = 26.0;

/// Экран инвентаря: творческая палитра, основной инвентарь и панель быстрого доступа.
pub fn draw(ui: &mut Ui, ctx: UiContext) {
    let screen = ui.screen();
    ui.fill(screen, ui.theme.dim);
    let s = ui.theme.slot_size;
    let cols = HOTBAR;
    let main_rows = (ctx.inventory.slots.len() - HOTBAR).div_ceil(cols);
    let palette_rows = if ctx.settings.creative { ctx.palette.len().div_ceil(cols) } else { 0 };

    let grid_h = |rows: usize| rows as f32 * s;
    let mut h = PAD + TITLE + grid_h(main_rows) + 8.0 + s + PAD;
    if palette_rows > 0 {
        h += TITLE + grid_h(palette_rows) + 8.0;
    }
    let panel = screen.centered(cols as f32 * s + 2.0 * PAD, h);
    ui.panel(panel);

    let mut y = panel.y + PAD;
    let x = panel.x + PAD;
    let cell = |i: usize, y: f32| Rect::new(x + (i % cols) as f32 * s, y + (i / cols) as f32 * s, s, s);
    let mut tooltip = None;

    if palette_rows > 0 {
        ui.text(Vec2::new(x, y), "Блоки", 18.0, ui.theme.text);
        y += TITLE;
        for (i, &block) in ctx.palette.iter().enumerate() {
            let r = ui.slot(cell(i, y), Some(Stack::new(block, 1)), ctx.reg, false);
            if r.hovered {
                tooltip = Some(ctx.reg.get(block).title().to_string());
            }
            if r.primary || r.secondary {
                // В творческом режиме клик по палитре с предметом в руке — «мусорка».
                ctx.inventory.cursor = match ctx.inventory.cursor {
                    Some(_) => None,
                    None => Some(Stack::new(block, if r.primary { MAX_STACK } else { 1 })),
                };
            }
        }
        y += grid_h(palette_rows) + 8.0;
    }

    ui.text(Vec2::new(x, y), "Инвентарь", 18.0, ui.theme.text);
    y += TITLE;
    let total = ctx.inventory.slots.len();
    let mut slot = |ui: &mut Ui, index: usize, rect: Rect| {
        let stack = ctx.inventory.slots[index];
        let r = ui.slot(rect, stack, ctx.reg, false);
        if r.hovered
            && let Some(st) = stack
        {
            tooltip = Some(ctx.reg.get(st.block).title().to_string());
        }
        if r.primary {
            ctx.inventory.click(index);
        } else if r.secondary {
            ctx.inventory.click_secondary(index);
        }
    };
    for i in HOTBAR..total {
        slot(ui, i, cell(i - HOTBAR, y));
    }
    y += grid_h(main_rows) + 8.0;
    for i in 0..HOTBAR {
        slot(ui, i, cell(i, y));
    }

    // Клик мимо окна с предметом в руке — выбросить его.
    if ui.clicked_nowhere() && !ui.hovered(panel) {
        ctx.inventory.cursor = None;
    }
    if let Some(c) = ctx.inventory.cursor {
        let m = ui.input.mouse;
        ui.stack(Rect::new(m.x - s / 2.0, m.y - s / 2.0, s, s), c, ctx.reg);
    } else if let Some(t) = tooltip {
        ui.tooltip(&t);
    }
}
