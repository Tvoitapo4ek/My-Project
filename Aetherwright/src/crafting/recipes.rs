//! Recipe data. Kept small on purpose: enough to turn raw materials into
//! metal, glass, parts and modules, with one magical refining chain.

use crate::items::{ItemId, inventory::Inventory};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Station {
    /// Crafted anywhere.
    Hand,
    /// Needs a workbench within reach.
    Workbench,
    /// Processed inside a smelter (needs coal).
    Smelter,
    /// Processed inside a grinder (needs its shaft to spin).
    Grinder,
}

impl Station {
    pub fn label(self) -> &'static str {
        match self {
            Station::Hand => "Руками",
            Station::Workbench => "Верстак",
            Station::Smelter => "Плавильня",
            Station::Grinder => "Дробилка",
        }
    }
}

pub struct Recipe {
    pub inputs: &'static [(&'static str, u32)],
    pub output: (&'static str, u32),
    pub station: Station,
    /// Processing time in seconds (machines only).
    pub time: f32,
}

macro_rules! r {
    ($st:ident, [$(($k:literal, $n:expr)),*], ($ok:literal, $on:expr), $t:expr) => {
        Recipe { inputs: &[$(($k, $n)),*], output: ($ok, $on), station: Station::$st, time: $t }
    };
}

pub static RECIPES: &[Recipe] = &[
    // By hand
    r!(Hand, [("wood", 1)], ("planks", 4), 0.0),
    r!(Hand, [("stone", 2)], ("bricks", 2), 0.0),
    r!(Hand, [("berries", 3), ("fiber", 1)], ("ration", 1), 0.0),
    r!(Hand, [("planks", 6), ("stone", 2)], ("workbench", 1), 0.0),
    r!(Hand, [("planks", 6)], ("crate", 1), 0.0),
    // Workbench
    r!(Workbench, [("stone", 10), ("coal", 2)], ("smelter", 1), 0.0),
    r!(Workbench, [("metal", 6), ("stone", 6)], ("grinder", 1), 0.0),
    r!(Workbench, [("metal", 2), ("planks", 2)], ("frame", 1), 0.0),
    r!(
        Workbench,
        [("metal", 6), ("copper", 3), ("aether_shard", 1)],
        ("engine", 1),
        0.0
    ),
    r!(
        Workbench,
        [("copper", 4), ("metal", 2), ("aether_shard", 2)],
        ("power_cell", 1),
        0.0
    ),
    r!(Workbench, [("metal", 3)], ("shaft", 1), 0.0),
    r!(Workbench, [("metal", 4)], ("gear", 1), 0.0),
    r!(
        Workbench,
        [("metal", 1), ("fiber", 4), ("planks", 2)],
        ("wheel", 1),
        0.0
    ),
    r!(
        Workbench,
        [("copper", 1), ("metal", 1)],
        ("connector", 2),
        0.0
    ),
    r!(
        Workbench,
        [("glass", 3), ("copper", 1), ("aether_shard", 2)],
        ("aether_jar", 1),
        0.0
    ),
    r!(
        Workbench,
        [("metal", 4), ("copper", 4), ("aether_shard", 4)],
        ("aether_dynamo", 1),
        0.0
    ),
    r!(Workbench, [("metal", 2)], ("metal_plate", 1), 0.0),
    r!(Workbench, [("metal", 1)], ("bolt", 8), 0.0),
    r!(
        Workbench,
        [
            ("metal", 4),
            ("copper", 2),
            ("aether_shard", 4),
            ("agitated_shard", 2)
        ],
        ("drill_module_2", 1),
        0.0
    ),
    r!(
        Workbench,
        [("copper", 4), ("metal", 2), ("aether_shard", 2)],
        ("cutter_module", 1),
        0.0
    ),
    // Smelter (1 coal fuels 4 operations)
    r!(Smelter, [("iron_ore", 1)], ("metal", 1), 4.0),
    r!(Smelter, [("copper_ore", 1)], ("copper", 1), 4.0),
    r!(Smelter, [("sand", 1)], ("glass", 1), 3.0),
    r!(Smelter, [("clay", 1)], ("bricks", 1), 3.0),
    // Grinder (driven by rotation)
    r!(Grinder, [("gravel", 1)], ("sand", 2), 2.0),
    r!(Grinder, [("stone", 1)], ("gravel", 1), 2.0),
    r!(Grinder, [("agitated_shard", 1)], ("aether_shard", 2), 3.0),
    r!(Grinder, [("ancient_stone", 1)], ("aether_shard", 3), 5.0),
];

impl Recipe {
    pub fn output_item(&self) -> ItemId {
        ItemId::k(self.output.0)
    }
    pub fn can_afford(&self, inv: &Inventory) -> bool {
        self.inputs
            .iter()
            .all(|(k, n)| inv.count(ItemId::k(k)) >= *n)
    }
    /// Consumes inputs and adds the output. Returns false (and changes
    /// nothing) if inputs are missing or the output does not fit.
    pub fn craft(&self, inv: &mut Inventory) -> bool {
        if !self.can_afford(inv) {
            return false;
        }
        let mut trial = inv.clone();
        for (k, n) in self.inputs {
            trial.remove(ItemId::k(k), *n);
        }
        if trial.add(self.output_item(), self.output.1) > 0 {
            return false;
        }
        *inv = trial;
        true
    }
}

/// Machine recipe for an input item at a station.
pub fn machine_recipe(station: Station, input: ItemId) -> Option<&'static Recipe> {
    RECIPES
        .iter()
        .find(|r| r.station == station && r.inputs.len() == 1 && ItemId::k(r.inputs[0].0) == input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_keys_exist() {
        for r in RECIPES {
            for (k, _) in r.inputs {
                assert!(ItemId::from_key(k).is_some(), "{k}");
            }
            assert!(ItemId::from_key(r.output.0).is_some(), "{}", r.output.0);
        }
    }

    #[test]
    fn crafting_is_atomic() {
        let mut inv = Inventory::default();
        inv.add(ItemId::k("wood"), 1);
        let planks = &RECIPES[0];
        assert!(planks.craft(&mut inv));
        assert_eq!(inv.count(ItemId::k("planks")), 4);
        assert_eq!(inv.count(ItemId::k("wood")), 0);
        assert!(!planks.craft(&mut inv), "no wood left");
        assert_eq!(inv.count(ItemId::k("planks")), 4);
    }

    #[test]
    fn every_part_is_craftable() {
        for p in crate::machinery::components::PartKind::ALL {
            assert!(
                RECIPES.iter().any(|r| r.output.0 == p.key()),
                "no recipe for {}",
                p.key()
            );
        }
    }
}
