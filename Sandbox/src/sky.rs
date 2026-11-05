//! Time of day and the sky, in the spirit of Minecraft / Hytale: a gradient
//! dome, a square sun and moon, slow blocky clouds, a haze towards the
//! horizon in the sky's colour (blue fog under water), soft light: the sun
//! warm, the shade cool and not too dark. The clock can be stopped and set
//! from the debug menu/console.

use bevy::{
    asset::RenderAssetUsages,
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster},
    mesh::{Indices, PrimitiveTopology},
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};

use crate::player::camera::MainCamera;

/// The square sun and moon.
#[derive(Component)]
struct Disc(bool);

/// The cloud layer (follows the camera, drifts with time).
#[derive(Component)]
struct Clouds;

#[derive(Resource)]
struct CloudMaterial(Handle<StandardMaterial>);

const CLOUD_CELL: f32 = 12.0;
const CLOUD_N: i32 = 48;
const CLOUD_Y: f32 = 110.0;

/// Is there a cloud in cell (x, z)? Blobby value noise, wrapping.
pub fn cloud_at(x: i32, z: i32) -> bool {
    let h = |x: i32, z: i32| {
        let (x, z) = (x.rem_euclid(CLOUD_N) as u32, z.rem_euclid(CLOUD_N) as u32);
        let mut v = x.wrapping_mul(374_761_393) ^ z.wrapping_mul(668_265_263);
        v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
        ((v ^ (v >> 16)) & 0xffff) as f32 / 65535.0
    };
    // Smooth over the neighbours: clouds come in patches.
    let mut s = 0.0;
    for dx in -1..=1 {
        for dz in -1..=1 {
            s += h(x + dx, z + dz) * if dx == 0 && dz == 0 { 2.0 } else { 1.0 };
        }
    }
    s / 10.0 > 0.56
}

/// Flat blocky clouds: top, bottom and the exposed sides of each cell.
fn cloud_mesh() -> Mesh {
    let (mut pos, mut nor, mut idx) = (Vec::new(), Vec::new(), Vec::<u32>::new());
    let (c, t) = (CLOUD_CELL, 4.0);
    let mut quad = |a: Vec3, b: Vec3, cc: Vec3, d: Vec3, n: Vec3| {
        let s = pos.len() as u32;
        for p in [a, b, cc, d] {
            pos.push(p.to_array());
            nor.push(n.to_array());
        }
        idx.extend_from_slice(&[s, s + 1, s + 2, s, s + 2, s + 3]);
    };
    let half = CLOUD_N / 2;
    for x in -half..half {
        for z in -half..half {
            if !cloud_at(x, z) {
                continue;
            }
            let (x0, z0) = (x as f32 * c, z as f32 * c);
            let (x1, z1) = (x0 + c, z0 + c);
            quad(Vec3::new(x0, t, z0), Vec3::new(x0, t, z1), Vec3::new(x1, t, z1), Vec3::new(x1, t, z0), Vec3::Y);
            quad(Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1), Vec3::new(x0, 0.0, z1), Vec3::NEG_Y);
            if !cloud_at(x + 1, z) {
                quad(Vec3::new(x1, 0.0, z0), Vec3::new(x1, t, z0), Vec3::new(x1, t, z1), Vec3::new(x1, 0.0, z1), Vec3::X);
            }
            if !cloud_at(x - 1, z) {
                quad(Vec3::new(x0, 0.0, z0), Vec3::new(x0, 0.0, z1), Vec3::new(x0, t, z1), Vec3::new(x0, t, z0), Vec3::NEG_X);
            }
            if !cloud_at(x, z + 1) {
                quad(Vec3::new(x0, 0.0, z1), Vec3::new(x1, 0.0, z1), Vec3::new(x1, t, z1), Vec3::new(x0, t, z1), Vec3::Z);
            }
            if !cloud_at(x, z - 1) {
                quad(Vec3::new(x0, 0.0, z0), Vec3::new(x0, t, z0), Vec3::new(x1, t, z0), Vec3::new(x1, 0.0, z0), Vec3::NEG_Z);
            }
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_indices(Indices::U32(idx));
    m
}

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
        // Two cascades, a moderate map: much lighter than the default four.
        CascadeShadowConfigBuilder { num_cascades: 2, maximum_distance: 64.0, first_cascade_far_bound: 16.0, ..default() }.build(),
        Transform::default(),
    ));
    commands.insert_resource(DirectionalLightShadowMap { size: 1536 });
    // Square sun and moon (HDR glow) far away on the dome.
    for (moon, c) in [(false, LinearRgba::rgb(9.0, 7.6, 4.6)), (true, LinearRgba::rgb(1.6, 1.7, 2.0))] {
        let mat = mats.add(StandardMaterial { base_color: Color::LinearRgba(c), unlit: true, fog_enabled: false, ..default() });
        let size = if moon { 22.0 } else { 30.0 };
        commands.spawn((Name::new(if moon { "Moon" } else { "Sun disc" }), Disc(moon), Mesh3d(meshes.add(Rectangle::new(size, size))), MeshMaterial3d(mat), Transform::default(), NotShadowCaster));
    }
    let cm = mats.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.82), alpha_mode: AlphaMode::Blend, unlit: true, fog_enabled: false, cull_mode: None, ..default() });
    commands.spawn((Name::new("Clouds"), Clouds, Mesh3d(meshes.add(cloud_mesh())), MeshMaterial3d(cm.clone()), Transform::from_xyz(0.0, CLOUD_Y, 0.0), NotShadowCaster));
    commands.insert_resource(CloudMaterial(cm));
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

