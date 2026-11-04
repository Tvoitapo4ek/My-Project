//! App-wide state: what owns the mouse/keyboard, the cursor, physics
//! layers and the keys a control panel can bind.

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

/// Who owns input right now.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Ui {
    /// Main menu (the game starts here).
    #[default]
    Menu,
    /// Playing: cursor locked, the world receives input.
    Game,
    /// Esc in the game: the world stops.
    Pause,
    /// Game settings (from the menu or the pause).
    Options,
    /// How a block works (hold W over an item in the inventory).
    Ponder,
    /// Inventory / catalogue screen.
    Inventory,
    /// Debug console (typing).
    Console,
    /// Debug menu with buttons.
    Debug,
    /// Block settings (wrench).
    Settings,
    /// Texture editor (from the pause).
    Paint,
}

/// Keys a control-panel channel can use (index 0 = none). Shift leaves
/// the seat and E opens the inventory, so they are not bindable.
pub const BINDABLE_KEYS: [(&str, Option<KeyCode>); 16] = [
    ("—", None),
    ("W", Some(KeyCode::KeyW)),
    ("S", Some(KeyCode::KeyS)),
    ("A", Some(KeyCode::KeyA)),
    ("D", Some(KeyCode::KeyD)),
    ("Пробел", Some(KeyCode::Space)),
    ("Q", Some(KeyCode::KeyQ)),
    ("R", Some(KeyCode::KeyR)),
    ("F", Some(KeyCode::KeyF)),
    ("Z", Some(KeyCode::KeyZ)),
    ("X", Some(KeyCode::KeyX)),
    ("C", Some(KeyCode::KeyC)),
    ("↑", Some(KeyCode::ArrowUp)),
    ("↓", Some(KeyCode::ArrowDown)),
    ("←", Some(KeyCode::ArrowLeft)),
    ("→", Some(KeyCode::ArrowRight)),
];

/// Short on-screen messages.
#[derive(Message, Debug, Clone)]
pub struct Toast(pub String);

pub fn toast(text: impl Into<String>) -> Toast {
    Toast(text.into())
}

/// Scripted runs (autotest) feed input themselves and keep the cursor free.
#[derive(Resource, Default)]
pub struct Scripted(pub bool);

fn cursor_follows_ui(ui: Res<State<Ui>>, scripted: Res<Scripted>, mut cursor: Single<&mut CursorOptions>) {
    let want = if *ui.get() == Ui::Game && !scripted.0 { CursorGrabMode::Locked } else { CursorGrabMode::None };
    if cursor.grab_mode != want {
        cursor.grab_mode = want;
        cursor.visible = want == CursorGrabMode::None;
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Ui>()
            .init_resource::<Scripted>()
            .add_message::<Toast>()
            .add_systems(Update, cursor_follows_ui);
    }
}
