//! Anomalies (V6, MASTER PROMPT §12): a gravitational anomaly, the
//! stationary generator that shapes it and the stabilizers that hold it.
//!
//! * **Generator** (`anomaly_generator`, a magic device): burns agitated /
//!   condensed / unique aether from the magic network — it does not make
//!   unstable magic from nothing — and works only where the surrounding
//!   field is unstable enough. After `form_time` seconds of fuelled work it
//!   rolls: the result may fail, and the shaped radius and strength spread
//!   around the settings. LMB on/off, RMB radius, Shift+RMB mode; a logic
//!   signal also switches it.
//! * **Anomaly** (`AnomalyField`): a sphere where gravity is multiplied —
//!   lighter, weightless, pulling up or heavier. Only the pull changes,
//!   inertia stays. After the generator stops the anomaly may linger.
//! * **Stabilizer** (existing `magic_stabilizer`, needs electricity):
//!   holds anomalies in reach — it does not remove or weaken them, it makes
//!   them predictable (no wandering, no swelling), tracks them in its panel
//!   and warns when its hold weakens.
//! * **Loss of control** (a held anomaly drops below 25 % containment):
//!   `AnomalyControlLost` is sent and every registered consequence runs.
//!   No consequences are registered yet — the concept is still being
//!   designed; add one with `app.add_anomaly_consequence(...)`.
//!
//! Rules are pure functions in `field` (unit-tested). Kinds and numbers are
//! data: `content/anomalies.ron`, `arcana.ron` (generator, stabilizer).

pub mod field;

use avian3d::{dynamics::solver::islands::WakeBody, prelude::*};
use bevy::{light::NotShadowCaster, prelude::*};
use serde::{Deserialize, Serialize};

use crate::{
    arcana::{MagicDemand, MagicNode},
    content::{AetherKind, AnomalyDef, AnomalyKind, MagicRole, registry},
    core::{events::Notify, states::in_game},
    electric::ElectricState,
    kinetics::KineticBlock,
    machinery::MachinerySet,
    magic::sources::AetherSources,
    player::movement::CharacterState,
    save::world_save::{SaveHook, SaveHookExt},
    tools::manipulator::MechanicsUse,
    wear::Integrity,
};

/// Containment at which an anomaly counts as held.
pub const HELD: f32 = 0.6;
/// Containment below which a held anomaly is lost.
pub const LOST: f32 = 0.25;

/// An anomaly in the world.
#[derive(Component, Debug, Clone)]
pub struct AnomalyField {
    /// `anomalies.ron` key.
    pub key: String,
    /// Shaped (set) values.
    pub radius: f32,
    pub gravity: f32,
    pub seed: u32,
    /// The generator sustaining it.
    pub generator: Option<Entity>,
    /// Seconds left once nothing sustains it (None = sustained).
    pub life: Option<f32>,
    /// Stabilizers' hold (0..1) this tick.
    pub containment: f32,
    /// What it does right now (after wandering and fading).
    pub now_radius: f32,
    pub now_gravity: f32,
    /// Was held by stabilizers (for loss-of-control detection).
    pub held: bool,
    /// The «weakening» warning was already given.
    pub warned: bool,
}

