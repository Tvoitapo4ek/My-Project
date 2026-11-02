//! Debug overlay (F3) and physics debug rendering toggle (F4).

use avian3d::prelude::*;
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

use super::theme;
use crate::player::{Player, camera::CameraRig, movement::CharacterState};

#[derive(Component)]
struct DebugText;

#[derive(Resource, Default)]
pub struct DebugOverlay {
    pub visible: bool,
}

fn spawn_debug(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Debug Panel"),
            theme::panel(Node {
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(12),
                ..default()
            }),
            Visibility::Hidden,
            DebugText,
        ))
        .with_children(|p| {
            p.spawn((Text::new(""), theme::font(14.0), TextColor(theme::TEXT)));
        });
}

fn toggle_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<DebugOverlay>,
    mut panel: Query<&mut Visibility, With<DebugText>>,
    mut gizmos: ResMut<GizmoConfigStore>,
) {
    if keys.just_pressed(KeyCode::F3) {
        overlay.visible = !overlay.visible;
        for mut v in &mut panel {
            *v = if overlay.visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
    if keys.just_pressed(KeyCode::F4) {
        let (cfg, _) = gizmos.config_mut::<PhysicsGizmos>();
        cfg.enabled = !cfg.enabled;
    }
}

fn disable_physics_gizmos_by_default(mut gizmos: ResMut<GizmoConfigStore>) {
    gizmos.config_mut::<PhysicsGizmos>().0.enabled = false;
}

fn update_debug(
    overlay: Res<DebugOverlay>,
    diagnostics: Res<DiagnosticsStore>,
    rig: Res<CameraRig>,
    player: Query<(&Transform, &LinearVelocity, &CharacterState), With<Player>>,
    bodies: Query<&RigidBody>,
    panel: Query<&Children, With<DebugText>>,
    mut texts: Query<&mut Text>,
) {
    if !overlay.visible {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let mut s = format!("FPS {fps:.0}\n");
    if let Ok((t, v, c)) = player.single() {
        let p = t.translation;
        s += &format!(
            "pos {:.1} {:.1} {:.1}\nvel {:.1} (h {:.1})\nground {} crouch {} fly {} water {:.2}\n",
            p.x,
            p.y,
            p.z,
            v.0.length(),
            Vec2::new(v.x, v.z).length(),
            c.grounded,
            c.crouching,
            c.flying,
            c.submerged
        );
    }
    let dynamic = bodies.iter().filter(|b| b.is_dynamic()).count();
    s += &format!(
        "yaw {:.2} pitch {:.2} cam {:?}\nbodies {} (dynamic {})",
        rig.yaw,
        rig.pitch,
        rig.mode,
        bodies.iter().count(),
        dynamic
    );
    for children in &panel {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = s.clone();
            }
        }
    }
}

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .init_resource::<DebugOverlay>()
            .add_systems(Startup, (spawn_debug, disable_physics_gizmos_by_default))
            .add_systems(Update, (toggle_debug, update_debug));
    }
}
