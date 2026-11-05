//! Translates raw devices into a device-independent `PlayerIntent` that the
//! fixed-step movement code consumes. Edge-triggered actions (jump) are
//! latched so they are not lost between fixed ticks.

use bevy::prelude::*;

use crate::core::states::{InputFocus, movement_allowed};

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

/// Speed factor while the holographic device is open (spec §24: slow walk).
pub const DEVICE_MOVE_FACTOR: f32 = 0.35;

fn read_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<State<InputFocus>>,
    mut intent: ResMut<PlayerIntent>,
) {
    if intent.scripted {
        return;
    }
    let device = *focus.get() == InputFocus::Device;
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
    if device {
        // Only careful walking: no sprint, jump or crouch with panels open.
        intent.movement *= DEVICE_MOVE_FACTOR;
        intent.sprint = false;
        intent.crouch = false;
        intent.descend = false;
        intent.jump_held = false;
        intent.jump_pressed = false;
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
                read_keyboard.run_if(movement_allowed),
                clear_when_inactive.run_if(not(movement_allowed)),
            )
                .after(bevy::input::InputSystems),
        );
    }
}
