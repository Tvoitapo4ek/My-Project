//! Gear transmission. Two gears mesh when their axes are parallel, their
//! centres lie in (almost) the same plane and the centre distance equals
//! the sum of radii. A velocity constraint then enforces
//! `ω₁·r₁ = −ω₂·r₂` (rim speeds match, directions opposite) using angular
//! impulses on both bodies — torque is transmitted both ways, so a driven
//! gear can also brake the driver. Shafts transmit rotation through the
//! ordinary joints they are welded/hinged with.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::components::Gear;

/// Pairs of meshing gears (recomputed every tick, also shown by the scanner).
#[derive(Resource, Default, Debug)]
pub struct GearMeshes(pub Vec<(Entity, Entity)>);

pub fn gears_mesh(p1: Vec3, a1: Vec3, r1: f32, p2: Vec3, a2: Vec3, r2: f32) -> bool {
    if a1.dot(a2).abs() < 0.95 {
        return false;
    }
    let d = p2 - p1;
    let along = d.dot(a1);
    let planar = (d - a1 * along).length();
    along.abs() < 0.3 && (planar - (r1 + r2)).abs() < 0.2
}

#[allow(clippy::type_complexity)]
fn couple_gears(
    mut gears: Query<(Entity, &Gear, &Position, &Rotation, &mut AngularVelocity, &ComputedMass, &RigidBody)>,
    mut meshes: ResMut<GearMeshes>,
) {
    meshes.0.clear();
    let list: Vec<(Entity, f32, Vec3, Vec3, f32, bool)> = gears
        .iter()
        .map(|(e, g, p, r, _, m, rb)| (e, g.radius, p.0, r.0 * Vec3::Y, m.value(), rb.is_dynamic()))
        .collect();
    for i in 0..list.len() {
        for j in i + 1..list.len() {
            let (e1, r1, p1, a1, m1, d1) = list[i];
            let (e2, r2, p2, a2, m2, d2) = list[j];
            if !gears_mesh(p1, a1, r1, p2, a2, r2) {
                continue;
            }
            meshes.0.push((e1, e2));
            let a2s = if a1.dot(a2) < 0.0 { -a2 } else { a2 };
            let [(_, _, _, _, mut w1, ..), (_, _, _, _, mut w2, ..)] = match gears.get_many_mut([e1, e2]) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Disc inertia about the axis; static gears have infinite inertia.
            let inv1 = if d1 { 1.0 / (0.5 * m1 * r1 * r1) } else { 0.0 };
            let inv2 = if d2 { 1.0 / (0.5 * m2 * r2 * r2) } else { 0.0 };
            let k = r1 * r1 * inv1 + r2 * r2 * inv2;
            if k <= 0.0 {
                continue;
            }
            let err = w1.0.dot(a1) * r1 + w2.0.dot(a2s) * r2;
            let lambda = -err / k * 0.9;
            w1.0 += a1 * (r1 * lambda * inv1);
            w2.0 += a2s * (r2 * lambda * inv2);
        }
    }
}

pub struct TransmissionPlugin;

impl Plugin for TransmissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GearMeshes>().add_systems(FixedUpdate, couple_gears.in_set(super::MachinerySet::Drive));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshing_geometry() {
        assert!(gears_mesh(Vec3::ZERO, Vec3::Y, 0.5, Vec3::new(1.0, 0.0, 0.0), Vec3::Y, 0.5));
        assert!(gears_mesh(Vec3::ZERO, Vec3::Y, 0.5, Vec3::new(0.0, 0.0, 1.05), Vec3::NEG_Y, 0.5));
        assert!(!gears_mesh(Vec3::ZERO, Vec3::Y, 0.5, Vec3::new(2.0, 0.0, 0.0), Vec3::Y, 0.5), "too far");
        assert!(!gears_mesh(Vec3::ZERO, Vec3::Y, 0.5, Vec3::new(1.0, 0.0, 0.0), Vec3::X, 0.5), "not parallel");
        assert!(!gears_mesh(Vec3::ZERO, Vec3::Y, 0.5, Vec3::new(1.0, 0.8, 0.0), Vec3::Y, 0.5), "offset along axis");
    }
}
