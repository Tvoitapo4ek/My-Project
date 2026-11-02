//! Инвентарь: чистая модель данных без интерфейса.
//!
//! Слоты `0..HOTBAR` — панель быстрого доступа, остальные — основной инвентарь.
//! «Курсор» — стопка, которую игрок держит мышью в открытом инвентаре.
//! Логика кликов как в Minecraft: ЛКМ берёт/кладёт/меняет/объединяет,
//! ПКМ берёт половину или кладёт по одному.

use crate::BlockId;

pub const HOTBAR: usize = 9;
pub const MAX_STACK: u16 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub block: BlockId,
    pub count: u16,
}

impl Stack {
    pub fn new(block: BlockId, count: u16) -> Self {
        Stack { block, count }
    }
}

pub struct Inventory {
    pub slots: Vec<Option<Stack>>,
    /// Выбранный слот панели быстрого доступа (0..HOTBAR).
    pub selected: usize,
    /// Стопка «в руке» при открытом инвентаре.
    pub cursor: Option<Stack>,
}

impl Inventory {
    /// `size` — всего слотов, включая панель быстрого доступа.
    pub fn new(size: usize) -> Self {
        Inventory { slots: vec![None; size.max(HOTBAR)], selected: 0, cursor: None }
    }

    pub fn selected_stack(&self) -> Option<Stack> {
        self.slots[self.selected]
    }

    pub fn select(&mut self, slot: usize) {
        self.selected = slot % HOTBAR;
    }

    /// Сдвинуть выбор колесом мыши.
    pub fn scroll(&mut self, steps: i32) {
        self.selected = (self.selected as i32 + steps).rem_euclid(HOTBAR as i32) as usize;
    }

    /// Добавить блоки: сначала в такие же стопки, потом в пустые слоты.
    /// Возвращает то, что не поместилось.
    pub fn add(&mut self, block: BlockId, mut count: u16) -> u16 {
        for slot in self.slots.iter_mut().flatten().filter(|s| s.block == block) {
            let moved = count.min(MAX_STACK - slot.count);
            slot.count += moved;
            count -= moved;
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            if count == 0 {
                break;
            }
            let moved = count.min(MAX_STACK);
            *slot = Some(Stack::new(block, moved));
            count -= moved;
        }
        count
    }

    /// Забрать `n` блоков из выбранного слота. `false`, если не хватает.
    pub fn take_selected(&mut self, n: u16) -> bool {
        let slot = &mut self.slots[self.selected];
        match slot {
            Some(s) if s.count >= n => {
                s.count -= n;
                if s.count == 0 {
                    *slot = None;
                }
                true
            }
            _ => false,
        }
    }

    /// Левый клик по слоту.
    pub fn click(&mut self, slot: usize) {
        let target = &mut self.slots[slot];
        match (self.cursor, *target) {
            (Some(c), Some(t)) if c.block == t.block => {
                let moved = c.count.min(MAX_STACK - t.count);
                *target = Some(Stack::new(t.block, t.count + moved));
                self.cursor = (c.count > moved).then(|| Stack::new(c.block, c.count - moved));
            }
            (c, t) => {
                *target = c;
                self.cursor = t;
            }
        }
    }

    /// Правый клик: пустой курсор берёт половину, полный кладёт один блок.
    pub fn click_secondary(&mut self, slot: usize) {
        let target = &mut self.slots[slot];
        match (self.cursor, *target) {
            (None, Some(t)) => {
                let half = t.count.div_ceil(2);
                self.cursor = Some(Stack::new(t.block, half));
                *target = (t.count > half).then(|| Stack::new(t.block, t.count - half));
            }
            (Some(c), t) if t.is_none_or(|t| t.block == c.block && t.count < MAX_STACK) => {
                *target = Some(Stack::new(c.block, t.map_or(0, |t| t.count) + 1));
                self.cursor = (c.count > 1).then(|| Stack::new(c.block, c.count - 1));
            }
            _ => {}
        }
    }

    /// Положить содержимое курсора обратно (при закрытии инвентаря).
    /// Возвращает то, что не поместилось.
    pub fn return_cursor(&mut self) -> Option<Stack> {
        let c = self.cursor.take()?;
        let left = self.add(c.block, c.count);
        (left > 0).then(|| Stack::new(c.block, left))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_merges_then_fills() {
        let mut inv = Inventory::new(HOTBAR);
        assert_eq!(inv.add(1, 100), 0);
        assert_eq!(inv.slots[0], Some(Stack::new(1, 64)));
        assert_eq!(inv.slots[1], Some(Stack::new(1, 36)));
        assert_eq!(inv.add(1, 64 * 8), 36);
    }

    #[test]
    fn clicks() {
        let mut inv = Inventory::new(HOTBAR);
        inv.slots[0] = Some(Stack::new(1, 10));
        inv.slots[1] = Some(Stack::new(1, 60));
        inv.slots[2] = Some(Stack::new(2, 5));
        inv.click(0); // взять
        assert_eq!((inv.cursor, inv.slots[0]), (Some(Stack::new(1, 10)), None));
        inv.click(1); // объединить: 4 блока поместилось, 6 осталось в руке
        assert_eq!((inv.cursor, inv.slots[1]), (Some(Stack::new(1, 6)), Some(Stack::new(1, 64))));
        inv.click(2); // поменять
        assert_eq!((inv.cursor, inv.slots[2]), (Some(Stack::new(2, 5)), Some(Stack::new(1, 6))));
        inv.click_secondary(3); // положить один
        assert_eq!((inv.cursor, inv.slots[3]), (Some(Stack::new(2, 4)), Some(Stack::new(2, 1))));
        inv.click(3);
        inv.cursor = None;
        inv.slots[4] = Some(Stack::new(3, 7));
        inv.click_secondary(4); // взять половину
        assert_eq!((inv.cursor, inv.slots[4]), (Some(Stack::new(3, 4)), Some(Stack::new(3, 3))));
    }

    #[test]
    fn take_selected() {
        let mut inv = Inventory::new(HOTBAR);
        inv.add(5, 1);
        assert!(inv.take_selected(1));
        assert!(!inv.take_selected(1));
        assert_eq!(inv.slots[0], None);
    }
}
