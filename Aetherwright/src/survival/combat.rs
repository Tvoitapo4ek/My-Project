//! Weapons. Melee: the resonance blade hits everything in a short cone.
//! Ranged: the arc caster fires physical bolts (rigid bodies with CCD);
//! hits are found by sweeping a ray along each bolt's path every tick, so
//! fast projectiles never tunnel through creatures.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    creatures::{Creature, Mind},
    damage::{Damage, DamageSource},
};
use crate::{
    core::{events::Notify, layers::Layer, resources::EyeRay, rules::ModeRules, states::gameplay_active},
    items::{inventory::Inventory, ItemId},
    physics::impulses::ImpulseQueue,
    player::Player,
    tools::ActiveTool,
};

#[derive(Resource, Default)]
pub struct WeaponCooldown(pub f32);

#[derive(Component, Debug)]
pub struct Bolt {
    pub last: Vec3,
    pub ttl: f32,
    pub spent: bool,
}

#[derive(Resource)]
struct BoltAssets {
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
}

pub const MELEE_DAMAGE: f32 = 22.0;
pub const BOLT_DAMAGE: f32 = 28.0;
const BOLT_SPEED: f32 = 45.0;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(BoltAssets {
        mesh: meshes.add(Capsule3d::new(0.04, 0.35)),
        mat: materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.8, 0.5), emissive: LinearRgba::rgb(2.0, 1.4, 0.4), metallic: 0.8, ..default() }),
    });
}

#[allow(clippy::too_many_arguments)]
fn attack(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    eye: Res<EyeRay>,
    assets: Res<BoltAssets>,
    mut cd: ResMut<WeaponCooldown>,
    mut player: Query<(Entity, &mut Inventory), With<Player>>,
    mut creatures: Query<(Entity, &Position, &mut Creature)>,
    mut impulses: ResMut<ImpulseQueue>,
    mut damage: MessageWriter<Damage>,
    mut notify: MessageWriter<Notify>,
    mut gizmos: Gizmos,
) {
    cd.0 = (cd.0 - time.delta_secs()).max(0.0);
    if !mouse.just_pressed(MouseButton::Left) || cd.0 > 0.0 {
        return;
    }
    let Ok((pe, mut inv)) = player.single_mut() else { return };
    match *tool {
        ActiveTool::Melee => {
            cd.0 = 0.45;
            let mut hit = 0;
            for (e, p, mut c) in &mut creatures {
                let to = p.0 - eye.origin;
                if to.length() < 2.8 && to.normalize_or_zero().dot(eye.direction) > 0.6 {
                    damage.write(Damage { target: e, amount: MELEE_DAMAGE, source: DamageSource::Weapon });
                    impulses.push(e, (eye.direction + Vec3::Y * 0.4) * 7.0);
                    c.mind = Mind::Recover(0.5);
                    hit += 1;
                }
            }
            // Swing arc.
            let right = eye.direction.cross(Vec3::Y).normalize_or_zero();
            for i in 0..6 {
                let a = -0.6 + i as f32 * 0.24;
                let d = (eye.direction + right * a).normalize();
                gizmos.line(eye.origin + d * 0.8, eye.origin + d * 2.4, Color::srgb(0.9, 0.95, 1.0));
            }
            let _ = (hit, pe);
        }
        ActiveTool::Ranged => {
            let bolt = ItemId::k("bolt");
            if rules.consume_items && !inv.remove(bolt, 1) {
                notify.write(Notify::warn("Нет болтов"));
                return;
            }
            cd.0 = 0.6;
            let start = eye.origin + eye.direction * 0.8;
            commands.spawn((
                Name::new("Bolt"),
                Bolt { last: start, ttl: 6.0, spent: false },
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.mat.clone()),
                Transform::from_translation(start).with_rotation(Quat::from_rotation_arc(Vec3::Y, eye.direction)),
                RigidBody::Dynamic,
                Collider::capsule(0.04, 0.35),
                Mass(0.2),
                LinearVelocity(eye.direction * BOLT_SPEED),
                SweptCcd::default(),
                CollisionLayers::new(Layer::Projectile, [Layer::Terrain, Layer::Prop, Layer::Creature]),
            ));
        }
        _ => {}
    }
}

/// Ray-sweeps each live bolt from its previous to its current position.
fn bolt_hits(
    mut commands: Commands,
    time: Res<Time>,
    spatial: SpatialQuery,
    mut bolts: Query<(Entity, &Position, &mut Bolt)>,
    creatures: Query<(), With<Creature>>,
    mut impulses: ResMut<ImpulseQueue>,
    mut damage: MessageWriter<Damage>,
) {
    for (e, p, mut b) in &mut bolts {
        b.ttl -= time.delta_secs();
        if b.ttl <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        if !b.spent {
            let seg = p.0 - b.last;
            if let Ok(dir) = Dir3::new(seg)
                && let Some(hit) = spatial.cast_ray(b.last, dir, seg.length() + 0.3, true, &SpatialQueryFilter::from_mask([Layer::Creature]).with_excluded_entities([e]))
                && creatures.get(hit.entity).is_ok()
            {
                damage.write(Damage { target: hit.entity, amount: BOLT_DAMAGE, source: DamageSource::Weapon });
                impulses.push(hit.entity, dir.as_vec3() * 4.0);
                b.spent = true;
                b.ttl = b.ttl.min(1.0);
            }
        }
        b.last = p.0;
    }
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponCooldown>()
            .add_systems(Startup, setup)
            .add_systems(Update, attack.after(crate::tools::update_entity_target).run_if(gameplay_active))
            .add_systems(FixedUpdate, bolt_hits.run_if(crate::core::states::in_game));
    }
}
