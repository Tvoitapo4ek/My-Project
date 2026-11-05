//! Kinetic network solver — pure data, no ECS (unit-tested).
//!
//! Rotation is not simulated by physics (like Create): mechanism cells form
//! a graph and a signed speed (RPM about the world `+axis` of the node)
//! propagates through links with fixed ratios.
//!
//! * **Axial links**: two nodes whose ports face each other (shaft ends,
//!   motor output, gearbox sides).
//! * **Meshing cogs** with the same axis: small–small side by side −1,
//!   small–large diagonal −½ / −2, large–large two cells apart −1.
//! * **Sources** (creative motors, speed-controller outputs) want a speed.
//!   All sources of a network must agree, otherwise it is in **conflict**
//!   and stops. Creative sources have unlimited power.
//! * Speeds are limited to `MAX_RPM`.
//! * **Stress** (like Create): every node uses `stress × |RPM| / 32` SU,
//!   sources give `capacity` SU. A network asking more than it gets slows
//!   down by `capacity / stress`; below `STALL` it stops (overstressed).

use std::collections::{HashMap, VecDeque};

use bevy::math::IVec3;

pub const MAX_RPM: f32 = 256.0;
/// Speed factor below which an overloaded network stops.
pub const STALL: f32 = 0.2;
/// Stress is quoted per 32 RPM.
pub const STRESS_RPM: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Axis {
    X,
    #[default]
    Y,
    Z,
}

