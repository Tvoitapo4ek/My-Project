//! Ropes, winches, rope anchors and the hand crank.
//!
//! * **Rope** (tool): LMB on a winch or an anchor, then LMB on another one —
//!   a rope between them, as long as they are apart (up to `MAX_LEN`). It
//!   pulls only when taut (a distance joint with limits 0..length), so it
//!   tows, lifts and hangs things, between any two grids: the world, a
//!   machine, a crane's moving part. RMB with the rope on an end — cut.
//! * **Winch**: a drum turned from the side (shaft or cog on its axis);
//!   turning reels the rope in or out at 0.5 m per turn.
//! * **Rope anchor**: an eyelet on any block face.
//! * **Hand crank**: on the face of a shaft (or anything kinetic): hold RMB
//!   on it and it turns at 32 RPM (256 SU).
//!
//! A rope belongs to its first end (`RopeEnd` on that block entity) and
//! follows both ends when blocks move between grids; it is saved with the
//! world.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    content::{Mech, Tool},
    core::{Ui, toast},
    grid::VoxelGrid,
    interact::{Clicks, Target},
    inventory::Inventory,
    mech::{BlockEntity, Spin},
    style::{BoxCache, Materials, part, spawn_parts},
};

pub const MAX_LEN: f32 = 48.0;
/// Metres of rope per drum turn.
pub const PER_TURN: f32 = 0.5;
pub const CRANK_RPM: f32 = 32.0;
pub const CRANK_POWER: f32 = 256.0;
const SEGMENTS: usize = 14;

/// A rope from this block to another block entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct RopeEnd {
    pub to: Entity,
    pub length: f32,
    pub joint: Option<Entity>,
    /// The bodies the joint was made for.
    bodies: (Entity, Entity),
}

impl RopeEnd {
    pub fn new(to: Entity, length: f32) -> Self {
        Self { to, length, joint: None, bodies: (Entity::PLACEHOLDER, Entity::PLACEHOLDER) }
    }
}

/// Segments drawing a rope (children of the rope's first end's grid
/// would turn with it; these live in world space).
#[derive(Component)]
struct RopeLook(Entity);

/// Held RMB on a hand crank this frame.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Crank {
    pub held: bool,
}

/// First end picked with the rope tool.
#[derive(Resource, Default)]
pub struct RopeStart(pub Option<Entity>);

fn is_end(m: Option<Mech>) -> bool {
    matches!(m, Some(Mech::Winch | Mech::RopeAnchor))
}

/// Where the rope leaves a block, in its grid's local space.
pub fn exit_point(b: &BlockEntity) -> Vec3 {
    let c = b.cell.as_vec3() + Vec3::splat(0.5);
    match b.mech() {
        // The anchor's eye, next to the face of the block it is on.
        Some(Mech::RopeAnchor) => c + b.dir().as_vec3() * 0.24,
        _ => c + b.dir().as_vec3() * 0.5,
    }
}

fn world_point(world_t: &Query<&GlobalTransform>, b: &BlockEntity) -> Option<Vec3> {
    Some(world_t.get(b.grid).ok()?.transform_point(exit_point(b)))
}

fn add_parts(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Crank>)>) {
    for (e, b) in &added {
        if b.mech() == Some(Mech::HandCrank) {
            commands.entity(e).insert(Crank::default());
        }
    }
}

