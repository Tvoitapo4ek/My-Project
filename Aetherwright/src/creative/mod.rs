//! Creative-mode developer tools. All systems run only in
//! `GameMode::Creative` (state-driven, no scattered checks):
//! * `[` / `]` — time of day ∓1 h, `\` — stop/start the clock;
//! * `-` / `=` — gravity ×0.5 / ×2, `0` — reset;
//! * Enter (manipulator "Выделение") — fill the selection with the last
//!   selected block; Delete — mass removal (selection tool, any mode);
//! * F4 — physics debug rendering (any mode), F — flight.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    building::selection::{GroupOp, PendingOps, select_mode_active},
    core::{
        config::GameConfig,
        events::Notify,
        resources::WorldClock,
        states::{GameMode, gameplay_active},
    },
    tools::ActiveTool,
    ui::theme,
    world::voxel::BlockId,
};

/// Last block the player held (used by fill).
#[derive(Resource, Debug)]
pub struct LastBlock(pub BlockId);

impl Default for LastBlock {
    fn default() -> Self {
        Self(crate::world::voxel::blocks::STONE)
    }
}

#[derive(Component)]
struct CreativePanel;

fn track_last_block(tool: Res<ActiveTool>, mut last: ResMut<LastBlock>) {
    if let ActiveTool::PlaceBlock(_, b) = *tool {
        last.0 = b;
    }
}

fn time_and_gravity(
    keys: Res<ButtonInput<KeyCode>>,
    cfg: Res<GameConfig>,
    mut clock: ResMut<WorldClock>,
    mut gravity: ResMut<Gravity>,
    mut notify: MessageWriter<Notify>,
) {
    if keys.just_pressed(KeyCode::BracketRight) {
        clock.time_of_day = (clock.time_of_day + 1.0 / 24.0).rem_euclid(1.0);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        clock.time_of_day = (clock.time_of_day - 1.0 / 24.0).rem_euclid(1.0);
    }
    if keys.just_pressed(KeyCode::Backslash) {
        clock.paused = !clock.paused;
        notify.write(Notify::info(if clock.paused {
            "Время остановлено"
        } else {
            "Время идёт"
        }));
    }
    let base = cfg.physics.gravity;
    let g = gravity.0.length();
    let new = if keys.just_pressed(KeyCode::Equal) {
        Some((g * 2.0).clamp(base * 0.125, base * 4.0))
    } else if keys.just_pressed(KeyCode::Minus) {
        Some(if g < base * 0.13 { 0.0 } else { g * 0.5 })
    } else if keys.just_pressed(KeyCode::Digit0) {
        Some(base)
    } else {
        None
    };
    if let Some(n) = new {
        gravity.0 = Vec3::NEG_Y * n;
        notify.write(Notify::info(format!("Гравитация: {:.2} g", n / base)));
    }
}

fn fill_selection(
    keys: Res<ButtonInput<KeyCode>>,
    last: Res<LastBlock>,
    mut ops: ResMut<PendingOps>,
) {
    if keys.just_pressed(KeyCode::Enter) {
        ops.0.push(GroupOp::Fill(last.0));
    }
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            CreativePanel,
            theme::panel(Node {
                position_type: PositionType::Absolute,
                right: px(16),
                bottom: px(110),
                flex_direction: FlexDirection::Column,
                ..default()
            }),
        ))
        .with_children(|p| {
            p.spawn((Text::new(""), theme::font(12.0), TextColor(theme::MAGIC)));
        });
}

fn despawn_panel(mut commands: Commands, q: Query<Entity, With<CreativePanel>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn update_panel(
    clock: Res<WorldClock>,
    gravity: Res<Gravity>,
    cfg: Res<GameConfig>,
    last: Res<LastBlock>,
    panel: Query<&Children, With<CreativePanel>>,
    mut texts: Query<&mut Text>,
) {
    let hours = clock.time_of_day * 24.0;
    let s = format!(
        "КРЕАТИВ\nВремя {:02}:{:02}{}  [ ] \\\nГравитация {:.2} g  - = 0\nЗаполнение: {} (Enter в «Выделении»)\nF — полёт · F4 — физика · M — режим",
        hours as u32,
        ((hours.fract()) * 60.0) as u32,
        if clock.paused { " (стоп)" } else { "" },
        gravity.0.length() / cfg.physics.gravity,
        last.0.def().name
    );
    for children in &panel {
        for c in children.iter() {
            if let Ok(mut t) = texts.get_mut(c)
                && t.0 != s
            {
                t.0 = s.clone();
            }
        }
    }
}

pub struct CreativePlugin;

impl Plugin for CreativePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastBlock>()
            .add_systems(Update, track_last_block)
            .add_systems(
                Update,
                (
                    time_and_gravity,
                    fill_selection.run_if(select_mode_active),
                    update_panel,
                )
                    .run_if(in_state(GameMode::Creative))
                    .run_if(gameplay_active),
            )
            .add_systems(OnEnter(GameMode::Creative), spawn_panel)
            .add_systems(OnExit(GameMode::Creative), despawn_panel);
    }
}
