//! Anomalies and ambience around magical sites.
//!
//! * Agitated crystals form an anomaly: every few seconds a pulse pushes
//!   dynamic objects (and the player) within its radius.
//! * Each cluster of sources gets a coloured point light and drifting motes,
//!   so magic is readable from a distance.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{energy::AetherKind, sources::AetherSources};

#[derive(Component, Debug)]
pub struct Anomaly {
    pub kind: AetherKind,
    pub radius: f32,
    pub timer: f32,
}

#[derive(Component)]
struct Mote {
    center: Vec3,
    phase: f32,
    radius: f32,
}

/// Groups sources into clusters (centre, kind, total strength).
pub fn clusters(sources: &[(Vec3, AetherKind, f32)], join: f32) -> Vec<(Vec3, AetherKind, f32)> {
    let mut out: Vec<(Vec3, AetherKind, f32, f32)> = Vec::new(); // centre sum, kind, strength, count
    for (p, k, s) in sources {
        if let Some(c) = out.iter_mut().find(|c| c.1 == *k && (c.0 / c.3).distance(*p) < join) {
            c.0 += *p;
            c.2 += s;
            c.3 += 1.0;
        } else {
            out.push((*p, *k, *s, 1.0));
        }
    }
    out.into_iter().map(|(sum, k, s, n)| (sum / n, k, s)).collect()
}

fn spawn_site_effects(
    mut commands: Commands,
    mut done: Local<bool>,
    sources: Res<AetherSources>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Runs once, as soon as the source index has been built.
    if *done || sources.0.is_empty() {
        return;
    }
    *done = true;
    let mote_mesh = meshes.add(Sphere::new(0.06).mesh().ico(1).unwrap());
    // Only strong clusters near the surface get lights/motes (deep ore
    // deposits of ancient stone would otherwise add hundreds of lights).
    let mut list: Vec<_> = clusters(&sources.0, 10.0).into_iter().filter(|(c, _, s)| *s >= 8.0 && c.y > 12.0).collect();
    list.sort_by(|a, b| b.2.total_cmp(&a.2));
    list.truncate(8);
    info!("Aether sites: {}", list.len());
    for (center, kind, strength) in list {
        let color = kind.color();
        commands.spawn((
            Name::new(format!("Aether light {kind:?}")),
            PointLight { color, intensity: 6_000.0 + 150.0 * strength.min(60.0), range: 18.0, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(center + Vec3::Y * 2.0),
        ));
        if kind == AetherKind::Agitated {
            commands.spawn((Name::new("Agitated anomaly"), Anomaly { kind, radius: 9.0, timer: 3.0 }, Transform::from_translation(center)));
        }
        let mat = materials.add(StandardMaterial { base_color: color, emissive: color.to_linear() * 8.0, unlit: true, ..default() });
        for i in 0..18 {
            commands.spawn((
                Mesh3d(mote_mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(center),
                Mote { center, phase: i as f32 * 0.37, radius: 2.0 + (i % 5) as f32 * 0.9 },
                bevy::light::NotShadowCaster,
            ));
        }
    }
}

fn animate_motes(time: Res<Time>, mut q: Query<(&mut Transform, &Mote)>) {
    let t = time.elapsed_secs();
    for (mut tr, m) in &mut q {
        let a = t * (0.25 + m.phase * 0.1) + m.phase * 5.0;
        tr.translation = m.center + Vec3::new(a.cos() * m.radius, 1.0 + (a * 0.7 + m.phase).sin() * 1.5 + m.phase % 2.0, a.sin() * m.radius);
    }
}

/// Agitated pulses: random-ish shoves to everything dynamic in range.
fn pulse(
    time: Res<Time>,
    mut anomalies: Query<(&Transform, &mut Anomaly)>,
    bodies: Query<(Entity, &RigidBody, &Position)>,
    mut impulses: ResMut<crate::physics::impulses::ImpulseQueue>,
    mut gizmos: Gizmos,
) {
    for (t, mut a) in &mut anomalies {
        a.timer -= time.delta_secs();
        let phase = (a.timer / 3.0).clamp(0.0, 1.0);
        gizmos.sphere(Isometry3d::from_translation(t.translation + Vec3::Y), a.radius * (1.0 - phase), a.kind.color().with_alpha(0.3 * (1.0 - phase)));
        if a.timer > 0.0 {
            continue;
        }
        a.timer = 3.0 + (time.elapsed_secs() * 7.13).sin().abs() * 2.0;
        for (e, rb, p) in &bodies {
            if !rb.is_dynamic() {
                continue;
            }
            let d = p.0 - t.translation;
            let dist = d.length();
            if dist > a.radius || dist < 0.01 {
                continue;
            }
            let swirl = Vec3::new(-d.z, 0.0, d.x).normalize_or_zero();
            let dv = (d.normalize() * 6.0 + swirl * 5.0 + Vec3::Y * 7.0) * (1.0 - dist / a.radius);
            impulses.push(e, dv);
        }
    }
}

pub struct AnomaliesPlugin;

impl Plugin for AnomaliesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_site_effects, animate_motes, pulse));
    }
}