impl AnomalyField {
    pub fn def(&self) -> Option<&'static AnomalyDef> {
        registry().anomaly(&self.key)
    }
    /// Gravity multiplier at a point.
    pub fn gravity_at(&self, center: Vec3, p: Vec3) -> f32 {
        field::gravity_at(center.distance(p), self.now_radius, self.now_gravity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GenStatus {
    Off,
    /// The surroundings are too calm.
    NoEnvironment,
    Forming,
    /// Shaping failed; retry after the cooldown (s).
    Failed(f32),
    Sustaining,
}

/// State of an anomaly generator block.
#[derive(Component, Debug, Clone)]
pub struct AnomalyGenerator {
    pub radius_idx: usize,
    pub mode_idx: usize,
    /// Shaping progress 0..1.
    pub charge: f32,
    pub status: GenStatus,
    pub anomaly: Option<Entity>,
    /// Shaping attempts so far (seeds the rolls).
    pub attempts: u32,
    /// Seconds without enough fuel while sustaining.
    pub starve: f32,
    /// Fuel share got last tick (diagnostics).
    pub fuel: f32,
    /// Unstable field strength at the generator (diagnostics).
    pub instability: f32,
}

impl Default for AnomalyGenerator {
    fn default() -> Self {
        Self { radius_idx: 1, mode_idx: 0, charge: 0.0, status: GenStatus::Off, anomaly: None, attempts: 0, starve: 0.0, fuel: 0.0, instability: 0.0 }
    }
}

impl AnomalyGenerator {
    pub fn setting(&self, def: &AnomalyDef) -> (f32, f32, String) {
        let r = def.radii.get(self.radius_idx % def.radii.len().max(1)).copied().unwrap_or(6.0);
        let (label, g) = def.modes.get(self.mode_idx % def.modes.len().max(1)).cloned().unwrap_or(("—".into(), 1.0));
        (r, g, label)
    }
}

/// Why control over an anomaly was lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossCause {
    /// No stabilizer in reach any more.
    NoStabilizer,
    /// Stabilizers have no electricity.
    NoPower,
    /// Stabilizers are broken.
    Broken,
    /// The anomaly is too big / strong for the stabilizers.
    Overload,
}

impl LossCause {
    pub fn label(self) -> &'static str {
        match self {
            LossCause::NoStabilizer => "рядом нет стабилизатора",
            LossCause::NoPower => "стабилизаторы без питания",
            LossCause::Broken => "стабилизаторы сломаны",
            LossCause::Overload => "стабилизаторы перегружены",
        }
    }
}

/// Sent when a held anomaly slips out of control.
#[derive(Message, Debug, Clone, Copy)]
#[allow(dead_code)] // `center` is for future consequences
pub struct AnomalyControlLost {
    pub anomaly: Entity,
    pub center: Vec3,
    pub cause: LossCause,
}

/// A consequence of losing control (expansion, change, secondary
/// anomalies, accidents …). Called once per loss with world access.
pub type Consequence = fn(&mut World, &AnomalyControlLost);

