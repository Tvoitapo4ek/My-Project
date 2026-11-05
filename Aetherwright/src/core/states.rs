//! Game states. Game-mode differences (survival vs creative) and input focus
//! are expressed as Bevy states so systems opt in with `run_if` instead of
//! scattering `if creative` checks.

use bevy::prelude::*;

/// Top-level lifecycle.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// World is being generated / loaded.
    #[default]
    Loading,
    InGame,
}

/// Rule set the world is played with.
#[derive(
    States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
}

/// Which layer currently owns mouse + keyboard. Gameplay input only runs in
/// `Gameplay`; UI screens get the cursor in every other variant.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InputFocus {
    #[default]
    Gameplay,
    Inventory,
    BuildMenu,
    Paused,
    /// In-game texture editor (F7).
    TextureEditor,
    /// The holographic device is open: the cursor drives the spatial
    /// panels, the character may still walk slowly and look around (RMB),
    /// but no gameplay action (tools, building, combat, vehicles) runs.
    Device,
}

/// True while a text field (search, note) owns the keyboard.
#[derive(Resource, Debug, Default)]
pub struct TypingFocus(pub bool);

/// Movement keys are read in gameplay and (slowed) while the device is open.
pub fn movement_allowed(
    app: Option<Res<State<AppState>>>,
    focus: Option<Res<State<InputFocus>>>,
    typing: Option<Res<TypingFocus>>,
) -> bool {
    matches!(app.map(|s| *s.get()), Some(AppState::InGame))
        && matches!(
            focus.map(|s| *s.get()),
            Some(InputFocus::Gameplay | InputFocus::Device)
        )
        && !typing.is_some_and(|t| t.0)
}

/// Mouse look: in gameplay, and with the device open (while RMB is held).
pub fn look_allowed(
    app: Option<Res<State<AppState>>>,
    focus: Option<Res<State<InputFocus>>>,
) -> bool {
    matches!(app.map(|s| *s.get()), Some(AppState::InGame))
        && matches!(
            focus.map(|s| *s.get()),
            Some(InputFocus::Gameplay | InputFocus::Device)
        )
}


/// Convenience run condition: in game and nothing overlays the gameplay.
pub fn gameplay_active(
    app: Option<Res<State<AppState>>>,
    focus: Option<Res<State<InputFocus>>>,
) -> bool {
    matches!(app.map(|s| *s.get()), Some(AppState::InGame))
        && matches!(focus.map(|s| *s.get()), Some(InputFocus::Gameplay))
}

pub fn in_game(app: Option<Res<State<AppState>>>) -> bool {
    matches!(app.map(|s| *s.get()), Some(AppState::InGame))
}

pub struct StatesPlugin;

impl Plugin for StatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_state::<GameMode>()
            .init_state::<InputFocus>()
            .init_resource::<TypingFocus>();
    }
}
