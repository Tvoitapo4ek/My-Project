//! Stage-2 "bare hands" block interaction (replaced by the manipulator tool
//! in stage 3): LMB breaks the targeted block, RMB places the chosen block,
//! keys 1-6 choose the block.

use bevy::prelude::*;

use crate::{
    core::states::gameplay_active,
    world::{
        chunks::EditCause,
        interaction::{BlockEdit, BlockTarget},
        voxel::{blocks, BlockId},
    },
};

#[derive(Resource)]
struct HandBlock(BlockId);

fn hands(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    target: Res<BlockTarget>,
    mut hand: ResMut<HandBlock>,
    mut edits: MessageWriter<BlockEdit>,
) {
    let palette = [blocks::STONE, blocks::DIRT, blocks::PLANKS, blocks::BRICKS, blocks::GLASS, blocks::METAL_PLATE];
    let keycodes = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6];
    for (k, b) in keycodes.iter().zip(palette) {
        if keys.just_pressed(*k) {
            hand.0 = b;
        }
    }
    let Some(hit) = target.0 else { return };
    if mouse.just_pressed(MouseButton::Left) {
        edits.write(BlockEdit { pos: hit.pos, block: blocks::AIR, cause: EditCause::Mined });
    }
    if mouse.just_pressed(MouseButton::Right) {
        edits.write(BlockEdit { pos: hit.place_pos(), block: hand.0, cause: EditCause::Placed });
    }
}

pub struct HandsPlugin;

impl Plugin for HandsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HandBlock(blocks::STONE)).add_systems(Update, hands.run_if(gameplay_active));
    }
}
