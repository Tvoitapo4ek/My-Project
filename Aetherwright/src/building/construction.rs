//! Applying selections and blueprints to the world (exclusive access: they
//! touch voxels, entities, joints and the inventory at once).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::blueprints::{Blueprint, BlueprintJoint, BlueprintPart};
use crate::{
    core::rules::ModeRules,
    items::{block_drop, inventory::Inventory, ItemId},
    machinery::components::{spawn_part, Part, PartAssets},
    physics::joints::{spawn_connection, Connection},
    player::Player,
    world::{
        chunks::{EditCause, VoxelWorld},
        interaction::BlockEdit,
        voxel::blocks,
    },
};

/// Axis-aligned block box (inclusive corners).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub min: IVec3,
    pub max: IVec3,
}

impl Area {
    pub fn new(a: IVec3, b: IVec3) -> Self {
        Self { min: a.min(b), max: a.max(b) }
    }
    pub fn size(&self) -> IVec3 {
        self.max - self.min + IVec3::ONE
    }
    pub fn volume(&self) -> i64 {
        let s = self.size();
        s.x as i64 * s.y as i64 * s.z as i64
    }
    pub fn contains_point(&self, p: Vec3) -> bool {
        p.cmpge(self.min.as_vec3()).all() && p.cmplt((self.max + IVec3::ONE).as_vec3()).all()
    }
    pub fn cells(&self) -> impl Iterator<Item = IVec3> + '_ {
        (self.min.y..=self.max.y).flat_map(move |y| {
            (self.min.z..=self.max.z).flat_map(move |z| (self.min.x..=self.max.x).map(move |x| IVec3::new(x, y, z)))
        })
    }
}

pub const MAX_AREA_VOLUME: i64 = 32 * 32 * 32;

/// Parts whose centre lies inside the area.
fn parts_in(world: &mut World, area: Area) -> Vec<(Entity, Part, Transform)> {
    let mut q = world.query::<(Entity, &Part, &Position, &Rotation)>();
    q.iter(world)
        .filter(|(_, _, p, _)| area.contains_point(p.0))
        .map(|(e, part, p, r)| (e, *part, Transform::from_translation(p.0).with_rotation(r.0)))
        .collect()
}

/// Copies an area into a blueprint.
pub fn capture(world: &mut World, area: Area) -> Blueprint {
    let voxels = world.resource::<VoxelWorld>();
    let blocks: Vec<(IVec3, crate::world::voxel::BlockId)> = area
        .cells()
        .filter_map(|c| {
            let b = voxels.get(c);
            (!b.is_air() && !b.is_liquid() && b != blocks::BEDROCK).then_some((c - area.min, b))
        })
        .collect();
    let parts = parts_in(world, area);
    let index_of = |e: Entity| parts.iter().position(|(pe, _, _)| *pe == e);
    let mut jq = world.query::<&Connection>();
    let joints = jq
        .iter(world)
        .filter_map(|c| {
            Some(BlueprintJoint { a: index_of(c.a)?, b: index_of(c.b)?, kind: c.kind, frame_a: c.frame_a, frame_b: c.frame_b })
        })
        .collect();
    Blueprint {
        size: area.size(),
        blocks,
        parts: parts
            .iter()
            .map(|(_, p, t)| BlueprintPart { kind: p.kind, offset: t.translation - area.min.as_vec3(), rotation: t.rotation })
            .collect(),
        joints,
    }
}

/// Removes everything in an area. In survival the items go back to the
/// player (what does not fit is lost — the area tool warns beforehand).
pub fn clear_area(world: &mut World, area: Area) -> usize {
    let refund = world.resource::<ModeRules>().consume_items;
    let mut removed = 0;
    let cells: Vec<IVec3> = area.cells().collect();
    let mut items: Vec<ItemId> = Vec::new();
    for c in cells {
        let b = world.resource::<VoxelWorld>().get(c);
        if b.is_air() || b.is_liquid() || b.def().hardness.is_infinite() {
            continue;
        }
        if let Some(i) = block_drop(b) {
            items.push(i);
        }
        world.write_message(BlockEdit { pos: c, block: blocks::AIR, cause: EditCause::Creative });
        removed += 1;
    }
    for (e, part, _) in parts_in(world, area) {
        if let Some(i) = ItemId::from_key(part.kind.key()) {
            items.push(i);
        }
        world.despawn(e);
        removed += 1;
    }
    if refund {
        let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
        if let Some(mut inv) = q.iter_mut(world).next() {
            for i in items {
                inv.add(i, 1);
            }
        }
    }
    removed
}

/// Why a paste failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    MissingItems(Vec<(ItemId, u32)>),
    OutOfWorld,
}

/// Builds a blueprint with its origin at `origin`. Occupied cells keep
/// their content (no silent destruction). Consumes items in survival.
pub fn paste(world: &mut World, bp: &Blueprint, origin: IVec3) -> Result<usize, PasteError> {
    {
        let voxels = world.resource::<VoxelWorld>();
        if !voxels.in_bounds(origin) || !voxels.in_bounds(origin + bp.size - IVec3::ONE) {
            return Err(PasteError::OutOfWorld);
        }
    }
    let consume = world.resource::<ModeRules>().consume_items;
    if consume {
        let mut q = world.query_filtered::<&Inventory, With<Player>>();
        let inv = q.iter(world).next().cloned().unwrap_or_default();
        let missing: Vec<(ItemId, u32)> = bp
            .cost()
            .into_iter()
            .filter_map(|(i, n)| {
                let have = inv.count(i);
                (have < n).then(|| (i, n - have))
            })
            .collect();
        if !missing.is_empty() {
            return Err(PasteError::MissingItems(missing));
        }
        let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
        if let Some(mut inv) = q.iter_mut(world).next() {
            for (i, n) in bp.cost() {
                inv.remove(i, n);
            }
        }
    }
    let mut placed = 0;
    for (rel, b) in &bp.blocks {
        let p = origin + *rel;
        if world.resource::<VoxelWorld>().get(p).is_replaceable() {
            world.write_message(BlockEdit { pos: p, block: *b, cause: EditCause::Creative });
            placed += 1;
        }
    }
    let assets = world.remove_resource::<PartAssets>();
    if let Some(assets) = assets {
        let mut spawned = Vec::new();
        {
            let mut commands = world.commands();
            for p in &bp.parts {
                let t = Transform::from_translation(origin.as_vec3() + p.offset).with_rotation(p.rotation);
                spawned.push(spawn_part(&mut commands, &assets, p.kind, t));
                placed += 1;
            }
            for j in &bp.joints {
                spawn_connection(&mut commands, Connection { a: spawned[j.a], b: spawned[j.b], kind: j.kind, frame_a: j.frame_a, frame_b: j.frame_b });
            }
        }
        world.flush();
        world.insert_resource(assets);
    }
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_math() {
        let a = Area::new(IVec3::new(5, 2, 1), IVec3::new(3, 4, 1));
        assert_eq!(a.min, IVec3::new(3, 2, 1));
        assert_eq!(a.size(), IVec3::new(3, 3, 1));
        assert_eq!(a.cells().count(), 9);
        assert!(a.contains_point(Vec3::new(5.9, 4.9, 1.5)));
        assert!(!a.contains_point(Vec3::new(6.0, 3.0, 1.5)));
    }
}
