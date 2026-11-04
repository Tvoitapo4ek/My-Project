//! Time of day and the sky: a blue gradient dome around the camera, the
//! sun (and a dim moon) as the directional light, ambient light following
//! the day. The clock can be stopped and set from the debug menu/console.

use bevy::{
    asset::RenderAssetUsages,
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// Hours 0..24; `speed` = game hours per real second.
#[derive(Resource, Debug, Clone)]
pub struct Clock {
    pub hours: f32,
    pub paused: bool,
    pub speed: f32,
}

impl Default for Clock {
    fn default() -> Self {
        // A full day in 20 real minutes.
        Self { hours: 10.0, paused: false, speed: 24.0 / 1200.0 }
    }
}

impl Clock {
    /// Sun elevation angle (radians): 0 at 6:00 and 18:00, max at noon.
    pub fn sun_angle(&self) -> f32 {
        (self.hours - 6.0) / 24.0 * std::f32::consts::TAU
    }
    /// 0 at night … 1 by day (smooth around sunrise / sunset).
    pub fn daylight(&self) -> f32 {
        (self.sun_angle().sin() * 3.0 + 0.3).clamp(0.0, 1.0)
    }
    pub fn label(&self) -> String {
        let h = self.hours.rem_euclid(24.0);
        format!("{:02}:{:02}", h as u32, ((h.fract()) * 60.0) as u32)
    }
}

#[derive(Component)]
pub struct Sun;

#[derive(Component)]
struct SkyDome;

#[derive(Resource)]
struct SkyMaterial(Handle<StandardMaterial>);

/// Inside-out hemisphere: opaque at the horizon, transparent at the
/// zenith, over the clear colour → a gradient recoloured by one material.
fn dome_mesh(radius: f32) -> Mesh {
    let (rings, segs) = (12, 32);
    let (mut pos, mut col, mut nor, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::<u32>::new());
    for r in 0..=rings {
        let el = -0.3 + (r as f32 / rings as f32) * (std::f32::consts::FRAC_PI_2 + 0.3);
        for s in 0..=segs {
            let az = s as f32 / segs as f32 * std::f32::consts::TAU;
            let p = Vec3::new(el.cos() * az.cos(), el.sin(), el.cos() * az.sin()) * radius;
            pos.push(p.to_array());
            nor.push((-p.normalize()).to_array());
            let h = (el.max(0.0) / std::f32::consts::FRAC_PI_2).powf(0.5);
            col.push([1.0, 1.0, 1.0, (1.0 - h).clamp(0.0, 1.0)]);
        }
    }
    let w = segs + 1;
    for r in 0..rings {
        for s in 0..segs {
            let a = (r * w + s) as u32;
            let b = a + w as u32;
            idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    m.insert_indices(Indices::U32(idx));
    m
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: true, ..default() },
        CascadeShadowConfigBuilder { maximum_distance: 80.0, first_cascade_far_bound: 12.0, ..default() }.build(),
        Transform::default(),
    ));
    let m = mats.add(StandardMaterial { base_color: Color::WHITE, unlit: true, fog_enabled: false, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    commands.spawn((Name::new("Sky"), SkyDome, Mesh3d(meshes.add(dome_mesh(400.0))), MeshMaterial3d(m.clone()), Transform::default(), NotShadowCaster));
    commands.insert_resource(SkyMaterial(m));
}

fn advance(time: Res<Time>, mut clock: ResMut<Clock>) {
    if !clock.paused {
        clock.hours = (clock.hours + time.delta_secs() * clock.speed).rem_euclid(24.0);
    }
}

fn lerp3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a.lerp(b, t.clamp(0.0, 1.0))
}

#[allow(clippy::too_many_arguments)]
fn update_sky(
    clock: Res<Clock>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    cams: Query<&GlobalTransform, (With<Camera3d>, Without<SkyDome>, Without<Sun>)>,
    mut dome: Query<&mut Transform, (With<SkyDome>, Without<Sun>)>,
    sky_mat: Option<Res<SkyMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let a = clock.sun_angle();
    let day = clock.daylight();
    let dir = Vec3::new(a.cos(), a.sin(), 0.3).normalize();
    let warm = (1.0 - dir.y.max(0.0) * 2.5).clamp(0.0, 1.0);
    for (mut t, mut l) in &mut sun {
        let light_dir = if dir.y > -0.05 { dir } else { Vec3::new(-dir.x, -dir.y, dir.z) };
        *t = Transform::from_translation(light_dir * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        l.illuminance = 300.0 + day * 9500.0;
        let c = if dir.y > -0.05 { lerp3(Vec3::new(1.0, 0.94, 0.84), Vec3::new(1.0, 0.64, 0.38), warm) } else { Vec3::new(0.55, 0.65, 0.95) };
        l.color = Color::srgb(c.x, c.y, c.z);
    }
    // Shadows lean cool blue-violet, sunlight warm.
    let amb = lerp3(Vec3::new(0.3, 0.32, 0.62), Vec3::new(0.82, 0.84, 0.97), day);
    ambient.color = Color::srgb(amb.x, amb.y, amb.z);
    ambient.brightness = 120.0 + day * 700.0;
    // Zenith (clear colour) and horizon (dome).
    let zenith = lerp3(Vec3::new(0.02, 0.03, 0.09), Vec3::new(0.25, 0.52, 0.92), day);
    let horizon = lerp3(Vec3::new(0.06, 0.08, 0.16), Vec3::new(0.68, 0.84, 1.0), day).lerp(Vec3::new(1.0, 0.62, 0.45), warm * day * 0.6);
    clear.0 = Color::srgb(zenith.x, zenith.y, zenith.z);
    if let Some(m) = sky_mat
        && let Some(mut mat) = mats.get_mut(&m.0)
    {
        mat.base_color = Color::srgb(horizon.x, horizon.y, horizon.z);
    }
    if let (Some(cam), Ok(mut d)) = (cams.iter().next(), dome.single_mut()) {
        d.translation = cam.translation();
    }
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clock>()
            .insert_resource(ClearColor(Color::srgb(0.25, 0.52, 0.92)))
            .add_systems(Startup, setup)
            .add_systems(Update, (advance, update_sky).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_and_night() {
        let mut c = Clock { hours: 12.0, ..default() };
        assert!(c.daylight() > 0.99 && c.label() == "12:00");
        c.hours = 0.0;
        assert_eq!(c.daylight(), 0.0);
        c.hours = 6.5;
        assert!(c.daylight() > 0.3 && c.daylight() < 1.0);
    }
}
