//! Player camera: first person / third person with obstacle avoidance.
//! Look angles live in `CameraRig`; the character reads yaw from it.

use avian3d::prelude::*;
use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use super::{Player, movement::CharacterState};
use crate::core::{
    config::GameConfig,
    layers::Layer,
    resources::{EyeRay, MainCamera},
    states::{InputFocus, gameplay_active},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    FirstPerson,
    ThirdPerson,
}

#[derive(Resource, Debug, Clone)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub mode: CameraMode,
    /// Desired third-person distance (user setting).
    pub distance: f32,
    /// Actual distance after obstacle avoidance (smoothed).
    pub current_distance: f32,
    /// Smoothed eye height above body centre (crouch transitions).
    pub eye_offset: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: -0.15,
            mode: CameraMode::FirstPerson,
            distance: 4.5,
            current_distance: 4.5,
            eye_offset: 0.74,
        }
    }
}

fn spawn_camera(mut commands: Commands, cfg: Res<GameConfig>, mut rig: ResMut<CameraRig>) {
    rig.distance = cfg.camera.third_person_distance;
    rig.current_distance = rig.distance;
    commands.spawn((
        Name::new("Main Camera"),
        MainCamera,
        Camera3d::default(),
        // HUD always draws on the world camera (the holo overlay camera has
        // its own layer).
        IsDefaultUiCamera,
        Projection::Perspective(PerspectiveProjection {
            fov: cfg.camera.fov_degrees.to_radians(),
            near: 0.05,
            far: 600.0,
            ..default()
        }),
        Tonemapping::AcesFitted,
        Transform::from_xyz(0.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Fog values are animated by the environment (day/night).
        DistanceFog {
            color: Color::srgb(0.62, 0.72, 0.82),
            directional_light_color: Color::srgba(1.0, 0.9, 0.7, 0.4),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::Linear {
                start: 60.0,
                end: 150.0,
            },
        },
    ));
}

fn mouse_look(
    motion: Res<AccumulatedMouseMotion>,
    cfg: Res<GameConfig>,
    mut rig: ResMut<CameraRig>,
    cursor: Single<&CursorOptions>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    focus: Res<State<InputFocus>>,
) {
    if *focus.get() == InputFocus::Device {
        // Device open: look around only while RMB is held.
        if !mouse.pressed(MouseButton::Right) {
            return;
        }
    } else if cursor.grab_mode == CursorGrabMode::None {
        return;
    }
    // Holding R is reserved for rotating held objects (physics gun).
    if keys.pressed(KeyCode::KeyR) {
        return;
    }
    let sens = cfg.controls.mouse_sensitivity;
    let invert = if cfg.controls.invert_y { -1.0 } else { 1.0 };
    rig.yaw -= motion.delta.x * sens;
    rig.pitch = (rig.pitch - motion.delta.y * sens * invert).clamp(-1.54, 1.54);
}

fn camera_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    cfg: Res<GameConfig>,
    mut rig: ResMut<CameraRig>,
) {
    if keys.just_pressed(KeyCode::KeyV) {
        rig.mode = match rig.mode {
            CameraMode::FirstPerson => CameraMode::ThirdPerson,
            CameraMode::ThirdPerson => CameraMode::FirstPerson,
        };
    }
    // Alt + wheel adjusts third-person distance (plain wheel = hotbar).
    if rig.mode == CameraMode::ThirdPerson
        && keys.pressed(KeyCode::AltLeft)
        && scroll.delta.y != 0.0
    {
        let step = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y * 0.5,
            MouseScrollUnit::Pixel => scroll.delta.y * 0.02,
        };
        rig.distance =
            (rig.distance - step).clamp(cfg.camera.third_person_min, cfg.camera.third_person_max);
    }
}

fn sync_yaw_to_character(rig: Res<CameraRig>, mut q: Query<&mut CharacterState, With<Player>>) {
    for mut s in &mut q {
        s.yaw = rig.yaw;
    }
}

/// Places the camera after physics interpolation, before transform propagation.
pub fn follow_player(
    time: Res<Time>,
    spatial: SpatialQuery,
    mut rig: ResMut<CameraRig>,
    player: Query<(Entity, &Transform, &CharacterState), (With<Player>, Without<MainCamera>)>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
    mut eye: ResMut<EyeRay>,
) {
    let Ok((player_entity, body, state)) = player.single() else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    let target_eye = state.eye_offset();
    rig.eye_offset += (target_eye - rig.eye_offset) * (1.0 - (-14.0 * dt).exp());

    let rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0);
    let eye_pos = body.translation + Vec3::Y * rig.eye_offset;
    let forward = rotation * Vec3::NEG_Z;
    *eye = EyeRay {
        origin: eye_pos,
        direction: forward,
    };

    match rig.mode {
        CameraMode::FirstPerson => {
            rig.current_distance = 0.0;
            cam.translation = eye_pos;
        }
        CameraMode::ThirdPerson => {
            // Pull the camera in when something is between it and the head.
            let back = -forward;
            let filter =
                SpatialQueryFilter::from_mask([Layer::Default, Layer::Terrain, Layer::Prop])
                    .with_excluded_entities([player_entity]);
            let mut wanted = rig.distance;
            if let Some(hit) = spatial.cast_shape(
                &Collider::sphere(0.25),
                eye_pos,
                Quat::IDENTITY,
                Dir3::new(back).unwrap_or(Dir3::Z),
                &ShapeCastConfig::from_max_distance(rig.distance),
                &filter,
            ) {
                wanted = (hit.distance - 0.05).max(0.3);
            }
            // Snap in immediately (no clipping), ease out slowly.
            if wanted < rig.current_distance {
                rig.current_distance = wanted;
            } else {
                rig.current_distance += (wanted - rig.current_distance) * (1.0 - (-4.0 * dt).exp());
            }
            cam.translation = eye_pos + back * rig.current_distance + rotation * Vec3::X * 0.35;
        }
    }
    cam.rotation = rotation;
}

/// Lock the cursor while playing; release it for menus.
fn apply_cursor_for_focus(focus: Res<State<InputFocus>>, mut cursor: Single<&mut CursorOptions>) {
    let gameplay = *focus.get() == InputFocus::Gameplay;
    cursor.visible = !gameplay;
    cursor.grab_mode = if gameplay {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
}

/// Clicking into the window re-captures the cursor (e.g. after alt-tab).
fn recapture_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    focus: Res<State<InputFocus>>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if *focus.get() == InputFocus::Gameplay
        && cursor.grab_mode == CursorGrabMode::None
        && mouse.just_pressed(MouseButton::Left)
    {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

fn pause_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<State<InputFocus>>,
    mut next: ResMut<NextState<InputFocus>>,
) {
    // The device closes itself (with its animation) on Escape.
    if *focus.get() == InputFocus::Device {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        next.set(match focus.get() {
            InputFocus::Gameplay => InputFocus::Paused,
            _ => InputFocus::Gameplay,
        });
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    mouse_look.run_if(crate::core::states::look_allowed),
                    camera_hotkeys.run_if(gameplay_active),
                    sync_yaw_to_character,
                    recapture_cursor,
                    pause_toggle.run_if(crate::core::states::in_game),
                    apply_cursor_for_focus.run_if(state_changed::<InputFocus>),
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                follow_player.before(TransformSystems::Propagate),
            );
    }
}
