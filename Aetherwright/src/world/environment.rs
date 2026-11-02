//! Day/night cycle: sun direction & colour, ambient light, sky clear colour
//! and distance fog are all driven by `WorldClock`.

use bevy::{light::CascadeShadowConfigBuilder, prelude::*};

use crate::core::{
    config::GameConfig,
    resources::{MainCamera, WorldClock},
};

#[derive(Component)]
pub struct Sun;

fn spawn_sun(mut commands: Commands, cfg: Res<GameConfig>) {
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight {
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadow_maps_enabled: cfg.video.shadows,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            first_cascade_far_bound: 16.0,
            maximum_distance: 140.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn lerp_color(a: Vec3, b: Vec3, t: f32) -> Color {
    let c = a.lerp(b, t.clamp(0.0, 1.0));
    Color::srgb(c.x, c.y, c.z)
}

fn update_environment(
    clock: Res<WorldClock>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut fog: Query<&mut DistanceFog, With<MainCamera>>,
) {
    let angle = clock.sun_angle();
    let day = clock.daylight();
    // Sun travels east → west, tilted towards south.
    let dir = Vec3::new(angle.cos(), angle.sin(), 0.35).normalize();
    // Warm low sun, white high sun.
    let warm = (1.0 - dir.y.max(0.0) * 2.5).clamp(0.0, 1.0);
    for (mut t, mut light) in &mut sun {
        // Below the horizon we keep a dim "moon" from the opposite side.
        let light_dir = if dir.y > -0.05 {
            dir
        } else {
            Vec3::new(-dir.x, -dir.y, dir.z)
        };
        *t = Transform::from_translation(light_dir * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        light.illuminance = 400.0 + day * 11000.0;
        light.color = if dir.y > -0.05 {
            lerp_color(Vec3::new(1.0, 0.97, 0.9), Vec3::new(1.0, 0.62, 0.38), warm)
        } else {
            Color::srgb(0.55, 0.65, 0.95)
        };
    }
    ambient.color = lerp_color(Vec3::new(0.30, 0.36, 0.62), Vec3::new(0.78, 0.85, 1.0), day);
    ambient.brightness = 120.0 + day * 700.0;

    let night_sky = Vec3::new(0.03, 0.04, 0.09);
    let day_sky = Vec3::new(0.55, 0.70, 0.86);
    let dusk = Vec3::new(0.85, 0.55, 0.42);
    let sky = night_sky.lerp(day_sky, day).lerp(dusk, warm * day * 0.6);
    clear.0 = Color::srgb(sky.x, sky.y, sky.z);
    for mut f in &mut fog {
        f.color = Color::srgb(
            sky.x * 0.95 + 0.03,
            sky.y * 0.95 + 0.03,
            sky.z * 0.95 + 0.04,
        );
    }
}

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.55, 0.7, 0.86)))
            .add_systems(Startup, spawn_sun)
            .add_systems(Update, update_environment);
    }
}