/// Registered consequences — empty until the concept is decided.
#[derive(Resource, Default)]
pub struct AnomalyConsequences(pub Vec<(&'static str, Consequence)>);

/// Losses waiting for consequences (filled by `update_fields`).
#[derive(Resource, Default)]
struct PendingLosses(Vec<AnomalyControlLost>);

#[allow(dead_code)] // extension point: no consequences are registered yet
pub trait AnomalyAppExt {
    /// Adds a consequence of losing control over an anomaly.
    fn add_anomaly_consequence(&mut self, name: &'static str, f: Consequence) -> &mut Self;
}

impl AnomalyAppExt for App {
    fn add_anomaly_consequence(&mut self, name: &'static str, f: Consequence) -> &mut Self {
        self.init_resource::<AnomalyConsequences>();
        self.world_mut().resource_mut::<AnomalyConsequences>().0.push((name, f));
        self
    }
}

/// Body pulled by an anomaly; `own_sleep` = we added `SleepingDisabled`.
#[derive(Component)]
struct InAnomaly {
    own_sleep: bool,
}

fn is_generator(key: &str) -> bool {
    registry().magic(key).is_some_and(|m| m.role == MagicRole::Generator)
}

fn attach_generators(mut commands: Commands, mut added: Query<(Entity, &mut KineticBlock), (Added<KineticBlock>, Without<AnomalyGenerator>)>) {
    for (e, mut k) in &mut added {
        if is_generator(&k.key) {
            // Placed switched off: the player starts it deliberately.
            k.on = false;
            commands.entity(e).insert((AnomalyGenerator::default(), MagicDemand(0.0)));
        }
    }
}

/// Unstable field strength at a point (0 when the dominant kind is calm).
pub fn instability(sources: &AetherSources, p: Vec3) -> f32 {
    let f = sources.sample(p);
    match f.kind {
        AetherKind::Agitated | AetherKind::Condensed | AetherKind::Unique => f.strength,
        _ => 0.0,
    }
}

fn spawn_field(commands: &mut Commands, key: &str, center: Vec3, radius: f32, gravity: f32, seed: u32, generator: Option<Entity>) -> Entity {
    commands
        .spawn((
            Name::new("Anomaly"),
            AnomalyField { key: key.into(), radius, gravity, seed, generator, life: None, containment: 0.0, now_radius: radius, now_gravity: gravity, held: false, warned: false },
            Transform::from_translation(center),
            Visibility::default(),
        ))
        .id()
}

/// The generator lets its anomaly go: it lingers or collapses (a roll).
fn release(field: &mut AnomalyField, seed: u64, notify: &mut MessageWriter<Notify>) {
    field.generator = None;
    let Some(def) = field.def() else {
        field.life = Some(3.0);
        return;
    };
    if field::rand01(seed) < def.persist_chance {
        let (a, b) = def.persist_time;
        field.life = Some(a + (b - a) * field::rand01(seed ^ 0x77));
        notify.write(Notify::warn("Генератор остановлен, но аномалия осталась"));
    } else {
        field.life = Some(3.0);
        notify.write(Notify::info("Генератор остановлен: аномалия рассеивается"));
    }
}

#[allow(clippy::too_many_arguments)]
fn run_generators(
    mut commands: Commands,
    time: Res<Time>,
    sources: Res<AetherSources>,
    mut gens: Query<(Entity, &mut AnomalyGenerator, &KineticBlock, &MagicNode, &mut MagicDemand, &GlobalTransform, Option<&Integrity>)>,
    mut fields: Query<(Entity, &mut AnomalyField, &mut Transform)>,
    mut notify: MessageWriter<Notify>,
) {
    let dt = time.delta_secs();
    for (e, mut g, k, node, mut demand, gt, integ) in &mut gens {
        let Some(mdef) = node.def() else { continue };
        let Some(adef) = mdef.anomaly.as_deref().and_then(|a| registry().anomaly(a)) else { continue };
        let pos = gt.translation() + Vec3::Y * 0.5;
        let seed = e.to_bits() ^ ((g.attempts as u64) << 32);
        g.fuel = node.output;
        g.instability = instability(&sources, pos);
        // Our anomaly may have vanished (despawned, loaded save).
        if g.anomaly.is_some_and(|a| fields.get(a).is_err()) {
            g.anomaly = None;
            if g.status == GenStatus::Sustaining {
                g.status = GenStatus::Forming;
                g.charge = 0.0;
            }
        }
        let broken = integ.is_some_and(|i| i.value <= 0.0);
        let chance = field::shaping_chance(g.instability, adef.min_instability, adef.success);
        let stop = |g: &mut AnomalyGenerator, fields: &mut Query<(Entity, &mut AnomalyField, &mut Transform)>, notify: &mut MessageWriter<Notify>| {
            if let Some(a) = g.anomaly.take()
                && let Ok((_, mut f, _)) = fields.get_mut(a)
            {
                release(&mut f, seed ^ 0xD1E, notify);
            }
            g.charge = 0.0;
        };
        if !k.on || broken {
            if g.status != GenStatus::Off {
                stop(&mut g, &mut fields, &mut notify);
                g.status = GenStatus::Off;
            }
            demand.0 = 0.0;
            continue;
        }
        let Some(chance) = chance else {
            if g.status != GenStatus::NoEnvironment {
                stop(&mut g, &mut fields, &mut notify);
                g.status = GenStatus::NoEnvironment;
                notify.write(Notify::warn("Генератор аномалий: среда слишком спокойна, нужна раздражённая магия рядом"));
            }
            demand.0 = 0.0;
            continue;
        };
        let fuelled = node.output >= 0.5;
        match g.status {
            GenStatus::Off | GenStatus::NoEnvironment => {
                g.status = GenStatus::Forming;
                g.charge = 0.0;
                demand.0 = mdef.rate;
            }
            GenStatus::Failed(cd) => {
                demand.0 = 0.0;
                g.status = if cd - dt <= 0.0 { GenStatus::Forming } else { GenStatus::Failed(cd - dt) };
            }
            GenStatus::Forming => {
                demand.0 = mdef.rate;
                if fuelled {
                    g.charge += dt / adef.form_time.max(0.1);
                }
                if g.charge >= 1.0 {
                    g.charge = 0.0;
                    g.attempts += 1;
                    let roll = field::rand01(seed ^ 0xF0F0);
                    if roll >= chance {
                        g.status = GenStatus::Failed(4.0);
                        notify.write(Notify::warn(format!("Аномалия не сформировалась (шанс был {:.0} %)", chance * 100.0)));
                        continue;
                    }
                    let (r, gv, label) = g.setting(adef);
                    let (r, gv) = field::shaped(r, gv, adef.spread, seed);
                    // A lingering anomaly in reach is captured, not doubled.
                    let near = fields.iter().find(|(_, f, t)| f.generator.is_none() && f.key == adef.key && t.translation.distance(pos) < f.radius.max(r)).map(|(fe, _, _)| fe);
                    let a = match near.and_then(|fe| fields.get_mut(fe).ok()) {
                        Some((fe, mut f, mut t)) => {
                            f.radius = r;
                            f.gravity = gv;
                            f.generator = Some(e);
                            f.life = None;
                            t.translation = pos;
                            fe
                        }
                        None => spawn_field(&mut commands, &adef.key, pos, r, gv, seed as u32, Some(e)),
                    };
                    g.anomaly = Some(a);
                    g.status = GenStatus::Sustaining;
                    g.starve = 0.0;
                    notify.write(Notify::ok(format!("{}: {label}, радиус {r:.1} м, тяжесть ×{gv:.2}", adef.name)));
                }
            }
            GenStatus::Sustaining => {
                demand.0 = mdef.rate;
                if fuelled {
                    g.starve = 0.0;
                } else {
                    g.starve += dt;
                    if g.starve > 3.0 {
                        notify.write(Notify::warn("Генератору аномалий не хватает раздражённого эфира"));
                        stop(&mut g, &mut fields, &mut notify);
                        g.status = GenStatus::Forming;
                    }
                }
            }
        }
    }
}

/// Mechanics mode: LMB on/off, RMB radius, Shift+RMB mode.
fn mechanics(
    mut reader: MessageReader<MechanicsUse>,
    keys: Res<ButtonInput<KeyCode>>,
    mut gens: Query<(&mut AnomalyGenerator, &mut KineticBlock, &MagicNode)>,
    mut fields: Query<&mut AnomalyField>,
    mut notify: MessageWriter<Notify>,
) {
    for u in reader.read() {
        let Ok((mut g, mut k, node)) = gens.get_mut(u.entity) else { continue };
        let Some(adef) = node.def().and_then(|d| d.anomaly.as_deref()).and_then(|a| registry().anomaly(a)) else { continue };
        if !u.secondary {
            k.on = !k.on;
            notify.write(Notify::info(if k.on { "Генератор аномалий включён" } else { "Генератор аномалий выключен" }));
            continue;
        }
        if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            g.mode_idx = (g.mode_idx + 1) % adef.modes.len().max(1);
        } else {
            g.radius_idx = (g.radius_idx + 1) % adef.radii.len().max(1);
        }
        let (r, gv, label) = g.setting(adef);
        notify.write(Notify::info(format!("Генератор: {label} (×{gv:.2}), радиус {r:.0} м")));
        // A sustained anomaly is reshaped to the new setting (with spread).
        if let Some(mut f) = g.anomaly.and_then(|a| fields.get_mut(a).ok()) {
            g.attempts += 1;
            let (r, gv) = field::shaped(r, gv, adef.spread, u.entity.to_bits() ^ ((g.attempts as u64) << 32));
            f.radius = r;
            f.gravity = gv;
        }
    }
}

/// Stabilizer hold, wandering, life time and loss of control.
#[allow(clippy::too_many_arguments)]
fn update_fields(
    mut commands: Commands,
    time: Res<Time>,
    mut fields: Query<(Entity, &mut AnomalyField, &Transform)>,
    stabs: Query<(&MagicNode, &GlobalTransform, Option<&ElectricState>, Option<&Integrity>)>,
    mut lost: MessageWriter<AnomalyControlLost>,
    mut pending: ResMut<PendingLosses>,
    mut notify: MessageWriter<Notify>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (e, mut f, tr) in &mut fields {
        let Some(def) = f.def() else { continue };
        let center = tr.translation;
        // Stabilizers in reach.
        let (mut holds, mut in_reach, mut powered, mut broken) = (0.0, 0, 0, 0);
        for (n, gt, el, integ) in &stabs {
            let Some(d) = n.def() else { continue };
            if d.role != MagicRole::Stabilizer || d.hold <= 0.0 {
                continue;
            }
            if gt.translation().distance(center) > f.radius + d.hold_range {
                continue;
            }
            in_reach += 1;
            let power = el.map(|s| s.satisfaction).unwrap_or(0.0);
            let i = integ.map(|i| i.value).unwrap_or(1.0);
            if power >= 0.5 {
                powered += 1;
            }
            if i <= 0.0 {
                broken += 1;
            }
            holds += d.hold * power * i;
        }
        f.containment = field::containment(holds, field::load(f.radius, f.gravity));
        let (r, mut g) = field::current(f.radius, f.gravity, def.jitter, f.containment, t, f.seed);
        // Fading out at the end of its life.
        if let Some(life) = f.life.as_mut() {
            *life -= dt;
            if *life <= 0.0 {
                commands.entity(e).despawn();
                continue;
            }
            let k = (*life / 5.0).min(1.0);
            g = 1.0 + (g - 1.0) * k;
        }
        f.now_radius = r;
        f.now_gravity = g;
        // Tracking: held → weakening warning → loss of control.
        if f.containment >= HELD {
            f.held = true;
            f.warned = false;
        } else if f.held {
            if !f.warned {
                f.warned = true;
                notify.write(Notify::warn(format!("Удержание аномалии ослабло: {:.0} %", f.containment * 100.0)));
            }
            if f.containment < LOST {
                f.held = false;
                let cause = if in_reach == 0 {
                    LossCause::NoStabilizer
                } else if broken == in_reach {
                    LossCause::Broken
                } else if powered == 0 {
                    LossCause::NoPower
                } else {
                    LossCause::Overload
                };
                notify.write(Notify::warn(format!("Потеря контроля над аномалией: {}", cause.label())));
                let msg = AnomalyControlLost { anomaly: e, center, cause };
                lost.write(msg);
                pending.0.push(msg);
            }
        }
    }
}

/// Runs the registered consequences of lost control (none yet).
fn run_consequences(world: &mut World) {
    let losses = std::mem::take(&mut world.resource_mut::<PendingLosses>().0);
    if losses.is_empty() {
        return;
    }
    let list = world.get_resource::<AnomalyConsequences>().map(|c| c.0.clone()).unwrap_or_default();
    for l in &losses {
        info!("anomaly {:?}: control lost ({:?}); consequences: {}", l.anomaly, l.cause, list.len());
        for (name, f) in &list {
            debug!("anomaly consequence {name}");
            f(world, l);
        }
    }
}

/// Gravity inside anomalies: extra acceleration g·(k − 1) on dynamic
/// bodies (the player too, unless flying). Bodies inside stay awake.
#[allow(clippy::type_complexity)]
fn apply_gravity(
    mut commands: Commands,
    time: Res<Time>,
    gravity: Res<Gravity>,
    fields: Query<(&AnomalyField, &Transform)>,
    mut bodies: Query<(Entity, &RigidBody, &Position, &mut LinearVelocity, Has<Sleeping>, Has<SleepingDisabled>, Option<&InAnomaly>, Option<&CharacterState>)>,
) {
    let dt = time.delta_secs();
    let list: Vec<(&AnomalyField, Vec3)> = fields.iter().map(|(f, t)| (f, t.translation)).collect();
    for (e, rb, p, mut v, sleeping, no_sleep, inside, ch) in &mut bodies {
        if !rb.is_dynamic() || ch.is_some_and(|c| c.flying) {
            continue;
        }
        // Strongest deviation wins where anomalies overlap.
        let mut k = 1.0f32;
        for (f, c) in &list {
            let m = f.gravity_at(*c, p.0);
            if (m - 1.0).abs() > (k - 1.0).abs() {
                k = m;
            }
        }
        if (k - 1.0).abs() < 1e-3 {
            if let Some(i) = inside {
                if i.own_sleep {
                    commands.entity(e).remove::<(InAnomaly, SleepingDisabled)>();
                } else {
                    commands.entity(e).remove::<InAnomaly>();
                }
            }
            continue;
        }
        if inside.is_none() {
            commands.entity(e).insert(InAnomaly { own_sleep: !no_sleep });
            if !no_sleep {
                commands.entity(e).insert(SleepingDisabled);
            }
        }
        if sleeping {
            commands.queue(WakeBody(e));
            continue;
        }
        v.0 += gravity.0 * (k - 1.0) * dt;
    }
}

// ------------------------------------------------------------ visuals

#[derive(Resource)]
struct AnomalyAssets {
    shell: Handle<Mesh>,
    shell_mat: Handle<StandardMaterial>,
    mote: Handle<Mesh>,
    mote_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Shell;

#[derive(Component)]
struct Debris {
    phase: f32,
    orbit: f32,
}

const DEBRIS: usize = 14;

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, style: Res<crate::style::StyleMaterials>) {
    commands.insert_resource(AnomalyAssets {
        shell: meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap()),
        shell_mat: mats.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.35, 1.0, 0.07),
            emissive: LinearRgba::rgb(0.3, 0.08, 0.35),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        mote: meshes.add(crate::style::pixel_box(Vec3::splat(0.16))),
        mote_mat: style.get("arcane_stone"),
    });
}

