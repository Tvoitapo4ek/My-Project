//! Shared power bookkeeping. The electric network itself (wires, internal
//! bus of constructions, storage, generation, consumption) lives in
//! `crate::electric`; this keeps `PowerStats` for UI.

use bevy::prelude::*;

/// Per-network energy summary for UI / scanner.
#[derive(Resource, Default, Debug)]
pub struct PowerStats {
    /// entity → (network energy, network capacity)
    pub by_entity: std::collections::HashMap<Entity, (f32, f32)>,
}

pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        // Energy flow itself is solved by `electric` (wires + bus).
        app.init_resource::<PowerStats>();
    }
}

