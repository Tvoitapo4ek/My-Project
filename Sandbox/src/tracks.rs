//! Modular tracks: a drive sprocket (turned by a shaft, like a wheel) and
//! any number of road wheels in a row beside it. The track band wraps the
//! row by itself.
//!
//! * Kinetics: neighbouring track blocks with the same axis turn together
//!   (the band), so one sprocket drives the whole row.
//! * Physics: every block of the row touches the ground through its own
//!   spring (no suspension block needed) and pushes like a driven wheel,
//!   so the load spreads over the whole length — steadier on the washboard
//!   and on block ramps. Side grip is strong; steer by running the left
//!   and right tracks at different speeds.

use bevy::prelude::*;

use crate::{
    content::Mech,
    grid::{VoxelGrid, facing_dir},
    mech::{BlockEntity, network::Node},
    style::{BoxCache, Layer, LayerNode, Materials, Part, part, spawn_parts},
};

pub fn is_track(m: Option<Mech>) -> bool {
    matches!(m, Some(Mech::TrackDrive | Mech::TrackRoller))
}

/// Directions across the axis (the band runs along these).
fn across(axis: IVec3) -> Vec<IVec3> {
    crate::grid::DIRS.iter().copied().filter(|d| d.dot(axis) == 0).collect()
}

/// Kinetic links of the track rows: neighbours on the same axis turn alike.
pub fn links(grid: &VoxelGrid, nodes: &std::collections::HashMap<IVec3, Node>, mech: impl Fn(IVec3) -> Option<Mech> + Copy) -> Vec<(IVec3, IVec3, f32)> {
    let mut out = Vec::new();
    for &c in nodes.keys() {
        if !is_track(mech(c)) {
            continue;
        }
        let axis = facing_dir(grid.get(c).facing).abs();
        for d in across(axis) {
            let n = c + d;
            if (d.x + d.y + d.z) > 0 && is_track(mech(n)) && facing_dir(grid.get(n).facing).abs() == axis {
                out.push((c, n, 1.0));
            }
        }
    }
    out
}

/// The band around one track block: top and bottom runs along `run`
/// (block-local, across the axis = local Y), and a curved end where the
/// row ends (`end_pos` / `end_neg`).
pub fn band_parts(run: Vec3, end_pos: bool, end_neg: bool) -> Vec<Part> {
    let run = run.normalize_or(Vec3::Z);
    let up = Vec3::Y.cross(run).normalize_or(Vec3::Z);
    // Box axes: x along the axle (local Y), y radial, z along the band.
    let frame = |radial: Vec3, along: Vec3| Quat::from_mat3(&Mat3::from_cols(Vec3::Y, radial, along));
    const R: f32 = 0.55;
    let mut v = Vec::new();
    // Straight runs top and bottom, half a block each way from the centre
    // (not past the ends of the row).
    for (half, end) in [(1.0f32, end_pos), (-1.0, end_neg)] {
        if end {
            continue;
        }
        for s in [R, -R] {
            let r = frame(up * s.signum(), run);
            v.push(part([0.7, 0.1, 0.5], "rubber", (up * s + run * half * 0.25).to_array()).rot(r));
            v.push(part([0.74, 0.06, 0.08], "steel", (up * (s + s.signum() * 0.07) + run * half * 0.25).to_array()).rot(r));
        }
    }
    // Round the ends of the row: a half circle about the centre.
    for (end, sign) in [(end_pos, 1.0f32), (end_neg, -1.0)] {
        if !end {
            continue;
        }
        let n = 7;
        for i in 0..n {
            let a = (i as f32 / (n - 1) as f32 - 0.5) * std::f32::consts::PI;
            let radial = run * sign * a.cos() + up * a.sin();
            let along = up * a.cos() - run * sign * a.sin();
            v.push(part([0.7, 0.1, 0.32], "rubber", (radial * R).to_array()).rot(frame(radial, along)));
            v.push(part([0.74, 0.06, 0.08], "steel", (radial * (R + 0.07)).to_array()).rot(frame(radial, along)));
        }
    }
    v
}

