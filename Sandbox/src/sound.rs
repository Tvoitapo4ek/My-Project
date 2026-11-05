//! Sound effects (CC0 sounds in `assets/sounds/`, see CREDITS.md).
//!
//! * `Sfx` message — any module asks for a one-shot sound by name, at a
//!   world point (spatial) or not (interface).
//! * Running motors hum: the nearest few within `HUM_RANGE` get a looping
//!   spatial sound whose pitch follows their speed.

use bevy::{audio::Volume, prelude::*};

use crate::{
    content::Mech,
    core::Ui,
    interact::Edited,
    mech::{BlockEntity, Spin},
    player::camera::MainCamera,
};

/// A one-shot sound: `name` = file in assets/sounds without `.ogg`.
#[derive(Message, Debug, Clone, Copy)]
pub struct Sfx {
    pub name: &'static str,
    pub at: Option<Vec3>,
    pub volume: f32,
}

pub fn sfx(name: &'static str) -> Sfx {
    Sfx { name, at: None, volume: 0.6 }
}

pub fn sfx_at(name: &'static str, at: Vec3) -> Sfx {
    Sfx { name, at: Some(at), volume: 0.8 }
}

const HUM_RANGE: f32 = 18.0;
const MAX_HUMS: usize = 4;

#[derive(Component)]
struct Hum;

fn add_listener(mut commands: Commands, cams: Query<Entity, Added<MainCamera>>) {
    for c in &cams {
        commands.entity(c).insert(SpatialListener::new(0.3));
    }
}

fn play(mut commands: Commands, assets: Res<AssetServer>, mut msgs: MessageReader<Sfx>, s: Res<crate::menu::GameSettings>) {
    for m in msgs.read() {
        // Spatial sounds are effects; the others are the interface.
        let k = if m.at.is_some() { s.effects } else { s.interface };
        let mut settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(m.volume * k));
        let mut e = commands.spawn(AudioPlayer::new(assets.load(format!("sounds/{}.ogg", m.name))));
        if let Some(p) = m.at {
            settings = settings.with_spatial(true);
            e.insert(Transform::from_translation(p));
        }
        e.insert(settings);
    }
}

/// Block edits by the player.
fn edits(mut edited: MessageReader<Edited>, grids: Query<&GlobalTransform>, mut out: MessageWriter<Sfx>) {
    for e in edited.read() {
        let at = grids.get(e.grid).map(|g| g.transform_point(e.cell.as_vec3() + Vec3::splat(0.5))).unwrap_or_default();
        let wood = e.block.def().key.contains("plank") || e.block.def().key == "seat";
        out.write(sfx_at(if !e.placed { "break" } else if wood { "place_wood" } else { "place" }, at));
    }
}

fn ui_open(mut out: MessageWriter<Sfx>) {
    out.write(sfx("ui_open"));
}

fn ui_close(mut out: MessageWriter<Sfx>) {
    out.write(sfx("ui_close"));
}

/// Looping hum on the nearest running motors.
#[allow(clippy::too_many_arguments)]
fn hums(mut commands: Commands, assets: Res<AssetServer>, s: Res<crate::menu::GameSettings>, mut sinks: Query<&mut SpatialAudioSink, With<Hum>>, cams: Query<&GlobalTransform, With<MainCamera>>, motors: Query<(Entity, &BlockEntity, &Spin, &GlobalTransform, Option<&Children>)>, hums: Query<(), With<Hum>>) {
    let Ok(cam) = cams.single() else { return };
    if s.is_changed() {
        for mut k in &mut sinks {
            k.set_volume(Volume::Linear(0.35 * s.motors));
        }
    }
    let mut running: Vec<(f32, Entity, f32)> = motors
        .iter()
        .filter(|(_, b, s, ..)| matches!(b.mech(), Some(Mech::Motor | Mech::DieselEngine)) && s.rpm.abs() > 1.0)
        .map(|(e, _, s, t, _)| (t.translation().distance(cam.translation()), e, s.rpm.abs()))
        .filter(|(d, ..)| *d < HUM_RANGE)
        .collect();
    running.sort_by(|a, b| a.0.total_cmp(&b.0));
    let keep: Vec<Entity> = running.iter().take(MAX_HUMS).map(|r| r.1).collect();
    for (e, b, spin, _, children) in &motors {
        let hum = children.and_then(|c| c.iter().find(|c| hums.contains(*c)));
        match (keep.contains(&e), hum) {
            (true, None) => {
                // The diesel rumbles lower than the electric creative motor.
                let base = if b.mech() == Some(Mech::DieselEngine) { 0.7 } else { 0.6 };
                let speed = (base + spin.rpm.abs() / 160.0).min(2.2);
                let file = if b.mech() == Some(Mech::DieselEngine) { "sounds/engine_loop.ogg" } else { "sounds/motor_loop.ogg" };
                commands.spawn((Hum, AudioPlayer::new(assets.load(file)), PlaybackSettings::LOOP.with_spatial(true).with_volume(Volume::Linear(0.35 * s.motors)).with_speed(speed), Transform::default(), ChildOf(e)));
            }
            (false, Some(h)) => commands.entity(h).despawn(),
            _ => {}
        }
    }
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Sfx>()
            .add_systems(Update, (add_listener, edits, play, hums))
            .add_systems(OnEnter(Ui::Inventory), ui_open)
            .add_systems(OnExit(Ui::Inventory), ui_close)
            .add_systems(OnEnter(Ui::Settings), ui_open)
            .add_systems(OnExit(Ui::Settings), ui_close);
    }
}
