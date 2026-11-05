//! Item tubes: like fluid pipes, but for items.
//!
//! * A **tube** joins with tubes, storages and drives on every side; a
//!   tube network is everything joined that way (storages are its ends —
//!   items do not pass through a chest).
//! * A **drive** is turned like a pump (a shaft or cog at its side meshes
//!   with its brass ring). Every 16 / |RPM| s it takes a «Пачка» of items
//!   from a storage of the network behind it and sends it to a storage
//!   with room in the network in front («Направление» swaps them).
//! * The packet is seen travelling through the glass tubes, cell by cell,
//!   at |RPM| / 16 cells a second (at least 2). If its target is full or
//!   gone when it arrives, it goes into any storage of the network or back.
//! * Works the same inside physical machines. Future drills and saws drop
//!   into storages, so tubes will carry their spoils too.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::{
    content::Mech,
    grid::{DIRS, VoxelGrid},
    inventory::Stack,
    mech::{BlockEntity, Params, Spin},
    storage::{ItemStore, STORAGE_MECHS},
    style::{BoxCache, LayerNode, Materials, Part, part, spawn_parts},
};

pub const TUBE_MECHS: [Mech; 5] = [Mech::ItemTube, Mech::ItemDrive, Mech::Chest, Mech::Barrel, Mech::Container];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Tube,
    Store,
    Drive { d: IVec3 },
}

pub fn role_of(m: Option<Mech>, dir: IVec3) -> Option<Role> {
    Some(match m? {
        Mech::ItemTube => Role::Tube,
        Mech::ItemDrive => Role::Drive { d: dir },
        m if STORAGE_MECHS.contains(&m) => Role::Store,
        _ => return None,
    })
}

/// Does a block of this role join on side `dir`?
pub fn opens(r: Role, dir: IVec3) -> bool {
    match r {
        Role::Tube | Role::Store => true,
        Role::Drive { d } => dir == d || dir == -d,
    }
}

/// Tube networks: a component index for every tube and store joined to a
/// tube (stores do not join stores directly; drives split networks).
pub fn components(nodes: &HashMap<IVec3, Role>) -> HashMap<IVec3, usize> {
    let mut comp = HashMap::new();
    let mut n = 0;
    for (&start, &r) in nodes {
        if r != Role::Tube || comp.contains_key(&start) {
            continue;
        }
        let mut q = VecDeque::from([start]);
        comp.insert(start, n);
        while let Some(c) = q.pop_front() {
            if nodes[&c] != Role::Tube {
                continue;
            }
            for d in DIRS {
                let o = c + d;
                if let Some(&ro) = nodes.get(&o)
                    && !matches!(ro, Role::Drive { .. })
                    && opens(ro, -d)
                    && !comp.contains_key(&o)
                {
                    comp.insert(o, n);
                    q.push_back(o);
                }
            }
        }
        n += 1;
    }
    comp
}

/// The way through tubes from `a` to `b` (both included), if any.
pub fn path(nodes: &HashMap<IVec3, Role>, a: IVec3, b: IVec3, via: Option<IVec3>) -> Option<Vec<IVec3>> {
    let mut prev: HashMap<IVec3, IVec3> = HashMap::new();
    let mut q = VecDeque::from([a]);
    let mut seen = HashSet::from([a]);
    while let Some(c) = q.pop_front() {
        if c == b {
            let mut out = vec![b];
            let mut x = b;
            while let Some(p) = prev.get(&x) {
                out.push(*p);
                x = *p;
            }
            out.reverse();
            return Some(out);
        }
        // Only tubes (and the drive itself) are passed through.
        if c != a && nodes.get(&c) != Some(&Role::Tube) && Some(c) != via {
            continue;
        }
        for d in DIRS {
            let o = c + d;
            if seen.contains(&o) {
                continue;
            }
            let Some(&r) = nodes.get(&o) else { continue };
            let pass = match r {
                Role::Drive { .. } => Some(o) == via && opens(r, d),
                _ => true,
            };
            if pass {
                seen.insert(o);
                prev.insert(o, c);
                q.push_back(o);
            }
        }
    }
    None
}

