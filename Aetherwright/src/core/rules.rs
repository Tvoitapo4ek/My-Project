//! Game-mode rules. The difference between survival and creative is data,
//! set once when `GameMode` changes; systems read `ModeRules` instead of
//! checking the mode themselves.

use bevy::prelude::*;

use super::states::GameMode;

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ModeRules {
    /// Blocks break instantly (no mining time, no tier limits).
    pub instant_break: bool,
    /// Mined blocks drop items.
    pub drops: bool,
    /// Placing / crafting / casting consumes resources.
    pub consume_items: bool,
    /// Hunger, thirst and temperature tick.
    pub needs: bool,
    /// The player can be hurt.
    pub player_damage: bool,
    /// Flight is allowed.
    pub flight: bool,
}

impl ModeRules {
    pub const SURVIVAL: Self = Self {
        instant_break: false,
        drops: true,
        consume_items: true,
        needs: true,
        player_damage: true,
        flight: false,
    };
    pub const CREATIVE: Self = Self {
        instant_break: true,
        drops: false,
        consume_items: false,
        needs: false,
        player_damage: false,
        flight: true,
    };
}

impl Default for ModeRules {
    fn default() -> Self {
        Self::SURVIVAL
    }
}

fn apply_rules(mode: Res<State<GameMode>>, mut rules: ResMut<ModeRules>) {
    *rules = match mode.get() {
        GameMode::Survival => ModeRules::SURVIVAL,
        GameMode::Creative => ModeRules::CREATIVE,
    };
}

/// M toggles the game mode (prototype convenience; a real game would gate it).
fn toggle_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<State<GameMode>>,
    mut next: ResMut<NextState<GameMode>>,
    mut notify: MessageWriter<super::events::Notify>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        let new = match mode.get() {
            GameMode::Survival => GameMode::Creative,
            GameMode::Creative => GameMode::Survival,
        };
        next.set(new);
        notify.write(super::events::Notify::info(match new {
            GameMode::Survival => "Режим: Выживание",
            GameMode::Creative => "Режим: Креатив",
        }));
    }
}

fn apply_launch_mode(opts: Res<super::config::LaunchOptions>, mut next: ResMut<NextState<GameMode>>) {
    if opts.creative {
        next.set(GameMode::Creative);
    }
}

pub struct RulesPlugin;

impl Plugin for RulesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModeRules>()
            .add_systems(Startup, apply_launch_mode)
            .add_systems(OnEnter(GameMode::Survival), apply_rules)
            .add_systems(OnEnter(GameMode::Creative), apply_rules)
            .add_systems(Update, toggle_mode.run_if(super::states::gameplay_active));
    }
}
