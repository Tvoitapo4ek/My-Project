//! Creatures. One hostile kind for the prototype: the **Hollowling**
//! ("Пустотник") — a crystal-backed crawler born from agitated aether. It
//! wanders, notices the player within 16 m, chases and bites. Behaviour is a
//! small state machine; bodies are real physics bodies (knockback works).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::damage::{Damage, DamageSource, Died, Health};
use crate::{
    core::{layers::Layer, resources::WorldClock, states::AppState},
    items::{drops::SpawnDrop, ItemId, ItemStack},
    magic::{anomalies::Anomaly, spells::MagicHit},
    physics::collision::{ImpactEvent, ImpactSensor},
    player::Player,
    tools::scanning::Scannable,
    world::chunks::VoxelWorld,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mind {
    Wander { dir: Vec2, timer: f32 },
    Chase,
    Recover(f32),
}

#[derive(Component, Debug)]
pub struct Creature {
    pub mind: Mind,
    pub attack_cooldown: f32,
    pub speed: f32,
}

const SIGHT: f32 = 16.0;
const BITE_RANGE: f32 = 1.7;
const BITE_DAMAGE: f32 = 9.0;
const MAX_CREATURES: usize = 6;

#[derive(Resource)]
struct CreatureAssets {
    body: Handle<Mesh>,
    spike: Handle<Mesh>,
    eye: Handle<Mesh>,
    shell: Handle<StandardMaterial>,
    crystal: Handle<StandardMaterial>,
    glow: Handle<StandardMaterial>,
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(CreatureAssets {
        body: meshes.add(Sphere::new(0.45).mesh().ico(2).unwrap()),
        spike: meshes.add(Cone::new(0.12, 0.45)),
        eye: meshes.add(Sphere::new(0.07).mesh().ico(1).unwrap()),
        shell: materials.add(StandardMaterial { base_color: Color::srgb(0.16, 0.14, 0.18), perceptual_roughness: 0.7, ..default() }),
        crystal: materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.3, 0.7), emissive: LinearRgba::rgb(1.2, 0.2, 0.7), ..default() }),
        glow: materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.9), emissive: LinearRgba::rgb(5.0, 1.0, 3.0), unlit: true, ..default() }),
    });
}

pub fn spawn_hollowling(commands: &mut Commands, at: Vec3) -> Entity {
    commands
        .spawn((
            Name::new("Hollowling"),
            Creature { mind: Mind::Wander { dir: Vec2::X, timer: 0.0 }, attack_cooldown: 0.0, speed: 3.6 },
            Health::new(40.0),
            Scannable { title: "Пустотник".into(), info: "Существо из раздражённой магии. Агрессивно.".into() },
            Transform::from_translation(at),
            Visibility::default(),
            RigidBody::Dynamic,
            Collider::sphere(0.45),
            LockedAxes::ROTATION_LOCKED,
            Mass(35.0),
            Friction::new(0.2),
            (ImpactSensor::default(), SleepingDisabled),
            CollisionLayers::new(Layer::Creature, [Layer::Terrain, Layer::Player, Layer::Prop, Layer::Creature, Layer::Projectile]),
        ))
        .id()
}

fn attach_visuals(mut commands: Commands, assets: Res<CreatureAssets>, added: Query<Entity, Added<Creature>>) {
    for e in &added {
        commands.entity(e).with_children(|p| {
            p.spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(assets.shell.clone()), Transform::default()));
            for (i, a) in [-0.5f32, 0.0, 0.5].iter().enumerate() {
                p.spawn((
                    Mesh3d(assets.spike.clone()),
                    MeshMaterial3d(assets.crystal.clone()),
                    Transform::from_xyz(*a * 0.4, 0.42, 0.1 * i as f32).with_rotation(Quat::from_rotation_z(-a * 0.6)),
                ));
            }
            for x in [-0.15, 0.15] {
                p.spawn((Mesh3d(assets.eye.clone()), MeshMaterial3d(assets.glow.clone()), Transform::from_xyz(x, 0.12, -0.42)));
            }
        });
    }
}

/// Initial population near the anomaly, plus night spawns near the player.
#[allow(clippy::too_many_arguments)]
fn populate(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<WorldClock>,
    world: Res<VoxelWorld>,
    anomalies: Query<&Transform, With<Anomaly>>,
    creatures: Query<(), With<Creature>>,
    player: Query<&Transform, With<Player>>,
    mut done_initial: Local<bool>,
    mut timer: Local<f32>,
) {
    let surface = |x: f32, z: f32| world.surface_y(x as i32, z as i32).map(|y| Vec3::new(x.floor() + 0.5, y as f32 + 1.6, z.floor() + 0.5));
    if !*done_initial
        && let Some(a) = anomalies.iter().next()
    {
        *done_initial = true;
        for i in 0..3 {
            let ang = i as f32 * 2.1;
            if let Some(p) = surface(a.translation.x + ang.cos() * 7.0, a.translation.z + ang.sin() * 7.0) {
                spawn_hollowling(&mut commands, p);
            }
        }
    }
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 30.0;
    if clock.daylight() > 0.2 || creatures.iter().count() >= MAX_CREATURES {
        return;
    }
    if let Ok(p) = player.single() {
        let ang = time.elapsed_secs() * 1.7;
        if let Some(pos) = surface(p.translation.x + ang.cos() * 20.0, p.translation.z + ang.sin() * 20.0) {
            spawn_hollowling(&mut commands, pos);
        }
    }
}

