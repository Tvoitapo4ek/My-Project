//! Hand-held tools. The selected hotbar item decides the `ActiveTool`;
//! every tool system runs only for its tool. Targeting (block vs entity
//! under the crosshair) is shared.

pub mod manipulator;
pub mod modules;
pub mod scanning;
pub mod viewmodel;

use avian3d::prelude::*;
use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use crate::{
    core::{config::GameConfig, layers::Layer, resources::EyeRay, states::gameplay_active},
    items::{
        ItemId, Use,
        inventory::{HOTBAR, Inventory},
    },
    player::Player,
    world::{interaction::BlockTarget, voxel::BlockId},
};

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTool {
    /// Empty hand: the manipulator's basic mining.
    #[default]
    Hand,
    Manipulator,
    PlaceBlock(ItemId, BlockId),
    PlacePart(ItemId),
    PhysGun,
    Melee,
    Ranged,
    Focus,
    Consume(ItemId),
    Module(ItemId),
    Inert(ItemId),
}

impl ActiveTool {
    /// Tools whose left button mines blocks.
    pub fn mines(self) -> bool {
        matches!(
            self,
            ActiveTool::Hand
                | ActiveTool::Manipulator
                | ActiveTool::PlaceBlock(..)
                | ActiveTool::Inert(_)
        )
    }
}

/// The non-terrain body under the crosshair, if it is closer than the block.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct EntityTarget {
    pub entity: Option<Entity>,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

/// Set by tools that use the mouse wheel themselves (e.g. physics gun
/// distance) so the hotbar does not scroll at the same time.
#[derive(Resource, Default)]
pub struct WheelCaptured(pub bool);

fn update_active_tool(player: Query<&Inventory, With<Player>>, mut tool: ResMut<ActiveTool>) {
    let Ok(inv) = player.single() else { return };
    let new = match inv.selected_item() {
        None => ActiveTool::Hand,
        Some(item) => match &item.def().use_ {
            Use::Manipulator => ActiveTool::Manipulator,
            Use::Block(_) => item
                .block()
                .map_or(ActiveTool::Inert(item), |b| ActiveTool::PlaceBlock(item, b)),
            Use::Part(_) => ActiveTool::PlacePart(item),
            Use::PhysGun => ActiveTool::PhysGun,
            Use::Melee => ActiveTool::Melee,
            Use::Ranged => ActiveTool::Ranged,
            Use::Focus => ActiveTool::Focus,
            Use::Eat { .. } => ActiveTool::Consume(item),
            Use::Module(_) => ActiveTool::Module(item),
            Use::None | Use::Wear(_) => ActiveTool::Inert(item),
        },
    };
    if *tool != new {
        *tool = new;
    }
}

fn hotbar_input(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    captured: Res<WheelCaptured>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut accum: Local<f32>,
) {
    let Ok(mut inv) = player.single_mut() else {
        return;
    };
    const DIGITS: [KeyCode; HOTBAR] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, k) in DIGITS.iter().enumerate() {
        if keys.just_pressed(*k) {
            inv.selected = i;
        }
    }
    if captured.0 || keys.pressed(KeyCode::AltLeft) {
        *accum = 0.0;
        return;
    }
    *accum += match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    while *accum >= 1.0 {
        inv.selected = (inv.selected + HOTBAR - 1) % HOTBAR;
        *accum -= 1.0;
    }
    while *accum <= -1.0 {
        inv.selected = (inv.selected + 1) % HOTBAR;
        *accum += 1.0;
    }
}

pub fn update_entity_target(
    eye: Res<EyeRay>,
    cfg: Res<GameConfig>,
    spatial: SpatialQuery,
    block: Res<BlockTarget>,
    player: Query<Entity, With<Player>>,
    mut target: ResMut<EntityTarget>,
) {
    *target = EntityTarget::default();
    let Ok(dir) = Dir3::new(eye.direction) else {
        return;
    };
    let mut filter = SpatialQueryFilter::from_mask([Layer::Prop, Layer::Creature, Layer::Item]);
    if let Ok(p) = player.single() {
        filter = filter.with_excluded_entities([p]);
    }
    let reach = cfg.player.reach;
    if let Some(hit) = spatial.cast_ray(eye.origin, dir, reach, true, &filter) {
        let block_closer = block.0.is_some_and(|b| b.distance < hit.distance);
        if !block_closer {
            *target = EntityTarget {
                entity: Some(hit.entity),
                point: eye.origin + eye.direction * hit.distance,
                normal: hit.normal,
                distance: hit.distance,
            };
        }
    }
}

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveTool>()
            .init_resource::<EntityTarget>()
            .init_resource::<WheelCaptured>()
            .add_plugins((
                manipulator::ManipulatorPlugin,
                modules::ModulesPlugin,
                scanning::ScanningPlugin,
                viewmodel::ViewModelPlugin,
            ))
            .add_systems(
                Update,
                (hotbar_input, update_active_tool, update_entity_target)
                    .chain()
                    .after(crate::world::interaction::InteractionSet)
                    .run_if(gameplay_active),
            );
    }
}
