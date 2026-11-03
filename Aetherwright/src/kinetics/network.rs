//! Kinetic network solver — pure data, no ECS (unit-tested).
//!
//! Like rotation in Create, rotation is not simulated by physics: grid
//! nodes (shafts, cogs, gearboxes…) form a graph; a signed speed (RPM about
//! the world `+axis` of the node) propagates through links with fixed
//! ratios. Model (documented in `docs/KINETICS.md`):
//!
//! * **Links.** Axial: two nodes facing each other with ports (shaft ends,
//!   gearbox faces). Mesh: cogs with the same axis side by side (small–small
//!   −1, small–large diagonal −½ / −2, large–large two cells apart −1).
//! * **Sources** want a speed and offer a *power* (SU, stress units).
//!   All sources of a network must agree on the base speed, otherwise the
//!   network is in **conflict** and stops.
//! * **Load.** Every node has a friction impact and consumers an impact,
//!   both in SU per RPM (power ∝ speed). Demand `D = Σ impact·|rpm|`.
//!   If `D > P` the network slows to `P/D` of its speed (power balance);
//!   below `STALL_FACTOR` it stalls.
//! * **Strength.** The transmitted power is `min(D, P)`; a node weaker than
//!   that breaks (the weakest one per solve).

use std::collections::{HashMap, VecDeque};

use bevy::math::IVec3;

