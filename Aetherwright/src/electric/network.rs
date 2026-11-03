//! Electric network solver — pure data, unit-tested.
//!
//! Power (W = energy per second) moves through a graph: devices are nodes,
//! wires and the internal bus of a welded construction are edges. Each
//! tick, in three stages, with the edge capacities shared between stages:
//!
//! 1. generators → consumers,
//! 2. storage (discharge) → still unmet consumers,
//! 3. left-over generation → storage (charge).
//!
//! Each stage is a max-flow (shortest augmenting paths). A wire carries at
//! most its `capacity`; flow along a path loses `loss` of each wire it
//! crosses, so far consumers need more source power. A big store does not
//! mean big power: discharge is limited by the store's `rate`.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ENode {
    /// Generation available this tick (W).
    pub generation: f32,
    /// Max discharge (W) — already limited by the stored energy.
    pub discharge: f32,
    /// Max charge (W) — already limited by the free room.
    pub charge: f32,
    /// Wanted consumption (W).
    pub demand: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EEdge {
    pub a: usize,
    pub b: usize,
    /// Max power through (W); `f32::INFINITY` for the internal bus.
    pub capacity: f32,
    /// Fraction of the flow lost (0..1).
    pub loss: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EResult {
    /// Delivered to each node's consumers (W).
    pub served: Vec<f32>,
    /// Taken from each node's generation (W).
    pub generated: Vec<f32>,
    /// Taken from storage (W).
    pub discharged: Vec<f32>,
    /// Put into storage (W).
    pub charged: Vec<f32>,
    /// Power through each edge (W, both directions summed).
    pub flow: Vec<f32>,
    /// Power lost in wires (W).
    pub lost: f32,
}

impl EResult {
    pub fn satisfaction(&self, i: usize, demand: f32) -> f32 {
        if demand <= 0.0 { 1.0 } else { (self.served[i] / demand).clamp(0.0, 1.0) }
    }
}

const EPS: f32 = 1e-5;

/// Residual capacities per directed arc (edge e → arcs 2e (a→b), 2e+1 (b→a)).
struct Graph {
    adj: Vec<Vec<(usize, usize)>>, // node → (neighbour, arc)
    residual: Vec<f32>,
    eta: Vec<f32>,
}

impl Graph {
    fn new(n: usize, edges: &[EEdge]) -> Self {
        let mut adj = vec![Vec::new(); n];
        let mut residual = Vec::new();
        let mut eta = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            adj[e.a].push((e.b, 2 * i));
            adj[e.b].push((e.a, 2 * i + 1));
            residual.extend([e.capacity, e.capacity]);
            let k = (1.0 - e.loss).clamp(0.0, 1.0);
            eta.extend([k, k]);
        }
        Self { adj, residual, eta }
    }

    /// Moves power from `supply` nodes to `sink` nodes; both vectors are
    /// reduced by what was used. Returns per-node (taken, delivered).
    fn flow(&mut self, supply: &mut [f32], sink: &mut [f32], res: &mut EResult, as_charge: bool) -> (Vec<f32>, Vec<f32>) {
        let n = supply.len();
        let mut taken = vec![0.0; n];
        let mut got = vec![0.0; n];
        // A node may serve itself directly (device with its own source).
        for i in 0..n {
            let d = supply[i].min(sink[i]);
            if d > EPS {
                supply[i] -= d;
                sink[i] -= d;
                taken[i] += d;
                got[i] += d;
            }
        }
        for _ in 0..256 {
            // BFS from all nodes with supply to the nearest node with sink.
            let mut prev: Vec<Option<(usize, usize)>> = vec![None; n];
            let mut seen = vec![false; n];
            let mut q = VecDeque::new();
            for i in 0..n {
                if supply[i] > EPS {
                    seen[i] = true;
                    q.push_back(i);
                }
            }
            let mut end = None;
            while let Some(u) = q.pop_front() {
                if sink[u] > EPS && !(supply[u] > EPS && prev[u].is_none()) {
                    end = Some(u);
                    break;
                }
                for &(v, arc) in &self.adj[u] {
                    if !seen[v] && self.residual[arc] > EPS {
                        seen[v] = true;
                        prev[v] = Some((u, arc));
                        q.push_back(v);
                    }
                }
            }
            let Some(t) = end else { break };
            // Walk back: path arcs, efficiency, start node.
            let mut arcs = Vec::new();
            let mut v = t;
            while let Some((u, arc)) = prev[v] {
                arcs.push(arc);
                v = u;
            }
            let s = v;
            let eta: f32 = arcs.iter().map(|a| self.eta[*a]).product();
            let mut f = supply[s].min(sink[t] / eta.max(EPS));
            for a in &arcs {
                f = f.min(self.residual[*a]);
            }
            if f <= EPS {
                break;
            }
            for a in &arcs {
                self.residual[*a] -= f;
                // Using one direction does not free the other: wires carry
                // power either way but their capacity is shared.
                self.residual[*a ^ 1] -= f;
                res.flow[*a / 2] += f;
            }
            supply[s] -= f;
            taken[s] += f;
            let delivered = f * eta;
            sink[t] -= delivered;
            got[t] += delivered;
            res.lost += f - delivered;
        }
        let _ = as_charge;
        (taken, got)
    }
}

