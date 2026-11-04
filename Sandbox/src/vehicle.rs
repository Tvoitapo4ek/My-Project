//! Seats and steering.
//! * RMB on a seat sits down (the player rides with the seat, also on a
//!   moving contraption); Shift stands up on top of it.
//! * A suspension turns towards `signal × steer°` (smoothly); its knuckle
//!   and the wheel next to it follow. In physics mode `contraption`
//!   drives the car with the wheels' speed and steering.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    content::Mech,
    core::{Toast, toast},
    grid::VoxelGrid,
    interact::UseBlock,
    mech::{BlockEntity, Params, Signal, transform_for},
    player::{Body, Player, Seated},
    style::{Layer, LayerNode},
};

/// Current steering angle of a suspension (radians), smoothed.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Steer(pub f32);

fn sit(mut used: MessageReader<UseBlock>, grids: Query<&VoxelGrid>, mut commands: Commands, players: Query<(Entity, Option<&Seated>), With<Player>>, mut toasts: MessageWriter<Toast>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    for u in used.read() {
        if u.mech != Mech::Seat {
            continue;
        }
        let Some(&seat) = grids.get(u.grid).ok().and_then(|g| g.entities.get(&u.cell)) else { continue };
        let Ok((p, seated)) = players.single() else { continue };
        if seated.is_some() {
            continue;
        }
        commands.entity(p).insert((Seated { seat }, RigidBody::Kinematic, ColliderDisabled));
        toasts.write(toast("Вы сели. Shift — встать"));
        sounds.write(crate::sound::sfx("seat"));
    }
}

/// Seated players follow the seat; Shift stands up.
fn ride(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, mut players: Query<(Entity, &Seated, &mut Position, &mut LinearVelocity, &mut Body), With<Player>>, seats: Query<&GlobalTransform>, scripted: Res<crate::player::movement::MoveInput>) {
    for (e, s, mut pos, mut vel, body) in &mut players {
        let Ok(gt) = seats.get(s.seat) else {
            commands.entity(e).remove::<(Seated, ColliderDisabled)>().insert(RigidBody::Dynamic);
            continue;
        };
        // Sit on the cushion: capsule centre ~0.35 m above the block centre.
        let seat_pos = gt.translation() + gt.up() * (body.height() * 0.5 - 0.35);
        pos.0 = seat_pos;
        vel.0 = Vec3::ZERO;
        let stand = keys.just_pressed(KeyCode::ShiftLeft) || (scripted.scripted && scripted.crouch);
        if stand {
            pos.0 = gt.translation() + Vec3::Y * (0.5 + body.height() * 0.5 + 0.05);
            commands.entity(e).remove::<(Seated, ColliderDisabled)>().insert(RigidBody::Dynamic);
        }
    }
}

fn attach_steer(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Steer>)>) {
    for (e, b) in &added {
        if b.mech() == Some(Mech::Suspension) {
            commands.entity(e).insert(Steer::default());
        }
    }
}

/// Steering: smooth towards the target; turn the knuckle and the wheel.
#[allow(clippy::type_complexity)]
fn steer(
    time: Res<Time>,
    mut susp: Query<(&BlockEntity, &Params, &Signal, &mut Steer, &Children)>,
    grids: Query<&VoxelGrid>,
    mut wheels: Query<(&BlockEntity, &mut Transform), Without<LayerNode>>,
    mut nodes: Query<(&LayerNode, &mut Transform), Without<BlockEntity>>,
) {
    let dt = time.delta_secs();
    for (b, p, sig, mut st, children) in &mut susp {
        let max = p.get(b.id, "steer").to_radians();
        let want = -sig.0 * max;
        st.0 += (want - st.0).clamp(-2.5 * dt, 2.5 * dt);
        // Grid up in the block's local space (the block's Y is the axle).
        let local_up = transform_for(b.cell, b.id, b.facing).rotation.inverse() * Vec3::Y;
        for c in children.iter() {
            if let Ok((l, mut t)) = nodes.get_mut(c)
                && l.0 == Layer::Steer
            {
                t.rotation = Quat::from_axis_angle(local_up, st.0);
            }
        }
        let Ok(grid) = grids.get(b.grid) else { continue };
        if let Some(&w) = grid.entities.get(&(b.cell + b.dir()))
            && let Ok((wb, mut t)) = wheels.get_mut(w)
            && wb.mech() == Some(Mech::Wheel)
        {
            let base = transform_for(wb.cell, wb.id, wb.facing);
            t.rotation = Quat::from_rotation_y(st.0) * base.rotation;
        }
    }
}

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (sit, attach_steer, steer.after(crate::mech::MechSet)))
            .add_systems(PostUpdate, ride.after(PhysicsSystems::Writeback).before(TransformSystems::Propagate));
    }
}
