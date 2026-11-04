//! Block texture atlas and per-chunk meshes + colliders for every grid.
//! A chunk view is a child of its grid entity, so contraptions carry their
//! meshes and colliders with them.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::{VoxelGrid, mesh};
use crate::content::registry;

pub const TILE: u32 = 32;
const COLS: u32 = 8;

/// The block atlas and the material every chunk uses.
#[derive(Resource, Clone)]
pub struct BlockAtlas {
    pub image: Handle<Image>,
    pub material: Handle<StandardMaterial>,
    pub rows: u32,
}

impl BlockAtlas {
    pub fn uv(&self, index: usize) -> [f32; 4] {
        let (c, r) = (index as u32 % COLS, index as u32 / COLS);
        let (w, h) = ((COLS * TILE) as f32, (self.rows * TILE) as f32);
        // Half-texel inset against bleeding.
        let e = 0.01;
        [(c * TILE) as f32 / w + e / w, (r * TILE) as f32 / h + e / h, ((c + 1) * TILE) as f32 / w - e / w, ((r + 1) * TILE) as f32 / h - e / h]
    }
}

/// RGBA8 pixels of a texture (TILE×TILE; magenta checker if missing).
pub fn load_texture(name: &str) -> Vec<u8> {
    let path = format!("assets/textures/{name}.png");
    match image::open(&path) {
        Ok(img) => {
            let img = img.to_rgba8();
            let img = if img.width() != TILE || img.height() != TILE {
                image::imageops::resize(&img, TILE, TILE, image::imageops::FilterType::Nearest)
            } else {
                img
            };
            img.into_raw()
        }
        Err(_) => {
            warn!("missing texture {path}");
            (0..TILE * TILE).flat_map(|i| if ((i % TILE) / 4 + (i / TILE) / 4) % 2 == 0 { [255, 0, 255, 255] } else { [0, 0, 0, 255] }).collect()
        }
    }
}

pub fn pixel_image(w: u32, h: u32, data: Vec<u8>) -> Image {
    let mut img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    let mut s = ImageSamplerDescriptor::nearest();
    s.set_address_mode(bevy::image::ImageAddressMode::Repeat);
    img.sampler = ImageSampler::Descriptor(s);
    img
}

fn build_atlas(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let names = &registry().textures;
    let rows = (names.len() as u32).div_ceil(COLS).max(1);
    let (w, h) = (COLS * TILE, rows * TILE);
    let mut data = vec![0u8; (w * h * 4) as usize];
    for (i, name) in names.iter().enumerate() {
        let px = load_texture(name);
        let (c, r) = (i as u32 % COLS, i as u32 / COLS);
        for y in 0..TILE {
            let src = (y * TILE * 4) as usize;
            let dst = (((r * TILE + y) * w + c * TILE) * 4) as usize;
            data[dst..dst + (TILE * 4) as usize].copy_from_slice(&px[src..src + (TILE * 4) as usize]);
        }
    }
    let mut img = pixel_image(w, h, data);
    let mut s = ImageSamplerDescriptor::nearest();
    s.set_address_mode(bevy::image::ImageAddressMode::ClampToEdge);
    img.sampler = ImageSampler::Descriptor(s);
    let image = images.add(img);
    let material = mats.add(StandardMaterial {
        base_color_texture: Some(image.clone()),
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        alpha_mode: AlphaMode::Mask(0.5),
        ..default()
    });
    commands.insert_resource(BlockAtlas { image, material, rows });
}

/// Chunk view entities of a grid.
#[derive(Component, Default)]
pub struct ChunkViews(pub HashMap<IVec3, Entity>);

#[derive(Component)]
pub struct ChunkView;

fn rebuild_chunks(
    mut commands: Commands,
    atlas: Option<Res<BlockAtlas>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut grids: Query<(Entity, &mut VoxelGrid, Option<&mut ChunkViews>)>,
) {
    let Some(atlas) = atlas else { return };
    for (ge, mut grid, views) in &mut grids {
        if grid.dirty.is_empty() {
            continue;
        }
        let Some(mut views) = views else {
            commands.entity(ge).insert(ChunkViews::default());
            continue;
        };
        let dirty: Vec<IVec3> = grid.dirty.drain().collect();
        for key in dirty {
            let data = mesh::mesh_chunk(&grid, key, &|i| atlas.uv(i));
            let boxes = mesh::chunk_boxes(&grid, key);
            if let Some(old) = views.0.remove(&key) {
                commands.entity(old).despawn();
            }
            if data.indices.is_empty() && boxes.is_empty() {
                continue;
            }
            let mut e = commands.spawn((ChunkView, Transform::default(), Visibility::default(), ChildOf(ge)));
            if !data.indices.is_empty() {
                e.insert((Mesh3d(meshes.add(data.into_mesh())), MeshMaterial3d(atlas.material.clone())));
            }
            if !boxes.is_empty() {
                let shapes = boxes.into_iter().map(|(c, h)| (c, Quat::IDENTITY, Collider::cuboid(h.x * 2.0, h.y * 2.0, h.z * 2.0))).collect();
                e.insert(Collider::compound(shapes));
            }
            views.0.insert(key, e.id());
        }
    }
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, build_atlas).add_systems(PostUpdate, rebuild_chunks);
    }
}
