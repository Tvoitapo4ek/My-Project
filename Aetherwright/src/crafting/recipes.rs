//! Recipe logic. Recipe data lives in `assets/content/recipes.ron` (and mods).

use crate::items::{ItemId, inventory::Inventory};

pub use crate::content::{RecipeDef as Recipe, Station};

/// All recipes (from `assets/content/recipes.ron` and mods).
pub fn recipes() -> &'static [Recipe] {
    &crate::content::registry().recipes
}

impl Recipe {
    pub fn output_item(&self) -> ItemId {
        ItemId::k(&self.output.0)
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
        for (k, n) in &self.inputs {
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
    recipes().iter().find(|r| {
        r.station == station
            && r.inputs.len() == 1
            && ItemId::from_key(&r.inputs[0].0) == Some(input)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_keys_exist() {
        for r in recipes() {
            for (k, _) in &r.inputs {
                assert!(ItemId::from_key(k).is_some(), "{k}");
            }
            assert!(ItemId::from_key(&r.output.0).is_some(), "{}", r.output.0);
        }
    }

    #[test]
    fn crafting_is_atomic() {
        let mut inv = Inventory::default();
        inv.add(ItemId::k("wood"), 1);
        let planks = &recipes()[0];
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
                recipes().iter().any(|r| r.output.0 == p.key()),
                "no recipe for {}",
                p.key()
            );
        }
    }
}
