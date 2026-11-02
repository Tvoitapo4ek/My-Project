//! Engineering: functional parts, energy, engines and transmissions.
//!
//! Nothing here builds a machine for the player: parts are independent
//! bodies; the player connects them with the physics gun (weld / hinge) and
//! the systems below give the connections meaning (energy flows through
//! connections, engines drive the hinges they sit on, gears couple).

pub mod components;
pub mod connections;
pub mod engines;
pub mod power;
pub mod transmission;

use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MachinerySet {
    Power,
    Drive,
}

pub struct MachineryPlugin;

impl Plugin for MachineryPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(FixedUpdate, (MachinerySet::Power, MachinerySet::Drive).chain())
            .add_plugins((
                components::ComponentsPlugin,
                power::PowerPlugin,
                engines::EnginesPlugin,
                transmission::TransmissionPlugin,
                connections::ConnectionsPlugin,
            ));
        let _ = PhysicsSystems::First;
    }
}
