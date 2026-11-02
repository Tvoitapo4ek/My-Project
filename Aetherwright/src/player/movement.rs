//! Dynamic-body character controller.
//!
//! The player is a real Avian rigid body (capsule, rotation locked). Each
//! fixed tick we *steer* its horizontal velocity towards the desired velocity
//! with a bounded acceleration instead of overwriting it. Consequences:
//! * collisions, slopes and pushes are resolved by the physics solver;
//! * external impulses (magic, explosions, being hit) persist — strongly in
//!   the air (low air acceleration), briefly on the ground;
//! * gravity is applied by Avian (except while flying / swimming).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    PLAYER_CROUCH_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, Player, input::PlayerIntent,
    player_collider,
};
use crate::{
    core::{config::GameConfig, layers::Layer, resources::SpawnPoint, states::GameMode},
    world::chunks::VoxelWorld,
};

/// Runtime state of the character controller.
#[derive(Component, Debug, Clone)]
pub struct CharacterState {
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub crouching: bool,
    pub flying: bool,
    /// 0 = dry, 1 = fully submerged. Written by the world (water volumes).
    pub submerged: f32,
    /// Seconds since last grounded (coyote time for jumps).
    pub air_time: f32,
    /// Most negative vertical speed since leaving the ground (for fall damage).
    pub fall_speed: f32,
    /// Fall speed at the moment of the last landing; consumed by survival.
    pub landed_with_speed: Option<f32>,
    /// Current capsule height (standing or crouched).
    pub height: f32,
    /// Yaw of the look direction (radians), written by the camera.
    pub yaw: f32,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            grounded: false,
            ground_normal: Vec3::Y,
            crouching: false,
            flying: false,
            submerged: 0.0,
            air_time: 0.0,
            fall_speed: 0.0,
            landed_with_speed: None,
            height: PLAYER_HEIGHT,
            yaw: 0.0,
        }
    }
}

impl CharacterState {
    pub fn eye_offset(&self) -> f32 {
        self.height * 0.5 - super::EYE_FROM_TOP
    }
}

const COYOTE_TIME: f32 = 0.12;
const MAX_WALK_SLOPE_COS: f32 = 0.64; // ~50°

fn solid_filter(player: Entity) -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask([Layer::Default, Layer::Terrain, Layer::Prop, Layer::Creature])
        .with_excluded_entities([player])
}

/// Ground probe: a sphere slightly thinner than the capsule cast downwards
/// from the body centre to just below the feet.
fn update_ground(
    spatial: SpatialQuery,
    mut q: Query<(Entity, &Position, &LinearVelocity, &mut CharacterState), With<Player>>,
    time: Res<Time>,
) {
    for (entity, pos, vel, mut state) in &mut q {
        let probe_radius = PLAYER_RADIUS * 0.9;
        let to_bottom_sphere = state.height * 0.5 - PLAYER_RADIUS;
        let max_dist = to_bottom_sphere + (PLAYER_RADIUS - probe_radius) + 0.08;
        let hit = spatial.cast_shape(
            &Collider::sphere(probe_radius),
            pos.0,
            Quat::IDENTITY,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(max_dist),
            &solid_filter(entity),
        );
        let was_grounded = state.grounded;
        let (grounded, normal) = match hit {
            Some(h) if h.normal1.y >= MAX_WALK_SLOPE_COS && vel.y <= 2.5 => (true, h.normal1),
            _ => (false, Vec3::Y),
        };
        state.grounded = grounded;
        state.ground_normal = normal;
        if grounded {
            if !was_grounded && state.fall_speed < -1.0 {
                state.landed_with_speed = Some(-state.fall_speed);
            }
            state.air_time = 0.0;
            state.fall_speed = 0.0;
        } else {
            state.air_time += time.delta_secs();
            if state.submerged > 0.3 || state.flying {
                state.fall_speed = 0.0;
            } else {
                state.fall_speed = state.fall_speed.min(vel.y);
            }
        }
    }
}

