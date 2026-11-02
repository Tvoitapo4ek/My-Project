//! The personal projector device and its open/close state machine.
//!
//! The device is a component (`HoloDevice`) on whoever carries it, so any
//! character archetype or model can have one (a mage's focus, a werewolf's
//! collar…). Only the emitter position and the look of the beams depend on
//! the carrier.
//!
//! Keys: Tab — open on the home screen / close, I — inventory, J — journal,
//! N — map, Esc — close (or stop typing). While open: the cursor drives the
//! panels, RMB held looks around, WASD walks slowly. Gameplay actions are
//! blocked because their systems require `InputFocus::Gameplay`.

use bevy::{
    camera::visibility::RenderLayers,
    light::NotShadowCaster,
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions},
};

use super::{
    HoloNav,
    panels::{HOLO_LAYER, HoloRoot, PanelSlot, HoloPanel},
};
use crate::{
    core::{
        resources::MainCamera,
        states::{AppState, InputFocus, TypingFocus},
    },
    player::{
        Player,
        camera::{CameraMode, CameraRig},
    },
};

/// Seconds to unfold / fold the projection.
pub const OPEN_TIME: f32 = 0.35;
pub const CLOSE_TIME: f32 = 0.22;

/// A projector carried by a character.
#[derive(Component, Debug, Clone)]
pub struct HoloDevice {
    /// Display name of the device (shown on the home screen).
    pub name: &'static str,
    /// Emitter position relative to the carrier's body in third person.
    pub emitter: Vec3,
}

