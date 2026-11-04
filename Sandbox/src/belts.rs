//! Mechanical belts (like Create's): rotation from one shaft to another,
//! and a moving surface that carries the player and physical bodies.
//!
//! * The belt tool: LMB on a shaft, then on another shaft with the same
//!   axis, in line with it across the axis: straight (flat or vertical) or
//!   at 45°, 2…`MAX_LEN` blocks, the cells between free. Both shafts become
//!   pulleys and the belt fills the gap; both pulleys turn the same way at
//!   the same speed.
//! * A flat or 45° belt carries what stands on it: surface speed =
//!   RPM × 2π × 0.5 m / 60, along the belt.
//! * Everything is plain blocks (pulleys and belt cells), so belts move
//!   with contraptions, blueprints and the world like anything else.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{
    content::{BlockId, Mech, Tool},
    contraption::Contraption,
    core::toast,
    grid::{Cell, VoxelGrid, WorldGrid, facing_dir, facing_of},
    interact::{Clicks, Target},
    inventory::Inventory,
    mech::{BlockEntity, Spin, network::Node},
    player::{Body, Player},
    style::{BoxCache, LayerNode, Materials, Part, part, spawn_parts},
};

pub const MAX_LEN: i32 = 20;
/// Pulley radius (m): the surface speed per RPM.
pub const RADIUS: f32 = 0.5;

/// One step along a belt from `a` to `b`, if they can be joined: same
/// axis, across it, straight or 45°.
pub fn belt_step(axis: IVec3, a: IVec3, b: IVec3) -> Option<IVec3> {
    let d = b - a;
    if d.dot(axis.abs()) != 0 || d == IVec3::ZERO {
        return None;
    }
    let n = d.abs().max_element();
    if n < 2 || n > MAX_LEN || d.abs().element_sum() != n * (d.abs().cmpgt(IVec3::ZERO).bitmask().count_ones() as i32) {
        return None;
    }
    let e = d / n;
    (e * n == d).then_some(e)
}

/// The block and facing of a belt cell for a step `e` (axis `axis`).
pub fn belt_cell(axis: IVec3, e: IVec3) -> (&'static str, u8) {
    let h = IVec3::new(e.x, 0, e.z);
    if e.y != 0 && h != IVec3::ZERO {
        // A 45° ramp: facing = the horizontal way down.
        let down = if e.y > 0 { -h } else { h };
        ("belt_slope", facing_of(down))
    } else if e.y == 0 {
        ("belt", facing_of(IVec3::Y))
    } else {
        // Vertical: its plane faces across.
        ("belt", facing_of(axis.cross(e)))
    }
}

/// Places a belt between two shafts. Returns an error text.
pub fn lay(grid: &mut VoxelGrid, a: IVec3, b: IVec3) -> Result<(), &'static str> {
    let (ca, cb) = (grid.get(a), grid.get(b));
    let shaft = BlockId::k("shaft");
    if ca.id != shaft || cb.id != shaft {
        return Err("Лента натягивается между двумя валами");
    }
    let axis = facing_dir(ca.facing).abs();
    if facing_dir(cb.facing).abs() != axis {
        return Err("У валов должна быть одна ось");
    }
    let Some(e) = belt_step(axis, a, b) else { return Err("Валы должны быть на одной линии поперёк оси: прямо или под 45°, до 20 блоков") };
    let n = (b - a).abs().max_element();
    if (1..n).any(|k| !grid.get(a + e * k).is_air()) {
        return Err("Между валами должно быть пусто");
    }
    let pulley = BlockId::k("belt_pulley");
    grid.set(a, Cell::new(pulley, ca.facing));
    grid.set(b, Cell::new(pulley, cb.facing));
    let (key, facing) = belt_cell(axis, e);
    for k in 1..n {
        grid.set(a + e * k, Cell::new(BlockId::k(key), facing));
    }
    Ok(())
}

fn is_belt(m: Option<Mech>) -> bool {
    matches!(m, Some(Mech::Belt | Mech::BeltPulley))
}

/// The direction a belt runs through a cell (towards a belt neighbour).
fn run_dir(grid: &VoxelGrid, c: IVec3, mech: impl Fn(IVec3) -> Option<Mech>) -> Option<IVec3> {
    let cell = grid.get(c);
    let key = cell.id.def().key.as_str();
    let n = facing_dir(cell.facing);
    let mut cands = Vec::new();
    if key == "belt_slope" {
        cands.push(IVec3::Y - n);
        cands.push(n - IVec3::Y);
    } else {
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let d = IVec3::new(x, y, z);
                    if d != IVec3::ZERO && d.dot(n) == 0 && d.abs().max_element() == 1 {
                        cands.push(d);
                    }
                }
            }
        }
    }
    cands.into_iter().find(|d| is_belt(mech(c + *d)))
}