/// Crouch: swaps the capsule for a shorter one, keeping the feet in place.
/// Standing up requires head room.
fn update_crouch(
    intent: Res<PlayerIntent>,
    mut params: ParamSet<(
        SpatialQuery,
        Query<(Entity, &mut Position, &mut Collider, &mut CharacterState), With<Player>>,
    )>,
) {
    // Pass 1: decide (read-only); the spatial query reads colliders itself.
    let decisions: Vec<(Entity, Vec3, bool, bool)> = params
        .p1()
        .iter()
        .map(|(e, pos, _, s)| {
            (
                e,
                pos.0,
                intent.crouch && !s.flying && s.submerged < 0.5,
                s.crouching,
            )
        })
        .collect();
    let delta = PLAYER_HEIGHT - PLAYER_CROUCH_HEIGHT;
    for (entity, pos, want, crouching) in decisions {
        if want == crouching {
            continue;
        }
        if !want {
            // Standing up requires head room for the full capsule.
            let stand_center = pos + Vec3::Y * delta * 0.5;
            let blocked = !params
                .p0()
                .shape_intersections(
                    &player_collider(PLAYER_HEIGHT * 0.98),
                    stand_center,
                    Quat::IDENTITY,
                    &solid_filter(entity),
                )
                .is_empty();
            if blocked {
                continue;
            }
        }
        let mut q = params.p1();
        let Ok((_, mut pos, mut collider, mut state)) = q.get_mut(entity) else {
            continue;
        };
        state.crouching = want;
        state.height = if want {
            PLAYER_CROUCH_HEIGHT
        } else {
            PLAYER_HEIGHT
        };
        *collider = player_collider(state.height);
        pos.0.y += if want { -delta * 0.5 } else { delta * 0.5 };
    }
}

