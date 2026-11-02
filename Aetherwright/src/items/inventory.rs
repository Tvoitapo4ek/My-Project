//! Slot inventory: one grid; the first `HOTBAR` slots are the hotbar.

use bevy::prelude::*;

use super::{ItemId, ItemStack, SavedStack};

pub const SLOTS: usize = 36;
pub const HOTBAR: usize = 9;

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
    /// Selected hotbar slot (0..HOTBAR).
    pub selected: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { slots: vec![None; SLOTS], selected: 0 }
    }
}

impl Inventory {
    pub fn selected_stack(&self) -> Option<ItemStack> {
        self.slots[self.selected]
    }

    pub fn selected_item(&self) -> Option<ItemId> {
        self.selected_stack().map(|s| s.item)
    }

    /// Adds items: fills existing stacks first (hotbar first), then empty
    /// slots. Returns how many did not fit.
    pub fn add(&mut self, item: ItemId, mut count: u32) -> u32 {
        let max = item.def().max_stack;
        for slot in self.slots.iter_mut().flatten() {
            if count == 0 {
                break;
            }
            if slot.item == item && slot.count < max {
                let n = (max - slot.count).min(count);
                slot.count += n;
                count -= n;
            }
        }
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let n = max.min(count);
                *slot = Some(ItemStack::new(item, n));
                count -= n;
            }
        }
        count
    }

    pub fn count(&self, item: ItemId) -> u32 {
        self.slots.iter().flatten().filter(|s| s.item == item).map(|s| s.count).sum()
    }

    /// Removes `count` items if available (all-or-nothing).
    pub fn remove(&mut self, item: ItemId, count: u32) -> bool {
        if self.count(item) < count {
            return false;
        }
        let mut left = count;
        // Take from the back so the hotbar keeps its stacks longest.
        for slot in self.slots.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if let Some(s) = slot
                && s.item == item
            {
                let n = s.count.min(left);
                s.count -= n;
                left -= n;
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
        true
    }

    /// Removes one item from a specific slot.
    pub fn take_one(&mut self, slot: usize) -> Option<ItemId> {
        let s = self.slots.get_mut(slot)?;
        let stack = s.as_mut()?;
        let item = stack.item;
        stack.count -= 1;
        if stack.count == 0 {
            *s = None;
        }
        Some(item)
    }

    /// Click on a slot while carrying `cursor` (classic grid interaction):
    /// empty cursor picks the stack up, same item merges, otherwise swaps.
    pub fn click_slot(&mut self, slot: usize, cursor: &mut Option<ItemStack>) {
        let Some(target) = self.slots.get_mut(slot) else { return };
        match (target.as_mut(), cursor.as_mut()) {
            (Some(t), Some(c)) if t.item == c.item => {
                let max = t.item.def().max_stack;
                let n = (max - t.count).min(c.count);
                t.count += n;
                c.count -= n;
                if c.count == 0 {
                    *cursor = None;
                }
            }
            _ => std::mem::swap(target, cursor),
        }
    }

    /// Right click: split half off, or drop a single item from the cursor.
    pub fn right_click_slot(&mut self, slot: usize, cursor: &mut Option<ItemStack>) {
        let Some(target) = self.slots.get_mut(slot) else { return };
        match (target.as_mut(), cursor.as_mut()) {
            (Some(t), None) => {
                let half = t.count.div_ceil(2);
                t.count -= half;
                *cursor = Some(ItemStack::new(t.item, half));
                if t.count == 0 {
                    *target = None;
                }
            }
            (None, Some(c)) => {
                *target = Some(ItemStack::new(c.item, 1));
                c.count -= 1;
                if c.count == 0 {
                    *cursor = None;
                }
            }
            (Some(t), Some(c)) if t.item == c.item && t.count < t.item.def().max_stack => {
                t.count += 1;
                c.count -= 1;
                if c.count == 0 {
                    *cursor = None;
                }
            }
            _ => {}
        }
    }

    pub fn to_saved(&self) -> Vec<Option<SavedStack>> {
        self.slots.iter().map(|s| s.map(|s| s.to_saved())).collect()
    }

    /// Unknown items in a save are skipped (with the slot left empty).
    pub fn from_saved(saved: &[Option<SavedStack>], selected: usize) -> Self {
        let mut inv = Self::default();
        for (i, s) in saved.iter().enumerate().take(SLOTS) {
            inv.slots[i] = s.as_ref().and_then(ItemStack::from_saved);
        }
        inv.selected = selected.min(HOTBAR - 1);
        inv
    }
}

/// Starting kit for a new survival character.
pub fn starter_inventory() -> Inventory {
    let mut inv = Inventory::default();
    for (k, n) in [
        ("manipulator", 1),
        ("physgun", 1),
        ("blade", 1),
        ("arc_caster", 1),
        ("aether_focus", 1),
        ("berries", 6),
        ("water_flask", 2),
        ("bolt", 24),
        ("planks", 16),
    ] {
        inv.add(ItemId::k(k), n);
    }
    inv
}

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, _app: &mut App) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_fill_then_overflow() {
        let mut inv = Inventory::default();
        let stone = ItemId::k("stone");
        assert_eq!(inv.add(stone, 100), 0);
        assert_eq!(inv.slots[0].unwrap().count, 64);
        assert_eq!(inv.slots[1].unwrap().count, 36);
        assert_eq!(inv.add(stone, 10), 0);
        assert_eq!(inv.slots[1].unwrap().count, 46, "existing stack filled first");
        assert_eq!(inv.count(stone), 110);
    }

    #[test]
    fn full_inventory_returns_leftover() {
        let mut inv = Inventory::default();
        let tool = ItemId::k("manipulator");
        assert_eq!(inv.add(tool, SLOTS as u32 + 3), 3);
    }

    #[test]
    fn remove_is_all_or_nothing() {
        let mut inv = Inventory::default();
        let wood = ItemId::k("wood");
        inv.add(wood, 5);
        assert!(!inv.remove(wood, 6));
        assert_eq!(inv.count(wood), 5);
        assert!(inv.remove(wood, 5));
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn click_merge_swap_split() {
        let mut inv = Inventory::default();
        let (a, b) = (ItemId::k("stone"), ItemId::k("dirt"));
        inv.slots[0] = Some(ItemStack::new(a, 60));
        inv.slots[1] = Some(ItemStack::new(b, 3));
        let mut cursor = None;
        inv.click_slot(1, &mut cursor); // pick up dirt
        assert_eq!(cursor, Some(ItemStack::new(b, 3)));
        inv.click_slot(0, &mut cursor); // swap with stone
        assert_eq!(inv.slots[0], Some(ItemStack::new(b, 3)));
        assert_eq!(cursor, Some(ItemStack::new(a, 60)));
        inv.right_click_slot(5, &mut cursor); // drop one
        assert_eq!(inv.slots[5], Some(ItemStack::new(a, 1)));
        inv.click_slot(5, &mut cursor); // merge 59 into 1 → 60
        assert_eq!(inv.slots[5].unwrap().count, 60);
        assert!(cursor.is_none());
        inv.right_click_slot(5, &mut cursor); // split half
        assert_eq!(cursor.unwrap().count, 30);
    }

    #[test]
    fn saved_roundtrip_skips_unknown() {
        let inv = starter_inventory();
        let mut saved = inv.to_saved();
        saved[20] = Some(SavedStack { item: "removed_item".into(), count: 3 });
        let back = Inventory::from_saved(&saved, 2);
        assert_eq!(back.slots[..20], inv.slots[..20]);
        assert!(back.slots[20].is_none());
        assert_eq!(back.selected, 2);
    }
}
