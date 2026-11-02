//! Engines drive hinges. Every hinge (revolute joint) that has an engine on
//! one side gets an angular motor whose target speed and torque come from
//! the engine state; without power the motor is disabled and the hinge
//! spins freely. The reaction torque acts on the engine body (and whatever
//! it is welded to), as it should.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::components::Engine;
use crate::physics::joints::{Connection, JointKind};

/// Sign that makes a hinge on any face of the engine turn the same way in
/// the engine's own frame: hinges on opposite faces (±Z) have opposite axes,
/// so wheels on both sides must get opposite motor targets to roll together.
pub fn axis_sign(axis_in_engine_frame: Vec3) -> f32 {
    let a = axis_in_engine_frame;
    let dominant = if a.z.abs() >= 0.5 {
        a.z
    } else if a.x.abs() >= 0.5 {
        a.x
    } else {
        a.y
    };
    if dominant < 0.0 { -1.0 } else { 1.0 }
}

fn drive_hinges(engines: Query<&Engine>, mut joints: Query<(&Connection, &mut RevoluteJoint)>) {
    for (c, mut joint) in &mut joints {
        if c.kind != JointKind::Hinge {
            continue;
        }
        // Hinge axis expressed in each body's local frame.
        let (engine, sign) = match (engines.get(c.a), engines.get(c.b)) {
            (Ok(e), _) => (e, axis_sign(c.frame_a.basis * Vec3::Z)),
            (_, Ok(e)) => (e, -axis_sign(c.frame_b.basis * Vec3::Z)),
            _ => continue,
        };
        let dir = if engine.reverse { -1.0 } else { 1.0 };
        let enabled = engine.powered;
        let target = engine.max_speed * engine.throttle * dir * sign;
        let m = &mut joint.motor;
        if m.enabled != enabled || (m.target_velocity - target).abs() > 1e-3 {
            *m = AngularMotor {
                enabled,
                target_velocity: target,
                max_torque: engine.max_torque * engine.throttle.max(0.25),
                motor_model: MotorModel::AccelerationBased {
                    stiffness: 0.0,
                    damping: 1.0,
                },
                ..AngularMotor::new_disabled(MotorModel::DEFAULT)
            };
        }
    }
}

/// Engines ask their model (if any) for the `work` clip while powered.
fn engine_animation(
    mut q: Query<(&Engine, &mut crate::models::AnimationRequest), Changed<Engine>>,
) {
    for (e, mut a) in &mut q {
        let clip = if e.powered { "work" } else { "idle" };
        if a.0 != clip {
            a.0 = clip.to_string();
        }
    }
}

pub struct EnginesPlugin;

impl Plugin for EnginesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, drive_hinges.in_set(super::MachinerySet::Drive))
            .add_systems(Update, engine_animation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_faces_get_opposite_signs() {
        assert_eq!(axis_sign(Vec3::Z), 1.0);
        assert_eq!(axis_sign(Vec3::NEG_Z), -1.0);
        assert_eq!(axis_sign(Vec3::NEG_X), -1.0);
        assert_eq!(axis_sign(Vec3::Y), 1.0);
    }
}