/// A drive's timer and turn-taking.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Drive {
    pub t: f32,
    pub next_from: usize,
    pub next_to: usize,
}

/// Items on their way: grid, the cells of the way, progress (cells),
/// speed (cells/s), the stack, the network it is in.
#[derive(Component, Debug, Clone)]
pub struct Packet {
    pub grid: Entity,
    pub path: Vec<IVec3>,
    pub at: f32,
    pub speed: f32,
    pub stack: Stack,
    pub to: IVec3,
    pub from: IVec3,
}

fn add_drives(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Drive>)>) {
    for (e, b) in &added {
        if b.mech() == Some(Mech::ItemDrive) {
            commands.entity(e).insert(Drive::default());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    time: Res<Time>,
    grids: Query<&VoxelGrid>,
    blocks: Query<&BlockEntity>,
    mut drives: Query<(&BlockEntity, &Spin, &Params, &mut Drive)>,
    mut stores: Query<&mut ItemStore>,
    mats: Option<Res<Materials>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
) {
    let _t = crate::profile::scope("трубы");
    let dt = time.delta_secs();
    let Some(mats) = mats else { return };
    for grid in &grids {
        let drive_cells: Vec<(IVec3, Entity)> = grid.entities_of(&[Mech::ItemDrive]).collect();
        if drive_cells.is_empty() {
            continue;
        }
        let mut nodes: HashMap<IVec3, Role> = HashMap::new();
        for (c, e) in grid.entities_of(&TUBE_MECHS) {
            if let Ok(b) = blocks.get(e)
                && let Some(r) = role_of(b.mech(), b.dir())
            {
                nodes.insert(c, r);
            }
        }
        let comp = components(&nodes);
        for (c, e) in drive_cells {
            let Ok((b, spin, p, mut dr)) = drives.get_mut(e) else { continue };
            let rate = spin.rpm.abs() / 16.0;
            if rate <= 0.0 {
                dr.t = 0.0;
                continue;
            }
            dr.t += dt * rate;
            if dr.t < 1.0 {
                continue;
            }
            dr.t = dr.t.min(2.0) - 1.0;
            let back = p.get(b.id, "direction") >= 0.5;
            let d = b.dir();
            let (src_side, dst_side) = if back { (c + d, c - d) } else { (c - d, c + d) };
            // The networks on each side: a tube's network, or the storage
            // standing right there.
            let net = |side: IVec3| -> Vec<IVec3> {
                match nodes.get(&side) {
                    Some(Role::Store) if !comp.contains_key(&side) => vec![side],
                    Some(_) => match comp.get(&side) {
                        Some(n) => {
                            let mut v: Vec<IVec3> = comp.iter().filter(|(cc, k)| *k == n && nodes[cc] == Role::Store).map(|(cc, _)| *cc).collect();
                            v.sort_by_key(|c| (c.y, c.z, c.x));
                            v
                        }
                        None => Vec::new(),
                    },
                    None => Vec::new(),
                }
            };
            let srcs = net(src_side);
            let dsts: Vec<IVec3> = net(dst_side).into_iter().filter(|x| !srcs.contains(x)).collect();
            if srcs.is_empty() || dsts.is_empty() {
                continue;
            }
            let batch = p.get(b.id, "batch").max(1.0) as u32;
            // Turn taking: the next source with items, the next target with room.
            let mut sent = false;
            for k in 0..srcs.len() {
                let s = srcs[(dr.next_from + k) % srcs.len()];
                let Some(se) = grid.entities.get(&s).copied() else { continue };
                let Some(peek) = stores.get(se).ok().and_then(|st| st.slots.iter().flatten().next().copied()) else { continue };
                for j in 0..dsts.len() {
                    let t = dsts[(dr.next_to + j) % dsts.len()];
                    let Some(te) = grid.entities.get(&t).copied() else { continue };
                    let room = stores.get(te).map(|st| st.room_for(peek)).unwrap_or(0);
                    if room == 0 {
                        continue;
                    }
                    let Some(stack) = stores.get_mut(se).ok().and_then(|mut st| st.take(batch.min(room))) else { continue };
                    let way = path(&nodes, s, t, Some(c)).unwrap_or_else(|| vec![s, c, t]);
                    spawn_packet(&mut commands, &mats, &mut meshes, &mut cache, Packet { grid: b.grid, path: way, at: 0.0, speed: rate.max(2.0), stack, to: t, from: s });
                    dr.next_from = (dr.next_from + k + 1) % srcs.len();
                    dr.next_to = (dr.next_to + j + 1) % dsts.len();
                    sent = true;
                    break;
                }
                if sent {
                    break;
                }
            }
        }
    }
}

/// The look of a packet: a small crate in the item's colours.
fn packet_parts(stack: Stack) -> Vec<Part> {
    let mat: &'static str = match stack.item.block().and_then(|b| b.def().textures.clone()) {
        Some((_, side, _)) => crate::bbmodel::material_name(&side),
        None => "brass",
    };
    vec![part([0.26, 0.26, 0.26], mat, [0.0, 0.0, 0.0]), part([0.28, 0.06, 0.28], "steel", [0.0, 0.1, 0.0])]
}

fn spawn_packet(commands: &mut Commands, mats: &Materials, meshes: &mut Assets<Mesh>, cache: &mut BoxCache, p: Packet) {
    let start = p.path[0].as_vec3() + Vec3::splat(0.5);
    let grid = p.grid;
    let parts = packet_parts(p.stack);
    let e = commands.spawn((Name::new("Item packet"), p, Transform::from_translation(start), Visibility::default(), ChildOf(grid))).id();
    spawn_parts(commands, e, &parts, meshes, cache, mats, false);
}

/// Packets move along their way and are delivered at the end.
fn travel(mut commands: Commands, time: Res<Time>, grids: Query<&VoxelGrid>, mut packets: Query<(Entity, &mut Packet, &mut Transform)>, mut stores: Query<&mut ItemStore>) {
    let dt = time.delta_secs();
    for (e, mut p, mut t) in &mut packets {
        p.at += p.speed * dt;
        let last = (p.path.len() - 1) as f32;
        if p.at < last {
            let i = p.at.floor() as usize;
            let f = p.at - i as f32;
            let a = p.path[i].as_vec3();
            let b = p.path[(i + 1).min(p.path.len() - 1)].as_vec3();
            t.translation = a.lerp(b, f) + Vec3::splat(0.5);
            t.rotation = Quat::from_rotation_y(p.at * 1.3);
            continue;
        }
        // Arrived: into the target, else any storage on the way's ends, else back.
        let Ok(grid) = grids.get(p.grid) else {
            commands.entity(e).despawn();
            continue;
        };
        let mut left = Some(p.stack);
        for c in [p.to, p.from] {
            if let (Some(l), Some(se)) = (left, grid.entities.get(&c)) {
                if let Ok(mut st) = stores.get_mut(*se) {
                    left = st.insert(l);
                }
            }
        }
        if let Some(l) = left {
            warn!("item packet lost: {} × {}", l.item.def().key, l.count);
        }
        commands.entity(e).despawn();
    }
}

// ------------------------------------------------------------ looks

/// Joined sides of a tube (bits of `DIRS`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TubeMask(pub u8);

/// A glass tube with brass rings, arms to the joined sides (block-local).
pub fn tube_parts(local: &[IVec3]) -> Vec<Part> {
    let dirs: Vec<IVec3> = if local.is_empty() { vec![IVec3::Y, IVec3::NEG_Y] } else { local.to_vec() };
    let mut v = vec![part([0.6, 0.6, 0.6], "glass", [0.0, 0.0, 0.0])];
    // A brass frame on the joint.
    for a in [-0.29f32, 0.29] {
        for b in [-0.29f32, 0.29] {
            v.push(part([0.06, 0.62, 0.06], "brass", [a, 0.0, b]));
        }
    }
    for d in dirs {
        let r = Quat::from_rotation_arc(Vec3::Y, d.as_vec3());
        let a = d.as_vec3();
        v.push(part([0.56, 0.22, 0.56], "glass", (a * 0.39).to_array()).rot(r));
        v.push(part([0.64, 0.07, 0.64], "brass", (a * 0.465).to_array()).rot(r));
        v.push(part([0.62, 0.05, 0.62], "steel", (a * 0.31).to_array()).rot(r));
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn update_tubes(
    mut commands: Commands,
    grids: Query<&VoxelGrid>,
    tubes: Query<(Entity, &BlockEntity, Option<&TubeMask>, Option<&Children>)>,
    all: Query<&BlockEntity>,
    nodes: Query<(), With<LayerNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (e, b, mask, children) in &tubes {
        if b.mech() != Some(Mech::ItemTube) {
            continue;
        }
        let Ok(grid) = grids.get(b.grid) else { continue };
        let mut m = 0u8;
        for (i, d) in DIRS.iter().enumerate() {
            if let Some(o) = grid.entities.get(&(b.cell + *d)).and_then(|o| all.get(*o).ok())
                && let Some(r) = role_of(o.mech(), o.dir())
                && opens(r, -*d)
            {
                m |= 1 << i;
            }
        }
        if mask.is_some_and(|k| k.0 == m) {
            continue;
        }
        let rot = crate::models::block_rot(b.id.def().placement, b.facing).inverse();
        let local: Vec<IVec3> = DIRS.iter().enumerate().filter(|(i, _)| m & (1 << i) != 0).map(|(_, d)| (rot * d.as_vec3()).round().as_ivec3()).collect();
        for c in children.into_iter().flatten() {
            if nodes.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        spawn_parts(&mut commands, e, &tube_parts(&local), &mut meshes, &mut cache, &mats, true);
        commands.entity(e).insert(TubeMask(m));
    }
}

pub struct TubesPlugin;

impl Plugin for TubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_drives, run.after(crate::mech::MechSet), travel, update_tubes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networks_and_ways() {
        // chest — tube — drive(+X) — tube — tube — barrel; a stray chest.
        let mut n = HashMap::new();
        n.insert(IVec3::new(0, 0, 0), Role::Store);
        n.insert(IVec3::new(1, 0, 0), Role::Tube);
        n.insert(IVec3::new(2, 0, 0), Role::Drive { d: IVec3::X });
        n.insert(IVec3::new(3, 0, 0), Role::Tube);
        n.insert(IVec3::new(3, 1, 0), Role::Tube);
        n.insert(IVec3::new(4, 1, 0), Role::Store);
        n.insert(IVec3::new(9, 0, 0), Role::Store);
        let c = components(&n);
        assert_eq!(c[&IVec3::ZERO], c[&IVec3::X], "the chest is on the first tube");
        assert_ne!(c[&IVec3::X], c[&IVec3::new(3, 0, 0)], "the drive splits the networks");
        assert_eq!(c[&IVec3::new(4, 1, 0)], c[&IVec3::new(3, 0, 0)]);
        assert!(!c.contains_key(&IVec3::new(9, 0, 0)), "a lone chest is in no network");
        let way = path(&n, IVec3::ZERO, IVec3::new(4, 1, 0), Some(IVec3::new(2, 0, 0))).unwrap();
        assert_eq!(way, vec![IVec3::new(0, 0, 0), IVec3::new(1, 0, 0), IVec3::new(2, 0, 0), IVec3::new(3, 0, 0), IVec3::new(3, 1, 0), IVec3::new(4, 1, 0)]);
        assert!(path(&n, IVec3::ZERO, IVec3::new(4, 1, 0), None).is_none(), "not through the drive unless sent by it");
    }
}
