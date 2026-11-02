//! Placement preview: a wireframe ghost whose colour tells the player
//! whether the placement is allowed.

use avian3d::prelude::*;
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    /// Can be placed.
    Ok,
    /// Overlaps the player, a creature or another object.
    Obstructed,
    /// Not allowed (no item, out of world, no surface...).
    Invalid,
}

impl Validity {
    pub fn color(self) -> Color {
        match self {
            Validity::Ok => Color::srgb(0.35, 1.0, 0.5),
            Validity::Obstructed => Color::srgb(1.0, 0.6, 0.15),
            Validity::Invalid => Color::srgb(1.0, 0.2, 0.25),
        }
    }
}

/// Shape of the ghost.
#[derive(Debug, Clone, Copy)]
pub enum GhostShape {
    Box(Vec3),
    /// radius, height (local Y axis)
    Cylinder(f32, f32),
}

/// What to preview this frame (written by placement/selection systems).
#[derive(Resource, Default, Debug)]
pub struct Ghosts(pub Vec<(GhostShape, Transform, Validity)>);

/// What was drawn last frame (for UI hints and tests).
#[derive(Resource, Default, Debug)]
pub struct LastGhosts(pub Vec<(GhostShape, Transform, Validity)>);

pub fn shape_of(collider: &Collider) -> GhostShape {
    if let Some(c) = collider.shape().as_cuboid() {
        GhostShape::Box(Vec3::from(c.half_extents) * 2.0)
    } else if let Some(c) = collider.shape().as_cylinder() {
        GhostShape::Cylinder(c.radius, c.half_height * 2.0)
    } else {
        GhostShape::Box(Vec3::ONE)
    }
}

fn draw(mut ghosts: ResMut<Ghosts>, mut last: ResMut<LastGhosts>, mut gizmos: Gizmos) {
    last.0 = ghosts.0.clone();
    for (shape, t, v) in ghosts.0.drain(..) {
        let color = v.color();
        match shape {
            GhostShape::Box(size) => {
                gizmos.cube(t.with_scale(size), color);
                // A diagonal cross makes "invalid" obvious at a glance.
                if v != Validity::Ok {
                    let h = size * 0.5;
                    gizmos.line(t * Vec3::new(-h.x, -h.y, -h.z), t * h, color);
                    gizmos.line(t * Vec3::new(h.x, -h.y, -h.z), t * Vec3::new(-h.x, h.y, h.z), color);
                }
            }
            GhostShape::Cylinder(r, h) => {
                let up = t.rotation * Vec3::Y;
                for y in [-h * 0.5, h * 0.5] {
                    let c = t.translation + up * y;
                    gizmos.circle(Isometry3d::new(c, t.rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), r, color);
                }
                for i in 0..6 {
                    let a = i as f32 / 6.0 * std::f32::consts::TAU;
                    let off = t.rotation * Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                    gizmos.line(t.translation + off - up * h * 0.5, t.translation + off + up * h * 0.5, color);
                }
            }
        }
    }
}

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ghosts>().init_resource::<LastGhosts>().add_systems(PostUpdate, draw);
    }
}