/// Holding RMB on a crank turns it (a source while held).
fn crank(mouse: Res<ButtonInput<MouseButton>>, ui: Res<State<Ui>>, target: Res<Target>, grids: Query<&VoxelGrid>, mut cranks: Query<(Entity, &mut Crank, &mut crate::mech::Signal)>) {
    let aimed = (*ui.get() == Ui::Game && mouse.pressed(MouseButton::Right)).then(|| target.hit).flatten().and_then(|a| grids.get(a.grid).ok()?.entities.get(&a.cell).copied());
    for (e, mut c, mut sig) in &mut cranks {
        let held = c.held || aimed == Some(e);
        // `held` set from outside (scripts) stays for this frame only.
        c.held = false;
        if held {
            sig.0 = 1.0;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn use_rope(
    mut commands: Commands,
    clicks: Res<Clicks>,
    target: Res<Target>,
    players: Query<&Inventory, With<crate::player::Player>>,
    grids: Query<&VoxelGrid>,
    blocks: Query<&BlockEntity>,
    ends: Query<&RopeEnd>,
    owners: Query<(Entity, &RopeEnd)>,
    tf: Query<&GlobalTransform>,
    mut start: ResMut<RopeStart>,
    mut toasts: MessageWriter<crate::core::Toast>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
) {
    let Ok(inv) = players.single() else { return };
    if crate::tools::held_tool(inv) != Some(Tool::Rope) || !(clicks.left || clicks.right) {
        return;
    }
    let hit = target.hit.and_then(|a| grids.get(a.grid).ok()?.entities.get(&a.cell).copied()).filter(|e| blocks.get(*e).is_ok_and(|b| is_end(b.mech())));
    if clicks.right {
        start.0 = None;
        if let Some(e) = hit {
            // Cut from either end.
            let owner = if ends.contains(e) { Some(e) } else { owners.iter().find(|(_, r)| r.to == e).map(|(o, _)| o) };
            if let Some(o) = owner
                && let Ok(r) = ends.get(o)
            {
                if let Some(j) = r.joint {
                    commands.entity(j).despawn();
                }
                commands.entity(o).remove::<RopeEnd>();
                toasts.write(toast("Верёвка перерезана"));
            }
        }
        return;
    }
    let Some(e) = hit else {
        toasts.write(toast("Верёвку крепят к лебёдке или креплению"));
        return;
    };
    match start.0.take() {
        None => {
            if ends.contains(e) {
                toasts.write(toast("Здесь уже есть верёвка (ПКМ — перерезать)"));
                return;
            }
            start.0 = Some(e);
            toasts.write(toast("Первый конец — ЛКМ по второму"));
        }
        Some(a) if a == e => {}
        Some(a) => {
            let (Ok(ba), Ok(bb)) = (blocks.get(a), blocks.get(e)) else { return };
            let (Some(pa), Some(pb)) = (world_point(&tf, ba), world_point(&tf, bb)) else { return };
            let len = pa.distance(pb);
            if len > MAX_LEN {
                toasts.write(toast(format!("Слишком далеко: {len:.0} м (не больше {MAX_LEN:.0})")));
                return;
            }
            commands.entity(a).insert(RopeEnd { to: e, length: len.max(0.5), joint: None, bodies: (Entity::PLACEHOLDER, Entity::PLACEHOLDER) });
            toasts.write(toast(format!("Верёвка {len:.1} м")));
            sounds.write(crate::sound::sfx("place"));
        }
    }
}

/// Winches reel; joints follow the ends' grids; ropes whose end is gone go.
fn run(mut commands: Commands, time: Res<Time>, mut ends: Query<(Entity, &BlockEntity, &mut RopeEnd, &Spin)>, blocks: Query<&BlockEntity>, bodies: Query<&RigidBody>, mut joints: Query<&mut DistanceJoint>) {
    let dt = time.delta_secs();
    for (e, b, mut r, spin) in &mut ends {
        let Ok(other) = blocks.get(r.to) else {
            if let Some(j) = r.joint {
                commands.entity(j).despawn();
            }
            commands.entity(e).remove::<RopeEnd>();
            continue;
        };
        if b.mech() == Some(Mech::Winch) {
            // + RPM about the drum axis reels in.
            let ax = crate::mech::side_axis(b);
            let s = (ax.x + ax.y + ax.z) as f32;
            let before = r.length;
            r.length = (r.length - spin.rpm * s / 60.0 * PER_TURN * dt).clamp(0.5, MAX_LEN);
            // A load resting on the ground sleeps: wake it to feel the rope.
            if r.length != before {
                for body in [b.grid, other.grid] {
                    if bodies.get(body).is_ok_and(|x| x.is_dynamic()) {
                        commands.queue(avian3d::dynamics::solver::islands::WakeBody(body));
                    }
                }
            }
        }
        let want = (b.grid, other.grid);
        let both_static = bodies.get(want.0).is_ok_and(|x| x.is_static()) && bodies.get(want.1).is_ok_and(|x| x.is_static());
        if r.bodies != want || r.joint.is_none() {
            if let Some(j) = r.joint.take() {
                commands.entity(j).despawn();
            }
            r.bodies = want;
            if want.0 != want.1 && !both_static {
                let j = commands.spawn((Name::new("Rope joint"), DistanceJoint::new(want.0, want.1).with_local_anchor1(exit_point(b)).with_local_anchor2(exit_point(other)).with_limits(0.0, r.length).with_compliance(0.00002))).id();
                r.joint = Some(j);
            }
        } else if let Some(j) = r.joint
            && let Ok(mut dj) = joints.get_mut(j)
        {
            dj.limits.max = r.length;
            dj.anchor1 = JointAnchor::Local(exit_point(b));
            dj.anchor2 = JointAnchor::Local(exit_point(other));
        }
    }
}

/// The rope as a hanging line of short segments (sags when slack).
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    ends: Query<(Entity, &BlockEntity, &RopeEnd)>,
    blocks: Query<&BlockEntity>,
    tf: Query<&GlobalTransform>,
    mut looks: Query<(Entity, &RopeLook, &Children, &mut Visibility)>,
    mut segs: Query<&mut Transform, Without<RopeLook>>,
    mats: Option<Res<Materials>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
) {
    let Some(mats) = mats else { return };
    for (e, look, ..) in &looks {
        if !ends.contains(look.0) {
            commands.entity(e).despawn();
        }
    }
    for (e, b, r) in &ends {
        let (Ok(o), Some(pa)) = (blocks.get(r.to), world_point(&tf, b)) else { continue };
        let Some(pb) = world_point(&tf, o) else { continue };
        let Some((_, _, children, _)) = looks.iter_mut().find(|l| l.1.0 == e) else {
            let root = commands.spawn((RopeLook(e), Transform::default(), Visibility::default())).id();
            for _ in 0..SEGMENTS {
                let s = commands.spawn((Transform::default(), Visibility::default(), ChildOf(root))).id();
                spawn_parts(&mut commands, s, &[part([0.07, 1.0, 0.07], "leather", [0.0, 0.0, 0.0])], &mut meshes, &mut cache, &mats, true);
            }
            continue;
        };
        let d = pa.distance(pb);
        let sag = ((r.length - d).max(0.0) * 0.45).min(r.length * 0.5);
        let at = |t: f32| pa.lerp(pb, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t);
        for (i, c) in children.iter().enumerate() {
            if let Ok(mut st) = segs.get_mut(c) {
                let (a, bq) = (at(i as f32 / SEGMENTS as f32), at((i + 1) as f32 / SEGMENTS as f32));
                let len = a.distance(bq).max(1e-3);
                *st = Transform::from_translation((a + bq) * 0.5).with_rotation(Quat::from_rotation_arc(Vec3::Y, (bq - a) / len)).with_scale(Vec3::new(1.0, len, 1.0));
            }
        }
    }
}

fn hook_save(w: &World, e: Entity, ctx: &crate::save::SaveCtx) -> Option<String> {
    let r = w.get::<RopeEnd>(e)?;
    let o = w.get::<BlockEntity>(r.to)?;
    crate::save::to_ron(&(*ctx.grid_index.get(&o.grid)?, o.cell, r.length))
}

/// Ropes point at a block entity: wait until the other end exists.
#[derive(Resource, Default)]
struct PendingRopes(Vec<(Entity, Entity, IVec3, f32, u32)>);

fn hook_load(w: &mut World, e: Entity, d: &str, ctx: &crate::save::LoadCtx) {
    if let Some((g, cell, len)) = crate::save::from_ron::<(usize, IVec3, f32)>(d)
        && let Some(&grid) = ctx.grids.get(g)
    {
        w.resource_mut::<PendingRopes>().0.push((e, grid, cell, len, 0));
    }
}

fn resolve_pending(mut commands: Commands, mut p: ResMut<PendingRopes>, grids: Query<&VoxelGrid>) {
    p.0.retain_mut(|(e, g, c, len, tries)| {
        *tries += 1;
        if let Some(to) = grids.get(*g).ok().and_then(|grid| grid.entities.get(c).copied()) {
            commands.entity(*e).insert(RopeEnd { to, length: *len, joint: None, bodies: (Entity::PLACEHOLDER, Entity::PLACEHOLDER) });
            return false;
        }
        *tries < 200
    });
}

pub struct RopesPlugin;

impl Plugin for RopesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RopeStart>()
            .init_resource::<PendingRopes>()
            .init_resource::<crate::save::SaveHooks>()
            .add_systems(Update, (add_parts, crank.after(crate::signal::SignalSet).before(crate::mech::MechSet), use_rope.after(crate::interact::InteractSet), run.after(crate::mech::MechSet), draw, resolve_pending));
        app.world_mut().resource_mut::<crate::save::SaveHooks>().add("rope", hook_save, hook_load);
    }
}