fn spawn_visuals(mut commands: Commands, a: Option<Res<AnomalyAssets>>, added: Query<Entity, Added<AnomalyField>>) {
    let Some(a) = a else { return };
    for e in &added {
        commands.entity(e).with_children(|c| {
            c.spawn((Shell, Mesh3d(a.shell.clone()), MeshMaterial3d(a.shell_mat.clone()), Transform::default(), NotShadowCaster));
            for i in 0..DEBRIS {
                c.spawn((
                    Debris { phase: i as f32 * 0.449, orbit: 0.25 + (i % 5) as f32 * 0.13 },
                    Mesh3d(a.mote.clone()),
                    MeshMaterial3d(a.mote_mat.clone()),
                    Transform::default(),
                    NotShadowCaster,
                ));
            }
        });
    }
}

/// Shell follows the radius; debris floats as gravity there would make it:
/// weightless drifts, inverted rises and wraps, heavy settles low.
fn animate(time: Res<Time>, fields: Query<(&AnomalyField, &Children)>, mut shells: Query<&mut Transform, (With<Shell>, Without<Debris>)>, mut debris: Query<(&Debris, &mut Transform), Without<Shell>>) {
    let t = time.elapsed_secs();
    for (f, children) in &fields {
        for c in children.iter() {
            if let Ok(mut s) = shells.get_mut(c) {
                s.scale = Vec3::splat(f.now_radius.max(0.1));
            }
            if let Ok((d, mut tr)) = debris.get_mut(c) {
                let r = f.now_radius * d.orbit;
                let a = t * (0.15 + d.orbit * 0.2) + d.phase * 6.0;
                let lift = 1.0 - f.now_gravity;
                // Height in −1..1 of the radius: rising loop for lift > 0.
                let h = if lift > 0.05 {
                    ((t * 0.12 * lift + d.phase).fract() * 2.0 - 1.0) * 0.8
                } else {
                    -0.7 + (t * 0.5 + d.phase * 3.0).sin().abs() * 0.1
                };
                tr.translation = Vec3::new(a.cos() * r, h * f.now_radius * 0.6, a.sin() * r);
                tr.rotation = Quat::from_rotation_y(a * 2.0) * Quat::from_rotation_x(a);
            }
        }
    }
}