impl Axis {
    pub fn unit(self) -> IVec3 {
        match self {
            Axis::X => IVec3::X,
            Axis::Y => IVec3::Y,
            Axis::Z => IVec3::Z,
        }
    }
    pub fn of(d: IVec3) -> Axis {
        if d.x != 0 {
            Axis::X
        } else if d.y != 0 {
            Axis::Y
        } else {
            Axis::Z
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Axial: passes rotation through its two ends.
    Shaft,
    Cog,
    LargeCog,
    /// Four ports around its axis; each port spins the same way seen from
    /// outside, so a straight pass-through reverses (like Create).
    Gearbox,
    /// Axial; disconnected while `toggled`.
    Clutch,
    /// Axial; the side towards −axis is reversed while `toggled`.
    Gearshift,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    pub role: Role,
    pub axis: Axis,
    /// Only this port connects (motors, speed-controller sides); None = all
    /// ports of the role.
    pub only: Option<IVec3>,
    /// Source: wanted RPM about world +axis.
    pub source: Option<f32>,
    pub toggled: bool,
    /// Stress at 32 RPM (SU).
    pub stress: f32,
    /// Power of a source (SU).
    pub capacity: f32,
}

impl Node {
    pub fn new(role: Role, axis: Axis) -> Self {
        Self { role, axis, only: None, source: None, toggled: false, stress: 0.0, capacity: 0.0 }
    }
    pub fn stress(mut self, su: f32) -> Self {
        self.stress = su;
        self
    }
    pub fn capacity(mut self, su: f32) -> Self {
        self.capacity = su;
        self
    }
    pub fn one_port(mut self, d: IVec3) -> Self {
        self.only = Some(d);
        self
    }
    pub fn source(mut self, rpm: f32) -> Self {
        self.source = Some(rpm);
        self
    }
    fn ports(&self) -> Vec<IVec3> {
        let u = self.axis.unit();
        let all = match self.role {
            Role::Gearbox => {
                let mut v = Vec::new();
                for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                    if d.dot(u) == 0 {
                        v.push(d);
                    }
                }
                v
            }
            Role::Clutch if self.toggled => vec![],
            _ => vec![u, -u],
        };
        match self.only {
            Some(d) => all.into_iter().filter(|p| *p == d).collect(),
            None => all,
        }
    }
    /// Signed speed (about world +axis of `d`) passed out through face `d`.
    fn face_out(&self, s: f32, d: IVec3) -> f32 {
        match self.role {
            Role::Gearbox => s * (d.x + d.y + d.z) as f32,
            Role::Gearshift if self.toggled && d.x + d.y + d.z < 0 => -s,
            _ => s,
        }
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
    let Some(a) = grid.get(&c) else { return vec![] };
    let mut out = Vec::new();
    for d in a.ports() {
        let n = c + d;
        if let Some(b) = grid.get(&n)
            && b.ports().contains(&-d)
        {
            let v = a.face_out(1.0, d);
            out.push((n, b.face_out(v, -d)));
        }
    }
    if let Some(ra) = a.cog_radius() {
        let u = a.axis.unit();
        for off in mesh_offsets(a.axis, a.role) {
            let n = c + off;
            if let Some(b) = grid.get(&n)
                && b.axis == a.axis
                && let Some(rb) = b.cog_radius()
            {
                let dist = (off.as_vec3() - u.as_vec3() * off.dot(u) as f32).length();
                if (dist - (ra + rb)).abs() < 0.2 {
                    out.push((n, -ra / rb));
                }
            }
        }
    }
    out
}

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
    /// The network has a source (even one asking for 0 RPM): its wheels
    /// are held by the drivetrain instead of rolling freely.
    pub driven: bool,
    pub conflict: bool,
    pub limited: bool,
    /// Power of its sources and what its nodes use at these speeds (SU).
    pub capacity: f32,
    pub stress: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Solution {
    pub speed: HashMap<IVec3, f32>,
    pub net_of: HashMap<IVec3, usize>,
    pub nets: Vec<NetInfo>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn solve(grid: &HashMap<IVec3, Node>) -> Solution {
    solve_with(grid, &[])
}

/// Like `solve`, with extra links between distant nodes (chains, belts,
/// track rows): `(a, b, r)` means `speed_b = r · speed_a`.
pub fn solve_with(grid: &HashMap<IVec3, Node>, extra: &[(IVec3, IVec3, f32)]) -> Solution {
    let mut more: HashMap<IVec3, Vec<(IVec3, f32)>> = HashMap::new();
    for &(a, b, r) in extra {
        if grid.contains_key(&a) && grid.contains_key(&b) && r != 0.0 {
            more.entry(a).or_default().push((b, r));
            more.entry(b).or_default().push((a, 1.0 / r));
        }
    }
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
            let mut near: Vec<(IVec3, f32)> = links(grid, c).into_iter().filter(|(n, _)| links(grid, *n).iter().any(|(m, _)| *m == c)).collect();
            near.extend(more.get(&c).cloned().unwrap_or_default());
            for (n, r) in near {
                let want = s * r;
                match rel.get(&n) {
                    Some(&have) if (have - want).abs() > 1e-3 * want.abs().max(1.0) => info.conflict = true,
                    Some(_) => {}
                    None => {
                        rel.insert(n, want);
                        q.push_back(n);
                    }
                }
            }
        }
        let mut base: Option<f32> = None;
        info.driven = info.members.iter().any(|c| grid[c].source.is_some());
        for c in &info.members {
            if let Some(rpm) = grid[c].source
                && rpm != 0.0
            {
                let b = rpm / rel[c];
                match base {
                    Some(x) if (x - b).abs() > 1e-3 * b.abs().max(1.0) => info.conflict = true,
                    _ => base = Some(b),
                }
            }
        }
        let mut base = if info.conflict { 0.0 } else { base.unwrap_or(0.0) };
        let max_rel = info.members.iter().map(|c| rel[c].abs()).fold(0.0, f32::max);
        if base.abs() * max_rel > MAX_RPM {
            base = base.signum() * MAX_RPM / max_rel;
            info.limited = true;
        }
        for c in &info.members {
            let v = base * rel[c];
            sol.speed.insert(*c, v);
            info.stress += grid[c].stress * v.abs() / STRESS_RPM;
            if grid[c].source.is_some() {
                info.capacity += grid[c].capacity;
            }
        }
        sol.nets.push(info);
    }
    sol
}

/// Speed factor of a network (or of networks joined through speed
/// controllers): 1 while it copes, `capacity / stress` when overloaded,
/// 0 below `STALL`.
pub fn factor(capacity: f32, stress: f32) -> f32 {
    if stress <= capacity || stress <= 1e-6 {
        1.0
    } else {
        let k = capacity / stress;
        if k < STALL { 0.0 } else { k }
    }
}

/// Multiplies the speeds of the given networks by `k`.
pub fn scale(sol: &mut Solution, nets: &[usize], k: f32) {
    if k == 1.0 {
        return;
    }
    for &n in nets {
        for c in sol.nets[n].members.clone() {
            if let Some(v) = sol.speed.get_mut(&c) {
                *v *= k;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaft(axis: Axis) -> Node {
        Node::new(Role::Shaft, axis)
    }

    #[test]
    fn motor_drives_a_shaft_line_from_its_output_only() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).one_port(IVec3::X).source(64.0));
        for x in 1..5 {
            g.insert(IVec3::new(x, 0, 0), shaft(Axis::X));
        }
        g.insert(IVec3::NEG_X, shaft(Axis::X)); // behind the motor
        g.insert(IVec3::new(5, 0, 0), shaft(Axis::Y));
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::new(4, 0, 0)], 64.0);
        assert_eq!(s.speed[&IVec3::NEG_X], 0.0);
        assert_eq!(s.speed[&IVec3::new(5, 0, 0)], 0.0);
    }

    #[test]
    fn cogs_reverse_and_change_ratio() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, Node::new(Role::Cog, Axis::Y).source(32.0));
        g.insert(IVec3::X, Node::new(Role::Cog, Axis::Y));
        g.insert(IVec3::new(2, 0, 1), Node::new(Role::LargeCog, Axis::Y));
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::X], -32.0);
        assert_eq!(s.speed[&IVec3::new(2, 0, 1)], 16.0);
    }

    #[test]
    fn gearbox_turns_corners_not_along_its_axis() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).source(16.0));
        g.insert(IVec3::X, Node::new(Role::Gearbox, Axis::Y));
        g.insert(IVec3::new(1, 0, 1), shaft(Axis::Z));
        g.insert(IVec3::new(2, 0, 0), shaft(Axis::X));
        g.insert(IVec3::new(1, 1, 0), shaft(Axis::Y));
        let s = solve(&g);
        assert_eq!(s.speed[&IVec3::new(1, 0, 1)].abs(), 16.0);
        assert_eq!(s.speed[&IVec3::new(2, 0, 0)], -16.0, "straight through reverses");
        assert_eq!(s.speed[&IVec3::new(1, 1, 0)], 0.0, "no port along the axis");
    }

    #[test]
    fn clutch_and_gearshift() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).one_port(IVec3::X).source(16.0));
        g.insert(IVec3::X, Node::new(Role::Gearshift, Axis::X));
        g.insert(IVec3::new(2, 0, 0), shaft(Axis::X));
        assert_eq!(solve(&g).speed[&IVec3::new(2, 0, 0)], 16.0);
        g.get_mut(&IVec3::X).unwrap().toggled = true;
        assert_eq!(solve(&g).speed[&IVec3::new(2, 0, 0)], -16.0);
        g.insert(IVec3::X, Node { toggled: true, ..Node::new(Role::Clutch, Axis::X) });
        assert_eq!(solve(&g).speed[&IVec3::new(2, 0, 0)], 0.0);
    }

    #[test]
    fn conflict_and_limit() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).source(32.0));
        g.insert(IVec3::X, shaft(Axis::X));
        g.insert(IVec3::new(2, 0, 0), shaft(Axis::X).source(64.0));
        let s = solve(&g);
        assert!(s.nets[0].conflict && s.speed[&IVec3::X] == 0.0);
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, Node::new(Role::LargeCog, Axis::Y).source(200.0));
        g.insert(IVec3::new(1, 0, 1), Node::new(Role::Cog, Axis::Y));
        let s = solve(&g);
        assert!(s.speed.values().all(|v| v.abs() <= MAX_RPM + 1e-3));
    }

    #[test]
    fn stress_of_shafts_and_cogs() {
        // A source at 64 RPM, 4 shafts (0.5) and a cog (1.0): 2 × (0.5·4 + 1).
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).one_port(IVec3::X).source(64.0).capacity(100.0));
        for x in 1..=4 {
            g.insert(IVec3::new(x, 0, 0), shaft(Axis::X).stress(0.5));
        }
        g.insert(IVec3::new(5, 0, 0), Node::new(Role::Cog, Axis::X).stress(1.0));
        let s = solve(&g);
        let n = &s.nets[s.net_of[&IVec3::X]];
        assert_eq!((n.capacity, n.stress), (100.0, 6.0));
    }

    #[test]
    fn overload_slows_then_stops() {
        assert_eq!(factor(2048.0, 1000.0), 1.0);
        assert_eq!(factor(2048.0, 4096.0), 0.5, "twice the load: half speed");
        assert_eq!(factor(2048.0, 20480.0), 0.0, "k < 0.2: stops");
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).one_port(IVec3::X).source(64.0).capacity(100.0));
        g.insert(IVec3::X, shaft(Axis::X).stress(100.0));
        let mut s = solve(&g);
        let n = s.net_of[&IVec3::X];
        let k = factor(s.nets[n].capacity, s.nets[n].stress);
        scale(&mut s, &[n], k);
        assert_eq!(s.speed[&IVec3::X], 32.0, "200 SU wanted, 100 given");
    }

    #[test]
    fn a_reduction_gear_halves_the_stress() {
        // The same load (a wheel of 50 SU) behind small→small and small→large.
        let load = |large: bool| {
            let mut g = HashMap::new();
            g.insert(IVec3::ZERO, Node::new(Role::Cog, Axis::Z).source(64.0).capacity(1e6));
            let (c, role) = if large { (IVec3::new(1, 1, 0), Role::LargeCog) } else { (IVec3::X, Role::Cog) };
            g.insert(c, Node::new(role, Axis::Z));
            g.insert(c + IVec3::Z, shaft(Axis::Z).stress(50.0));
            let s = solve(&g);
            (s.speed[&(c + IVec3::Z)].abs(), s.nets[0].stress)
        };
        let (fast, high) = load(false);
        let (slow, low) = load(true);
        assert_eq!((fast, slow), (64.0, 32.0));
        assert_eq!(low * 2.0, high);
    }

    #[test]
    fn extra_links_join_distant_nodes() {
        // Two cogs 5 apart on a chain, opposite sides: reversed; large on
        // the other end: half speed.
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, Node::new(Role::Cog, Axis::Z).source(32.0));
        g.insert(IVec3::new(5, 0, 0), Node::new(Role::Cog, Axis::Z));
        g.insert(IVec3::new(5, 4, 0), Node::new(Role::LargeCog, Axis::Z));
        let s = solve_with(&g, &[(IVec3::ZERO, IVec3::new(5, 0, 0), -1.0), (IVec3::new(5, 0, 0), IVec3::new(5, 4, 0), 0.5)]);
        assert_eq!(s.speed[&IVec3::new(5, 0, 0)], -32.0);
        assert_eq!(s.speed[&IVec3::new(5, 4, 0)], -16.0);
    }

    #[test]
    fn driven_networks() {
        let mut g = HashMap::new();
        g.insert(IVec3::ZERO, shaft(Axis::X).source(0.0));
        g.insert(IVec3::X, shaft(Axis::X));
        g.insert(IVec3::new(5, 0, 0), shaft(Axis::X));
        let s = solve(&g);
        assert!(s.nets[s.net_of[&IVec3::X]].driven, "a source asking for 0 still holds");
        assert!(!s.nets[s.net_of[&IVec3::new(5, 0, 0)]].driven);
    }
}
