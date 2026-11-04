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

use std::collections::{HashMap, VecDeque};

use bevy::math::IVec3;

pub const MAX_RPM: f32 = 256.0;

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
}

impl Node {
    pub fn new(role: Role, axis: Axis) -> Self {
        Self { role, axis, only: None, source: None, toggled: false }
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
}

#[derive(Debug, Clone, Default)]
pub struct Solution {
    pub speed: HashMap<IVec3, f32>,
    pub net_of: HashMap<IVec3, usize>,
    pub nets: Vec<NetInfo>,
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
                if !links(grid, n).iter().any(|(m, _)| *m == c) {
                    continue;
                }
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
            sol.speed.insert(*c, base * rel[c]);
        }
        sol.nets.push(info);
    }
    sol
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
