//! Sky in the game's style (docs/STYLE_GUIDE.md): a horizon-to-zenith
//! gradient dome that follows the camera and chunky voxel clouds drifting
//! with the wind. Colours follow the day cycle (`WorldClock`).

use bevy::{asset::RenderAssetUsages, light::NotShadowCaster, mesh::{Indices, PrimitiveTopology}, prelude::*};

use crate::core::{config::GameConfig, resources::{MainCamera, WorldClock}};

#[derive(Component)]
struct SkyDome;

#[derive(Component)]
struct Cloud {
    /// Position in the cloud field (wraps around).
    home: Vec2,
}

#[derive(Resource)]
struct SkyMaterials {
    dome: Handle<StandardMaterial>,
    cloud: Handle<StandardMaterial>,
}

/// Cloud field size (m) — it wraps, so clouds never run out.
const FIELD: f32 = 480.0;
const CELL: f32 = 12.0;
const WIND: f32 = 1.2;

/// Inside-out hemisphere whose vertex alpha fades from the horizon (opaque)
/// to the zenith (transparent): over the clear colour (zenith) it gives a
/// gradient that is recoloured by changing one material colour.
fn dome_mesh(radius: f32) -> Mesh {
    let (rings, segs) = (12, 32);
    let mut pos = Vec::new();
    let mut col = Vec::new();
    let mut nor = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    for r in 0..=rings {
        // From slightly below the horizon up to the zenith.
        let el = -0.15 + (r as f32 / rings as f32) * (std::f32::consts::FRAC_PI_2 + 0.15);
        for sgm in 0..=segs {
            let az = sgm as f32 / segs as f32 * std::f32::consts::TAU;
            let p = Vec3::new(el.cos() * az.cos(), el.sin(), el.cos() * az.sin()) * radius;
            pos.push(p.to_array());
            nor.push((-p.normalize()).to_array());
            let h = (el.max(0.0) / std::f32::consts::FRAC_PI_2).powf(0.55);
            col.push([1.0, 1.0, 1.0, (1.0 - h).clamp(0.0, 1.0)]);
        }
    }
    let w = segs + 1;
    for r in 0..rings {
        for sgm in 0..segs {
            let a = (r * w + sgm) as u32;
            let b = a + w as u32;
            // Seen from inside.
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

/// Hash noise for the cloud layout (deterministic, no RNG state).
fn hash(x: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

fn cloudy(x: i32, z: i32) -> bool {
    // Blobby: a cell is cloud if it and enough neighbours score high.
    let n = |dx: i32, dz: i32| hash((x + dx).div_euclid(3), (z + dz).div_euclid(3));
    let v = n(0, 0) * 0.6 + hash(x, z) * 0.4;
    v > 0.62
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, cfg: Res<GameConfig>) {
    let dome = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        fog_enabled: false,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    let cloud = mats.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.88),
        unlit: true,
        fog_enabled: false,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Name::new("Sky dome"),
        SkyDome,
        Mesh3d(meshes.add(dome_mesh(560.0))),
        MeshMaterial3d(dome.clone()),
        Transform::default(),
        NotShadowCaster,
    ));
    let height = (cfg.world.chunk_size * cfg.world.height_chunks) as f32 + 24.0;
    let n = (FIELD / CELL) as i32;
    for x in 0..n {
        for z in 0..n {
            if !cloudy(x, z) {
                continue;
            }
            let thick = 3.0 + hash(x * 7, z * 3) * 3.0;
            commands.spawn((
                Cloud { home: Vec2::new(x as f32 * CELL, z as f32 * CELL) },
                Mesh3d(meshes.add(Cuboid::new(CELL, thick, CELL))),
                MeshMaterial3d(cloud.clone()),
                Transform::from_xyz(0.0, height + hash(x, z * 5) * 4.0, 0.0),
                NotShadowCaster,
            ));
        }
    }
    commands.insert_resource(SkyMaterials { dome, cloud });
}

fn update_sky(
    time: Res<Time>,
    clock: Res<WorldClock>,
    mats: Option<Res<SkyMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam: Query<&Transform, (With<MainCamera>, Without<SkyDome>, Without<Cloud>)>,
    mut dome: Query<&mut Transform, (With<SkyDome>, Without<Cloud>)>,
    mut clouds: Query<(&Cloud, &mut Transform), Without<SkyDome>>,
    fog: Query<&DistanceFog, With<MainCamera>>,
    mut last: Local<f32>,
) {
    let (Some(m), Ok(cam)) = (mats, cam.single()) else { return };
    let c = cam.translation;
    if let Ok(mut t) = dome.single_mut() {
        t.translation = c;
    }
    // Clouds drift and wrap around the camera.
    let drift = time.elapsed_secs() * WIND;
    for (cl, mut t) in &mut clouds {
        let x = (cl.home.x + drift - c.x + FIELD / 2.0).rem_euclid(FIELD) - FIELD / 2.0 + c.x;
        let z = (cl.home.y - c.z + FIELD / 2.0).rem_euclid(FIELD) - FIELD / 2.0 + c.z;
        t.translation.x = x;
        t.translation.z = z;
    }
    // Colours: only when the time of day moved noticeably.
    if (clock.time_of_day - *last).abs() < 0.002 {
        return;
    }
    *last = clock.time_of_day;
    let day = clock.daylight();
    let warm = (1.0 - clock.sun_angle().sin().max(0.0) * 2.5).clamp(0.0, 1.0);
    if let (Some(mut mat), Ok(f)) = (materials.get_mut(&m.dome), fog.single()) {
        mat.base_color = f.color.with_alpha(1.0);
    }
    let cloud = Vec3::splat(0.35).lerp(Vec3::ONE, day).lerp(Vec3::new(1.0, 0.82, 0.72), warm * day * 0.6);
    if let Some(mut mat) = materials.get_mut(&m.cloud) {
        mat.base_color = Color::srgba(cloud.x, cloud.y, cloud.z, 0.88);
    }
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(PostUpdate, update_sky.after(crate::player::camera::follow_player).before(bevy::transform::TransformSystems::Propagate));
    }
}
