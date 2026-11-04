//! Player inventory: hotbar (9) + inventory (6 × 9). Pure data, unit-tested.

use bevy::prelude::*;

use crate::content::{ItemId, registry};

pub const HOTBAR: usize = 9;
pub const ROWS: usize = 6;
pub const SLOTS: usize = HOTBAR + ROWS * 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stack {
    pub item: ItemId,
    pub count: u32,
}

impl Stack {
    pub fn new(item: ItemId, count: u32) -> Self {
        Self { item, count: count.min(item.def().max_stack).max(1) }
    }
    pub fn full(item: ItemId) -> Self {
        Self::new(item, item.def().max_stack)
    }
}

/// Slots 0..9 are the hotbar, 9..63 the inventory rows.
#[derive(Component, Debug, Clone)]
pub struct Inventory {
    pub slots: Vec<Option<Stack>>,
    pub selected: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { slots: vec![None; SLOTS], selected: 0 }
    }
}

impl Inventory {
    pub fn held(&self) -> Option<Stack> {
        self.slots[self.selected]
    }
    /// Creative pick: select the hotbar slot holding `item`, or put it in
    /// the selected slot.
    pub fn pick(&mut self, item: ItemId) {
        if let Some(i) = (0..HOTBAR).find(|i| self.slots[*i].is_some_and(|s| s.item == item)) {
            self.selected = i;
        } else if let Some(i) = (0..HOTBAR).find(|i| self.slots[*i].is_none()).filter(|_| self.slots[self.selected].is_some()) {
            self.selected = i;
            self.slots[i] = Some(Stack::full(item));
        } else {
            self.slots[self.selected] = Some(Stack::full(item));
        }
    }
    /// First empty hotbar slot, then inventory.
    pub fn add(&mut self, s: Stack) -> bool {
        if let Some(i) = (0..SLOTS).find(|i| self.slots[*i].is_none()) {
            self.slots[i] = Some(s);
            true
        } else {
            false
        }
    }
    /// Minecraft-style click with a carried stack. Left: place / swap /
    /// merge everything. Right: place one / take half.
    pub fn click(&mut self, slot: usize, carried: &mut Option<Stack>, right: bool) {
        let here = &mut self.slots[slot];
        match (here.as_mut(), carried.as_mut()) {
            (None, None) => {}
            (None, Some(c)) => {
                if right {
                    *here = Some(Stack { item: c.item, count: 1 });
                    c.count -= 1;
                    if c.count == 0 {
                        *carried = None;
                    }
                } else {
                    *here = carried.take();
                }
            }
            (Some(h), None) => {
                if right {
                    let take = h.count.div_ceil(2);
                    *carried = Some(Stack { item: h.item, count: take });
                    h.count -= take;
                    if h.count == 0 {
                        *here = None;
                    }
                } else {
                    *carried = here.take();
                }
            }
            (Some(h), Some(c)) => {
                if h.item == c.item {
                    let max = h.item.def().max_stack;
                    let n = if right { 1 } else { c.count }.min(max - h.count);
                    h.count += n;
                    c.count -= n;
                    if c.count == 0 {
                        *carried = None;
                    }
                } else if !right {
                    std::mem::swap(here, carried);
                }
            }
        }
    }
}

/// What a new player starts with.
pub fn starting() -> Inventory {
    let mut inv = Inventory::default();
    let r = registry();
    for (i, k) in ["white_block", "stone", "planks", "steel_frame", "glass", "wrench", "glue", "gravity_gun", "physgun"].iter().enumerate() {
        if let Some(item) = r.item(k) {
            inv.slots[i] = Some(Stack::full(item));
        }
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_move_merge_split() {
        let mut inv = Inventory::default();
        let a = ItemId::k("stone");
        let b = ItemId::k("planks");
        let mut cur = Some(Stack::new(a, 10));
        inv.click(3, &mut cur, true);
        assert_eq!((inv.slots[3].unwrap().count, cur.unwrap().count), (1, 9));
        inv.click(3, &mut cur, false);
        assert_eq!((inv.slots[3].unwrap().count, cur), (10, None));
        inv.click(3, &mut cur, true);
        assert_eq!((inv.slots[3].unwrap().count, cur.unwrap().count), (5, 5));
        let mut other = Some(Stack::new(b, 2));
        inv.click(3, &mut other, false);
        assert_eq!(inv.slots[3].unwrap().item, b);
        assert_eq!(other.unwrap().item, a);
    }

    #[test]
    fn pick_selects_or_fills() {
        let mut inv = starting();
        inv.pick(ItemId::k("glue"));
        assert_eq!(inv.held().unwrap().item, ItemId::k("glue"));
        inv.selected = 0;
        inv.pick(ItemId::k("shaft"));
        assert_eq!(inv.held().unwrap().item, ItemId::k("shaft"));
        // Tools do not stack.
        assert_eq!(Stack::full(ItemId::k("wrench")).count, 1);
    }
}
