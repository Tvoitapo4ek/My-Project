//! The world: an engineering proving ground floating in the void.
//!
//! * A 128 × 128 platform of dark concrete with lighter marking lines on
//!   the chunk borders (every 16 blocks).
//! * A test zone on the west edge (x −63 … −29), away from the centre
//!   where builds happen. Five lanes climb towards −Z to a plateau 3
//!   blocks high and back down:
//!   1. smooth 15° ramp, 2. smooth 30° ramp (static concrete solids),
//!   3. ≈27° ramp of half slopes, 4. ≈14° ramp of quarter slopes (blocks),
//!   5. a washboard of slabs.
//! * A test pool (32 × 32, 10 deep) on the east side, with a 25° ramp
//!   for driving in; the water is a volume (`water`), not blocks.

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::{
    content::BlockId,
    grid::{Cell, VoxelGrid, WorldGrid},
    style::Materials,
};

/// Half size of the platform (blocks); the top of the floor is at y = 0.
pub const PLATFORM_HALF: i32 = 64;

/// Where the player appears.
pub const SPAWN: Vec3 = Vec3::new(0.5, 1.0, 0.5);

/// The world grid entity.
#[derive(Resource)]
pub struct WorldEntity(pub Entity);

/// Test-zone lanes: west x of each 6-wide lane.
pub fn lane_x(k: i32) -> i32 {
    -63 + k * 7
}
/// Where the climb starts (lanes rise towards −Z from here).
pub const LANE_START_Z: i32 = 12;
pub const LANE_HEIGHT: f32 = 3.0;

/// The pool: x and z from `POOL.0` (inclusive) for `POOL_SIZE` blocks.
pub const POOL: IVec2 = IVec2::new(30, 16);
pub const POOL_SIZE: i32 = 32;
pub const POOL_DEPTH: i32 = 10;
/// Water surface: one block below the platform top.
pub const WATER_LEVEL: f32 = -1.0;

/// The water volume of the pool (min, max corners).
pub fn pool() -> (Vec3, Vec3) {
    (Vec3::new(POOL.x as f32, -POOL_DEPTH as f32, POOL.y as f32), Vec3::new((POOL.x + POOL_SIZE) as f32, WATER_LEVEL, (POOL.y + POOL_SIZE) as f32))
}

fn in_pool(x: i32, z: i32) -> bool {
    (POOL.x..POOL.x + POOL_SIZE).contains(&x) && (POOL.y..POOL.y + POOL_SIZE).contains(&z)
}

/// Basin: floor cut out, concrete walls and bottom.
fn pool_basin(grid: &mut VoxelGrid) {
    let (x0, z0, n) = (POOL.x, POOL.y, POOL_SIZE);
    for x in x0 - 1..=x0 + n {
        for z in z0 - 1..=z0 + n {
            let wall = !in_pool(x, z);
            for y in -POOL_DEPTH - 1..-1 {
                if wall || y == -POOL_DEPTH - 1 {
                    grid.set(IVec3::new(x, y, z), Cell::new(BlockId::k("concrete"), 2));
                }
            }
            if !wall {
                grid.set(IVec3::new(x, -1, z), Cell::AIR);
            }
        }
    }
}

fn floor_block(x: i32, z: i32) -> &'static str {
    if x.rem_euclid(16) == 0 || z.rem_euclid(16) == 0 { "concrete_line" } else { "concrete" }
}

/// Block lanes (3: 27°, 4: 14°, 5: washboard) into the world grid.
fn block_lanes(grid: &mut VoxelGrid) {
    let set = |g: &mut VoxelGrid, c: IVec3, k: &str, f: u8| {
        g.set(c, Cell::new(BlockId::k(k), f));
    };
    // (lane, pieces of one block step, rise per cell)
    for (lane, pieces) in [(2, vec!["steel_slope_half_low", "steel_slope_half_high"]), (3, vec!["steel_slope_q1", "steel_slope_q2", "steel_slope_q3", "steel_slope_q4"])] {
        let x0 = lane_x(lane);
        let n = pieces.len() as i32 * LANE_HEIGHT as i32;
        for x in x0..x0 + 6 {
            let mut z = LANE_START_Z;
            // Up (low side towards +Z = facing 4), then the plateau, then down.
            for i in 0..n {
                let h = i / pieces.len() as i32;
                for y in 0..h {
                    set(grid, IVec3::new(x, y, z), "concrete", 2);
                }
                set(grid, IVec3::new(x, h, z), pieces[i as usize % pieces.len()], 4);
                z -= 1;
            }
            for _ in 0..8 {
                for y in 0..LANE_HEIGHT as i32 {
                    set(grid, IVec3::new(x, y, z), "concrete", 2);
                }
                z -= 1;
            }
            for i in (0..n).rev() {
                let h = i / pieces.len() as i32;
                for y in 0..h {
                    set(grid, IVec3::new(x, y, z), "concrete", 2);
                }
                set(grid, IVec3::new(x, h, z), pieces[i as usize % pieces.len()], 5);
                z -= 1;
            }
        }
    }
    // Washboard: a slab every other block.
    let x0 = lane_x(4);
    for x in x0..x0 + 6 {
        for z in (-36..=LANE_START_Z).step_by(2) {
            set(grid, IVec3::new(x, 0, z), "steel_slab", 3);
        }
    }
}

