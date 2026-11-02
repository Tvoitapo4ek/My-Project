//! Full-screen overlays: loading progress and pause.

use bevy::prelude::*;

use super::theme;
use crate::{
    core::states::{AppState, InputFocus},
    world::LoadProgress,
};

#[derive(Component)]
struct LoadingScreen;

#[derive(Component)]
struct LoadingText;

#[derive(Component)]
struct PauseScreen;

fn fullscreen(bg: Color) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(12),
            ..default()
        },
        BackgroundColor(bg),
        GlobalZIndex(100),
    )
}

fn spawn_loading(mut commands: Commands) {
    commands.spawn((LoadingScreen, fullscreen(Color::srgb(0.02, 0.04, 0.06)))).with_children(|p| {
        p.spawn((Text::new("AETHERWRIGHT"), theme::font(48.0), TextColor(theme::ACCENT)));
        p.spawn((LoadingText, Text::new("…"), theme::font(18.0), TextColor(theme::TEXT)));
    });
}

fn update_loading(progress: Res<LoadProgress>, mut q: Query<&mut Text, With<LoadingText>>) {
    for mut t in &mut q {
        t.0 = format!(
            "{} — чанки: сгенерировано {}/{}, геометрия {}/{}",
            progress.stage, progress.generated, progress.total, progress.meshed, progress.total
        );
    }
}

fn despawn_loading(mut commands: Commands, q: Query<Entity, With<LoadingScreen>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn spawn_pause(mut commands: Commands) {
    commands.spawn((PauseScreen, fullscreen(Color::srgba(0.0, 0.02, 0.04, 0.55)))).with_children(|p| {
        p.spawn((Text::new("ПАУЗА"), theme::font(40.0), TextColor(theme::ACCENT)));
        p.spawn((
            Text::new("Esc — продолжить   ·   F5 — сохранить   ·   F9 — загрузить"),
            theme::font(18.0),
            TextColor(theme::TEXT),
        ));
    });
}

fn despawn_pause(mut commands: Commands, q: Query<Entity, With<PauseScreen>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Loading), spawn_loading)
            .add_systems(Update, update_loading.run_if(in_state(AppState::Loading)))
            .add_systems(OnExit(AppState::Loading), despawn_loading)
            .add_systems(OnEnter(InputFocus::Paused), spawn_pause)
            .add_systems(OnExit(InputFocus::Paused), despawn_pause);
    }
}
