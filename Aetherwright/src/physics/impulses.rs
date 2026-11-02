//! One-shot velocity changes (spells, explosions, anomalies, hits).
//!
//! Changing the velocity of a *sleeping* body directly is lost: the island
//! stays asleep (or is restored when it wakes). Requests are therefore
//! queued: a sleeping body temporarily gets `SleepingDisabled` (Avian then
//! wakes its island), and the velocity change is applied on a later fixed
//! tick once the body is really awake. The temporary marker is removed
//! after half a second.

use avian3d::{dynamics::solver::islands::WakeBody, prelude::*};
use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct ImpulseQueue(pub Vec<(Entity, Vec3, u8)>);

impl ImpulseQueue {
    /// Queues a velocity change (m/s) for a body.
    pub fn push(&mut self, entity: Entity, delta_v: Vec3) {
        self.0.push((entity, delta_v, 0));
    }
}

/// `SleepingDisabled` inserted by the impulse system (removed when expired).
#[derive(Component)]
struct TempAwake(f32);

fn apply(
    mut commands: Commands,
    mut queue: ResMut<ImpulseQueue>,
    mut bodies: Query<(
        &mut LinearVelocity,
        Has<Sleeping>,
        Has<SleepingDisabled>,
        &RigidBody,
    )>,
) {
    let pending = std::mem::take(&mut queue.0);
    for (e, dv, tries) in pending {
        let Ok((mut v, sleeping, no_sleep, rb)) = bodies.get_mut(e) else {
            continue;
        };
        if !rb.is_dynamic() {
            continue;
        }
        if sleeping && tries < 60 {
            if !no_sleep {
                commands
                    .entity(e)
                    .insert((SleepingDisabled, TempAwake(0.5)));
            }
            commands.queue(WakeBody(e));
            queue.0.push((e, dv, tries + 1));
        } else {
            v.0 += dv;
            if !no_sleep {
                commands
                    .entity(e)
                    .insert((SleepingDisabled, TempAwake(0.5)));
            }
        }
    }
}

fn expire(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut TempAwake)>) {
    for (e, mut t) in &mut q {
        t.0 -= time.delta_secs();
        if t.0 <= 0.0 {
            commands.entity(e).remove::<(SleepingDisabled, TempAwake)>();
        }
    }
}

pub struct ImpulsesPlugin;

impl Plugin for ImpulsesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ImpulseQueue>()
            .add_systems(FixedUpdate, (apply, expire).chain());
    }
}
