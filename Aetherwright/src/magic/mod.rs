//! Magic: a natural force of the world. A background aether field fed by
//! crystals; accumulators that collect and share it; a dynamo that turns it
//! into machine energy; spells cast from the player's reserve; anomalies.

pub mod anomalies;
pub mod energy;
pub mod sources;
pub mod spells;

use bevy::prelude::*;

use crate::tools::scanning::ScanExtras;

fn field_scan_line(world: &World, p: Vec3) -> Option<String> {
    let f = world.get_resource::<sources::AetherSources>()?.sample(p);
    Some(format!("Эфирное поле: {:.2} ({})", f.strength, f.kind.label()))
}

fn store_scan_line(world: &World, p: Vec3) -> Option<String> {
    let _ = (world, p);
    None
}

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((sources::SourcesPlugin, spells::SpellsPlugin, anomalies::AnomaliesPlugin));
        app.world_mut().resource_mut::<ScanExtras>().0.push(field_scan_line);
        let _ = store_scan_line;
    }
}
