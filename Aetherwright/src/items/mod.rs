//! Items, stacks and inventories (pure data, no UI).
//!
//! Item definitions come from the content registry (`assets/content/
//! items.ron`, mods). Saves refer to items by their stable string `key`,
//! never by index, so content can be reordered or extended safely.

pub mod drops;
pub mod inventory;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub use crate::content::{Category, ItemDef, ItemUse as Use};
use crate::world::voxel::BlockId;

/// Index of an item in the content registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u16);

/// All registered items (in registry order; `ItemId(i)` ↔ `all_items()[i]`).
pub fn all_items() -> &'static [ItemDef] {
    &crate::content::registry().items
}

impl ItemId {
    pub fn def(self) -> &'static ItemDef {
        &all_items()[self.0 as usize]
    }
    pub fn from_key(key: &str) -> Option<ItemId> {
        crate::content::registry().item_id(key).map(ItemId)
    }
    /// Panics on unknown key — for keys written in code (checked by tests).
    pub fn k(key: &str) -> ItemId {
        Self::from_key(key).unwrap_or_else(|| panic!("unknown item key {key}"))
    }
    pub fn block(self) -> Option<BlockId> {
        match &self.def().use_ {
            Use::Block(k) => BlockId::from_key(k),
            _ => None,
        }
    }
    pub fn color(self) -> Color {
        let (r, g, b) = self.def().color;
        Color::srgb(r, g, b)
    }
}

/// Item dropped by a block (resolves the block's `drop` key).
pub fn block_drop(b: BlockId) -> Option<ItemId> {
    let k = &b.def().drop;
    if k.is_empty() {
        None
    } else {
        ItemId::from_key(k)
    }
}

/// Item that places a given block (for creative pick-block / refunds).
pub fn item_for_block(b: BlockId) -> Option<ItemId> {
    let key = &b.def().key;
    all_items()
        .iter()
        .position(|i| matches!(&i.use_, Use::Block(k) if k == key))
        .map(|i| ItemId(i as u16))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u32,
}

/// Serialized form (stable keys).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub count: u32,
}

impl ItemStack {
    pub fn new(item: ItemId, count: u32) -> Self {
        Self { item, count }
    }
    pub fn to_saved(self) -> SavedStack {
        SavedStack {
            item: self.item.def().key.to_string(),
            count: self.count,
        }
    }
    pub fn from_saved(s: &SavedStack) -> Option<Self> {
        let item = ItemId::from_key(&s.item)?;
        (s.count > 0).then(|| Self {
            item,
            count: s.count.min(item.def().max_stack),
        })
    }
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((inventory::InventoryPlugin, drops::DropsPlugin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_valid() {
        for i in all_items() {
            if let Use::Block(b) = &i.use_ {
                assert!(
                    BlockId::from_key(b).is_some(),
                    "item {} places unknown block {b}",
                    i.key
                );
            }
            assert!(i.max_stack >= 1);
        }
        for b in &crate::content::registry().blocks {
            if !b.drop.is_empty() {
                assert!(
                    ItemId::from_key(&b.drop).is_some(),
                    "block {} drops unknown item {}",
                    b.key,
                    b.drop
                );
            }
        }
    }
}