/// A solid with a (z, y) profile extruded along X from `x0` to `x1`, as
/// a mesh (world-scale UVs) and a convex-hull collider.
fn prism(x0: f32, x1: f32, profile: &[(f32, f32)]) -> (Mesh, Collider) {
    let (mut pos, mut nor, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::<u32>::new());
    let n = profile.len();
    let centroid = {
        let (sz, sy) = profile.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        Vec3::new((x0 + x1) * 0.5, sy / n as f32, sz / n as f32)
    };
    // A convex polygon, wound so that it faces away from the centroid.
    let mut poly = |pts: Vec<Vec3>| {
        let mut nrm = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize_or_zero();
        let mid = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        let flip = nrm.dot(mid - centroid) < 0.0;
        if flip {
            nrm = -nrm;
        }
        let s = pos.len() as u32;
        for p in &pts {
            pos.push(p.to_array());
            nor.push(nrm.to_array());
            uv.push(if nrm.y.abs() > 0.5 { [p.x, p.z + p.y] } else if nrm.x.abs() > 0.5 { [p.z, -p.y] } else { [p.x, -p.y] });
        }
        for i in 1..pts.len() as u32 - 1 {
            if flip {
                idx.extend_from_slice(&[s, s + i + 1, s + i]);
            } else {
                idx.extend_from_slice(&[s, s + i, s + i + 1]);
            }
        }
    };
    for i in 0..n {
        let (za, ya) = profile[i];
        let (zb, yb) = profile[(i + 1) % n];
        poly(vec![Vec3::new(x0, ya, za), Vec3::new(x0, yb, zb), Vec3::new(x1, yb, zb), Vec3::new(x1, ya, za)]);
    }
    for x in [x0, x1] {
        poly(profile.iter().map(|(z, y)| Vec3::new(x, *y, *z)).collect());
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    let pts: Vec<Vec3> = profile.iter().flat_map(|(z, y)| [Vec3::new(x0, *y, *z), Vec3::new(x1, *y, *z)]).collect();
    (m, Collider::convex_hull(&pts).expect("ramp hull"))
}

/// Smooth ramps (lanes 1 and 2): up, plateau, down — three static solids.
fn smooth_ramps(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mats: Res<Materials>) {
    let h = LANE_HEIGHT;
    for (lane, deg) in [(0, 15.0f32), (1, 30.0)] {
        let x0 = lane_x(lane) as f32;
        let x1 = x0 + 6.0;
        let run = h / deg.to_radians().tan();
        let z0 = LANE_START_Z as f32 + 1.0;
        let (zt, zp) = (z0 - run, z0 - run - 8.0);
        let zd = zp - run;
        // Profiles (z, y); faces are wound outwards automatically.
        let parts: [Vec<(f32, f32)>; 3] = [
            vec![(z0, 0.0), (zt, 0.0), (zt, h)],
            vec![(zt, 0.0), (zp, 0.0), (zp, h), (zt, h)],
            vec![(zp, 0.0), (zd, 0.0), (zp, h)],
        ];
        for p in parts {
            let (mesh, col) = prism(x0, x1, &p);
            commands.spawn((Name::new(format!("Ramp {deg}°")), Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats.get("concrete")), Transform::default(), RigidBody::Static, col));
        }
    }
    // Into the pool: 25° from the rim down to the bottom, along +Z.
    let (x0, z0) = (POOL.x as f32, POOL.y as f32);
    let run = POOL_DEPTH as f32 / 25f32.to_radians().tan();
    let (mesh, col) = prism(x0, x0 + 6.0, &[(z0, 0.0), (z0, -POOL_DEPTH as f32), (z0 + run, -POOL_DEPTH as f32)]);
    commands.spawn((Name::new("Pool ramp"), Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats.get("concrete")), Transform::default(), RigidBody::Static, col));
}

fn spawn_world(mut commands: Commands) {
    let mut grid = VoxelGrid::default();
    for x in -PLATFORM_HALF..PLATFORM_HALF {
        for z in -PLATFORM_HALF..PLATFORM_HALF {
            grid.set(IVec3::new(x, -1, z), Cell::new(BlockId::k(floor_block(x, z)), 2));
        }
    }
    block_lanes(&mut grid);
    pool_basin(&mut grid);
    let e = commands.spawn((Name::new("World grid"), WorldGrid, grid, Transform::default(), Visibility::default(), RigidBody::Static)).id();
    commands.insert_resource(WorldEntity(e));
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_world, smooth_ramps));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proving_ground_layout() {
        assert_eq!(floor_block(16, 3), "concrete_line");
        assert_eq!(floor_block(5, 3), "concrete");
        // The test zone stays clear of the centre (|x| < 30 used by tests).
        assert!(lane_x(4) + 6 <= -28);
        let mut g = VoxelGrid::default();
        block_lanes(&mut g);
        // 27° lane: the top of its last up-piece is at the plateau height.
        let x = lane_x(2);
        assert_eq!(g.get(IVec3::new(x, 2, LANE_START_Z - 5)).id, BlockId::k("steel_slope_half_high"));
        assert!(!g.get(IVec3::new(x, 2, LANE_START_Z - 6)).is_air());
        // The pool: no floor over the water, a bottom, walls; clear of the lanes.
        let mut g = VoxelGrid::default();
        pool_basin(&mut g);
        assert!(g.get(IVec3::new(POOL.x + 3, -1, POOL.y + 3)).is_air());
        assert!(!g.get(IVec3::new(POOL.x + 3, -POOL_DEPTH - 1, POOL.y + 3)).is_air());
        assert!(!g.get(IVec3::new(POOL.x - 1, -5, POOL.y + 3)).is_air());
        assert!(POOL.x > lane_x(4) + 6);
        let (_, col) = prism(0.0, 6.0, &[(1.0, 0.0), (-10.0, 0.0), (-10.0, 3.0)]);
        assert!(col.mass_properties(1.0).mass > 0.0);
    }
}