/// Kinetic links of a grid's belts: pulley ↔ pulley at the other end
/// (same speed). Also the track rows (`tracks`).
pub fn links(grid: &VoxelGrid, nodes: &std::collections::HashMap<IVec3, Node>, mech: impl Fn(IVec3) -> Option<Mech> + Copy) -> Vec<(IVec3, IVec3, f32)> {
    let mut out = Vec::new();
    for &c in nodes.keys() {
        if mech(c) != Some(Mech::BeltPulley) {
            continue;
        }
        let axis = facing_dir(grid.get(c).facing).abs();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let d = IVec3::new(x, y, z);
                    if d == IVec3::ZERO || d.dot(axis) != 0 || mech(c + d) != Some(Mech::Belt) {
                        continue;
                    }
                    let mut k = 1;
                    while k <= MAX_LEN && mech(c + d * k) == Some(Mech::Belt) {
                        k += 1;
                    }
                    let end = c + d * k;
                    if mech(end) == Some(Mech::BeltPulley) && facing_dir(grid.get(end).facing).abs() == axis {
                        out.push((c, end, 1.0));
                    }
                }
            }
        }
    }
    out.extend(crate::tracks::links(grid, nodes, mech));
    out
}

/// Surface velocity of a belt cell (grid-local), or None for vertical
/// belts / standing belts.
pub fn surface_velocity(grid: &VoxelGrid, c: IVec3, mech: impl Fn(IVec3) -> Option<Mech> + Copy, rpm_of: impl Fn(IVec3) -> Option<f32>) -> Option<Vec3> {
    let cell = grid.get(c);
    let key = cell.id.def().key.as_str();
    let f = facing_dir(cell.facing).as_vec3();
    let up = match key {
        "belt_slope" => (Vec3::Y + f).normalize(),
        "belt" if f.y > 0.5 => Vec3::Y,
        _ => return None,
    };
    let dir = run_dir(grid, c, mech)?;
    // Walk to a pulley for the speed and the axis.
    for s in [dir, -dir] {
        let mut k = 1;
        while k <= MAX_LEN && mech(c + s * k) == Some(Mech::Belt) {
            k += 1;
        }
        let p = c + s * k;
        if mech(p) == Some(Mech::BeltPulley) {
            let axis = facing_dir(grid.get(p).facing).abs().as_vec3();
            let w = rpm_of(p)? / 60.0 * std::f32::consts::TAU;
            return Some(axis.cross(up) * w * RADIUS);
        }
    }
    None
}

// ------------------------------------------------------------ tool

#[derive(Resource, Default)]
pub struct BeltStart(pub Option<(Entity, IVec3)>);

fn use_belt(clicks: Res<Clicks>, target: Res<Target>, players: Query<&Inventory, With<Player>>, mut grids: Query<&mut VoxelGrid>, mut start: ResMut<BeltStart>, mut toasts: MessageWriter<crate::core::Toast>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    let Ok(inv) = players.single() else { return };
    if crate::tools::held_tool(inv) != Some(Tool::Belt) || !(clicks.left || clicks.right) {
        return;
    }
    if clicks.right {
        start.0 = None;
        return;
    }
    let Some(a) = target.hit else { return };
    let Ok(mut grid) = grids.get_mut(a.grid) else { return };
    if grid.get(a.cell).id != BlockId::k("shaft") {
        toasts.write(toast("Лента натягивается между валами"));
        return;
    }
    match start.0 {
        Some((g, first)) if g == a.grid && first != a.cell => {
            match lay(&mut grid, first, a.cell) {
                Ok(()) => {
                    toasts.write(toast("Лента натянута"));
                    sounds.write(crate::sound::sfx("place"));
                }
                Err(e) => {
                    toasts.write(toast(e));
                }
            }
            start.0 = None;
        }
        _ => {
            start.0 = Some((a.grid, a.cell));
            toasts.write(toast("Вал выбран: ЛКМ по второму валу"));
            sounds.write(crate::sound::sfx("ui_click"));
        }
    }
}

