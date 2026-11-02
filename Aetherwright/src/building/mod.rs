//! Construction: placement with preview, area selection and blueprints
//! (copy / cut / delete / paste / rotate groups of blocks and parts).
//!
//! Hybrid model: architectural blocks snap to the voxel grid; functional
//! parts are placed freely on any surface with rotation.

pub mod blueprints;
pub mod construction;
pub mod placement;
pub mod preview;
pub mod selection;

use bevy::prelude::*;

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            placement::PlacementPlugin,
            selection::SelectionPlugin,
            preview::PreviewPlugin,
        ));
    }
}
