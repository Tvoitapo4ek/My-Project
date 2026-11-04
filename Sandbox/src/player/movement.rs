//! Character controller. Each fixed tick the capsule's velocity is steered
//! towards the wanted velocity (bounded acceleration), so collisions and
//! pushes stay physical. Crouching on the ground never walks off an edge —
//! the move is cancelled per axis when there would be no ground under the
//! feet (like sneaking in Minecraft).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{Body, CROUCH_HEIGHT, HEIGHT, Player, RADIUS, Seated, collider};

pub const WALK: f32 = 4.3;
pub const SPRINT: f32 = 5.6;
pub const CROUCH: f32 = 1.3;
pub const FLY: f32 = 10.9;
pub const JUMP_SPEED: f32 = 8.3;

/// What the player wants this frame (keyboard, or a script).
#[derive(Resource, Debug, Clone, Default)]
pub struct MoveInput {
    /// x = right, y = forward (−1..1).
    pub axis: Vec2,
    pub jump: bool,
    pub jump_pressed: bool,
    pub crouch: bool,
    pub sprint: bool,
    /// Set by scripts: keyboard is ignored.
    pub scripted: bool,
}

fn read_keys(keys: Res<ButtonInput<KeyCode>>, mut input: ResMut<MoveInput>, ui: Res<State<crate::core::Ui>>) {
    if input.scripted {
        return;
    }
    if *ui.get() != crate::core::Ui::Game {
        *input = MoveInput::default();
        return;
    }
    let k = |c| keys.pressed(c) as i32 as f32;
    input.axis = Vec2::new(k(KeyCode::KeyD) - k(KeyCode::KeyA), k(KeyCode::KeyW) - k(KeyCode::KeyS));
    input.jump = keys.pressed(KeyCode::Space);
    input.jump_pressed |= keys.just_pressed(KeyCode::Space);
    input.crouch = keys.pressed(KeyCode::ShiftLeft);
    input.sprint = keys.pressed(KeyCode::ControlLeft);
}

fn move_toward(cur: Vec3, target: Vec3, max: f32) -> Vec3 {
    let d = target - cur;
    if d.length() <= max { target } else { cur + d.normalize() * max }
}

/// Is there something to stand on under a capsule centred at `pos`?
fn ground_under(spatial: &SpatialQuery, me: Entity, pos: Vec3, height: f32) -> bool {
    let feet = pos.y - height * 0.5;
    // Narrower than the capsule: the rounded bottom must stay supported,
    // or it slides off the edge.
    let probe = Collider::cuboid(RADIUS, 0.5, RADIUS);
    let filter = SpatialQueryFilter::default().with_excluded_entities([me]);
    !spatial.shape_intersections(&probe, Vec3::new(pos.x, feet - 0.3, pos.z), Quat::IDENTITY, &filter).is_empty()
}

#[allow(clippy::type_complexity)]
fn step(
    mut commands: Commands,
    time: Res<Time>,
    mut input: ResMut<MoveInput>,
    spatial: SpatialQuery,
    mut q: Query<(Entity, &Position, &mut LinearVelocity, &mut GravityScale, &mut Body), (With<Player>, Without<Seated>)>,
) {
    let dt = time.delta_secs();
    for (e, pos, mut vel, mut grav, mut body) in &mut q {
        let mut pos = *pos;
        let filter = SpatialQueryFilter::default().with_excluded_entities([e]);
        // Ground.
        let h = body.height();
        let hit = spatial.cast_shape(&Collider::sphere(RADIUS * 0.9), pos.0, Quat::IDENTITY, Dir3::NEG_Y, &ShapeCastConfig::from_max_distance(h * 0.5 - RADIUS * 0.9 + 0.08), &filter);
        body.grounded = hit.is_some_and(|h| h.normal1.y > 0.6) && vel.y <= 2.0;
        // Flight: double tap Space.
        body.last_jump_press += dt;
        if input.jump_pressed {
            if body.last_jump_press < 0.4 {
                body.flying = !body.flying;
                body.last_jump_press = 10.0;
            } else {
                body.last_jump_press = 0.0;
            }
        }
        if body.flying && body.grounded && input.crouch {
            body.flying = false;
        }
        // Crouch (not while flying): swap the capsule, keep the feet.
        let want_crouch = input.crouch && !body.flying;
        if want_crouch != body.crouching {
            let delta = HEIGHT - CROUCH_HEIGHT;
            let ok = want_crouch || spatial.shape_intersections(&collider(HEIGHT * 0.98), pos.0 + Vec3::Y * delta * 0.5, Quat::IDENTITY, &filter).is_empty();
            if ok {
                body.crouching = want_crouch;
                commands.entity(e).insert(collider(body.height()));
                pos.0.y += if want_crouch { -delta * 0.5 } else { delta * 0.5 };
                commands.entity(e).insert(pos);
            }
        }
        body.sprinting = input.sprint && input.axis.y > 0.1 && !body.crouching;
        let rot = Quat::from_rotation_y(body.yaw);
        let wish = (rot * Vec3::new(input.axis.x, 0.0, -input.axis.y)).normalize_or_zero();
        if body.flying {
            grav.0 = 0.0;
            let speed = if body.sprinting { FLY * 2.0 } else { FLY };
            let up = (input.jump as i32 - input.crouch as i32) as f32;
            vel.0 = move_toward(vel.0, wish * speed + Vec3::Y * up * speed * 0.7, 60.0 * dt);
        } else {
            grav.0 = 2.6;
            let speed = if body.crouching { CROUCH } else if body.sprinting { SPRINT } else { WALK };
            let accel = if body.grounded { 70.0 } else { 18.0 };
            let target = wish * speed;
            let h_now = Vec3::new(vel.x, 0.0, vel.z);
            let mut h_new = move_toward(h_now, target, accel * dt);
            // Sneak edge guard: never step off into the void while crouching.
            if body.crouching && body.grounded {
                let look = 0.12;
                if h_new.x != 0.0 && !ground_under(&spatial, e, pos.0 + Vec3::X * h_new.x.signum() * look, h) {
                    h_new.x = 0.0;
                }
                if h_new.z != 0.0 && !ground_under(&spatial, e, pos.0 + Vec3::Z * h_new.z.signum() * look, h) {
                    h_new.z = 0.0;
                }
            }
            vel.x = h_new.x;
            vel.z = h_new.z;
            if input.jump && body.grounded {
                vel.y = JUMP_SPEED;
                body.grounded = false;
            }
        }
        body.speed = Vec2::new(vel.x, vel.z).length();
        if body.swing > 0.0 {
            body.swing = (body.swing - dt * 4.0).max(0.0);
        }
    }
    input.jump_pressed = false;
}

/// Falling off the platform: back to the spawn point.
fn respawn(mut q: Query<(&mut Position, &mut LinearVelocity), With<Player>>) {
    for (mut p, mut v) in &mut q {
        if p.0.y < -60.0 {
            p.0 = crate::world::SPAWN + Vec3::Y * 1.0;
            v.0 = Vec3::ZERO;
        }
    }
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoveInput>()
            .add_systems(Update, read_keys)
            .add_systems(FixedUpdate, (step, respawn));
    }
}
