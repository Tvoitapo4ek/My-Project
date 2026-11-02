//! Translates raw devices into a device-independent `PlayerIntent` that the
//! fixed-step movement code consumes. Edge-triggered actions (jump) are
//! latched so they are not lost between fixed ticks.

use bevy::prelude::*;

use crate::core::states::gameplay_active;

#[derive(Resource, Debug, Default, Clone)]
pub struct PlayerIntent {
    /// x = strafe right, y = forward. Length ≤ 1.
    pub movement: Vec2,
    pub sprint: bool,
    pub crouch: bool,
    /// Held jump (used for swimming up / flying up).
    pub jump_held: bool,
    /// Latched jump press; cleared by the movement step that consumes it.
    pub jump_pressed: bool,
    /// Fly down (creative).
    pub descend: bool,
    /// Set by the autotest: devices are ignored and the script writes intent.
    pub scripted: bool,
}

fn read_keyboard(keys: Res<ButtonInput<KeyCode>>, mut intent: ResMut<PlayerIntent>) {
    if intent.scripted {
        return;
    }
    let axis =
        |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i8 - keys.pressed(neg) as i8) as f32;
    intent.movement = Vec2::new(
        axis(KeyCode::KeyD, KeyCode::KeyA),
        axis(KeyCode::KeyW, KeyCode::KeyS),
    )
    .clamp_length_max(1.0);
    intent.sprint = keys.pressed(KeyCode::ShiftLeft);
    intent.crouch = keys.pressed(KeyCode::ControlLeft);
    intent.descend = intent.crouch;
    intent.jump_held = keys.pressed(KeyCode::Space);
    if keys.just_pressed(KeyCode::Space) {
        intent.jump_pressed = true;
    }
}

fn clear_when_inactive(mut intent: ResMut<PlayerIntent>) {
    if intent.scripted {
        return;
    }
    *intent = PlayerIntent::default();
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerIntent>().add_systems(
            PreUpdate,
            (
                read_keyboard.run_if(gameplay_active),
                clear_when_inactive.run_if(not(gameplay_active)),
            )
                .after(bevy::input::InputSystems),
        );
    }
}
