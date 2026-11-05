//! Hand tools without a gun: the crowbar and the flashlight.
//!
//! * **Crowbar** (like Garry's Mod): LMB swings it (0.45 s). A physical body
//!   within reach is knocked at the point hit — a light one flies, a heavy
//!   machine barely rocks; frozen bodies are knocked loose. Blocks of the
//!   world only clang.
//! * **Flashlight**: RMB switches the beam on and off; it shines from the
//!   hand along the view while the flashlight is held.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    content::Tool,
    interact::Eye,
    inventory::Inventory,
    player::{Body, Player},
    tools::guns::{Frozen, GunInput},
};

pub const REACH: f32 = 2.8;
/// Impulse of a hit (N·s), at most `MAX_DV` m/s of speed for light things.
pub const IMPULSE: f32 = 900.0;
pub const MAX_DV: f32 = 9.0;
const COOLDOWN: f32 = 0.45;

/// Whether the flashlight's beam is on.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct Flashlight(pub bool);

#[derive(Component)]
struct Beam;

#[allow(clippy::too_many_arguments)]
fn crowbar(
    mut commands: Commands,
    time: Res<Time>,
    eye: Res<Eye>,
    gi: Res<GunInput>,
    spatial: SpatialQuery,
    mut players: Query<(Entity, &Inventory, &mut Body), With<Player>>,
    colliders: Query<&ColliderOf>,
    bodies: Query<(&RigidBody, &ComputedMass, Option<&Frozen>)>,
    mut forces: Query<Forces>,
    mut cooldown: Local<f32>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
) {
    *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    let Ok((me, inv, mut body)) = players.single_mut() else { return };
    if super::held_tool(inv) != Some(Tool::Crowbar) || !gi.left_pressed || *cooldown > 0.0 {
        return;
    }
    *cooldown = COOLDOWN;
    body.swing = 1.0;
    let Ok(dir) = Dir3::new(eye.dir) else { return };
    let Some(hit) = spatial.cast_ray(eye.pos, dir, REACH, true, &SpatialQueryFilter::default().with_excluded_entities([me])) else {
        sounds.write(crate::sound::sfx("rotate"));
        return;
    };
    let point = eye.pos + eye.dir * hit.distance;
    let b = colliders.get(hit.entity).map(|c| c.body).unwrap_or(hit.entity);
    match bodies.get(b) {
        Ok((rb, mass, frozen)) if rb.is_dynamic() || frozen.is_some() => {
            if frozen.is_some() {
                commands.entity(b).remove::<Frozen>().insert(RigidBody::Dynamic);
            }
            let j = IMPULSE.min(mass.value().max(1.0) * MAX_DV);
            if let Ok(mut f) = forces.get_mut(b) {
                f.apply_linear_impulse_at_point(eye.dir * j, point);
            }
            sounds.write(crate::sound::sfx_at("punt", point));
        }
        _ => {
            sounds.write(crate::sound::sfx_at("break", point));
        }
    }
}

fn flashlight(
    mut commands: Commands,
    gi: Res<GunInput>,
    mut on: ResMut<Flashlight>,
    players: Query<&Inventory, With<Player>>,
    cams: Query<Entity, With<crate::player::camera::MainCamera>>,
    beams: Query<Entity, With<Beam>>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
) {
    let held = players.single().ok().and_then(super::held_tool) == Some(Tool::Flashlight);
    if held && gi.right_pressed {
        on.0 = !on.0;
        sounds.write(crate::sound::sfx("toggle"));
    }
    let want = held && on.0;
    let have = !beams.is_empty();
    if want && !have {
        if let Ok(cam) = cams.single() {
            commands.spawn((
                Beam,
                SpotLight { intensity: 2.5e6, range: 40.0, outer_angle: 0.42, inner_angle: 0.22, color: Color::srgb(1.0, 0.95, 0.82), shadow_maps_enabled: true, ..default() },
                // From the right hand, along the view (a spot shines along −Z).
                Transform::from_xyz(0.25, -0.2, -0.3),
                ChildOf(cam),
            ));
        }
    } else if !want && have {
        for e in &beams {
            commands.entity(e).despawn();
        }
    }
}

pub struct HandPlugin;

impl Plugin for HandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Flashlight>().add_systems(Update, (crowbar, flashlight).after(crate::interact::InteractSet));
    }
}
