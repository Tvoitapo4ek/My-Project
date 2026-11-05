//! Connections between bodies. A `Connection` is the game-level description
//! (which bodies, kind, local frames) and is the source of truth for saving;
//! the Avian joint component on the same entity is derived from it.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JointKind {
    /// Rigid weld.
    Weld,
    /// Rotates about the frame's local Z axis.
    Hinge,
}

/// A body-local frame: anchor point + basis (hinge axis = basis * Z).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalFrame {
    pub anchor: Vec3,
    pub basis: Quat,
}

impl LocalFrame {
    /// Expresses a world-space frame in a body's local space.
    pub fn from_world(body_pos: Vec3, body_rot: Quat, anchor: Vec3, basis: Quat) -> Self {
        let inv = body_rot.inverse();
        Self {
            anchor: inv * (anchor - body_pos),
            basis: (inv * basis).normalize(),
        }
    }
    fn iso(self) -> Isometry3d {
        Isometry3d::new(self.anchor, self.basis)
    }
}

#[derive(Component, Debug, Clone)]
pub struct Connection {
    pub a: Entity,
    pub b: Entity,
    pub kind: JointKind,
    pub frame_a: LocalFrame,
    pub frame_b: LocalFrame,
}

/// Spawns a joint entity (Connection + Avian joint). Bodies joined together
/// don't collide with each other.
pub fn spawn_connection(commands: &mut Commands, c: Connection) -> Entity {
    let mut e = commands.spawn((
        Name::new(format!("{:?} joint", c.kind)),
        JointCollisionDisabled,
    ));
    match c.kind {
        JointKind::Weld => {
            e.insert(
                FixedJoint::new(c.a, c.b)
                    .with_local_frame1(c.frame_a.iso())
                    .with_local_frame2(c.frame_b.iso()),
            );
        }
        JointKind::Hinge => {
            e.insert(
                RevoluteJoint::new(c.a, c.b)
                    .with_local_frame1(c.frame_a.iso())
                    .with_local_frame2(c.frame_b.iso())
                    .with_hinge_axis(Vec3::Z),
            );
        }
    }
    e.insert(c);
    e.id()
}

/// Removes joints whose bodies no longer exist.
fn prune_dangling(
    mut commands: Commands,
    joints: Query<(Entity, &Connection)>,
    bodies: Query<(), With<RigidBody>>,
) {
    for (e, c) in &joints {
        if bodies.get(c.a).is_err() || bodies.get(c.b).is_err() {
            commands.entity(e).despawn();
        }
    }
}

/// Connected component of bodies (through any joints) — "a construction".
pub fn construction_of(start: Entity, joints: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut seen = vec![start];
    let mut i = 0;
    while i < seen.len() {
        let cur = seen[i];
        for &(a, b) in joints {
            let other = if a == cur {
                b
            } else if b == cur {
                a
            } else {
                continue;
            };
            if !seen.contains(&other) {
                seen.push(other);
            }
        }
        i += 1;
    }
    seen
}

fn draw_joints(
    joints: Query<&Connection>,
    bodies: Query<(&Position, &Rotation)>,
    tool: Res<crate::tools::ActiveTool>,
    mut gizmos: Gizmos,
) {
    if *tool != crate::tools::ActiveTool::PhysGun {
        return;
    }
    for c in &joints {
        let Ok((pa, ra)) = bodies.get(c.a) else {
            continue;
        };
        let p = pa.0 + ra.0 * c.frame_a.anchor;
        let color = match c.kind {
            JointKind::Weld => Color::srgb(1.0, 0.8, 0.3),
            JointKind::Hinge => Color::srgb(0.4, 1.0, 0.8),
        };
        gizmos.sphere(Isometry3d::from_translation(p), 0.08, color);
        if c.kind == JointKind::Hinge {
            let axis = ra.0 * c.frame_a.basis * Vec3::Z;
            gizmos.line(p - axis * 0.4, p + axis * 0.4, color);
        }
    }
}

pub struct JointsPlugin;

impl Plugin for JointsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (prune_dangling, draw_joints));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_frame_roundtrip() {
        let pos = Vec3::new(3.0, 1.0, -2.0);
        let rot = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.3);
        let anchor = Vec3::new(3.5, 2.0, -2.0);
        let basis = Quat::from_rotation_arc(Vec3::Z, Vec3::X);
        let f = LocalFrame::from_world(pos, rot, anchor, basis);
        assert!((pos + rot * f.anchor - anchor).length() < 1e-5);
        assert!(((rot * f.basis) * Vec3::Z - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn construction_graph() {
        let e = |i| Entity::from_raw_u32(i).unwrap();
        let joints = [(e(1), e(2)), (e(2), e(3)), (e(5), e(6))];
        let mut c = construction_of(e(3), &joints);
        c.sort_by_key(|e| e.index());
        assert_eq!(c, vec![e(1), e(2), e(3)]);
    }
}
