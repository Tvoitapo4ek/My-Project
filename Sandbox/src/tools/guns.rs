//! Physics tools.
//!
//! **Gravity gun** (like Half-Life 2): RMB pulls an object from afar and
//! holds it in front of you (RMB again drops it); LMB punts — launches the
//! held object or pushes the one you look at. Heavy objects can only be
//! pushed.
//!
//! **Physgun** (like Garry's Mod): hold LMB to grab an object at the point
//! you hit; it follows your aim, the mouse wheel pulls it in / pushes it
//! away, holding E lets the mouse rotate it. RMB while holding freezes it in
//! the air; grabbing a frozen object unfreezes it, R unfreezes the one you
//! look at.

use avian3d::prelude::*;
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};

use crate::{
    content::Tool,
    core::{Toast, Ui, toast},
    interact::{Eye, WheelBusy},
    inventory::Inventory,
    player::{Body, Player, camera::LookLocked},
};

pub const GRAVGUN_RANGE: f32 = 14.0;
pub const GRAVGUN_MAX_MASS: f32 = 1500.0;
pub const PHYSGUN_RANGE: f32 = 60.0;

/// Frozen by the physgun (kept static until unfrozen).
#[derive(Component)]
pub struct Frozen;

/// What a gun is holding.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct Held {
    pub body: Option<Entity>,
    pub tool: Option<Tool>,
    /// Physgun: grab point in body space and distance from the eye.
    pub local: Vec3,
    pub distance: f32,
}

/// Mouse buttons for guns (scripts can set them too).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct GunInput {
    pub left: bool,
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub rotate: bool,
    pub unfreeze: bool,
    pub wheel: f32,
    pub mouse: Vec2,
    pub scripted: bool,
}

fn read_input(mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, scroll: Res<AccumulatedMouseScroll>, motion: Res<AccumulatedMouseMotion>, ui: Res<State<Ui>>, mut gi: ResMut<GunInput>) {
    if gi.scripted {
        return;
    }
    if *ui.get() != Ui::Game {
        *gi = GunInput::default();
        return;
    }
    gi.left = mouse.pressed(MouseButton::Left);
    gi.left_pressed = mouse.just_pressed(MouseButton::Left);
    gi.right_pressed = mouse.just_pressed(MouseButton::Right);
    gi.rotate = keys.pressed(KeyCode::KeyE);
    gi.unfreeze = keys.just_pressed(KeyCode::KeyR);
    gi.wheel = scroll.delta.y;
    gi.mouse = motion.delta;
}

/// The physical body under the crosshair: (body, hit point, distance).
fn aim_body(spatial: &SpatialQuery, eye: &Eye, me: Entity, range: f32, colliders: &Query<&ColliderOf>, bodies: &Query<(&RigidBody, Option<&Frozen>)>) -> Option<(Entity, Vec3, f32)> {
    let hit = spatial.cast_ray(eye.pos, Dir3::new(eye.dir).ok()?, range, true, &SpatialQueryFilter::default().with_excluded_entities([me]))?;
    let body = colliders.get(hit.entity).map(|c| c.body).unwrap_or(hit.entity);
    let (rb, frozen) = bodies.get(body).ok()?;
    (rb.is_dynamic() || frozen.is_some()).then_some((body, eye.pos + eye.dir * hit.distance, hit.distance))
}

