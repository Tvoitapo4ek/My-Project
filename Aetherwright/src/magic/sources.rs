//! Aether sources (magical blocks in the world) and accumulators.
//!
//! * `AetherSources` indexes every block that emits aether; kept up to date
//!   on block changes.
//! * `AetherStore` (on aether jars) charges from the field and shares with
//!   other jars within `LINK_RANGE` (visible beams): aether flows from the
//!   fuller jar to the emptier one.
//! * The aether dynamo drains nearby jars and feeds its energy network —
//!   the bridge between magic and engineering.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::energy::{AetherKind, sample_field};
use crate::{
    machinery::components::{AetherDynamo, Part, PartKind, PowerCell},
    physics::joints::Connection,
    world::{WorldReady, chunks::VoxelWorld, interaction::BlockChanged, voxel::BlockId},
};

pub const LINK_RANGE: f32 = 7.0;

#[derive(Resource, Default, Debug)]
pub struct AetherSources(pub Vec<(Vec3, AetherKind, f32)>);

impl AetherSources {
    pub fn sample(&self, p: Vec3) -> super::energy::FieldSample {
        sample_field(&self.0, p)
    }
}

/// Kind of magic a block emits (`aether_kind` in blocks.ron; default calm).
pub fn kind_of(b: BlockId) -> AetherKind {
    b.def().aether_kind.unwrap_or_default()
}

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct AetherStore {
    pub amount: f32,
    pub capacity: f32,
    pub kind: AetherKind,
}

pub fn rebuild_sources(
    world: Res<VoxelWorld>,
    mut sources: ResMut<AetherSources>,
    mut ready: MessageReader<WorldReady>,
) {
    if ready.read().count() == 0 {
        return;
    }
    sources.0.clear();
    for (c, chunk) in &world.chunks {
        if chunk.non_air == 0 {
            continue;
        }
        let origin = *c * world.chunk_size;
        for (i, b) in chunk.blocks.iter().enumerate() {
            let a = b.def().aether;
            if a > 0.0 {
                let n = world.chunk_size as usize;
                let (x, z, y) = (i % n, (i / n) % n, i / (n * n));
                let p = origin + IVec3::new(x as i32, y as i32, z as i32);
                sources
                    .0
                    .push((p.as_vec3() + Vec3::splat(0.5), kind_of(*b), a));
            }
        }
    }
    info!("Aether sources: {}", sources.0.len());
}

fn track_changes(mut changed: MessageReader<BlockChanged>, mut sources: ResMut<AetherSources>) {
    for c in changed.read() {
        let center = c.pos.as_vec3() + Vec3::splat(0.5);
        if c.old.def().aether > 0.0 {
            sources
                .0
                .retain(|(p, _, _)| p.distance_squared(center) > 0.01);
        }
        if c.new.def().aether > 0.0 {
            sources.0.push((center, kind_of(c.new), c.new.def().aether));
        }
    }
}

fn add_stores(mut commands: Commands, parts: Query<(Entity, &Part), Added<Part>>) {
    for (e, p) in &parts {
        if p.kind == PartKind::AetherJar {
            commands.entity(e).insert(AetherStore {
                amount: 0.0,
                capacity: 100.0,
                kind: AetherKind::Calm,
            });
        }
    }
}

/// Jars collect from the field.
fn charge_jars(
    time: Res<Time>,
    sources: Res<AetherSources>,
    mut jars: Query<(&Position, &mut AetherStore)>,
) {
    let dt = time.delta_secs();
    for (p, mut s) in &mut jars {
        let f = sources.sample(p.0);
        s.amount = (s.amount + f.strength * 0.6 * dt).min(s.capacity);
        if f.strength > super::energy::BACKGROUND * 1.5 {
            s.kind = f.kind;
        }
    }
}

/// Visible links between jars and from jars to dynamos.
#[derive(Resource, Default, Debug)]
pub struct AetherLinks(pub Vec<(Entity, Entity)>);

