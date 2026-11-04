//! Item icons rendered from the real 3D models: every item is placed in a
//! hidden scene (its own render layer) and photographed once by an
//! orthographic isometric camera into one atlas image. Cube blocks use the
//! block atlas, mechanisms and tools their models — so icons always match
//! the game.

use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
};

use crate::{
    content::{ItemKind, registry},
    grid::{Cell, VoxelGrid, mesh, render::BlockAtlas},
    models,
    style::{BoxCache, Materials, spawn_parts},
};

const LAYER: usize = 7;
const CELL_PX: u32 = 64;
const COLS: u32 = 8;
/// World size of one icon cell.
const S: f32 = 1.7;

#[derive(Resource)]
pub struct ItemIcons {
    pub image: Handle<Image>,
    pub rows: u32,
}

impl ItemIcons {
    /// Pixel rect of an item's icon in the atlas.
    pub fn rect(&self, item: usize) -> Rect {
        let (c, r) = (item as u32 % COLS, item as u32 / COLS);
        Rect::new((c * CELL_PX) as f32, (r * CELL_PX) as f32, ((c + 1) * CELL_PX) as f32, ((r + 1) * CELL_PX) as f32)
    }
}

#[derive(Component)]
struct IconRig(u32);

#[allow(clippy::too_many_arguments)]
fn build(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut cache: ResMut<BoxCache>, mats: Res<Materials>, atlas: Res<BlockAtlas>) {
    let items = &registry().items;
    let rows = (items.len() as u32).div_ceil(COLS);
    let image = images.add(Image::new_target_texture(COLS * CELL_PX, rows * CELL_PX, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    let layer = RenderLayers::layer(LAYER);
    let dir = Vec3::new(-1.0, -0.82, -1.0).normalize();
    let cam_rot = Transform::IDENTITY.looking_to(dir, Vec3::Y).rotation;
    let (right, up) = (cam_rot * Vec3::X, cam_rot * Vec3::Y);
    let origin = Vec3::new(1000.0, 1000.0, 1000.0);
    commands.spawn((
        IconRig(0),
        Camera3d::default(),
        Camera { order: -10, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        RenderTarget::Image(image.clone().into()),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed { width: COLS as f32 * S, height: rows as f32 * S },
            near: 0.1,
            far: 200.0,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_translation(origin - dir * 50.0).with_rotation(cam_rot),
        layer.clone(),
    ));
    commands.spawn((DirectionalLight { illuminance: 6000.0, ..default() }, Transform::IDENTITY.looking_to(Vec3::new(-0.3, -1.0, -0.6), Vec3::Y), layer.clone()));
    for (i, item) in items.iter().enumerate() {
        let (c, r) = (i as u32 % COLS, i as u32 / COLS);
        let at = origin + right * ((c as f32 + 0.5) - COLS as f32 / 2.0) * S + up * (rows as f32 / 2.0 - (r as f32 + 0.5)) * S;
        let root = commands.spawn((Transform::from_translation(at), Visibility::default(), layer.clone())).id();
        match item.kind {
            ItemKind::Block(b) if b.def().is_cube() => {
                let mut g = VoxelGrid::default();
                g.set(IVec3::ZERO, Cell::new(b, 2));
                let data = mesh::mesh_chunk(&g, IVec3::ZERO, &|t| atlas.uv(t));
                commands.spawn((Mesh3d(meshes.add(data.into_mesh())), MeshMaterial3d(atlas.material.clone()), Transform::from_translation(Vec3::splat(-0.5)), layer.clone(), ChildOf(root)));
            }
            ItemKind::Block(b) => {
                let def = b.def();
                let Some(m) = def.mech else { continue };
                let facing = if def.placement == crate::content::Placement::Horizontal { 5 } else { 2 };
                let (scale, shift) = match m {
                    crate::content::Mech::LargeCog => (0.62, Vec3::ZERO),
                    // The sensor plate sits on the face of its block.
                    crate::content::Mech::Sensor => (1.5, Vec3::new(0.0, -0.55, 0.0)),
                    _ => (0.95, Vec3::ZERO),
                };
                let node = commands.spawn((Transform::from_rotation(models::block_rot(def.placement, facing)).with_scale(Vec3::splat(scale)).with_translation(shift), Visibility::default(), layer.clone(), ChildOf(root))).id();
                spawn_parts(&mut commands, node, &models::block_model(m), &mut meshes, &mut cache, &mats, false);
            }
            ItemKind::Tool(t) => {
                // Tool models start at the grip and point along −Z: centre them.
                let r = Quat::from_rotation_y(-2.4) * Quat::from_rotation_x(0.5);
                let scale = if t == crate::content::Tool::Glue { 3.6 } else { 2.9 };
                let center = if t == crate::content::Tool::Glue { Vec3::ZERO } else { Vec3::new(0.0, 0.0, -0.17) };
                let node = commands.spawn((Transform::from_rotation(r).with_scale(Vec3::splat(scale)).with_translation(-(r * center) * scale), Visibility::default(), layer.clone(), ChildOf(root))).id();
                spawn_parts(&mut commands, node, &models::tool_model(t), &mut meshes, &mut cache, &mats, false);
            }
        }
    }
    commands.insert_resource(ItemIcons { image, rows });
}

/// Model meshes spawned deeper in the hierarchy get the icon layer too.
fn propagate_layer(mut commands: Commands, q: Query<Entity, (Added<Mesh3d>, Without<RenderLayers>)>, parents: Query<&ChildOf>, layers: Query<&RenderLayers>) {
    for e in &q {
        if parents.iter_ancestors(e).any(|a| layers.get(a).is_ok_and(|l| l.intersects(&RenderLayers::layer(LAYER)))) {
            commands.entity(e).insert(RenderLayers::layer(LAYER));
        }
    }
}

/// The scene is photographed for a while (shaders for this view compile
/// asynchronously), then the camera is switched off.
fn stop_camera(time: Res<Time>, mut q: Query<(&mut IconRig, &mut Camera)>) {
    for (mut rig, mut cam) in &mut q {
        rig.0 += 1;
        if rig.0 > 240 && time.elapsed_secs() > 6.0 {
            cam.is_active = false;
        }
    }
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build).add_systems(Update, (propagate_layer, stop_camera));
    }
}
