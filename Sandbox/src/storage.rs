//! Storages: chest, barrel (27 slots each) and the container — containers
//! next to each other (any face) are one storage, 36 slots per block, like
//! tanks are one tank.
//!
//! * RMB opens it above the inventory (the same clicks as the inventory;
//!   Shift+click moves a stack across).
//! * Item tubes and drives (`tubes`) put items in and take them out; later
//!   drills and saws will drop what they break into them.
//! * Contents are saved with the world and travel with machines.

use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;

use crate::{
    content::Mech,
    core::Ui,
    grid::{DIRS, VoxelGrid},
    interact::UseBlock,
    inventory::Stack,
    mech::BlockEntity,
};

pub const STORAGE_MECHS: [Mech; 3] = [Mech::Chest, Mech::Barrel, Mech::Container];

/// Slots of a storage block.
#[derive(Component, Debug, Clone, Default)]
pub struct ItemStore {
    pub slots: Vec<Option<Stack>>,
}

impl ItemStore {
    pub fn new(n: usize) -> Self {
        Self { slots: vec![None; n] }
    }
    /// Puts as much of `s` as fits; returns what is left.
    pub fn insert(&mut self, mut s: Stack) -> Option<Stack> {
        let max = s.item.def().max_stack;
        for slot in self.slots.iter_mut().flatten() {
            if slot.item == s.item && slot.count < max {
                let n = (max - slot.count).min(s.count);
                slot.count += n;
                s.count -= n;
                if s.count == 0 {
                    return None;
                }
            }
        }
        for slot in &mut self.slots {
            if slot.is_none() {
                *slot = Some(s);
                return None;
            }
        }
        Some(s)
    }
    /// Takes up to `n` of the first item kind found.
    pub fn take(&mut self, n: u32) -> Option<Stack> {
        let i = self.slots.iter().position(|s| s.is_some())?;
        let mut s = self.slots[i]?;
        let got = s.count.min(n);
        s.count -= got;
        self.slots[i] = (s.count > 0).then_some(s);
        Some(Stack { item: s.item, count: got })
    }
    pub fn room_for(&self, s: Stack) -> u32 {
        let max = s.item.def().max_stack;
        self.slots.iter().map(|x| match x {
            None => max,
            Some(h) if h.item == s.item => max - h.count,
            _ => 0,
        }).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| s.is_none())
    }
}

pub fn slots_of(m: Mech) -> usize {
    match m {
        Mech::Container => 36,
        _ => 27,
    }
}

/// The container blocks joined with `start` (face neighbours), sorted.
pub fn container_group(grid: &VoxelGrid, start: IVec3) -> Vec<IVec3> {
    let is = |c: IVec3| grid.get(c).id.def().mech == Some(Mech::Container);
    if !is(start) {
        return vec![start];
    }
    let mut seen = HashSet::from([start]);
    let mut q = VecDeque::from([start]);
    while let Some(c) = q.pop_front() {
        for d in DIRS {
            let n = c + d;
            if is(n) && seen.insert(n) && seen.len() < 512 {
                q.push_back(n);
            }
        }
    }
    let mut v: Vec<IVec3> = seen.into_iter().collect();
    v.sort_by_key(|c| (c.y, c.z, c.x));
    v
}

/// The storage shown above the inventory: its blocks and their slot counts.
#[derive(Resource, Default, Debug, Clone)]
pub struct OpenStore {
    pub title: String,
    pub parts: Vec<(Entity, usize)>,
}

impl OpenStore {
    pub fn len(&self) -> usize {
        self.parts.iter().map(|p| p.1).sum()
    }
    /// Slot `i` of the whole view → (block entity, its slot).
    pub fn locate(&self, mut i: usize) -> Option<(Entity, usize)> {
        for (e, n) in &self.parts {
            if i < *n {
                return Some((*e, i));
            }
            i -= n;
        }
        None
    }
}

fn add_stores(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<ItemStore>)>) {
    for (e, b) in &added {
        if let Some(m) = b.mech().filter(|m| STORAGE_MECHS.contains(m)) {
            commands.entity(e).insert(ItemStore::new(slots_of(m)));
        }
    }
}

