//! Moving blocks between grids: split a group off into its own grid entity
//! and merge a grid back into another one. Used by the physics toggle
//! (world ↔ contraption) and by bearings and pistons (a moving part on top
//! of its base).
//!
//! Everything a block carries moves with it: its block entity (settings,
//! state), links that point at it and the glue around it.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::{
    contraption::snap_rotation,
    grid::{Cell, VoxelGrid, facing_dir, facing_of},
    mech::{BlockEntity, transform_for},
    signal::Linked,
    tools::glue::{Glue, inside},
};

/// Moves `cells` of grid `src` into a new grid entity placed exactly where
/// `src` is (same cell coordinates). Glue boxes touching the cells go with
/// them; `take_glue` also removes them from `src`. Returns the new entity
/// (no physics yet: the caller adds the body).
pub fn split_off(world: &mut World, src: Entity, cells: &[IVec3], take_glue: bool) -> Entity {
    let t = world.get::<GlobalTransform>(src).map(|g| g.compute_transform()).unwrap_or_default();
    let boxes: Vec<(IVec3, IVec3)> = world.get::<Glue>(src).map(|g| g.0.iter().copied().filter(|(lo, hi)| cells.iter().any(|c| inside(*c, *lo, *hi))).collect()).unwrap_or_default();
    let mut new_grid = VoxelGrid::default();
    let mut moved = Vec::new();
    {
        let mut g = world.get_mut::<VoxelGrid>(src).expect("source grid");
        for c in cells {
            let cell = g.get(*c);
            if let Some(e) = g.entities.remove(c) {
                new_grid.entities.insert(*c, e);
                moved.push(e);
            }
            g.set(*c, Cell::AIR);
            new_grid.set(*c, cell);
        }
    }
    if take_glue && let Some(mut g) = world.get_mut::<Glue>(src) {
        g.0.retain(|b| !boxes.contains(b));
    }
    let set: HashSet<IVec3> = cells.iter().copied().collect();
    // Chains with both ends in the group go with it.
    let mut chains = crate::chains::Chains::default();
    if let Some(mut ch) = world.get_mut::<crate::chains::Chains>(src) {
        let (go, stay): (Vec<_>, Vec<_>) = ch.0.iter().copied().partition(|l| set.contains(&l.a) && set.contains(&l.b));
        if !go.is_empty() {
            ch.0 = stay;
        }
        chains.0 = go;
    }
    let body = world.spawn((Name::new("Contraption"), new_grid, Glue(boxes), chains, t, Visibility::default())).id();
    for e in moved {
        if let Some(mut b) = world.get_mut::<BlockEntity>(e) {
            b.grid = body;
        }
        world.entity_mut(e).insert(ChildOf(body));
    }
    let mut q = world.query::<&mut Linked>();
    for mut l in q.iter_mut(world) {
        if let Some(p) = l.0.as_mut()
            && p.grid == src
            && set.contains(&p.cell)
        {
            p.grid = body;
        }
    }
    body
}

/// Where the cells of `src` land in `dst`: (cell map, rotation of facings).
/// The relative rotation is snapped to the nearest grid orientation.
pub fn mapping(world: &World, src: Entity, dst: Entity) -> Option<(impl Fn(IVec3) -> IVec3 + use<>, Quat)> {
    let ts = *world.get::<GlobalTransform>(src)?;
    let td = *world.get::<GlobalTransform>(dst)?;
    let inv = td.affine().inverse();
    let (tdt, tst) = (td.compute_transform(), ts.compute_transform());
    let rot = snap_rotation(tdt.rotation.inverse() * tst.rotation);
    // Positions with the snapped rotation (the body is turned onto the grid
    // about its origin), so no two cells land in the same place.
    let world_rot = tdt.rotation * rot;
    let origin = tst.translation;
    let map = move |c: IVec3| inv.transform_point3(origin + world_rot * (c.as_vec3() + Vec3::splat(0.5))).floor().as_ivec3();
    Some((map, rot))
}

/// Why blocks could not go back into a grid.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeError {
    /// A target cell is taken by a block (even one block higher).
    NoRoom,
    /// The player or another body is where the blocks would appear (they
    /// would be shot out of it).
    Blocked { player: bool },
}

/// Dynamic bodies (the player, other machines) overlapping these cells of
/// `dst`, except `src` and parts mounted on it.
pub fn bodies_in(world: &mut World, src: Entity, dst: Entity, cells: &[IVec3]) -> Vec<Entity> {
    use avian3d::prelude::*;
    let Some(td) = world.get::<GlobalTransform>(dst).map(|g| g.compute_transform()) else { return Vec::new() };
    let mut state = bevy::ecs::system::SystemState::<(SpatialQuery, Query<&ColliderOf>, Query<&RigidBody>, Query<&crate::mounts::MountedOn>, Query<&BlockEntity>)>::new(world);
    let Ok((spatial, colliders, bodies, mounted, blocks)) = state.get(world) else { return Vec::new() };
    let shape = Collider::cuboid(0.9, 0.9, 0.9);
    let filter = SpatialQueryFilter::default();
    let mut out = Vec::new();
    for c in cells {
        let p = td.transform_point(c.as_vec3() + Vec3::splat(0.5));
        for hit in spatial.shape_intersections(&shape, p, td.rotation, &filter) {
            let body = colliders.get(hit).map(|c| c.body).unwrap_or(hit);
            if body == src || body == dst || out.contains(&body) {
                continue;
            }
            // Parts riding on the returning grid come back with it.
            if mounted.get(body).is_ok_and(|m| blocks.get(m.mount).is_ok_and(|b| b.grid == src)) {
                continue;
            }
            if bodies.get(body).is_ok_and(|r| r.is_dynamic()) {
                out.push(body);
            }
        }
    }
    out
}

