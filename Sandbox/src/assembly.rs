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

/// Puts every block of `src` into `dst` at the nearest grid position and
/// orientation, then removes `src`. Parts mounted on `src` (bearings,
/// pistons) are merged into it first. Returns false — and changes nothing
/// in `src` — if a target cell is taken.
pub fn merge_into(world: &mut World, src: Entity, dst: Entity) -> bool {
    if !crate::mounts::collapse_all(world, src) {
        return false;
    }
    let Some((map, rot)) = mapping(world, src, dst) else { return false };
    let turn = |f: u8| facing_of((rot * facing_dir(f).as_vec3()).round().as_ivec3());
    let (cells, glue_boxes, entities) = {
        let g = world.get::<VoxelGrid>(src).expect("source grid");
        let glue = world.get::<Glue>(src).map(|g| g.0.clone()).unwrap_or_default();
        (g.cells(), glue, g.entities.iter().map(|(c, e)| (*c, *e)).collect::<Vec<_>>())
    };
    {
        let dg = world.get::<VoxelGrid>(dst).expect("target grid");
        if cells.iter().any(|(c, _)| !dg.get(map(*c)).is_air()) {
            return false;
        }
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
    true
}

/// Mass of a set of cells (kg).
pub fn mass_of(grid: &VoxelGrid, cells: &[IVec3]) -> f32 {
    cells.iter().map(|c| grid.get(*c).id.def().mass).sum()
}
