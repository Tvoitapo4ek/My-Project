//! The player's aether reserve and spells cast with the aether focus.
//!
//! Q cycles spells, LMB casts:
//! * **Импульс** — a cone that pushes bodies, creatures and dropped items
//!   (velocity change scaled by mass, so heavy things barely move);
//! * **Левитация** — the targeted body floats for a few seconds;
//! * **Разряд** — a beam that transfers aether into a target: charges power
//!   cells and aether jars, hurts creatures.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{energy::AetherKind, sources::{AetherSources, AetherStore}};
use crate::{
    core::{events::Notify, layers::Layer, resources::EyeRay, rules::ModeRules, states::gameplay_active},
    machinery::components::PowerCell,
    player::Player,
    tools::{ActiveTool, EntityTarget},
};

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct AetherReserve {
    pub amount: f32,
    pub max: f32,
}

impl Default for AetherReserve {
    fn default() -> Self {
        Self { amount: 60.0, max: 100.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spell {
    #[default]
    Impulse,
    Levitate,
    Discharge,
}

impl Spell {
    pub const ALL: [Spell; 3] = [Spell::Impulse, Spell::Levitate, Spell::Discharge];
    pub fn label(self) -> &'static str {
        match self {
            Spell::Impulse => "Импульс",
            Spell::Levitate => "Левитация",
            Spell::Discharge => "Разряд",
        }
    }
    pub fn cost(self) -> f32 {
        match self {
            Spell::Impulse => 15.0,
            Spell::Levitate => 20.0,
            Spell::Discharge => 25.0,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct SpellState {
    pub selected: Spell,
    pub cooldown: f32,
}

/// Floating object (levitation spell).
#[derive(Component, Debug)]
pub struct Levitating {
    pub ttl: f32,
}

/// Damage dealt by magic (consumed by survival/creatures).
#[derive(Message, Debug, Clone, Copy)]
pub struct MagicHit {
    pub target: Entity,
    pub damage: f32,
    pub kind: AetherKind,
}

/// Short-lived visual effect.
#[derive(Component)]
pub struct Vfx {
    pub ttl: f32,
    pub velocity: Vec3,
    pub grow: f32,
}

#[derive(Resource)]
struct VfxAssets {
    mesh: Handle<Mesh>,
    calm: Handle<StandardMaterial>,
    agitated: Handle<StandardMaterial>,
    ancient: Handle<StandardMaterial>,
}

impl VfxAssets {
    fn mat(&self, k: AetherKind) -> Handle<StandardMaterial> {
        match k {
            AetherKind::Calm => self.calm.clone(),
            AetherKind::Agitated => self.agitated.clone(),
            AetherKind::Ancient => self.ancient.clone(),
        }
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut glow = |k: AetherKind| {
        let c = k.color();
        materials.add(StandardMaterial { base_color: c.with_alpha(0.7), emissive: c.to_linear() * 6.0, alpha_mode: AlphaMode::Add, unlit: true, ..default() })
    };
    commands.insert_resource(VfxAssets {
        mesh: meshes.add(Sphere::new(0.12).mesh().ico(1).unwrap()),
        calm: glow(AetherKind::Calm),
        agitated: glow(AetherKind::Agitated),
        ancient: glow(AetherKind::Ancient),
    });
}

pub fn burst(commands: &mut Commands, assets_mesh: Handle<Mesh>, mat: Handle<StandardMaterial>, at: Vec3, dir: Vec3, n: usize, spread: f32) {
    for i in 0..n {
        let a = i as f32 * 2.399;
        let side = Vec3::new(a.cos(), (i as f32 * 0.7).sin(), a.sin()) * spread;
        commands.spawn((
            Mesh3d(assets_mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at),
            Vfx { ttl: 0.6, velocity: dir * 9.0 + side * 4.0, grow: 1.5 },
            bevy::light::NotShadowCaster,
        ));
    }
}

/// Regenerates the reserve from the local field.
fn regenerate(time: Res<Time>, sources: Res<AetherSources>, mut q: Query<(&Transform, &mut AetherReserve), With<Player>>) {
    for (t, mut r) in &mut q {
        let f = sources.sample(t.translation);
        r.amount = (r.amount + (0.8 + f.strength * 2.0) * time.delta_secs()).min(r.max);
    }
}

#[allow(clippy::too_many_arguments)]
fn cast(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    eye: Res<EyeRay>,
    target: Res<EntityTarget>,
    sources: Res<AetherSources>,
    vfx: Res<VfxAssets>,
    spatial: SpatialQuery,
    mut state: ResMut<SpellState>,
    mut player: Query<(Entity, &mut AetherReserve), With<Player>>,
    phys: (Query<(&RigidBody, &Position, &ComputedMass)>, ResMut<crate::physics::impulses::ImpulseQueue>),
    mut targets: (Query<&mut PowerCell>, Query<&mut AetherStore>),
    mut out: (MessageWriter<MagicHit>, MessageWriter<Notify>),
) {
    let (cells, stores) = (&mut targets.0, &mut targets.1);
    let (bodies, mut impulses) = phys;
    let (hits, notify) = (&mut out.0, &mut out.1);
    state.cooldown = (state.cooldown - time.delta_secs()).max(0.0);
    if *tool != ActiveTool::Focus {
        return;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        let i = Spell::ALL.iter().position(|s| *s == state.selected).unwrap_or(0);
        state.selected = Spell::ALL[(i + 1) % Spell::ALL.len()];
        notify.write(Notify::info(format!("Заклинание: {}", state.selected.label())));
    }
    if !mouse.just_pressed(MouseButton::Left) || state.cooldown > 0.0 {
        return;
    }
    let Ok((player_e, mut reserve)) = player.single_mut() else { return };
    let spell = state.selected;
    if rules.consume_items && reserve.amount < spell.cost() {
        notify.write(Notify::warn("Недостаточно эфира"));
        return;
    }
    let field = sources.sample(eye.origin);
    let kind = if field.strength > super::energy::BACKGROUND * 1.5 { field.kind } else { AetherKind::Calm };
    // Stronger fields amplify spells.
    let power = 1.0 + (field.strength - super::energy::BACKGROUND).max(0.0) * 0.5;
    let mat = vfx.mat(kind);
    let hand = eye.origin + eye.direction * 0.7 - Vec3::Y * 0.2;
    match spell {
        Spell::Impulse => {
            let filter = SpatialQueryFilter::from_mask([Layer::Prop, Layer::Creature, Layer::Item]).with_excluded_entities([player_e]);
            let hits_in = spatial.shape_intersections(&Collider::sphere(8.0), eye.origin, Quat::IDENTITY, &filter);
            for e in hits_in {
                let Ok((rb, p, mass)) = bodies.get(e) else { continue };
                if !rb.is_dynamic() {
                    continue;
                }
                let to = p.0 - eye.origin;
                let d = to.length();
                if d > 8.0 || to.normalize_or_zero().dot(eye.direction) < 0.8 {
                    continue;
                }
                // Impulse (N·s); light objects fly, heavy ones barely move.
                let m = mass.value().max(1.0);
                let impulse = (900.0 * power * (1.0 - d / 9.0)).min(25.0 * m);
                impulses.push(e, (to.normalize_or_zero() + Vec3::Y * 0.3) * impulse / m);
                hits.write(MagicHit { target: e, damage: 8.0 * power, kind });
            }
        }
        Spell::Levitate => {
            let Some(e) = target.entity else {
                notify.write(Notify::warn("Нет цели"));
                return;
            };
            commands.entity(e).insert(Levitating { ttl: 6.0 * power });
            burst(&mut commands, vfx.mesh.clone(), mat, target.point, Vec3::Y * 0.3, 10, 0.6);
        }
        Spell::Discharge => {
            let Some(e) = target.entity else {
                notify.write(Notify::warn("Нет цели"));
                return;
            };
            let amount = 40.0 * power * kind.potency();
            if let Ok(mut c) = cells.get_mut(e) {
                c.energy = (c.energy + amount).min(c.capacity);
                notify.write(Notify::ok(format!("Энергоячейка заряжена: {:.0}/{:.0}", c.energy, c.capacity)));
            } else if let Ok(mut s) = stores.get_mut(e) {
                s.amount = (s.amount + amount * 0.5).min(s.capacity);
            } else {
                hits.write(MagicHit { target: e, damage: 30.0 * power, kind });
            }
            let dir = (target.point - hand).normalize_or_zero();
            let n = (target.distance * 3.0) as usize;
            for i in 0..n.max(3) {
                let p = hand + dir * (i as f32 / 3.0);
                commands.spawn((
                    Mesh3d(vfx.mesh.clone()),
                    MeshMaterial3d(vfx.mat(kind)),
                    Transform::from_translation(p),
                    Vfx { ttl: 0.35, velocity: Vec3::ZERO, grow: -1.0 },
                ));
            }
        }
    }
    if rules.consume_items {
        reserve.amount -= spell.cost();
    }
    state.cooldown = 0.35;
}

fn levitate(mut commands: Commands, time: Res<Time>, gravity: Res<Gravity>, mut q: Query<(Entity, &mut Levitating, &mut LinearVelocity, &RigidBody)>) {
    let dt = time.delta_secs();
    for (e, mut l, mut v, rb) in &mut q {
        l.ttl -= dt;
        if l.ttl <= 0.0 || !rb.is_dynamic() {
            commands.entity(e).remove::<Levitating>();
            continue;
        }
        // Cancel gravity and drift gently upwards.
        v.0 -= gravity.0 * dt;
        v.y = v.y.lerp(1.2, (dt * 2.0).min(1.0));
        v.x *= 0.98;
        v.z *= 0.98;
    }
}

fn levitate_vfx(q: Query<&Position, With<Levitating>>, time: Res<Time>, mut gizmos: Gizmos) {
    let t = time.elapsed_secs();
    for p in &q {
        for i in 0..3 {
            let a = t * 2.0 + i as f32 * 2.09;
            gizmos.circle(Isometry3d::new(p.0 - Vec3::Y * (0.6 + i as f32 * 0.15), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), 0.5 + 0.1 * a.sin(), AetherKind::Calm.color());
        }
    }
}

fn update_vfx(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Transform, &mut Vfx)>) {
    let dt = time.delta_secs();
    for (e, mut t, mut v) in &mut q {
        v.ttl -= dt;
        if v.ttl <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        t.translation += v.velocity * dt;
        v.velocity *= 0.9;
        t.scale = Vec3::splat((t.scale.x + v.grow * dt).clamp(0.05, 3.0));
    }
}

pub struct SpellsPlugin;

impl Plugin for SpellsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpellState>()
            .add_message::<MagicHit>()
            .add_systems(Startup, setup)
            .add_systems(Update, cast.after(crate::tools::update_entity_target).run_if(gameplay_active))
            .add_systems(Update, (regenerate, levitate_vfx, update_vfx).run_if(crate::core::states::in_game))
            .add_systems(FixedUpdate, levitate);
    }
}