/// Puts every block of `src` into `dst` at the nearest grid position and
/// orientation (or one or two blocks higher if it sank in), then removes
/// `src`. Parts mounted on `src` (bearings, pistons) are merged into it
/// first. On an error nothing in `src` changes.
pub fn merge_into(world: &mut World, src: Entity, dst: Entity) -> Result<(), MergeError> {
    if !crate::mounts::collapse_all(world, src) {
        return Err(MergeError::NoRoom);
    }
    let Some((map0, rot)) = mapping(world, src, dst) else { return Err(MergeError::NoRoom) };
    let turn = |f: u8| facing_of((rot * facing_dir(f).as_vec3()).round().as_ivec3());
    let (cells, glue_boxes, entities) = {
        let g = world.get::<VoxelGrid>(src).expect("source grid");
        let glue = world.get::<Glue>(src).map(|g| g.0.clone()).unwrap_or_default();
        (g.cells(), glue, g.entities.iter().map(|(c, e)| (*c, *e)).collect::<Vec<_>>())
    };
    let lift = {
        let dg = world.get::<VoxelGrid>(dst).expect("target grid");
        (0..3).map(|k| IVec3::Y * k).find(|off| cells.iter().all(|(c, _)| dg.get(map0(*c) + *off).is_air()))
    };
    let Some(lift) = lift else { return Err(MergeError::NoRoom) };
    let map = move |c: IVec3| map0(c) + lift;
    let targets: Vec<IVec3> = cells.iter().map(|(c, _)| map(*c)).collect();
    let player = world.query_filtered::<Entity, With<crate::player::Player>>().iter(world).next();
    let blockers = bodies_in(world, src, dst, &targets);
    if !blockers.is_empty() {
        return Err(MergeError::Blocked { player: player.is_some_and(|p| blockers.contains(&p)) });
    }
    {
        let mut dg = world.get_mut::<VoxelGrid>(dst).unwrap();
        for (c, e) in &entities {
            dg.entities.insert(map(*c), *e);
        }
        for (c, cell) in &cells {
            dg.set(map(*c), Cell::new(cell.id, turn(cell.facing)));
        }
    }
    for (c, e) in &entities {
        if let Some(mut b) = world.get_mut::<BlockEntity>(*e) {
            b.grid = dst;
            b.cell = map(*c);
            b.facing = turn(b.facing);
            let t = transform_for(b.cell, b.id, b.facing);
            if let Some(mut tr) = world.get_mut::<Transform>(*e) {
                *tr = t;
            }
        }
        world.entity_mut(*e).insert(ChildOf(dst));
    }
    let mut q = world.query::<&mut Linked>();
    for mut l in q.iter_mut(world) {
        if let Some(p) = l.0.as_mut()
            && p.grid == src
        {
            p.grid = dst;
            p.cell = map(p.cell);
        }
    }
    let mapped: Vec<(IVec3, IVec3)> = glue_boxes
        .iter()
        .map(|(lo, hi)| {
            let (a, b) = (map(*lo), map(*hi));
            (a.min(b), a.max(b))
        })
        .collect();
    let mut all = world.get::<Glue>(dst).map(|g| g.0.clone()).unwrap_or_default();
    for b in mapped {
        if !all.contains(&b) {
            all.push(b);
        }
    }
    world.entity_mut(dst).insert(Glue(all));
    // Chains: mapped like the cells (the sides turn with them).
    let moved: Vec<crate::chains::ChainLink> = world.get::<crate::chains::Chains>(src).map(|c| c.0.clone()).unwrap_or_default();
    if !moved.is_empty() {
        let mut dst_chains = world.get::<crate::chains::Chains>(dst).cloned().unwrap_or_default();
        dst_chains.0.extend(moved.into_iter().map(|l| crate::chains::ChainLink { a: map(l.a), b: map(l.b), ..l }));
        world.entity_mut(dst).insert(dst_chains);
    }
    world.get_mut::<VoxelGrid>(src).unwrap().entities.clear();
    world.entity_mut(src).despawn();
    Ok(())
}

/// Mass of a set of cells (kg).
pub fn mass_of(grid: &VoxelGrid, cells: &[IVec3]) -> f32 {
    cells.iter().map(|c| grid.get(*c).id.def().mass).sum()
}