#[allow(clippy::too_many_arguments)]
fn guns(
    mut commands: Commands,
    time: Res<Time>,
    eye: Res<Eye>,
    mut gi: ResMut<GunInput>,
    mut held: ResMut<Held>,
    mut wheel_busy: ResMut<WheelBusy>,
    mut locked: ResMut<LookLocked>,
    spatial: SpatialQuery,
    mut players: Query<(Entity, &Inventory, &mut Body), With<Player>>,
    colliders: Query<&ColliderOf>,
    bodies: Query<(&RigidBody, Option<&Frozen>)>,
    mut dynamics: Query<(&GlobalTransform, &mut LinearVelocity, &mut AngularVelocity, &ComputedMass, &ComputedCenterOfMass)>,
    mut gizmos: Gizmos,
    mut toasts: MessageWriter<Toast>,
) {
    let dt = time.delta_secs().max(1e-4);
    wheel_busy.0 = false;
    locked.0 = false;
    let Ok((me, inv, mut body)) = players.single_mut() else { return };
    let tool = super::held_tool(inv).filter(|t| matches!(t, Tool::GravityGun | Tool::PhysGun));
    // Switching away drops whatever is held.
    if tool != held.tool {
        *held = Held { tool, ..default() };
    }
    let Some(tool) = tool else { return };
    if held.body.is_some_and(|b| dynamics.get(b).is_err() && bodies.get(b).map(|(_, f)| f.is_none()).unwrap_or(true)) {
        held.body = None;
    }
    let muzzle = eye.pos + eye.dir * 0.6 - Vec3::Y * 0.25;
    match tool {
        Tool::GravityGun => {
            if gi.right_pressed {
                if held.body.take().is_none()
                    && let Some((b, _, d)) = aim_body(&spatial, &eye, me, GRAVGUN_RANGE, &colliders, &bodies)
                {
                    if let Ok((.., m, _)) = dynamics.get(b)
                        && m.value() > GRAVGUN_MAX_MASS
                    {
                        toasts.write(toast("Слишком тяжело для гравитационной пушки"));
                    } else {
                        held.body = Some(b);
                        held.distance = d;
                    }
                }
                body.swing = 1.0;
            }
            if gi.left_pressed {
                body.swing = 1.0;
                let target = held.body.take().or_else(|| aim_body(&spatial, &eye, me, GRAVGUN_RANGE * 0.6, &colliders, &bodies).map(|(b, ..)| b));
                if let Some(b) = target
                    && let Ok((_, mut v, _, m, _)) = dynamics.get_mut(b)
                {
                    // Same push for everything, lighter flies further.
                    let impulse = 9000.0;
                    v.0 += eye.dir * (impulse / m.value().max(1.0)).min(30.0);
                    commands.entity(b).remove::<Frozen>();
                }
            }
            if let Some(b) = held.body
                && let Ok((gt, mut v, mut w, _, com)) = dynamics.get_mut(b)
            {
                // Pull in from afar, then hold ~2.5 m in front.
                held.distance = (held.distance - 20.0 * dt).max(2.5);
                let center = gt.transform_point(com.0);
                let target = eye.pos + eye.dir * held.distance;
                v.0 = (target - center) * 10.0;
                w.0 *= 0.85;
                gizmos.line(muzzle, center, Color::srgb(1.0, 0.65, 0.15));
            }
        }
        Tool::PhysGun => {
            if gi.left_pressed {
                body.swing = 1.0;
                if let Some((b, p, d)) = aim_body(&spatial, &eye, me, PHYSGUN_RANGE, &colliders, &bodies)
                    && let Ok((gt, ..)) = dynamics.get(b)
                {
                    held.body = Some(b);
                    held.local = gt.affine().inverse().transform_point3(p);
                    held.distance = d;
                    commands.entity(b).remove::<Frozen>().insert(RigidBody::Dynamic);
                }
            }
            if !gi.left {
                held.body = None;
            }
            if gi.unfreeze
                && let Some((b, ..)) = aim_body(&spatial, &eye, me, PHYSGUN_RANGE, &colliders, &bodies)
                && bodies.get(b).is_ok_and(|(_, f)| f.is_some())
            {
                commands.entity(b).remove::<Frozen>().insert(RigidBody::Dynamic);
                toasts.write(toast("Разморожено"));
            }
            if let Some(b) = held.body {
                wheel_busy.0 = true;
                held.distance = (held.distance + gi.wheel.signum() * 0.8 * (gi.wheel != 0.0) as i32 as f32).clamp(1.5, PHYSGUN_RANGE);
                if let Ok((gt, mut v, mut w, _, _)) = dynamics.get_mut(b) {
                    let grab = gt.transform_point(held.local);
                    let target = eye.pos + eye.dir * held.distance;
                    v.0 = (target - grab) * 12.0;
                    if gi.rotate {
                        // E + mouse: rotate about the view axes.
                        locked.0 = true;
                        let right = eye.dir.cross(Vec3::Y).normalize_or(Vec3::X);
                        w.0 = Vec3::Y * gi.mouse.x * 0.8 + right * gi.mouse.y * 0.8;
                    } else {
                        w.0 *= 0.8;
                    }
                    gizmos.line(muzzle, grab, Color::srgb(0.3, 0.85, 1.0));
                    if gi.right_pressed {
                        v.0 = Vec3::ZERO;
                        w.0 = Vec3::ZERO;
                        commands.entity(b).insert((Frozen, RigidBody::Static));
                        held.body = None;
                        toasts.write(toast("Заморожено — R чтобы разморозить"));
                    }
                }
            }
        }
        _ => {}
    }
    if gi.scripted {
        gi.left_pressed = false;
        gi.right_pressed = false;
        gi.unfreeze = false;
        gi.wheel = 0.0;
    }
}

pub struct GunsPlugin;

impl Plugin for GunsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Held>().init_resource::<GunInput>().add_systems(Update, (read_input, guns).chain().after(crate::interact::InteractSet));
    }
}
