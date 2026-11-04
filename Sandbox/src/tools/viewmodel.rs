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
struct Hand(Option<ItemId>);

fn spawn_hand(mut commands: Commands, cams: Query<Entity, Added<MainCamera>>) {
    for c in &cams {
        commands.spawn((Hand(None), Transform::from_xyz(0.32, -0.3, -0.55), Visibility::default(), ChildOf(c)));
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hand(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<CameraView>,
    players: Query<(&Inventory, &Body), With<Player>>,
    mut hands: Query<(Entity, &mut Hand, &mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
    atlas: Option<Res<crate::grid::render::BlockAtlas>>,
) {
    let (Ok((inv, body)), Some(mats)) = (players.single(), mats) else { return };
    let item = inv.held().map(|s| s.item);
    for (e, mut hand, mut t, mut vis) in &mut hands {
        *vis = if view.0 == View::First && item.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if hand.0 != item {
            hand.0 = item;
            commands.entity(e).despawn_children();
            if let Some(item) = item {
                match item.def().kind {
                    ItemKind::Tool(tool) => {
                        let node = commands.spawn((Transform::from_scale(Vec3::splat(1.0)), Visibility::default(), ChildOf(e))).id();
                        spawn_parts(&mut commands, node, &models::tool_model(tool), &mut meshes, &mut cache, &mats, false);
                    }
                    ItemKind::Block(b) if b.def().is_cube() => {
                        if let Some(atlas) = &atlas {
                            let mut g = crate::grid::VoxelGrid::default();
                            g.set(IVec3::ZERO, crate::grid::Cell::new(b, 2));
                            let data = crate::grid::mesh::mesh_chunk(&g, IVec3::ZERO, &|t| atlas.uv(t));
                            commands.spawn((Mesh3d(meshes.add(data.into_mesh())), MeshMaterial3d(atlas.material.clone()), Transform::from_scale(Vec3::splat(0.28)).with_translation(Vec3::splat(-0.14)).with_rotation(Quat::from_rotation_y(0.5)), ChildOf(e)));
                        }
                    }
                    ItemKind::Block(b) => {
                        if let Some(m) = b.def().mech {
                            let node = commands.spawn((Transform::from_scale(Vec3::splat(0.3)).with_rotation(Quat::from_rotation_y(0.5)), Visibility::default(), ChildOf(e))).id();
                            spawn_parts(&mut commands, node, &models::block_model(m), &mut meshes, &mut cache, &mats, false);
                        }
                    }
                }
            }
        }
        let bob = (time.elapsed_secs() * 9.0).sin() * 0.012 * (body.speed / 4.3).min(1.5) * (!body.flying) as i32 as f32;
        let swing = (body.swing * std::f32::consts::PI).sin();
        t.translation = Vec3::new(0.32, -0.3 + bob - swing * 0.08, -0.55 - swing * 0.1);
        t.rotation = Quat::from_rotation_x(-swing * 0.6);
    }
}

pub struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_hand, update_hand));
    }
}