pub fn solve(nodes: &[ENode], edges: &[EEdge]) -> EResult {
    let n = nodes.len();
    let mut res = EResult {
        served: vec![0.0; n],
        generated: vec![0.0; n],
        discharged: vec![0.0; n],
        charged: vec![0.0; n],
        flow: vec![0.0; edges.len()],
        lost: 0.0,
    };
    let mut g = Graph::new(n, edges);
    let mut generation: Vec<f32> = nodes.iter().map(|x| x.generation.max(0.0)).collect();
    let mut demand: Vec<f32> = nodes.iter().map(|x| x.demand.max(0.0)).collect();
    // 1. generation → consumers
    let (taken, got) = g.flow(&mut generation, &mut demand, &mut res, false);
    for i in 0..n {
        res.generated[i] += taken[i];
        res.served[i] += got[i];
    }
    // 2. storage → consumers
    let mut discharge: Vec<f32> = nodes.iter().map(|x| x.discharge.max(0.0)).collect();
    let (taken, got) = g.flow(&mut discharge, &mut demand, &mut res, false);
    for i in 0..n {
        res.discharged[i] += taken[i];
        res.served[i] += got[i];
    }
    // 3. surplus generation → storage
    let mut room: Vec<f32> = nodes.iter().map(|x| x.charge.max(0.0)).collect();
    let (taken, got) = g.flow(&mut generation, &mut room, &mut res, true);
    for i in 0..n {
        res.generated[i] += taken[i];
        res.charged[i] += got[i];
    }
    res
}

/// Connected components by edges (for grouping in UI / overlays).
pub fn components(n: usize, edges: &[EEdge]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for e in edges {
        let (a, b) = (find(&mut parent, e.a), find(&mut parent, e.b));
        if a != b {
            parent[a] = b;
        }
    }
    (0..n).map(|i| find(&mut parent, i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(a: usize, b: usize, capacity: f32, loss: f32) -> EEdge {
        EEdge { a, b, capacity, loss }
    }

    #[test]
    fn generator_feeds_consumer_through_a_wire() {
        let nodes = [ENode { generation: 10.0, ..default() }, ENode { demand: 6.0, ..default() }];
        let r = solve(&nodes, &[wire(0, 1, 100.0, 0.0)]);
        assert!((r.served[1] - 6.0).abs() < 1e-3);
        assert!((r.generated[0] - 6.0).abs() < 1e-3);
        // Without a wire nothing arrives.
        let r = solve(&nodes, &[]);
        assert_eq!(r.served[1], 0.0);
    }

    #[test]
    fn wire_capacity_limits_power_and_losses_cost_extra() {
        let nodes = [ENode { generation: 100.0, ..default() }, ENode { demand: 50.0, ..default() }];
        let r = solve(&nodes, &[wire(0, 1, 20.0, 0.0)]);
        assert!((r.served[1] - 20.0).abs() < 1e-3, "{r:?}");
        assert!((r.flow[0] - 20.0).abs() < 1e-3);
        let r = solve(&nodes, &[wire(0, 1, 100.0, 0.1)]);
        assert!((r.served[1] - 50.0).abs() < 1e-2);
        assert!(r.generated[0] > 55.0 && r.lost > 5.0, "{r:?}");
    }

    #[test]
    fn storage_fills_the_gap_and_surplus_charges() {
        // gen 4, demand 10: 6 from the cell. Then gen 20, demand 5: 15 charges.
        let nodes = [
            ENode { generation: 4.0, ..default() },
            ENode { discharge: 50.0, charge: 50.0, ..default() },
            ENode { demand: 10.0, ..default() },
        ];
        let edges = [wire(0, 1, 100.0, 0.0), wire(1, 2, 100.0, 0.0)];
        let r = solve(&nodes, &edges);
        assert!((r.served[2] - 10.0).abs() < 1e-3);
        assert!((r.discharged[1] - 6.0).abs() < 1e-3);
        let nodes = [
            ENode { generation: 20.0, ..default() },
            ENode { discharge: 50.0, charge: 50.0, ..default() },
            ENode { demand: 5.0, ..default() },
        ];
        let r = solve(&nodes, &edges);
        assert!((r.charged[1] - 15.0).abs() < 1e-3, "{r:?}");
        assert_eq!(r.discharged[1], 0.0);
    }

    #[test]
    fn big_store_low_rate_is_not_big_power() {
        let nodes = [ENode { discharge: 5.0, ..default() }, ENode { demand: 30.0, ..default() }];
        let r = solve(&nodes, &[wire(0, 1, f32::INFINITY, 0.0)]);
        assert!((r.served[1] - 5.0).abs() < 1e-3);
        assert!(r.satisfaction(1, 30.0) < 0.2);
    }

    #[test]
    fn shared_wire_splits_between_consumers() {
        // 0 —(10 W)— 1 ; 1 feeds two consumers 2 and 3 over the internal bus.
        let nodes = [
            ENode { generation: 100.0, ..default() },
            ENode::default(),
            ENode { demand: 8.0, ..default() },
            ENode { demand: 8.0, ..default() },
        ];
        let edges = [wire(0, 1, 10.0, 0.0), wire(1, 2, f32::INFINITY, 0.0), wire(1, 3, f32::INFINITY, 0.0)];
        let r = solve(&nodes, &edges);
        let total = r.served[2] + r.served[3];
        assert!((total - 10.0).abs() < 1e-3, "{r:?}");
        let c = components(4, &edges);
        assert!(c.iter().all(|x| *x == c[0]));
    }

    fn default() -> ENode {
        ENode::default()
    }
}