pub const MAX_RPM: f32 = 256.0;
pub const STALL_FACTOR: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default)]
pub enum Axis {
    X,
    #[default]
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
    pub fn unit(self) -> IVec3 {
        match self {
            Axis::X => IVec3::X,
            Axis::Y => IVec3::Y,
            Axis::Z => IVec3::Z,
        }
    }
    pub fn of(d: IVec3) -> Option<Axis> {
        match (d.x != 0, d.y != 0, d.z != 0) {
            (true, false, false) => Some(Axis::X),
            (false, true, false) => Some(Axis::Y),
            (false, false, true) => Some(Axis::Z),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}

/// Behaviour class of a node (parameters come from data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Role {
    Shaft,
    Cog,
    LargeCog,
    /// Bevel box: all six faces; each face spins the same way seen from
    /// outside, so a straight pass-through reverses.
    Gearbox,
    /// Shaft that can be disengaged.
    Clutch,
    /// Shaft that can reverse one side.
    Gearshift,
    /// Source driven by the player.
    Crank,
    /// Source driven by electric power.
    Motor,
    /// Drives a physics body hinged to it (boundary to physics).
    Bearing,
    /// A machine taking rotation from any face.
    Consumer,
    /// Occupies a cell, no rotation (magical devices, displays…).
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub role: Role,
    pub axis: Axis,
    /// SU per RPM consumed by friction / work.
    pub impact: f32,
    /// Max transmitted power (SU); INFINITY = unbreakable.
    pub strength: f32,
    /// Source: wanted RPM (signed, about +axis) and available power (SU).
    pub source: Option<(f32, f32)>,
    /// Clutch disengaged / gearshift reversed.
    pub toggled: bool,
}

impl Node {
    pub fn new(role: Role, axis: Axis) -> Self {
        Self { role, axis, impact: 0.0, strength: f32::INFINITY, source: None, toggled: false }
    }
    fn axial(&self) -> bool {
        !matches!(self.role, Role::Gearbox | Role::Consumer)
    }
    /// Faces through which the node connects axially.
    fn ports(&self) -> Vec<IVec3> {
        if !self.axial() {
            return vec![IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
        }
        if (self.role == Role::Clutch && self.toggled) || self.role == Role::Static {
            return vec![];
        }
        let u = self.axis.unit();
        vec![u, -u]
    }
    /// Signed speed (about world +axis of `d`) passed out through face `d`
    /// for node speed `s`.
    fn face_out(&self, s: f32, d: IVec3) -> f32 {
        if self.axial() {
            let neg_side = d.x + d.y + d.z < 0;
            if self.role == Role::Gearshift && self.toggled && neg_side {
                -s
            } else {
                s
            }
        } else {
            s * (d.x + d.y + d.z) as f32
        }
    }
    /// Node speed for a speed `v` (about world +axis) arriving at face `d`.
    fn face_in(&self, v: f32, d: IVec3) -> f32 {
        // Both mappings are their own inverse (±1 factors).
        self.face_out(v, d)
    }
    fn cog_radius(&self) -> Option<f32> {
        match self.role {
            Role::Cog => Some(0.5),
            Role::LargeCog => Some(1.0),
            _ => None,
        }
    }
}

/// Neighbours of `c` with the ratio `speed_neighbour = r · speed_c`.
pub fn links(grid: &HashMap<IVec3, Node>, c: IVec3) -> Vec<(IVec3, f32)> {
    let Some(a) = grid.get(&c) else {
        return vec![];
    };
    let mut out = Vec::new();
    for d in a.ports() {
        let n = c + d;
        if let Some(b) = grid.get(&n)
            && b.ports().contains(&-d)
        {
            // Axis along d: a pushes v through d, b receives it on face −d.
            let v = a.face_out(1.0, d);
            out.push((n, b.face_in(v, -d)));
        }
    }
    if let Some(ra) = a.cog_radius() {
        let u = a.axis.unit();
        for b_off in mesh_offsets(a.axis, a.role) {
            let n = c + b_off;
            if let Some(b) = grid.get(&n)
                && b.axis == a.axis
                && let Some(rb) = b.cog_radius()
            {
                let dist = (b_off.as_vec3() - u.as_vec3() * b_off.dot(u) as f32).length();
                if (dist - (ra + rb)).abs() < 0.2 {
                    out.push((n, -ra / rb));
                }
            }
        }
    }
    out
}

/// Candidate mesh partner offsets in the plane ⟂ axis.
fn mesh_offsets(axis: Axis, role: Role) -> Vec<IVec3> {
    let (p, q) = match axis {
        Axis::X => (IVec3::Y, IVec3::Z),
        Axis::Y => (IVec3::X, IVec3::Z),
        Axis::Z => (IVec3::X, IVec3::Y),
    };
    let mut v = vec![p, -p, q, -q, p + q, p - q, -p + q, -p - q];
    if role == Role::LargeCog {
        v.extend([p * 2, -p * 2, q * 2, -q * 2]);
    }
    v
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetInfo {
    pub members: Vec<IVec3>,
    /// Available power (SU).
    pub capacity: f32,
    /// Demand at full speed (SU).
    pub demand: f32,
    /// Speed factor after the power balance (0 = stopped).
    pub factor: f32,
    pub conflict: bool,
    pub stalled: bool,
    /// Base speed was limited to keep every node ≤ MAX_RPM.
    pub limited: bool,
}

impl NetInfo {
    pub fn load(&self) -> f32 {
        if self.capacity > 0.0 { self.demand / self.capacity } else if self.demand > 0.0 { f32::INFINITY } else { 0.0 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Solution {
    pub speed: HashMap<IVec3, f32>,
    pub net_of: HashMap<IVec3, usize>,
    pub nets: Vec<NetInfo>,
    /// Nodes that break this solve (transmitted power above strength).
    pub broken: Vec<IVec3>,
}

pub fn solve(grid: &HashMap<IVec3, Node>) -> Solution {
    let mut sol = Solution::default();
    let mut cells: Vec<IVec3> = grid.keys().copied().collect();
    cells.sort_by_key(|c| (c.x, c.y, c.z));
    let mut rel: HashMap<IVec3, f32> = HashMap::new();
    for &start in &cells {
        if rel.contains_key(&start) {
            continue;
        }
        let id = sol.nets.len();
        let mut info = NetInfo::default();
        let mut q = VecDeque::from([start]);
        rel.insert(start, 1.0);
        while let Some(c) = q.pop_front() {
            info.members.push(c);
            sol.net_of.insert(c, id);
            let s = rel[&c];
            for (n, r) in links(grid, c) {
                // Links must be mutual (both nodes agree they connect).
                if !links(grid, n).iter().any(|(m, _)| *m == c) {
                    continue;
                }
                let want = s * r;
                match rel.get(&n) {
                    Some(&have) => {
                        if (have - want).abs() > 1e-3 * want.abs().max(1.0) {
                            info.conflict = true;
                        }
                    }
                    None => {
                        rel.insert(n, want);
                        q.push_back(n);
                    }
                }
            }
        }
        // Sources fix the base speed.
        let mut base: Option<f32> = None;
        for c in &info.members {
            let node = &grid[c];
            if let Some((rpm, power)) = node.source {
                info.capacity += power.max(0.0);
                if rpm == 0.0 || power <= 0.0 {
                    continue;
                }
                let b = rpm / rel[c];
                match base {
                    Some(x) if (x - b).abs() > 1e-3 * b.abs().max(1.0) => info.conflict = true,
                    _ => base = Some(b),
                }
            }
        }
        let mut base = base.unwrap_or(0.0);
        let max_rel = info.members.iter().map(|c| rel[c].abs()).fold(0.0, f32::max);
        if base.abs() * max_rel > MAX_RPM {
            base = base.signum() * MAX_RPM / max_rel;
            info.limited = true;
        }
        info.demand = info.members.iter().map(|c| grid[c].impact * (base * rel[c]).abs()).sum();
        info.factor = if info.conflict || base == 0.0 {
            0.0
        } else if info.demand <= info.capacity {
            1.0
        } else {
            info.capacity / info.demand
        };
        if info.factor > 0.0 && info.factor < STALL_FACTOR {
            info.factor = 0.0;
            info.stalled = true;
        }
        for c in &info.members {
            sol.speed.insert(*c, base * rel[c] * info.factor);
        }
        // Strength: the weakest node that cannot carry the power breaks.
        let transmitted = info.demand.min(info.capacity) * if info.factor > 0.0 { 1.0 } else { 0.0 };
        if let Some(c) = info
            .members
            .iter()
            .filter(|c| grid[c].strength < transmitted)
            .min_by(|a, b| grid[a].strength.total_cmp(&grid[b].strength))
        {
            sol.broken.push(*c);
        }
        sol.nets.push(info);
    }
    sol
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaft(axis: Axis) -> Node {
        Node { impact: 0.1, ..Node::new(Role::Shaft, axis) }
    }
    fn motor(axis: Axis, rpm: f32, power: f32) -> Node {
        Node { source: Some((rpm, power)), ..Node::new(Role::Motor, axis) }
    }

    #[test]
    fn shaft_line_carries_speed() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, motor(Axis::X, 64.0, 1000.0));
        for x in 1..5 {
            g.insert(IVec3::new(x, 0, 0), shaft(Axis::X));
        }
        // A shaft on another axis does not connect.
        g.insert(IVec3::new(5, 0, 0), shaft(Axis::Y));
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::new(4, 0, 0)], 64.0);
        assert_eq!(s.speed[&IVec3::new(5, 0, 0)], 0.0);
        assert_ne!(s.net_of[&IVec3::ZERO], s.net_of[&IVec3::new(5, 0, 0)]);
    }

    #[test]
    fn cogs_reverse_and_gear_ratio() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, motor(Axis::Y, 32.0, 1000.0));
        g.insert(IVec3::Y, Node::new(Role::Cog, Axis::Y));
        g.insert(IVec3::new(1, 1, 0), Node::new(Role::Cog, Axis::Y));
        g.insert(IVec3::new(2, 1, 1), Node::new(Role::LargeCog, Axis::Y));
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::Y], 32.0);
        assert_eq!(s.speed[&IVec3::new(1, 1, 0)], -32.0, "meshing cogs reverse");
        assert_eq!(s.speed[&IVec3::new(2, 1, 1)], 16.0, "small → large halves the speed");
    }

    #[test]
    fn gearbox_and_clutch() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, motor(Axis::X, 16.0, 1000.0));
        g.insert(IVec3::X, Node::new(Role::Gearbox, Axis::Y));
        g.insert(IVec3::new(1, 1, 0), shaft(Axis::Y)); // turns the corner
        g.insert(IVec3::new(2, 0, 0), shaft(Axis::X)); // straight through
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::new(1, 1, 0)].abs(), 16.0);
        assert_eq!(s.speed[&IVec3::new(2, 0, 0)], -16.0, "straight through a gearbox reverses");
        g.insert(IVec3::new(3, 0, 0), Node { toggled: true, ..Node::new(Role::Clutch, Axis::X) });
        g.insert(IVec3::new(4, 0, 0), shaft(Axis::X));
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::new(4, 0, 0)], 0.0, "open clutch separates");
    }

    #[test]
    fn conflicting_sources_stop_the_network() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, motor(Axis::X, 32.0, 100.0));
        g.insert(IVec3::X, shaft(Axis::X));
        g.insert(IVec3::new(2, 0, 0), motor(Axis::X, 64.0, 100.0));
        let s = solve(&g);
        assert!(s.nets[0].conflict);
        assert_eq!(s.speed[&IVec3::X], 0.0);
        // Same speed: they add their power.
        g.insert(IVec3::new(2, 0, 0), motor(Axis::X, 32.0, 100.0));
        let s = solve(&g);
        assert!(!s.nets[0].conflict);
        assert_eq!(s.nets[0].capacity, 200.0);
    }

    #[test]
    fn overload_slows_then_stalls_and_strength_breaks() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, motor(Axis::X, 64.0, 128.0));
        g.insert(IVec3::X, Node { impact: 4.0, ..Node::new(Role::Consumer, Axis::Y) });
        // Demand 4·64 = 256 > 128 → half speed.
        let s = solve(&g);
        assert!((s.nets[0].factor - 0.5).abs() < 1e-4);
        assert!((s.speed[&IVec3::ZERO] - 32.0).abs() < 1e-3);
        // Much more load → stall.
        g.insert(IVec3::X, Node { impact: 40.0, ..Node::new(Role::Consumer, Axis::Y) });
        let s = solve(&g);
        assert!(s.nets[0].stalled && s.speed[&IVec3::ZERO] == 0.0);
        // A weak shaft between them breaks when the power is transmitted.
        g.insert(IVec3::X, Node { strength: 50.0, ..shaft(Axis::X) });
        g.insert(IVec3::new(2, 0, 0), Node { impact: 1.0, ..Node::new(Role::Consumer, Axis::Y) });
        let s = solve(&g);
        assert_eq!(s.broken, vec![IVec3::X]);
    }

    #[test]
    fn diagonal_small_cogs_do_not_mesh() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, Node::new(Role::Cog, Axis::Y));
        g.insert(IVec3::new(1, 0, 1), Node::new(Role::Cog, Axis::Y));
        g.insert(IVec3::new(1, 0, 0), Node::new(Role::LargeCog, Axis::Y));
        assert!(links(&g, IVec3::ZERO).is_empty());
    }

    #[test]
    fn rpm_is_limited() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, motor(Axis::Y, 200.0, 1000.0));
        g.insert(IVec3::Y, Node::new(Role::LargeCog, Axis::Y));
        g.insert(IVec3::new(1, 1, 1), Node::new(Role::Cog, Axis::Y));
        let s = solve(&g);
        assert!(s.nets[0].limited);
        assert!(s.speed.values().all(|v| v.abs() <= MAX_RPM + 1e-3));
    }
}