/// A road wheel (steel) or the sprocket (brass teeth) on local Y.
pub fn roller_parts(sprocket: bool) -> Vec<Part> {
    let mut v = vec![
        part([0.6, 0.56, 0.6], "steel", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
        part([0.6, 0.56, 0.6], "steel", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)).layer(Layer::Rotor),
        part([0.24, 0.62, 0.24], "brass", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
        part([0.22, 0.5, 0.22], "shaft", [0.0, -0.25, 0.0]).layer(Layer::Rotor),
    ];
    if sprocket {
        for i in 0..10 {
            let a = i as f32 / 10.0 * std::f32::consts::TAU;
            v.push(part([0.12, 0.3, 0.14], "brass", [a.cos() * 0.4, 0.0, a.sin() * 0.4]).rot(Quat::from_rotation_y(-a)).layer(Layer::Rotor));
        }
    }
    v
}

/// What a track block's band was built for.
#[derive(Component, PartialEq)]
struct BandLook(IVec3, bool, bool);

#[derive(Component)]
struct Band;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn looks(
    mut commands: Commands,
    grids: Query<&VoxelGrid>,
    tracks: Query<(Entity, &BlockEntity, Option<&BandLook>, Option<&Children>)>,
    bands: Query<(), With<Band>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (e, b, look, children) in &tracks {
        if !is_track(b.mech()) {
            continue;
        }
        let Ok(grid) = grids.get(b.grid) else { continue };
        let axis = b.dir().abs();
        let same = |c: IVec3| {
            let cell = grid.get(c);
            !cell.is_air() && is_track(cell.id.def().mech) && facing_dir(cell.facing).abs() == axis
        };
        // The row runs horizontally if it can.
        let dirs = across(axis);
        let run = dirs.iter().copied().filter(|d| d.y == 0).find(|d| same(b.cell + *d) || same(b.cell - *d)).or_else(|| dirs.iter().copied().find(|d| d.y == 0)).unwrap_or(IVec3::X);
        let run = run.abs();
        let want = BandLook(run, !same(b.cell + run), !same(b.cell - run));
        if look == Some(&want) {
            continue;
        }
        for c in children.into_iter().flatten() {
            if bands.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        // Block-local: the model's +Y is the facing; the band lives in a
        // node of its own (not spinning).
        let rot = crate::models::block_rot(b.id.def().placement, b.facing).inverse();
        let run_l = rot * run.as_vec3();
        let node = commands.spawn((Band, LayerNode(Layer::Body), Transform::default(), Visibility::default(), ChildOf(e))).id();
        let (pos, neg) = (want.1, want.2);
        // Ends in local terms: run_l may point either way.
        spawn_parts(&mut commands, node, &band_parts(run_l, pos, neg), &mut meshes, &mut cache, &mats, true);
        commands.entity(e).insert(want);
    }
}

pub struct TracksPlugin;

impl Plugin for TracksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, looks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::BlockId,
        grid::{Cell, facing_of},
        mech::network::{Axis, Role, solve_with},
    };

    #[test]
    fn one_sprocket_drives_the_row() {
        let mut g = VoxelGrid::default();
        let f = facing_of(IVec3::X);
        g.set(IVec3::ZERO, Cell::new(BlockId::k("track_sprocket"), f));
        for z in 1..=3 {
            g.set(IVec3::new(0, 0, z), Cell::new(BlockId::k("track_roller"), f));
        }
        let mech = |c: IVec3| {
            let cell = g.get(c);
            if cell.is_air() { None } else { cell.id.def().mech }
        };
        let mut nodes = std::collections::HashMap::new();
        nodes.insert(IVec3::ZERO, Node::new(Role::Shaft, Axis::X).source(48.0));
        for z in 1..=3 {
            nodes.insert(IVec3::new(0, 0, z), Node::new(Role::Shaft, Axis::X));
        }
        let l = links(&g, &nodes, mech);
        assert_eq!(l.len(), 3);
        let s = solve_with(&nodes, &l);
        assert_eq!(s.speed[&IVec3::new(0, 0, 3)], 48.0);
    }
}