// ------------------------------------------------------------ panels

/// Lines for engineering panels and the scanner: (lines, diagnostics).
pub fn describe(world: &World, e: Entity) -> Option<(Vec<String>, Vec<String>)> {
    let mut lines = Vec::new();
    let mut diag = Vec::new();
    let mut fq = world.try_query::<(Entity, &AnomalyField, &Transform)>()?;
    if let Some(g) = world.get::<AnomalyGenerator>(e) {
        let adef = world.get::<MagicNode>(e)?.def()?.anomaly.as_deref().and_then(|a| registry().anomaly(a))?;
        let (r, gv, label) = g.setting(adef);
        let on = world.get::<KineticBlock>(e).is_some_and(|k| k.on);
        lines.push(format!("Генератор: {} · режим «{label}» (×{gv:.2}) · радиус {r:.0} м", if on { "вкл" } else { "выкл" }));
        lines.push(format!("Нестабильность среды: {:.2} (нужно {:.2}) · топливо {:.0} %", g.instability, adef.min_instability, g.fuel * 100.0));
        match g.status {
            GenStatus::Off => {}
            GenStatus::NoEnvironment => diag.push("Среда слишком спокойна: нужна раздражённая магия рядом".into()),
            GenStatus::Forming => {
                lines.push(format!("Формирование: {:.0} %", g.charge * 100.0));
                if g.fuel < 0.5 {
                    diag.push("Не хватает раздражённого эфира: подведите его из сети".into());
                }
            }
            GenStatus::Failed(cd) => diag.push(format!("Аномалия не сформировалась, новая попытка через {cd:.0} с")),
            GenStatus::Sustaining => {
                if g.fuel < 0.5 {
                    diag.push("Топлива мало: аномалия скоро выйдет из-под управления генератора".into());
                }
            }
        }
        if let Some((_, f, _)) = g.anomaly.and_then(|a| fq.get(world, a).ok()) {
            lines.push(format!("Аномалия: тяжесть ×{:.2} (задано ×{:.2}) · радиус {:.1} м", f.now_gravity, f.gravity, f.now_radius));
        }
    }
    if let Some(n) = world.get::<MagicNode>(e)
        && let Some(d) = n.def()
        && d.role == MagicRole::Stabilizer
        && d.hold > 0.0
    {
        let pos = world.get::<GlobalTransform>(e)?.translation();
        for (_, f, t) in fq.iter(world) {
            if t.translation.distance(pos) > f.radius + d.hold_range {
                continue;
            }
            lines.push(format!("Удерживает аномалию: {:.0} % · тяжесть ×{:.2} · радиус {:.1} м", f.containment * 100.0, f.now_gravity, f.now_radius));
            if f.containment < HELD {
                diag.push(format!("Удержания мало ({:.0} %): аномалия непредсказуема", f.containment * 100.0));
            }
        }
    }
    if lines.is_empty() && diag.is_empty() { None } else { Some((lines, diag)) }
}