#[allow(clippy::type_complexity)]
fn think(
    time: Res<Time>,
    player: Query<(Entity, &Position), With<Player>>,
    mut q: Query<(Entity, &Position, &mut LinearVelocity, &mut Creature, &mut Transform), Without<Player>>,
    mut damage: MessageWriter<Damage>,
) {
    let dt = time.delta_secs();
    let Ok((pe, pp)) = player.single() else { return };
    for (e, pos, mut vel, mut c, mut t) in &mut q {
        c.attack_cooldown = (c.attack_cooldown - dt).max(0.0);
        let to_player = pp.0 - pos.0;
        let dist = to_player.length();
        c.mind = match c.mind {
            Mind::Recover(r) if r > 0.0 => Mind::Recover(r - dt),
            Mind::Recover(_) => Mind::Wander { dir: Vec2::X, timer: 0.0 },
            _ if dist < SIGHT => Mind::Chase,
            Mind::Chase => Mind::Wander { dir: Vec2::X, timer: 0.0 },
            Mind::Wander { dir, timer } if timer > 0.0 => Mind::Wander { dir, timer: timer - dt },
            Mind::Wander { .. } => {
                let a = (time.elapsed_secs() * 3.7 + e.index_u32() as f32).sin() * std::f32::consts::PI;
                Mind::Wander { dir: Vec2::new(a.cos(), a.sin()), timer: 3.0 }
            }
        };
        let wish = match c.mind {
            Mind::Chase => Vec2::new(to_player.x, to_player.z).normalize_or_zero() * c.speed,
            Mind::Wander { dir, .. } => dir * c.speed * 0.35,
            Mind::Recover(_) => Vec2::ZERO,
        };
        if !matches!(c.mind, Mind::Recover(_)) {
            let h = Vec2::new(vel.x, vel.z).lerp(wish, (dt * 6.0).min(1.0));
            vel.x = h.x;
            vel.z = h.y;
        }
        if wish.length_squared() > 0.01 {
            t.rotation = Quat::from_rotation_y((-wish.x).atan2(-wish.y));
        }
        if matches!(c.mind, Mind::Chase) && dist < BITE_RANGE && c.attack_cooldown <= 0.0 {
            c.attack_cooldown = 1.2;
            damage.write(Damage { target: pe, amount: BITE_DAMAGE, source: DamageSource::Creature });
        }
    }
}

/// Magic and hard impacts hurt creatures; being hit stuns briefly.
fn creature_hits(
    mut magic: MessageReader<MagicHit>,
    mut impacts: MessageReader<ImpactEvent>,
    mut q: Query<&mut Creature>,
    mut damage: MessageWriter<Damage>,
) {
    for h in magic.read() {
        if let Ok(mut c) = q.get_mut(h.target) {
            c.mind = Mind::Recover(0.6);
            damage.write(Damage { target: h.target, amount: h.damage, source: DamageSource::Magic });
        }
    }
    for i in impacts.read() {
        if let Ok(mut c) = q.get_mut(i.entity) {
            c.mind = Mind::Recover(0.4);
            damage.write(Damage { target: i.entity, amount: (i.delta_v - 8.0) * 2.0, source: DamageSource::Weapon });
        }
    }
}

fn creature_death(
    mut commands: Commands,
    mut died: MessageReader<Died>,
    q: Query<&Position, With<Creature>>,
    mut drops: MessageWriter<SpawnDrop>,
    mut notify: MessageWriter<crate::core::events::Notify>,
) {
    for d in died.read() {
        if let Ok(p) = q.get(d.entity) {
            drops.write(SpawnDrop { stack: ItemStack::new(ItemId::k("agitated_shard"), 2), position: p.0 + Vec3::Y * 0.5, velocity: Vec3::Y * 3.0 });
            commands.entity(d.entity).despawn();
            notify.write(crate::core::events::Notify::ok("Пустотник повержен"));
        }
    }
}

pub struct CreaturesPlugin;

impl Plugin for CreaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (attach_visuals, populate, creature_hits, creature_death).run_if(in_state(AppState::InGame)))
            .add_systems(FixedUpdate, think.run_if(crate::core::states::in_game));
    }
}
