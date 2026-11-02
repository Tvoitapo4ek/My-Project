//! Visual language of the projection: cold cyan lines on translucent glass,
//! white text, violet (magic) and amber (engineering) secondary accents.
//! Colours separate *kinds* of information, never "danger" by themselves.
//! Readability first: panels have a dark translucent backing behind text.

use bevy::prelude::*;

pub const GLASS: Color = Color::srgba(0.03, 0.10, 0.14, 0.78);
pub const GLASS_LIGHT: Color = Color::srgba(0.08, 0.22, 0.28, 0.70);
pub const LINE: Color = Color::srgba(0.45, 0.92, 1.0, 0.85);
pub const LINE_DIM: Color = Color::srgba(0.45, 0.92, 1.0, 0.30);
pub const CYAN: Color = Color::srgb(0.45, 0.92, 1.0);
pub const WHITE: Color = Color::srgb(0.93, 0.98, 1.0);
pub const DIM: Color = Color::srgba(0.80, 0.92, 1.0, 0.62);
pub const VIOLET: Color = Color::srgb(0.78, 0.55, 1.0);
pub const AMBER: Color = Color::srgb(1.0, 0.76, 0.35);
pub const SELECTED: Color = Color::srgba(0.30, 0.85, 1.0, 0.42);


pub fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

/// A framed glass box.
pub fn frame(node: Node) -> impl Bundle {
    (Node { border: UiRect::all(px(1)), ..node }, BackgroundColor(GLASS), BorderColor::all(LINE_DIM))
}

/// Standard holographic button (hover highlight is added by `HoverGlow`).
pub fn button(node: Node) -> impl Bundle {
    (
        Node { padding: UiRect::axes(px(10), px(5)), border: UiRect::all(px(1)), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..node },
        BackgroundColor(GLASS_LIGHT),
        BorderColor::all(LINE_DIM),
        HoverGlow,
    )
}

/// Text that does not block picking of its parent.
pub fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), font(size), TextColor(color), Pickable::IGNORE)
}

/// Highlights a node's border while the pointer is over it.
#[derive(Component, Default)]
pub struct HoverGlow;

fn hover_in(ev: On<PointerOver>, mut q: Query<&mut BorderColor, With<HoverGlow>>) {
    if let Ok(mut b) = q.get_mut(ev.event_target()) {
        *b = BorderColor::all(LINE);
    }
}

fn hover_out(ev: On<PointerOut>, mut q: Query<&mut BorderColor, With<HoverGlow>>) {
    if let Ok(mut b) = q.get_mut(ev.event_target()) {
        *b = BorderColor::all(LINE_DIM);
    }
}

pub struct HoloStylePlugin;

impl Plugin for HoloStylePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(hover_in).add_observer(hover_out);
    }
}