/// Anomaly strength at a point (|gravity − 1|, capped at 1) — the
/// anomaly sensor.
pub fn strength_at(world: &mut World, p: Vec3) -> f32 {
    let mut q = world.query::<(&AnomalyField, &Transform)>();
    q.iter(world).map(|(f, t)| (f.gravity_at(t.translation, p) - 1.0).abs()).fold(0.0, f32::max).min(1.0)
}

// ------------------------------------------------------------ save

#[derive(Serialize, Deserialize)]
struct SavedGenerator {
    cell: IVec3,
    radius_idx: usize,
    mode_idx: usize,
    charge: f32,
    status: GenStatus,
    attempts: u32,
    /// Index into `fields`.
    anomaly: Option<usize>,
}

#[derive(Serialize, Deserialize)]
struct SavedField {
    key: String,
    center: Vec3,
    radius: f32,
    gravity: f32,
    seed: u32,
    life: Option<f32>,
    held: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct SavedAnomalies {
    fields: Vec<SavedField>,
    generators: Vec<SavedGenerator>,
}

pub(crate) fn capture(world: &mut World) -> serde_json::Value {
    let mut fq = world.query::<(Entity, &AnomalyField, &Transform)>();
    let list: Vec<(Entity, SavedField)> = fq
        .iter(world)
        .map(|(e, f, t)| (e, SavedField { key: f.key.clone(), center: t.translation, radius: f.radius, gravity: f.gravity, seed: f.seed, life: f.life, held: f.held }))
        .collect();
    let mut gq = world.query::<(&AnomalyGenerator, &KineticBlock)>();
    let generators = gq
        .iter(world)
        .map(|(g, k)| SavedGenerator {
            cell: k.cell,
            radius_idx: g.radius_idx,
            mode_idx: g.mode_idx,
            charge: g.charge,
            status: g.status,
            attempts: g.attempts,
            anomaly: g.anomaly.and_then(|a| list.iter().position(|(e, _)| *e == a)),
        })
        .collect();
    serde_json::to_value(SavedAnomalies { fields: list.into_iter().map(|(_, f)| f).collect(), generators }).unwrap_or_default()
}

pub(crate) fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let mut q = world.query_filtered::<Entity, With<AnomalyField>>();
    for e in q.iter(world).collect::<Vec<_>>() {
        world.entity_mut(e).despawn();
    }
    let s: SavedAnomalies = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut kq = world.query::<(Entity, &KineticBlock)>();
    let cells: std::collections::HashMap<IVec3, Entity> = kq.iter(world).map(|(e, k)| (k.cell, e)).collect();
    let mut spawned = Vec::new();
    for f in &s.fields {
        let mut c = world.commands();
        let e = spawn_field(&mut c, &f.key, f.center, f.radius, f.gravity, f.seed, None);
        spawned.push(e);
    }
    world.flush();
    for (e, f) in spawned.iter().zip(&s.fields) {
        if let Some(mut a) = world.get_mut::<AnomalyField>(*e) {
            a.life = f.life;
            a.held = f.held;
        }
    }
    for g in &s.generators {
        let Some(&ge) = cells.get(&g.cell) else { continue };
        let anomaly = g.anomaly.and_then(|i| spawned.get(i).copied());
        if let Some(a) = anomaly
            && let Some(mut f) = world.get_mut::<AnomalyField>(a)
        {
            f.generator = Some(ge);
        }
        world.entity_mut(ge).insert((
            AnomalyGenerator { radius_idx: g.radius_idx, mode_idx: g.mode_idx, charge: g.charge, status: g.status, anomaly, attempts: g.attempts, ..default() },
            MagicDemand(0.0),
        ));
    }
}

