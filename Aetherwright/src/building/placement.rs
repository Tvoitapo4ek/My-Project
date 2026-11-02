//! Placing blocks (grid) and functional parts (free, on any surface).
//!
//! Parts: the ghost sits on the targeted surface (block or object), its
//! orientation is `yaw` (R: +45°) and `tilt` (Shift+R: 90° about X — e.g. to
//! stand a wheel or a shaft on its side). Stations snap to the block grid.
//! RMB places when the preview is green.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::preview::{GhostShape, Ghosts, Validity, shape_of};
use crate::{
    core::{
        events::Notify, layers::Layer, resources::EyeRay, rules::ModeRules, states::gameplay_active,
    },
    items::inventory::Inventory,
    machinery::components::{PartAssets, PartKind, spawn_part},
    player::Player,
    tools::{ActiveTool, EntityTarget},
    world::{chunks::VoxelWorld, interaction::BlockTarget},
};

#[derive(Resource, Debug, Default)]
pub struct PlacementState {
    /// Yaw in 45° steps.
    pub yaw: u8,
    /// Tilt in 90° steps about X.
    pub tilt: u8,
}

impl PlacementState {
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw as f32 * std::f32::consts::FRAC_PI_4)
            * Quat::from_rotation_x(self.tilt as f32 * std::f32::consts::FRAC_PI_2)
    }
}

/// Computes where a part would go for the current target.
pub fn part_pose(
    kind: PartKind,
    state: &PlacementState,
    block: &BlockTarget,
    entity: &EntityTarget,
    eye: &EyeRay,
) -> Option<Transform> {
    let (point, normal) = if let Some(_) = entity.entity {
        (entity.point, entity.normal)
    } else if let Some(hit) = block.0 {
        (
            eye.origin + eye.direction * hit.distance,
            hit.normal.as_vec3(),
        )
    } else {
        return None;
    };
    if kind.is_station() {
        let cell = block.0.map(|h| h.place_pos())?;
        let rot = Quat::from_rotation_y((state.yaw / 2) as f32 * std::f32::consts::FRAC_PI_2);
        return Some(
            Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5)).with_rotation(rot),
        );
    }
    let rot = state.rotation();
    let aabb = kind.collider().aabb(Vec3::ZERO, rot, 0.0);
    let half = (Vec3::from(aabb.max) - Vec3::from(aabb.min)) * 0.5;
    let offset = (normal.abs() * half).element_sum() + 0.01;
    Some(Transform::from_translation(point + normal * offset).with_rotation(rot))
}

pub fn check_pose(spatial: &SpatialQuery, collider: &Collider, t: &Transform) -> Validity {
    let filter = SpatialQueryFilter::from_mask([
        Layer::Terrain,
        Layer::Player,
        Layer::Prop,
        Layer::Creature,
    ]);
    // Slightly shrunk so touching a surface is fine.
    let mut shrunk = collider.clone();
    shrunk.set_scale(Vec3::splat(0.94), 8);
    if spatial
        .shape_intersections(&shrunk, t.translation, t.rotation, &filter)
        .is_empty()
    {
        Validity::Ok
    } else {
        Validity::Obstructed
    }
}

#[allow(clippy::too_many_arguments)]
fn place_parts(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    eye: Res<EyeRay>,
    block: Res<BlockTarget>,
    entity: Res<EntityTarget>,
    spatial: SpatialQuery,
    assets: Option<Res<PartAssets>>,
    mut state: ResMut<PlacementState>,
    mut ghosts: ResMut<Ghosts>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let ActiveTool::PlacePart(item) = *tool else {
        return;
    };
    let Some(assets) = assets else { return };
    let crate::items::Use::Part(key) = item.def().use_ else {
        return;
    };
    let Some(kind) = PartKind::from_key(key) else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyR) {
        if keys.pressed(KeyCode::ShiftLeft) {
            state.tilt = (state.tilt + 1) % 4;
        } else {
            state.yaw = (state.yaw + 1) % 8;
        }
    }
    let Some(pose) = part_pose(kind, &state, &block, &entity, &eye) else {
        return;
    };
    let collider = kind.collider();
    let validity = check_pose(&spatial, &collider, &pose);
    ghosts.0.push((shape_of(&collider), pose, validity));
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    if validity != Validity::Ok {
        notify.write(Notify::warn("Здесь нельзя разместить: мешает препятствие"));
        return;
    }
    let Ok(mut inv) = player.single_mut() else {
        return;
    };
    if rules.consume_items && !inv.remove(item, 1) {
        return;
    }
    spawn_part(&mut commands, &assets, kind, pose);
}

/// Grid preview for blocks (green / orange / red cube).
fn preview_blocks(
    tool: Res<ActiveTool>,
    target: Res<BlockTarget>,
    entity: Res<EntityTarget>,
    world: Res<VoxelWorld>,
    spatial: SpatialQuery,
    mut ghosts: ResMut<Ghosts>,
) {
    let ActiveTool::PlaceBlock(..) = *tool else {
        return;
    };
    let Some(hit) = target.0 else { return };
    if entity.entity.is_some() {
        return;
    }
    let pos = if hit.block.is_replaceable() {
        hit.pos
    } else {
        hit.place_pos()
    };
    let t = Transform::from_translation(pos.as_vec3() + Vec3::splat(0.5));
    let validity = if !world.in_bounds(pos) {
        Validity::Invalid
    } else {
        let filter = SpatialQueryFilter::from_mask([Layer::Player, Layer::Prop, Layer::Creature]);
        if spatial
            .shape_intersections(
                &Collider::cuboid(0.98, 0.98, 0.98),
                t.translation,
                Quat::IDENTITY,
                &filter,
            )
            .is_empty()
        {
            Validity::Ok
        } else {
            Validity::Obstructed
        }
    };
    ghosts
        .0
        .push((GhostShape::Box(Vec3::splat(1.0)), t, validity));
}

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacementState>().add_systems(
            Update,
            (place_parts, preview_blocks)
                .after(crate::tools::update_entity_target)
                .run_if(gameplay_active),
        );
    }
}
