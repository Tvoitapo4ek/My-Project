//! Slot inventory: one grid; the first `HOTBAR` slots are the hotbar.

use bevy::prelude::*;

use super::{Category, ItemId, ItemStack, SavedStack};

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
        Self {
            slots: vec![None; SLOTS],
            selected: 0,
        }
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
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum()
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
        let Some(target) = self.slots.get_mut(slot) else {
            return;
        };
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
        let Some(target) = self.slots.get_mut(slot) else {
            return;
        };
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

    /// Moves the stack in `from` onto `to`: merges same items (leftover
    /// stays in `from`), otherwise swaps.
    pub fn move_stack(&mut self, from: usize, to: usize) {
        if from == to || from >= self.slots.len() || to >= self.slots.len() {
            return;
        }
        let mut carried = self.slots[from].take();
        self.click_slot(to, &mut carried);
        self.slots[from] = carried;
    }

    /// Splits half of a stack into the first empty slot. False if the
    /// stack has one item or there is no room.
    pub fn split(&mut self, slot: usize) -> bool {
        let Some(Some(st)) = self.slots.get(slot).copied() else {
            return false;
        };
        if st.count < 2 {
            return false;
        }
        let Some(empty) = (HOTBAR..self.slots.len())
            .chain(0..HOTBAR)
            .find(|&i| self.slots[i].is_none())
        else {
            return false;
        };
        let half = st.count / 2;
        self.slots[slot] = Some(ItemStack::new(st.item, st.count - half));
        self.slots[empty] = Some(ItemStack::new(st.item, half));
        true
    }

    /// Collects all stacks of the item in `slot` into it (up to max stack).
    pub fn gather(&mut self, slot: usize) {
        let Some(Some(st)) = self.slots.get(slot).copied() else {
            return;
        };
        for i in 0..self.slots.len() {
            if i != slot && self.slots[i].is_some_and(|o| o.item == st.item) {
                self.move_stack(i, slot);
            }
        }
    }

    /// Sorts the backpack (hotbar untouched): merges stacks and orders by
    /// category, then name.
    pub fn sort_backpack(&mut self) {
        let mut items: Vec<ItemStack> = self.slots[HOTBAR..].iter_mut().filter_map(Option::take).collect();
        items.sort_by(|a, b| {
            let (da, db) = (a.item.def(), b.item.def());
            let ca = Category::ALL.iter().position(|c| *c == da.category);
            let cb = Category::ALL.iter().position(|c| *c == db.category);
            ca.cmp(&cb).then(da.name.cmp(&db.name)).then(b.count.cmp(&a.count))
        });
        let mut packed: Vec<ItemStack> = Vec::new();
        for st in items {
            let mut left = st.count;
            if let Some(last) = packed.last_mut().filter(|l| l.item == st.item) {
                let room = st.item.def().max_stack.saturating_sub(last.count);
                let n = room.min(left);
                last.count += n;
                left -= n;
            }
            if left > 0 {
                packed.push(ItemStack::new(st.item, left));
            }
        }
        for (i, st) in packed.into_iter().enumerate() {
            self.slots[HOTBAR + i] = Some(st);
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
        assert_eq!(
            inv.slots[1].unwrap().count,
            46,
            "existing stack filled first"
        );
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
    fn move_split_gather_sort() {
        let mut inv = Inventory::default();
        let stone = ItemId::k("stone");
        let wood = ItemId::k("wood");
        inv.slots[HOTBAR] = Some(ItemStack::new(wood, 3));
        inv.slots[HOTBAR + 2] = Some(ItemStack::new(stone, 10));
        inv.slots[HOTBAR + 5] = Some(ItemStack::new(stone, 7));
        assert!(inv.split(HOTBAR + 2));
        assert_eq!(inv.count(stone), 17);
        inv.gather(HOTBAR + 5);
        assert_eq!(inv.slots[HOTBAR + 5].unwrap().count, 17);
        inv.move_stack(HOTBAR + 5, 0);
        assert_eq!(inv.slots[0].unwrap().count, 17);
        inv.move_stack(0, HOTBAR); // swap with wood
        assert_eq!(inv.slots[0].unwrap().item, wood);
        inv.slots[HOTBAR + 8] = Some(ItemStack::new(stone, 5));
        inv.sort_backpack();
        let total: u32 = inv.slots.iter().flatten().map(|s| s.count).sum();
        assert_eq!(total, 3 + 17 + 5);
        assert_eq!(inv.slots[HOTBAR].unwrap().count, 22, "stacks merged");
        assert!(inv.slots[HOTBAR + 1].is_none());
    }

    #[test]
    fn saved_roundtrip_skips_unknown() {
        let inv = starter_inventory();
        let mut saved = inv.to_saved();
        saved[20] = Some(SavedStack {
            item: "removed_item".into(),
            count: 3,
        });
        let back = Inventory::from_saved(&saved, 2);
        assert_eq!(back.slots[..20], inv.slots[..20]);
        assert!(back.slots[20].is_none());
        assert_eq!(back.selected, 2);
    }
}