fn open(mut used: MessageReader<UseBlock>, grids: Query<&VoxelGrid>, mut open: ResMut<OpenStore>, mut next: ResMut<NextState<Ui>>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    for u in used.read() {
        if !STORAGE_MECHS.contains(&u.mech) {
            continue;
        }
        let Ok(grid) = grids.get(u.grid) else { continue };
        let cells = container_group(grid, u.cell);
        let parts: Vec<(Entity, usize)> = cells.iter().filter_map(|c| grid.entities.get(c).map(|e| (*e, slots_of(u.mech)))).collect();
        if parts.is_empty() {
            continue;
        }
        let name = grid.get(u.cell).id.def().name.clone();
        open.title = if parts.len() > 1 { format!("{name} × {}", parts.len()) } else { name };
        open.parts = parts;
        next.set(Ui::Inventory);
        sounds.write(crate::sound::sfx("ui_click"));
    }
}

fn close(mut open: ResMut<OpenStore>) {
    open.parts.clear();
}

fn hook_save(w: &World, e: Entity, _: &crate::save::SaveCtx) -> Option<String> {
    let s = w.get::<ItemStore>(e)?;
    if s.is_empty() {
        return None;
    }
    let v: Vec<(usize, String, u32)> = s.slots.iter().enumerate().filter_map(|(i, x)| x.map(|x| (i, x.item.def().key.clone(), x.count))).collect();
    crate::save::to_ron(&v)
}

fn hook_load(w: &mut World, e: Entity, d: &str, _: &crate::save::LoadCtx) {
    let Some(v) = crate::save::from_ron::<Vec<(usize, String, u32)>>(d) else { return };
    if let Some(mut s) = w.get_mut::<ItemStore>(e) {
        for (i, k, n) in v {
            if let (Some(slot), Some(item)) = (s.slots.get_mut(i), crate::content::registry().item(&k)) {
                *slot = Some(Stack { item, count: n });
            }
        }
    }
}

pub struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenStore>().init_resource::<crate::save::SaveHooks>()
            .add_systems(Update, (add_stores, open.after(crate::interact::InteractSet)))
            .add_systems(OnExit(Ui::Inventory), close);
        app.world_mut().resource_mut::<crate::save::SaveHooks>().add("items", hook_save, hook_load);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::{BlockId, ItemId}, grid::Cell};

    #[test]
    fn insert_take_and_room() {
        let mut s = ItemStore::new(2);
        let stone = ItemId::k("stone");
        assert_eq!(s.insert(Stack::new(stone, 60)), None);
        assert_eq!(s.insert(Stack::new(stone, 10)), None, "tops up, then a new slot");
        assert_eq!(s.slots[0].unwrap().count, 64);
        assert_eq!(s.room_for(Stack::new(stone, 1)), 64 - 6);
        let wrench = ItemId::k("wrench");
        assert_eq!(s.insert(Stack::new(wrench, 1)).map(|x| x.count), Some(1), "full: it comes back");
        assert_eq!(s.take(8).map(|x| x.count), Some(8));
        assert_eq!(s.slots[0].unwrap().count, 56);
    }

    #[test]
    fn containers_join() {
        let mut g = VoxelGrid::default();
        let c = Cell::new(BlockId::k("item_container"), 2);
        for p in [IVec3::ZERO, IVec3::X, IVec3::new(1, 1, 0), IVec3::new(5, 0, 0)] {
            g.set(p, c);
        }
        assert_eq!(container_group(&g, IVec3::ZERO).len(), 3, "touching ones join, the far one does not");
        g.set(IVec3::new(0, 0, 1), Cell::new(BlockId::k("chest"), 2));
        assert_eq!(container_group(&g, IVec3::new(0, 0, 1)), vec![IVec3::new(0, 0, 1)], "chests stay alone");
        let o = OpenStore { title: String::new(), parts: vec![(Entity::PLACEHOLDER, 36), (Entity::PLACEHOLDER, 36)] };
        assert_eq!(o.len(), 72);
        assert_eq!(o.locate(40).map(|x| x.1), Some(4));
        assert_eq!(o.locate(72), None);
    }
}
