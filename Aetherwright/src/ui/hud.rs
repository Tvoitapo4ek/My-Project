//! Gameplay HUD: crosshair and notification feed.

use bevy::prelude::*;

use super::theme;
use crate::core::events::{Notify, NotifyKind};

#[derive(Component)]
struct NotificationFeed;

#[derive(Component)]
struct NotificationLine {
    ttl: f32,
}

fn spawn_hud(mut commands: Commands) {
    // Crosshair.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                Node {
                    width: px(6),
                    height: px(6),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BorderColor::all(theme::ACCENT),
                BackgroundColor(Color::srgba(0.3, 0.9, 1.0, 0.25)),
            ));
        });
    commands.spawn((
        NotificationFeed,
        Node {
            position_type: PositionType::Absolute,
            right: px(16),
            top: px(16),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: px(4),
            ..default()
        },
    ));
}

fn show_notifications(
    mut commands: Commands,
    mut reader: MessageReader<Notify>,
    feed: Query<Entity, With<NotificationFeed>>,
) {
    let Ok(feed) = feed.single() else { return };
    for n in reader.read() {
        let color = match n.kind {
            NotifyKind::Info => theme::TEXT,
            NotifyKind::Success => theme::OK,
            NotifyKind::Warning => theme::WARN,
        };
        let line = commands
            .spawn((
                NotificationLine { ttl: 4.0 },
                theme::panel(Node {
                    padding: UiRect::axes(px(8), px(3)),
                    ..default()
                }),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(n.text.clone()),
                    theme::font(15.0),
                    TextColor(color),
                ));
            })
            .id();
        commands.entity(feed).add_child(line);
    }
}

fn expire_notifications(
    mut commands: Commands,
    time: Res<Time>,
    mut lines: Query<(Entity, &mut NotificationLine)>,
) {
    for (e, mut l) in &mut lines {
        l.ttl -= time.delta_secs();
        if l.ttl <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(Update, (show_notifications, expire_notifications));
    }
}