/// Run conditions / ordering label.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnomalySet;

pub struct AnomalyPlugin;

impl Plugin for AnomalyPlugin {
    fn build(&self, app: &mut App) {
        // Every anomaly kind needs its effect code; only gravity exists.
        debug_assert!(registry().anomalies.iter().all(|a| a.kind == AnomalyKind::Gravity));
        app.init_resource::<AnomalyConsequences>()
            .init_resource::<PendingLosses>()
            .add_message::<AnomalyControlLost>()
            .add_save_hook(SaveHook { key: "anomalies", capture, restore })
            .add_systems(Startup, setup_assets)
            .add_systems(Update, (attach_generators, mechanics, spawn_visuals, animate).run_if(in_game))
            .add_systems(
                FixedUpdate,
                (run_generators, update_fields, run_consequences, apply_gravity)
                    .chain()
                    .in_set(AnomalySet)
                    .after(MachinerySet::Power)
                    .run_if(in_game),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_is_consistent() {
        let r = registry();
        let g = r.magic("anomaly_generator").expect("generator device");
        assert_eq!(g.role, MagicRole::Generator);
        let a = r.anomaly(g.anomaly.as_deref().unwrap()).expect("its anomaly");
        assert!(!a.modes.is_empty() && !a.radii.is_empty());
        assert!(r.kinetic("anomaly_generator").is_some() && r.kinetic("sensor_anomaly").is_some());
        assert!(r.magic("magic_stabilizer").unwrap().hold > 0.0);
    }

    #[test]
    fn generator_settings_cycle() {
        let a = registry().anomaly("gravity").unwrap();
        let mut g = AnomalyGenerator::default();
        let first = g.setting(a);
        g.mode_idx += a.modes.len();
        g.radius_idx += a.radii.len();
        assert_eq!(g.setting(a), first);
    }
}