impl Default for HoloDevice {
    fn default() -> Self {
        Self { name: "Наручный проектор", emitter: Vec3::new(0.32, 0.05, -0.28) }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

/// Openness 0..1 driven by the state machine.
#[derive(Resource, Debug, Default)]
pub struct DeviceAnim {
    pub t: f32,
}

impl DeviceAnim {
    /// Ease-out cubic.
    pub fn eased(&self) -> f32 {
        1.0 - (1.0 - self.t.clamp(0.0, 1.0)).powi(3)
    }
}

/// Request to open (optionally on a section) or close the device.
#[derive(Message, Debug, Clone, Copy)]
pub enum DeviceCommand {
    Open(Option<&'static str>),
    Close,
}

fn attach_device(mut commands: Commands, added: Query<Entity, (Added<Player>, Without<HoloDevice>)>) {
    for e in &added {
        commands.entity(e).insert(HoloDevice::default());
    }
}

/// Keyboard → device commands (one input layer: when the device is open the
/// keys go here, never to gameplay).
fn device_keys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<State<InputFocus>>,
    state: Res<DeviceState>,
    typing: Res<TypingFocus>,
    nav: Res<HoloNav>,
    carrier: Query<(), (With<Player>, With<HoloDevice>)>,
    mut cmd: MessageWriter<DeviceCommand>,
) {
    if typing.0 || carrier.is_empty() {
        return;
    }
    let shortcut = [
        (KeyCode::KeyI, "inventory"),
        (KeyCode::KeyJ, "journal"),
        (KeyCode::KeyN, "map"),
    ]
    .into_iter()
    .find(|(k, _)| keys.just_pressed(*k))
    .map(|(_, s)| s);
    match (*focus.get(), *state) {
        (InputFocus::Gameplay, DeviceState::Closed) => {
            if keys.just_pressed(KeyCode::Tab) {
                cmd.write(DeviceCommand::Open(Some("home")));
            } else if let Some(s) = shortcut {
                cmd.write(DeviceCommand::Open(Some(s)));
            }
        }
        (InputFocus::Device, DeviceState::Open | DeviceState::Opening) => {
            if keys.just_pressed(KeyCode::Tab) || keys.just_pressed(KeyCode::Escape) {
                cmd.write(DeviceCommand::Close);
            } else if let Some(s) = shortcut {
                // The same shortcut again closes, another one switches.
                if nav.current == s {
                    cmd.write(DeviceCommand::Close);
                } else {
                    cmd.write(DeviceCommand::Open(Some(s)));
                }
            }
        }
        _ => {}
    }
}

fn apply_commands(
    mut cmds: MessageReader<DeviceCommand>,
    mut state: ResMut<DeviceState>,
    mut nav: ResMut<HoloNav>,
    mut next: ResMut<NextState<InputFocus>>,
    focus: Res<State<InputFocus>>,
) {
    for c in cmds.read() {
        match *c {
            DeviceCommand::Open(section) => {
                if let Some(s) = section {
                    nav.go(s);
                }
                if matches!(*state, DeviceState::Closed | DeviceState::Closing) {
                    *state = DeviceState::Opening;
                    nav.rebuild = true;
                }
                if *focus.get() != InputFocus::Device {
                    next.set(InputFocus::Device);
                }
            }
            DeviceCommand::Close => {
                if matches!(*state, DeviceState::Open | DeviceState::Opening) {
                    *state = DeviceState::Closing;
                }
            }
        }
    }
}

/// Advances the animation and keeps the state machine consistent with the
/// input focus (another screen may take focus, the autotest may set it).
fn animate(
    time: Res<Time>,
    focus: Res<State<InputFocus>>,
    mut state: ResMut<DeviceState>,
    mut anim: ResMut<DeviceAnim>,
    mut next: ResMut<NextState<InputFocus>>,
    mut nav: ResMut<HoloNav>,
    mut typing: ResMut<TypingFocus>,
) {
    let dt = time.delta_secs();
    let device_focus = *focus.get() == InputFocus::Device;
    match *state {
        DeviceState::Closed if device_focus => {
            *state = DeviceState::Opening;
            nav.rebuild = true;
        }
        DeviceState::Opening | DeviceState::Open if !device_focus => {
            // Focus taken elsewhere: vanish immediately.
            *state = DeviceState::Closed;
            anim.t = 0.0;
        }
        DeviceState::Opening => {
            anim.t = (anim.t + dt / OPEN_TIME).min(1.0);
            if anim.t >= 1.0 {
                *state = DeviceState::Open;
            }
        }
        DeviceState::Closing => {
            anim.t = (anim.t - dt / CLOSE_TIME).max(0.0);
            if anim.t <= 0.0 {
                *state = DeviceState::Closed;
                if device_focus {
                    next.set(InputFocus::Gameplay);
                }
            }
        }
        _ => {}
    }
    if *state != DeviceState::Open && typing.0 {
        typing.0 = false;
    }
}

/// RMB held while the device is open: capture the mouse to look around.
fn look_capture(
    mouse: Res<ButtonInput<MouseButton>>,
    focus: Res<State<InputFocus>>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if *focus.get() != InputFocus::Device {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    } else if mouse.just_released(MouseButton::Right) {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
    }
}

#[derive(Component)]
struct Emitter;

#[derive(Component)]
struct Beam(usize);

fn spawn_emitter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let layer = RenderLayers::layer(HOLO_LAYER);
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.95, 1.0, 0.9),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let beam = materials.add(StandardMaterial {
        base_color: Color::srgba(0.45, 0.92, 1.0, 0.18),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    commands.spawn((
        Name::new("Holo Emitter"),
        Emitter,
        Mesh3d(meshes.add(Torus::new(0.018, 0.03))),
        MeshMaterial3d(glow),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
        layer.clone(),
    ));
    // Unit-length beam along +Z, scaled to the target distance.
    let mesh = meshes.add(Cuboid::new(0.004, 0.004, 1.0));
    for i in 0..4 {
        commands.spawn((
            Beam(i),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(beam.clone()),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            layer.clone(),
        ));
    }
}

/// Emitter on the wrist and four light beams to the centre panel corners.
/// Strong while unfolding / folding, faint while open.
#[allow(clippy::type_complexity)]
fn update_emitter(
    time: Res<Time>,
    anim: Res<DeviceAnim>,
    state: Res<DeviceState>,
    rig: Res<CameraRig>,
    cam: Query<&Transform, (With<MainCamera>, Without<Emitter>, Without<Beam>)>,
    carrier: Query<(&Transform, &HoloDevice), (With<Player>, Without<Emitter>, Without<Beam>)>,
    root: Query<&Transform, (With<HoloRoot>, Without<Emitter>, Without<Beam>)>,
    panels: Query<(&HoloPanel, &Transform), (Without<HoloRoot>, Without<Emitter>, Without<Beam>)>,
    mut emitter: Query<(&mut Transform, &mut Visibility), (With<Emitter>, Without<Beam>)>,
    mut beams: Query<(&Beam, &mut Transform, &mut Visibility), Without<Emitter>>,
) {
    let shown = anim.t > 0.001;
    let (Ok(cam), Ok((body, device)), Ok(root_t)) = (cam.single(), carrier.single(), root.single())
    else {
        return;
    };
    let wrist = match rig.mode {
        CameraMode::FirstPerson => cam.translation + cam.rotation * Vec3::new(0.2, -0.3, -0.42),
        CameraMode::ThirdPerson => {
            body.translation + Quat::from_rotation_y(rig.yaw) * device.emitter
        }
    };
    let vis = if shown { Visibility::Visible } else { Visibility::Hidden };
    if let Ok((mut t, mut v)) = emitter.single_mut() {
        *v = vis;
        t.translation = wrist;
        t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 3.0) * cam.rotation;
        t.scale = Vec3::splat(0.6 + 0.4 * anim.eased());
    }
    // Corners of the centre panel in world space.
    let Some((panel, local)) = panels.iter().find(|(p, _)| p.slot == PanelSlot::Content) else {
        return;
    };
    let m = root_t.to_matrix() * local.to_matrix();
    let (hx, hy) = (panel.size.x / 2.0, panel.size.y / 2.0);
    let corners = [Vec3::new(-hx, -hy, 0.0), Vec3::new(hx, -hy, 0.0), Vec3::new(-hx, hy, 0.0), Vec3::new(hx, hy, 0.0)];
    let strength = match *state {
        DeviceState::Opening | DeviceState::Closing => 1.0,
        _ => 0.25,
    };
    for (beam, mut t, mut v) in &mut beams {
        *v = vis;
        let to = m.transform_point3(corners[beam.0]);
        let d = to - wrist;
        let len = d.length().max(0.01);
        *t = Transform::from_translation(wrist + d / 2.0)
            .looking_to(d, Vec3::Y)
            .with_scale(Vec3::new(strength, strength, len));
    }
}

pub struct DevicePlugin;

impl Plugin for DevicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeviceState>()
            .init_resource::<DeviceAnim>()
            .add_message::<DeviceCommand>()
            .add_systems(Startup, spawn_emitter)
            .add_systems(
                Update,
                (attach_device, device_keys, apply_commands, animate, look_capture)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                update_emitter
                    .after(super::panels::place_root)
                    .before(TransformSystems::Propagate),
            );
    }
}