fn move_toward(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let diff = target - current;
    let len = diff.length();
    if len <= max_delta || len < 1e-6 {
        target
    } else {
        current + diff / len * max_delta
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_movement(
    time: Res<Time>,
    cfg: Res<GameConfig>,
    mode: Res<State<GameMode>>,
    mut intent: ResMut<PlayerIntent>,
    spatial: SpatialQuery,
    mut q: Query<
        (
            Entity,
            &Position,
            &mut LinearVelocity,
            &mut GravityScale,
            &mut CharacterState,
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();
    let p = &cfg.player;
    for (entity, pos, mut vel, mut gravity_scale, mut state) in &mut q {
        if *mode.get() != GameMode::Creative {
            state.flying = false;
        }
        let rot = Quat::from_rotation_y(state.yaw);
        let forward = rot * Vec3::NEG_Z;
        let right = rot * Vec3::X;
        let wish = (right * intent.movement.x + forward * intent.movement.y).normalize_or_zero();

        if state.flying {
            gravity_scale.0 = 0.0;
            let vertical = (intent.jump_held as i8 - intent.descend as i8) as f32;
            let speed = if intent.sprint {
                p.fly_speed * 2.0
            } else {
                p.fly_speed
            };
            let target = wish * speed + Vec3::Y * vertical * speed * 0.7;
            vel.0 = move_toward(vel.0, target, 80.0 * dt);
            intent.jump_pressed = false;
            continue;
        }

        let swimming = state.submerged > 0.45;
        if swimming {
            // Buoyancy roughly cancels gravity; drag; swim up with Space.
            gravity_scale.0 = 0.15;
            let vertical = if intent.jump_held {
                1.0
            } else if intent.descend {
                -1.0
            } else {
                -0.1
            };
            let target = wish * p.swim_speed + Vec3::Y * vertical * p.swim_speed;
            vel.0 = move_toward(vel.0, target, 18.0 * dt);
            // Hop out of water onto a ledge.
            if intent.jump_held && state.submerged < 0.75 {
                vel.y = vel.y.max(4.5);
            }
            intent.jump_pressed = false;
            continue;
        }
        gravity_scale.0 = 1.0;

        let speed = if state.crouching {
            p.crouch_speed
        } else if intent.sprint && intent.movement.y > 0.1 {
            p.sprint_speed
        } else {
            p.walk_speed
        };
        let accel = if state.grounded {
            p.ground_accel
        } else {
            p.air_accel
        };

        // Horizontal steering on the ground plane (projected onto slopes).
        let horizontal = Vec3::new(vel.x, 0.0, vel.z);
        let target = wish * speed;
        // In the air, never brake an impulse that exceeds walking speed
        // unless the player actively steers against it.
        let new_h = if !state.grounded && wish == Vec3::ZERO {
            horizontal
        } else {
            move_toward(horizontal, target, accel * dt)
        };
        vel.x = new_h.x;
        vel.z = new_h.z;

        // Jump (with coyote time).
        let can_jump = state.grounded || state.air_time < COYOTE_TIME;
        if intent.jump_pressed && can_jump {
            vel.y = p.jump_speed;
            state.grounded = false;
            state.air_time = COYOTE_TIME;
        }
        intent.jump_pressed = false;

        // Auto-step: walking into a knee-high obstacle with free space above
        // gives a small hop, so 1-block ledges don't require jumping.
        if state.grounded && wish != Vec3::ZERO && !state.crouching {
            let feet = pos.0.y - state.height * 0.5;
            let filter = solid_filter(entity);
            let dir = Dir3::new(wish).unwrap_or(Dir3::NEG_Z);
            let probe = PLAYER_RADIUS + 0.25;
            let low = spatial.cast_ray(
                Vec3::new(pos.x, feet + 0.45, pos.z),
                dir,
                probe,
                true,
                &filter,
            );
            let high = spatial.cast_ray(
                Vec3::new(pos.x, feet + 1.25, pos.z),
                dir,
                probe + 0.2,
                true,
                &filter,
            );
            if let (Some(hit), None) = (low, high)
                && hit.normal.y.abs() < 0.3
            {
                vel.y = vel.y.max(6.2);
            }
        }
    }
}

/// Fraction of the body inside liquid blocks (samples feet, waist, head).
fn update_submersion(
    world: Res<VoxelWorld>,
    mut q: Query<(&Position, &mut CharacterState), With<Player>>,
) {
    for (pos, mut state) in &mut q {
        let feet = pos.0.y - state.height * 0.5;
        let samples = [0.15, state.height * 0.5, state.height - 0.2];
        let wet = samples
            .iter()
            .filter(|&&h| {
                world
                    .get(Vec3::new(pos.x, feet + h, pos.z).floor().as_ivec3())
                    .is_liquid()
            })
            .count();
        state.submerged = wet as f32 / samples.len() as f32;
    }
}

/// Creative flight toggle (double-tap is unreliable on trackpads → key F).
fn toggle_fly(
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut CharacterState, With<Player>>,
    mut notify: MessageWriter<crate::core::events::Notify>,
) {
    if keys.just_pressed(KeyCode::KeyF) {
        for mut s in &mut q {
            s.flying = !s.flying;
            notify.write(crate::core::events::Notify::info(if s.flying {
                "Полёт: вкл"
            } else {
                "Полёт: выкл"
            }));
        }
    }
}

fn disable_fly_outside_creative(mut q: Query<&mut CharacterState, With<Player>>) {
    for mut s in &mut q {
        s.flying = false;
    }
}

/// Safety net: if the player falls out of the world, put them back.
fn respawn_if_fallen(
    spawn: Res<SpawnPoint>,
    mut q: Query<(&mut Position, &mut LinearVelocity), With<Player>>,
) {
    for (mut pos, mut vel) in &mut q {
        if pos.0.y < -60.0 {
            pos.0 = spawn.0;
            vel.0 = Vec3::ZERO;
        }
    }
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                update_submersion,
                update_ground,
                update_crouch,
                apply_movement,
                respawn_if_fallen,
            )
                .chain()
                .run_if(crate::core::states::in_game),
        )
        .add_systems(
            Update,
            toggle_fly
                .run_if(in_state(GameMode::Creative))
                .run_if(crate::core::states::gameplay_active),
        )
        .add_systems(OnEnter(GameMode::Survival), disable_fly_outside_creative);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_toward_clamps_step() {
        let v = move_toward(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), 2.0);
        assert!((v.x - 2.0).abs() < 1e-5);
        let v = move_toward(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 2.0);
        assert_eq!(v, Vec3::new(1.0, 0.0, 0.0));
    }
}