/// Breaking a pulley or a belt cell removes the whole belt (the pulleys
/// become shafts again).
fn break_belts(mut edited: MessageReader<crate::interact::Edited>, mut grids: Query<&mut VoxelGrid>) {
    for e in edited.read() {
        if e.placed || !is_belt(e.block.def().mech) {
            continue;
        }
        let Ok(mut g) = grids.get_mut(e.grid) else { continue };
        // Flood the belt cells from the break, turn pulleys back into shafts.
        let mut todo = vec![e.cell];
        let mut seen = std::collections::HashSet::new();
        while let Some(c) = todo.pop() {
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let n = c + IVec3::new(x, y, z);
                        if !seen.insert(n) {
                            continue;
                        }
                        match g.get(n).id.def().mech {
                            Some(Mech::Belt) => {
                                g.set(n, Cell::AIR);
                                todo.push(n);
                            }
                            Some(Mech::BeltPulley) => {
                                let f = g.get(n).facing;
                                g.set(n, Cell::new(BlockId::k("shaft"), f));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------ carrying

/// Belt velocity under the player's feet (for the movement controller).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct BeltCarry(pub Vec3);

#[allow(clippy::type_complexity)]
fn carry(
    world_grid: Query<(&VoxelGrid, &GlobalTransform), With<WorldGrid>>,
    blocks: Query<(&BlockEntity, &Spin)>,
    players: Query<(&Position, &Body), With<Player>>,
    mut bodies: Query<(&VoxelGrid, &GlobalTransform, &ComputedMass, Forces), (With<Contraption>, Without<WorldGrid>)>,
    mut out: ResMut<BeltCarry>,
) {
    let Ok((wg, _)) = world_grid.single() else { return };
    let mech = |c: IVec3| -> Option<Mech> {
        let cell = wg.get(c);
        if cell.is_air() { None } else { cell.id.def().mech }
    };
    let rpm = |c: IVec3| wg.entities.get(&c).and_then(|e| blocks.get(*e).ok()).map(|(_, s)| s.rpm);
    let under = |p: Vec3| -> Option<Vec3> {
        let c = p.floor().as_ivec3();
        for cc in [c, c - IVec3::Y] {
            if mech(cc) == Some(Mech::Belt) {
                return surface_velocity(wg, cc, mech, rpm);
            }
        }
        None
    };
    out.0 = Vec3::ZERO;
    if let Ok((p, b)) = players.single() {
        let feet = p.0 - Vec3::Y * (b.height() * 0.5 + 0.05);
        if let Some(v) = under(feet) {
            out.0 = v;
        }
    }
    // Bodies resting on a belt: the belt drags their bottom along.
    for (grid, gt, mass, mut forces) in &mut bodies {
        let cells = grid.cells();
        if cells.len() > 512 || !mass.value().is_finite() {
            continue;
        }
        let bottom: Vec<IVec3> = cells.iter().map(|(c, _)| *c).filter(|c| grid.get(*c - IVec3::Y).is_air()).collect();
        let per = mass.value() / bottom.len().max(1) as f32;
        for c in bottom {
            let p = gt.transform_point(c.as_vec3() + Vec3::new(0.5, -0.1, 0.5));
            if let Some(v) = under(p) {
                let rel = v - forces.velocity_at_point(p);
                let f = Vec3::new(rel.x, rel.y.max(0.0), rel.z) * per * 4.0;
                forces.apply_force_at_point(f, p);
            }
        }
    }
}

// ------------------------------------------------------------ looks

/// What a belt cell's model was built for.
#[derive(Component, PartialEq)]
struct BeltLook(IVec3);

/// Rubber runs (top and bottom) along `dir` through a cell, `n` = the
/// belt plane's normal (block-local).
pub fn belt_parts(dir: Vec3, n: Vec3) -> Vec<Part> {
    let r = Quat::from_mat3(&Mat3::from_cols(n.cross(dir).normalize(), n, dir.normalize()));
    let len = dir.length();
    let mut v = Vec::new();
    for s in [0.36f32, -0.36] {
        v.push(part([0.86, 0.08, len], "rubber", (n * s).to_array()).rot(r));
    }
    for k in [-0.25f32, 0.25] {
        v.push(part([0.9, 0.1, 0.06], "steel", (n * 0.38 + dir.normalize() * k * len).to_array()).rot(r));
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn looks(
    mut commands: Commands,
    grids: Query<&VoxelGrid>,
    belts: Query<(Entity, &BlockEntity, Option<&BeltLook>, Option<&Children>)>,
    nodes: Query<(), With<LayerNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (e, b, look, children) in &belts {
        if b.mech() != Some(Mech::Belt) {
            continue;
        }
        let Ok(grid) = grids.get(b.grid) else { continue };
        let mech = |c: IVec3| -> Option<Mech> {
            let cell = grid.get(c);
            if cell.is_air() { None } else { cell.id.def().mech }
        };
        let Some(dir) = run_dir(grid, b.cell, mech) else { continue };
        if look.is_some_and(|l| l.0 == dir) {
            continue;
        }
        for c in children.into_iter().flatten() {
            if nodes.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        let rot = crate::models::block_rot(b.id.def().placement, b.facing).inverse();
        let f = facing_dir(b.facing).as_vec3();
        let n_world = if b.id.def().key == "belt_slope" { (Vec3::Y + f).normalize() } else { f };
        let (d, n) = (rot * dir.as_vec3(), rot * n_world);
        spawn_parts(&mut commands, e, &belt_parts(d, n), &mut meshes, &mut cache, &mats, true);
        commands.entity(e).insert(BeltLook(dir));
    }
}

pub struct BeltsPlugin;

impl Plugin for BeltsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BeltStart>()
            .init_resource::<BeltCarry>()
            .add_systems(Update, (use_belt.after(crate::interact::InteractSet), break_belts.after(crate::interact::InteractSet), looks))
            .add_systems(FixedUpdate, carry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn belts_run_straight_or_at_45_degrees() {
        // Axis Z: across it in XY.
        assert_eq!(belt_step(IVec3::Z, IVec3::ZERO, IVec3::new(5, 0, 0)), Some(IVec3::X));
        assert_eq!(belt_step(IVec3::Z, IVec3::ZERO, IVec3::new(0, 4, 0)), Some(IVec3::Y), "vertical");
        assert_eq!(belt_step(IVec3::Z, IVec3::ZERO, IVec3::new(3, 3, 0)), Some(IVec3::new(1, 1, 0)), "45°");
        assert_eq!(belt_step(IVec3::Z, IVec3::ZERO, IVec3::new(3, 2, 0)), None, "other angles");
        assert_eq!(belt_step(IVec3::Z, IVec3::ZERO, IVec3::new(3, 0, 1)), None, "not across the axis");
        assert_eq!(belt_step(IVec3::Z, IVec3::ZERO, IVec3::X), None, "too short");
        // Cells: flat, vertical, a ramp facing its way down.
        assert_eq!(belt_cell(IVec3::Z, IVec3::X), ("belt", facing_of(IVec3::Y)));
        assert_eq!(belt_cell(IVec3::Z, IVec3::new(1, 1, 0)), ("belt_slope", facing_of(IVec3::NEG_X)));
    }

    #[test]
    fn a_laid_belt_links_its_pulleys_and_moves_its_surface() {
        let mut g = VoxelGrid::default();
        let shaft = BlockId::k("shaft");
        g.set(IVec3::ZERO, Cell::new(shaft, facing_of(IVec3::Z)));
        g.set(IVec3::new(4, 0, 0), Cell::new(shaft, facing_of(IVec3::Z)));
        lay(&mut g, IVec3::ZERO, IVec3::new(4, 0, 0)).unwrap();
        let mech = |c: IVec3| {
            let cell = g.get(c);
            if cell.is_air() { None } else { cell.id.def().mech }
        };
        assert_eq!(mech(IVec3::new(2, 0, 0)), Some(Mech::Belt));
        let mut nodes = std::collections::HashMap::new();
        for x in [0, 4] {
            nodes.insert(IVec3::new(x, 0, 0), Node::new(crate::mech::network::Role::Shaft, crate::mech::network::Axis::Z));
        }
        let l = links(&g, &nodes, mech);
        assert!(l.contains(&(IVec3::ZERO, IVec3::new(4, 0, 0), 1.0)));
        // 60 RPM about +Z: the top moves at 2π·0.5 m/s towards −X.
        let v = surface_velocity(&g, IVec3::new(2, 0, 0), mech, |_| Some(60.0)).unwrap();
        assert!((v - Vec3::new(-std::f32::consts::PI, 0.0, 0.0)).length() < 1e-4, "{v}");
        assert!(lay(&mut g, IVec3::ZERO, IVec3::new(0, 3, 0)).is_err(), "not on two shafts");
    }
}