fn transfer(
    time: Res<Time>,
    mut links: ResMut<AetherLinks>,
    mut jars: Query<(Entity, &Position, &mut AetherStore)>,
    mut dynamos: Query<(Entity, &Position, &mut AetherDynamo)>,
    joints: Query<&Connection>,
    parts: Query<Entity, With<Part>>,
    mut cells: Query<&mut PowerCell>,
) {
    let dt = time.delta_secs();
    links.0.clear();
    let list: Vec<(Entity, Vec3, f32)> = jars
        .iter()
        .map(|(e, p, s)| (e, p.0, s.amount / s.capacity))
        .collect();
    // Jar ↔ jar balancing.
    for i in 0..list.len() {
        for j in i + 1..list.len() {
            let (a, pa, fa) = list[i];
            let (b, pb, fb) = list[j];
            if pa.distance(pb) > LINK_RANGE {
                continue;
            }
            links.0.push((a, b));
            let flow = ((fa - fb) * 20.0 * dt).clamp(-2.0, 2.0);
            if flow.abs() < 1e-4 {
                continue;
            }
            if let Ok([(_, _, mut sa), (_, _, mut sb)]) = jars.get_many_mut([a, b]) {
                let amount = flow.min(sa.amount).max(-sb.amount);
                sa.amount -= amount;
                sb.amount += amount;
            }
        }
    }
    // Dynamos: aether → energy into their network's cells.
    let all_parts: Vec<Entity> = parts.iter().collect();
    let pairs: Vec<(Entity, Entity)> = joints.iter().map(|c| (c.a, c.b)).collect();
    let nets = crate::machinery::power::networks(&all_parts, &pairs);
    for (d, dp, mut dynamo) in &mut dynamos {
        let mut produced = 0.0;
        for (j, jp, _) in &list {
            if dp.0.distance(*jp) > LINK_RANGE {
                continue;
            }
            links.0.push((*j, d));
            if let Ok((_, _, mut s)) = jars.get_mut(*j) {
                let take = (8.0 * dt).min(s.amount);
                s.amount -= take;
                produced += take * 3.0 * s.kind.potency();
            }
        }
        dynamo.output = produced / dt.max(1e-4);
        if produced <= 0.0 {
            continue;
        }
        let Some(di) = all_parts.iter().position(|e| *e == d) else {
            continue;
        };
        let my_net = nets[di];
        let mut targets: Vec<Entity> = all_parts
            .iter()
            .zip(&nets)
            .filter(|(_, n)| **n == my_net)
            .map(|(e, _)| *e)
            .collect();
        targets.retain(|e| cells.get(*e).is_ok_and(|c| c.energy < c.capacity));
        let share = produced / targets.len().max(1) as f32;
        for e in targets {
            if let Ok(mut c) = cells.get_mut(e) {
                c.energy = (c.energy + share).min(c.capacity);
            }
        }
    }
}

fn draw_links(
    links: Res<AetherLinks>,
    q: Query<(&Position, Option<&AetherStore>)>,
    time: Res<Time>,
    mut gizmos: Gizmos,
) {
    let t = time.elapsed_secs();
    for (a, b) in &links.0 {
        let (Ok((pa, sa)), Ok((pb, _))) = (q.get(*a), q.get(*b)) else {
            continue;
        };
        let kind = sa.map(|s| s.kind).unwrap_or_default();
        let c = kind.color().with_alpha(0.5 + 0.3 * (t * 6.0).sin());
        let mid = (pa.0 + pb.0) * 0.5 + Vec3::Y * 0.4;
        gizmos.line(pa.0, mid, c);
        gizmos.line(mid, pb.0, c);
    }
}

pub struct SourcesPlugin;

impl Plugin for SourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AetherSources>()
            .init_resource::<AetherLinks>()
            .add_systems(
                Update,
                (rebuild_sources, track_changes, add_stores, draw_links),
            )
            .add_systems(
                FixedUpdate,
                (charge_jars, transfer)
                    .chain()
                    .before(crate::machinery::MachinerySet::Power),
            );
    }
}
