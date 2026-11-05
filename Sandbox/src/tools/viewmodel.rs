//! First-person hand: the held item (tool model or block) in the lower
//! right of the view, swinging on use, bobbing while walking.

use bevy::prelude::*;

use crate::{
    content::{ItemId, ItemKind},
    inventory::Inventory,
    models,
    player::{Body, Player, camera::{CameraView, MainCamera, View}},
    style::{BoxCache, Materials, spawn_parts},
};

#[derive(Component)]
struct Hand {
    item: Option<ItemId>,
    /// 0 → 1 after switching items: the new one rises into view.
    equip: f32,
}

/// Holds the item model (rebuilt on switch); the arm is its sibling.
#[derive(Component)]
struct Slot;

fn spawn_hand(mut commands: Commands, cams: Query<Entity, Added<MainCamera>>, mut meshes: ResMut<Assets<Mesh>>, mut cache: ResMut<BoxCache>, mats: Option<Res<Materials>>) {
    let Some(mats) = mats else { return };
    for c in &cams {
        let hand = commands.spawn((Hand { item: None, equip: 1.0 }, Transform::from_xyz(0.32, -0.3, -0.55), Visibility::default(), ChildOf(c))).id();
        // The player's arm (Minecraft proportions), reaching up to the item.
        let arm = commands.spawn((Transform::from_xyz(0.07, -0.1, 0.2).with_rotation(Quat::from_rotation_x(0.35) * Quat::from_rotation_y(-0.12)), Visibility::default(), ChildOf(hand))).id();
        spawn_parts(&mut commands, arm, &[crate::style::part([0.11, 0.11, 0.46], "player", [0.0, 0.0, 0.0]), crate::style::part([0.115, 0.03, 0.12], "leather", [0.0, 0.0, -0.17])], &mut meshes, &mut cache, &mats, false);
        commands.spawn((Slot, Transform::default(), Visibility::default(), ChildOf(hand)));
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hand(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<CameraView>,
    ui: Res<State<crate::core::Ui>>,
    players: Query<(&Inventory, &Body), With<Player>>,
    mut hands: Query<(&mut Hand, &Children, &mut Transform, &mut Visibility), Without<Slot>>,
    slots: Query<Entity, With<Slot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
    atlas: Option<Res<crate::grid::render::BlockAtlas>>,
) {
    let (Ok((inv, body)), Some(mats)) = (players.single(), mats) else { return };
    let item = inv.held().map(|s| s.item);
    let dt = time.delta_secs();
    for (mut hand, children, mut t, mut vis) in &mut hands {
        // Empty hand: the arm alone, like Minecraft.
        let menu = matches!(ui.get(), crate::core::Ui::Menu | crate::core::Ui::Worlds);
        *vis = if view.0 == View::First && !menu { Visibility::Inherited } else { Visibility::Hidden };
        let Some(slot) = children.iter().find(|c| slots.contains(*c)) else { continue };
        if hand.item != item {
            hand.item = item;
            hand.equip = 0.0;
            commands.entity(slot).despawn_children();
            if let Some(item) = item {
                match item.def().kind {
                    ItemKind::Tool(tool) => {
                        let node = commands.spawn((Transform::from_scale(Vec3::splat(0.42)).with_translation(Vec3::new(0.0, 0.0, -0.02)).with_rotation(Quat::from_rotation_y(0.1) * Quat::from_rotation_x(0.12)), Visibility::default(), ChildOf(slot))).id();
                        spawn_parts(&mut commands, node, &models::tool_model(tool), &mut meshes, &mut cache, &mats, false);
                    }
                    ItemKind::Block(b) if b.def().is_cube() => {
                        if let Some(atlas) = &atlas {
                            let mut g = crate::grid::VoxelGrid::default();
                            g.set(IVec3::ZERO, crate::grid::Cell::new(b, 2));
                            let data = crate::grid::mesh::mesh_chunk(&g, IVec3::ZERO, &|t| atlas.uv(t));
                            // A tilted cube on the palm, like Minecraft.
                            let holder = commands.spawn((Transform::from_xyz(-0.02, 0.06, -0.06).with_rotation(Quat::from_rotation_y(0.8) * Quat::from_rotation_x(0.2)), Visibility::default(), ChildOf(slot))).id();
                            commands.spawn((Mesh3d(meshes.add(data.into_mesh())), MeshMaterial3d(atlas.material.clone()), Transform::from_scale(Vec3::splat(0.24)).with_translation(Vec3::splat(-0.12)), ChildOf(holder)));
                        }
                    }
                    ItemKind::Block(b) => {
                        if b.def().mech.is_some() {
                            let node = commands.spawn((Transform::from_scale(Vec3::splat(0.26)).with_translation(Vec3::new(-0.02, 0.06, -0.06)).with_rotation(Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.2)), Visibility::default(), ChildOf(slot))).id();
                            spawn_parts(&mut commands, node, &models::model_of(b), &mut meshes, &mut cache, &mats, false);
                        }
                    }
                }
            }
        }
        hand.equip = (hand.equip + dt * 5.0).min(1.0);
        let lower = (1.0 - hand.equip).powi(2) * 0.35;
        let walk = (body.speed / 4.3).min(1.5) * (!body.flying) as i32 as f32;
        let phase = time.elapsed_secs() * 9.0;
        let bob = Vec3::new(phase.cos() * 0.008, -phase.sin().abs() * 0.014, 0.0) * walk;
        // Minecraft swing: a quick arc forward and down from the shoulder.
        let s = body.swing.clamp(0.0, 1.0);
        let arc = (s * std::f32::consts::PI).sin();
        let twist = (s.sqrt() * std::f32::consts::PI).sin();
        t.translation = Vec3::new(0.32 - twist * 0.08, -0.3 - lower - arc * 0.06, -0.55 - arc * 0.12) + bob;
        t.rotation = Quat::from_rotation_y(twist * 0.35) * Quat::from_rotation_x(-arc * 0.9) * Quat::from_rotation_z(twist * 0.2);
    }
}

pub struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_hand, update_hand));
    }
}
