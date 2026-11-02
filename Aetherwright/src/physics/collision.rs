//! Collision-driven gameplay: hard impacts of heavy bodies are reported as
//! `ImpactEvent`s (used for creature/player damage), measured from the
//! change of velocity of dynamic bodies between physics ticks.

use avian3d::prelude::*;
use bevy::prelude::*;

/// A body experienced a large sudden velocity change (Δv in m/s).
#[derive(Message, Debug, Clone, Copy)]
pub struct ImpactEvent {
    pub entity: Entity,
    pub delta_v: f32,
}

/// Tracks the previous velocity of bodies that care about impacts.
#[derive(Component, Default, Debug)]
pub struct ImpactSensor {
    last_velocity: Vec3,
}

fn detect_impacts(mut q: Query<(Entity, &LinearVelocity, &mut ImpactSensor)>, mut out: MessageWriter<ImpactEvent>) {
    for (e, v, mut s) in &mut q {
        let dv = (v.0 - s.last_velocity).length();
        // Gravity alone changes velocity by ~0.3 m/s per tick.
        if dv > 8.0 {
            out.write(ImpactEvent { entity: e, delta_v: dv });
        }
        s.last_velocity = v.0;
    }
}

pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ImpactEvent>().add_systems(FixedPostUpdate, detect_impacts.after(PhysicsSystems::Writeback));
    }
}
