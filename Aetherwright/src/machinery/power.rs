//! Energy networks. Parts joined by any connection (weld or hinge) form a
//! network; power cells store energy, dynamos produce it, engines consume
//! it. Computed every fixed tick with union-find over the joint graph.

use bevy::prelude::*;

use super::components::{Engine, Part, PowerCell};
use crate::physics::joints::Connection;

/// Groups entities into networks. Returns network id per entity.
pub fn networks(parts: &[Entity], links: &[(Entity, Entity)]) -> Vec<usize> {
    let index = |e: Entity| parts.iter().position(|p| *p == e);
    let mut parent: Vec<usize> = (0..parts.len()).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for &(a, b) in links {
        if let (Some(i), Some(j)) = (index(a), index(b)) {
            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
            if ri != rj {
                parent[ri] = rj;
            }
        }
    }
    (0..parts.len()).map(|i| find(&mut parent, i)).collect()
}

/// Per-network energy summary for UI / scanner.
#[derive(Resource, Default, Debug)]
pub struct PowerStats {
    /// entity → (network energy, network capacity)
    pub by_entity: std::collections::HashMap<Entity, (f32, f32)>,
}

fn update_power(
    time: Res<Time>,
    joints: Query<&Connection>,
    parts: Query<Entity, With<Part>>,
    mut cells: Query<&mut PowerCell>,
    mut engines: Query<&mut Engine>,
    mut stats: ResMut<PowerStats>,
) {
    let dt = time.delta_secs();
    let list: Vec<Entity> = parts.iter().collect();
    let links: Vec<(Entity, Entity)> = joints.iter().map(|c| (c.a, c.b)).collect();
    let net = networks(&list, &links);
    stats.by_entity.clear();
    let mut ids: Vec<usize> = net.clone();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let members: Vec<Entity> = list.iter().zip(&net).filter(|(_, n)| **n == id).map(|(e, _)| *e).collect();
        let stored: f32 = members.iter().filter_map(|e| cells.get(*e).ok()).map(|c| c.energy).sum();
        let capacity: f32 = members.iter().filter_map(|e| cells.get(*e).ok()).map(|c| c.capacity).sum();
        let demand: f32 = members
            .iter()
            .filter_map(|e| engines.get(*e).ok())
            .filter(|en| en.on)
            .map(|en| en.draw * en.throttle * dt)
            .sum();
        let powered = demand <= stored && demand > 0.0;
        if powered {
            // Drain cells proportionally to their charge.
            for e in &members {
                if let Ok(mut c) = cells.get_mut(*e)
                    && stored > 0.0
                {
                    c.energy = (c.energy - demand * c.energy / stored).max(0.0);
                }
            }
        }
        for e in &members {
            if let Ok(mut en) = engines.get_mut(*e) {
                en.powered = powered && en.on;
            }
            stats.by_entity.insert(*e, (stored - if powered { demand } else { 0.0 }, capacity));
        }
    }
}

pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PowerStats>().add_systems(FixedUpdate, update_power.in_set(super::MachinerySet::Power));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_find_groups() {
        let e = |i| Entity::from_raw_u32(i).unwrap();
        let parts = [e(1), e(2), e(3), e(4)];
        let n = networks(&parts, &[(e(1), e(2)), (e(3), e(4)), (e(9), e(1))]);
        assert_eq!(n[0], n[1]);
        assert_eq!(n[2], n[3]);
        assert_ne!(n[0], n[2]);
    }
}
