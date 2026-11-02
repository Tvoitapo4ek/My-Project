//! Visual language of the UI: minimal holographic sci-fi (cyan) with
//! magical accents (violet/gold).

use bevy::prelude::*;

pub const PANEL_BG: Color = Color::srgba(0.02, 0.06, 0.09, 0.62);
pub const PANEL_BORDER: Color = Color::srgba(0.35, 0.85, 1.0, 0.55);
pub const TEXT: Color = Color::srgb(0.82, 0.95, 1.0);
pub const TEXT_DIM: Color = Color::srgba(0.7, 0.85, 0.95, 0.65);
pub const ACCENT: Color = Color::srgb(0.35, 0.9, 1.0);
pub const MAGIC: Color = Color::srgb(0.78, 0.5, 1.0);
pub const WARN: Color = Color::srgb(1.0, 0.55, 0.3);
pub const OK: Color = Color::srgb(0.5, 1.0, 0.6);

pub fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

/// Standard holographic panel node.
pub fn panel(node: Node) -> impl Bundle {
    (
        Node {
            border: UiRect::all(px(1)),
            padding: UiRect::all(px(8)),
            ..node
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
    )
}

/// The UI font (embedded: Cyrillic coverage is required, the Bevy default
/// font is ASCII-only).
#[derive(Resource, Clone)]
pub struct UiFont {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
}

pub fn load_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let regular = fonts.add(Font::from_bytes(
        include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf").to_vec(),
    ));
    let bold = fonts.add(Font::from_bytes(
        include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf").to_vec(),
    ));
    commands.insert_resource(UiFont { regular, bold });
}

/// Every new text gets the UI font (bold for large titles), so UI code
/// never has to pass font handles around.
pub fn apply_font(font: Option<Res<UiFont>>, mut q: Query<&mut TextFont, Added<TextFont>>) {
    let Some(font) = font else { return };
    for mut t in &mut q {
        let big = matches!(t.font_size, bevy::text::FontSize::Px(px) if px >= 18.0);
        t.font = bevy::text::FontSource::Handle(if big {
            font.bold.clone()
        } else {
            font.regular.clone()
        });
    }
}
