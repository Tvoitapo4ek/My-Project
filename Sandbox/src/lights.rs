//! Headlights: a lamp on a block face shining along its facing. A signal
//! from a linked sensor switches it: each press (signal goes from 0 to
//! non-zero) turns it on or off. On: glowing lens and a spot light.

use bevy::prelude::*;

use crate::{
    content::Mech,
    mech::{BlockEntity, Signal},
    style::{BoxCache, Materials},
};

#[derive(Component, Debug, Default)]
pub struct Headlight {
    pub on: bool,
    pressed: bool,
}

#[derive(Component)]
struct Lens;

#[derive(Component)]
struct Beam;

/// Toggles on a rising edge of the signal.
pub fn toggle(on: bool, was_pressed: bool, signal: f32) -> (bool, bool) {
    let pressed = signal.abs() > 0.5;
    (if pressed && !was_pressed { !on } else { on }, pressed)
}

fn attach(mut commands: Commands, added: Query<(Entity, &BlockEntity), Added<BlockEntity>>, mut meshes: ResMut<Assets<Mesh>>, mut cache: ResMut<BoxCache>, mats: Option<Res<Materials>>) {
    let Some(mats) = mats else { return };
    for (e, b) in &added {
        if b.mech() != Some(Mech::Headlight) {
            continue;
        }
        commands.entity(e).insert(Headlight::default());
        commands.spawn((Lens, Mesh3d(cache.get(&mut meshes, Vec3::new(0.48, 0.06, 0.48))), MeshMaterial3d(mats.get("glass_lens")), Transform::from_xyz(0.0, 0.0, 0.0), ChildOf(e)));
        commands.spawn((
            Beam,
            SpotLight { intensity: 4.0e6, range: 45.0, outer_angle: 0.55, inner_angle: 0.3, color: Color::srgb(1.0, 0.92, 0.75), shadow_maps_enabled: false, ..default() },
            // A spot light shines along its −Z; turn that onto the block's +Y.
            Transform::from_xyz(0.0, 0.05, 0.0).with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::Y)),
            Visibility::Hidden,
            ChildOf(e),
        ));
    }
}

fn update(mats: Option<Res<Materials>>, mut sounds: MessageWriter<crate::sound::Sfx>, mut lights: Query<(&Signal, &mut Headlight, &Children, &GlobalTransform)>, mut lenses: Query<&mut MeshMaterial3d<StandardMaterial>, With<Lens>>, mut beams: Query<&mut Visibility, With<Beam>>) {
    let Some(mats) = mats else { return };
    for (sig, mut h, children, gt) in &mut lights {
        let (on, pressed) = toggle(h.on, h.pressed, sig.0);
        h.pressed = pressed;
        if on == h.on {
            continue;
        }
        h.on = on;
        sounds.write(crate::sound::sfx_at("toggle", gt.translation()));
        for c in children.iter() {
            if let Ok(mut m) = lenses.get_mut(c) {
                m.0 = mats.get(if on { "glow_lamp" } else { "glass_lens" });
            }
            if let Ok(mut v) = beams.get_mut(c) {
                *v = if on { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        // After signals are written for this frame.
        app.add_systems(Update, (attach, update.after(crate::mech::MechSet)).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_press_toggles() {
        let (on, p) = toggle(false, false, 1.0);
        assert!(on && p);
        // Still held: no change.
        assert_eq!(toggle(on, p, 1.0), (true, true));
        // Released, pressed again: off.
        let (on, p) = toggle(on, p, 0.0);
        assert_eq!(toggle(on, p, -1.0), (false, true));
    }
}
