//! Survival: needs, health and damage, creatures and weapons.

pub mod combat;
pub mod creatures;
pub mod damage;
pub mod needs;

use bevy::prelude::*;

pub struct SurvivalPlugin;

impl Plugin for SurvivalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            damage::DamagePlugin,
            needs::NeedsPlugin,
            creatures::CreaturesPlugin,
            combat::CombatPlugin,
        ));
    }
}
