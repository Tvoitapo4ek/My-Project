//! One-shot velocity changes (spells, explosions, anomalies, hits).
//!
//! Changing the velocity of a *sleeping* body directly is lost: the island
//! stays asleep and its velocities stay zero. Requests are therefore queued:
//! the body is woken with Avian's `WakeBody` command and the change is
//! applied on the next fixed tick, once the body is awake.

use avian3d::{dynamics::solver::islands::WakeBody, prelude::*};
use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct ImpulseQueue(pub Vec<(Entity, Vec3)>);

impl ImpulseQueue {
    /// Queues a velocity change (m/s) for a body.
    pub fn push(&mut self, entity: Entity, delta_v: Vec3) {
        self.0.push((entity, delta_v));
    }
}

fn apply(mut commands: Commands, mut queue: ResMut<ImpulseQueue>, mut bodies: Query<(&mut LinearVelocity, Has<Sleeping>, &RigidBody)>) {
    let pending = std::mem::take(&mut queue.0);
    for (e, dv) in pending {
        let Ok((mut v, sleeping, rb)) = bodies.get_mut(e) else { continue };
        if !rb.is_dynamic() {
            continue;
        }
        if sleeping {
            commands.queue(WakeBody(e));
            queue.0.push((e, dv));
        } else {
            v.0 += dv;
        }
    }
}

pub struct ImpulsesPlugin;

impl Plugin for ImpulsesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ImpulseQueue>().add_systems(FixedUpdate, apply);
    }
}