/// Haze towards the horizon in the sky's colour; blue fog under water.
fn fog(mut commands: Commands, water: Res<crate::water::Water>, clock: Res<Clock>, settings: Res<crate::menu::GameSettings>, mut cams: Query<(Entity, &GlobalTransform, Option<&mut DistanceFog>), With<MainCamera>>) {
    let day = clock.daylight();
    let far = crate::menu::VIEW[settings.view.min(crate::menu::VIEW.len() - 1)];
    for (e, t, f) in &mut cams {
        let under = water.depth(t.translation()) > 0.0;
        let (color, start, end) = if under {
            (Color::srgb(0.08, 0.27, 0.42), 0.5, 22.0)
        } else {
            let h = lerp3(Vec3::new(0.06, 0.08, 0.16), Vec3::new(0.72, 0.86, 1.0), day);
            (Color::srgb(h.x, h.y, h.z), far * 0.3, far * 0.95)
        };
        let want = DistanceFog { color, falloff: FogFalloff::Linear { start, end }, ..default() };
        match f {
            Some(mut f) => {
                f.color = want.color;
                f.falloff = want.falloff;
            }
            None => {
                commands.entity(e).insert(want);
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_sky(
    clock: Res<Clock>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    cams: Query<&GlobalTransform, (With<Camera3d>, Without<SkyDome>, Without<Sun>)>,
    mut dome: Query<&mut Transform, (With<SkyDome>, Without<Sun>)>,
    sky_mat: Option<Res<SkyMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut discs: Query<(&Disc, &mut Transform), (Without<SkyDome>, Without<Sun>, Without<Clouds>)>,
    mut clouds: Query<&mut Transform, (With<Clouds>, Without<SkyDome>, Without<Sun>)>,
    cloud_mat: Option<Res<CloudMaterial>>,
    time: Res<Time>,
) {
    let a = clock.sun_angle();
    let day = clock.daylight();
    let dir = Vec3::new(a.cos(), a.sin(), 0.3).normalize();
    let warm = (1.0 - dir.y.max(0.0) * 2.5).clamp(0.0, 1.0);
    for (mut t, mut l) in &mut sun {
        let light_dir = if dir.y > -0.05 { dir } else { Vec3::new(-dir.x, -dir.y, dir.z) };
        *t = Transform::from_translation(light_dir * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        l.illuminance = 300.0 + day * 8000.0;
        let c = if dir.y > -0.05 { lerp3(Vec3::new(1.0, 0.94, 0.84), Vec3::new(1.0, 0.64, 0.38), warm) } else { Vec3::new(0.55, 0.65, 0.95) };
        l.color = Color::srgb(c.x, c.y, c.z);
    }
    // Shadows lean cool blue-violet, sunlight warm.
    let amb = lerp3(Vec3::new(0.3, 0.32, 0.62), Vec3::new(0.82, 0.84, 0.97), day);
    ambient.color = Color::srgb(amb.x, amb.y, amb.z);
    ambient.brightness = 220.0 + day * 1300.0;
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
    if let Some(cam) = cams.iter().next() {
        let c = cam.translation();
        for (disc, mut t) in &mut discs {
            let d = if disc.0 { -dir } else { dir };
            let p = c + d * 350.0;
            *t = Transform::from_translation(p).looking_at(c, Vec3::Y);
        }
        // Clouds drift east and wrap around the camera.
        let period = CLOUD_CELL * CLOUD_N as f32;
        let drift = time.elapsed_secs() * 1.5;
        for mut t in &mut clouds {
            let ox = ((c.x - drift) / period).round() * period + drift;
            let oz = (c.z / period).round() * period;
            t.translation = Vec3::new(ox, CLOUD_Y, oz);
        }
    }
    if let Some(m) = cloud_mat
        && let Some(mut mat) = mats.get_mut(&m.0)
    {
        let k = 0.25 + day * 0.75;
        mat.base_color = Color::srgba(k, k, k * 1.02, 0.82);
    }
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clock>()
            .insert_resource(ClearColor(Color::srgb(0.25, 0.52, 0.92)))
            .add_systems(Startup, setup)
            .add_systems(Update, (advance, update_sky, fog).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clouds_come_in_patches() {
        let n = (0..CLOUD_N).flat_map(|x| (0..CLOUD_N).map(move |z| (x, z))).filter(|(x, z)| cloud_at(*x, *z)).count();
        let share = n as f32 / (CLOUD_N * CLOUD_N) as f32;
        assert!(share > 0.1 && share < 0.6, "{share}");
        assert_eq!(cloud_at(3, 5), cloud_at(3 + CLOUD_N, 5 - CLOUD_N), "wraps");
    }

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
