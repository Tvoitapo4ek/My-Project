//! Worn equipment: one item per `EquipSlot` (two accessories).
//! Items declare their slot with `use: Wear(<slot>)` in `items.ron`.
//! The equipment has no stats yet — the slots, transfer and projection are
//! the foundation for armour/archetype gear (concept §6, §7).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ItemId, ItemStack, SavedStack, Use, inventory::Inventory};
use crate::{
    content::EquipSlot,
    player::Player,
    save::world_save::{SaveHook, SaveHookExt},
};

/// Slot index: `EquipSlot` plus a second accessory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotRef {
    pub slot: EquipSlot,
    pub index: u8,
}

impl SlotRef {
    pub const ALL: [SlotRef; 6] = [
        SlotRef { slot: EquipSlot::Head, index: 0 },
        SlotRef { slot: EquipSlot::Body, index: 0 },
        SlotRef { slot: EquipSlot::Hands, index: 0 },
        SlotRef { slot: EquipSlot::Legs, index: 0 },
        SlotRef { slot: EquipSlot::Accessory, index: 0 },
        SlotRef { slot: EquipSlot::Accessory, index: 1 },
    ];
    pub fn label(self) -> String {
        if self.slot == EquipSlot::Accessory {
            format!("{} {}", self.slot.label(), self.index + 1)
        } else {
            self.slot.label().to_string()
        }
    }
}

#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Equipment {
    pub worn: Vec<(SlotRef, ItemStack)>,
}

/// Slot an item can be worn in.
pub fn wear_slot(item: ItemId) -> Option<EquipSlot> {
    match item.def().use_ {
        Use::Wear(s) => Some(s),
        _ => None,
    }
}

impl Equipment {
    /// The starting test set (keeps the equipment screen meaningful).
    pub fn test_set() -> Self {
        let mut e = Equipment::default();
        for (slot, key) in [
            (SlotRef::ALL[0], "test_visor"),
            (SlotRef::ALL[1], "test_coat"),
            (SlotRef::ALL[3], "test_boots"),
        ] {
            if let Some(i) = ItemId::from_key(key) {
                e.worn.push((slot, ItemStack::new(i, 1)));
            }
        }
        e
    }

    pub fn get(&self, s: SlotRef) -> Option<ItemStack> {
        self.worn.iter().find(|(r, _)| *r == s).map(|(_, st)| *st)
    }

    /// Takes the item off into the inventory. False if it does not fit.
    pub fn unequip(&mut self, s: SlotRef, inv: &mut Inventory) -> bool {
        let Some(pos) = self.worn.iter().position(|(r, _)| *r == s) else {
            return false;
        };
        let stack = self.worn[pos].1;
        let mut trial = inv.clone();
        if trial.add(stack.item, stack.count) > 0 {
            return false;
        }
        *inv = trial;
        self.worn.remove(pos);
        true
    }

    /// Wears one item from inventory slot `from` in `s`; the previous item
    /// goes back to the inventory. All-or-nothing.
    pub fn equip_from(&mut self, s: SlotRef, inv: &mut Inventory, from: usize) -> bool {
        let Some(stack) = inv.slots.get(from).copied().flatten() else {
            return false;
        };
        if wear_slot(stack.item) != Some(s.slot) {
            return false;
        }
        let mut trial_inv = inv.clone();
        let mut trial = self.clone();
        trial_inv.slots[from] = if stack.count > 1 {
            Some(ItemStack::new(stack.item, stack.count - 1))
        } else {
            None
        };
        if trial.get(s).is_some() && !trial.unequip(s, &mut trial_inv) {
            return false;
        }
        trial.worn.push((s, ItemStack::new(stack.item, 1)));
        *inv = trial_inv;
        *self = trial;
        true
    }
}

#[derive(Serialize, Deserialize)]
struct SavedEquipment(Vec<(SlotRef, SavedStack)>);

fn capture(world: &mut World) -> serde_json::Value {
    let mut q = world.query_filtered::<&Equipment, With<Player>>();
    let worn = q
        .iter(world)
        .next()
        .map(|e| e.worn.iter().map(|(s, st)| (*s, st.to_saved())).collect())
        .unwrap_or_default();
    serde_json::to_value(SavedEquipment(worn)).unwrap_or_default()
}

fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let eq = match v.and_then(|v| serde_json::from_value::<SavedEquipment>(v.clone()).ok()) {
        Some(s) => Equipment {
            worn: s.0.iter().filter_map(|(r, st)| ItemStack::from_saved(st).map(|x| (*r, x))).collect(),
        },
        None => Equipment::test_set(),
    };
    let mut q = world.query_filtered::<&mut Equipment, With<Player>>();
    if let Some(mut e) = q.iter_mut(world).next() {
        *e = eq;
    }
}

fn attach(mut commands: Commands, added: Query<Entity, (Added<Player>, Without<Equipment>)>) {
    for e in &added {
        commands.entity(e).insert(Equipment::test_set());
    }
}

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach).add_save_hook(SaveHook {
            key: "equipment",
            capture,
            restore,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equip_swap_and_unequip() {
        let mut inv = Inventory::default();
        let visor = ItemId::k("test_visor");
        inv.add(visor, 1);
        let from = inv.slots.iter().position(|s| s.is_some()).unwrap();
        let mut eq = Equipment::test_set();
        let head = SlotRef::ALL[0];
        // Head already has a visor: equipping swaps (old one to inventory).
        assert!(eq.equip_from(head, &mut inv, from));
        assert_eq!(inv.count(visor), 1);
        assert_eq!(eq.get(head).map(|s| s.item), Some(visor));
        // Wrong slot is refused, nothing changes.
        let before = (eq.clone(), inv.clone());
        let from = inv.slots.iter().position(|s| s.is_some()).unwrap();
        assert!(!eq.equip_from(SlotRef::ALL[3], &mut inv, from));
        assert_eq!((eq.clone(), inv.clone()), before);
        assert!(eq.unequip(head, &mut inv));
        assert_eq!(inv.count(visor), 2);
        assert!(eq.get(head).is_none());
    }
}
