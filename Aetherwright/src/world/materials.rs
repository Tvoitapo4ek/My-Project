//! Block textures and materials.
//!
//! For every texture in the content registry the game looks for a PNG:
//! `mods/<mod>/textures/blocks/<name>.png` (last mod wins), then
//! `assets/textures/blocks/<name>.png`. Without a file the built-in
//! procedural pattern is used (`pixel`). PNGs are re-read when they change
//! on disk (hot reload), so textures can be edited while the game runs —
//! in an external editor or the in-game texture editor.
//! Material parameters (alpha, emission, roughness…) come from
//! `textures.ron`.

use std::{path::PathBuf, time::SystemTime};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::{generation::hash3, voxel::TexId};
use crate::content::{AlphaKind, TextureDef, registry};

pub const TEX_SIZE: u32 = 16;

#[derive(Resource)]
pub struct BlockMaterials {
    pub by_tex: Vec<Handle<StandardMaterial>>,
    pub images: Vec<Handle<Image>>,
}

impl BlockMaterials {
    pub fn get(&self, t: TexId) -> Handle<StandardMaterial> {
        self.by_tex
            .get(t.0 as usize)
            .or(self.by_tex.first())
            .cloned()
            .unwrap_or_default()
    }
}

/// Where each texture was loaded from and when (hot reload).
#[derive(Resource, Default)]
pub struct TextureFiles(pub Vec<Option<(PathBuf, SystemTime)>>);

fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn n(seed: u32, x: u32, y: u32) -> f32 {
    (hash3(seed, x as i32, y as i32, 0) & 0xFFFF) as f32 / 65535.0
}

/// Returns RGBA for pixel (x, y) of a texture (y = 0 is the top row).
/// Procedural pattern by name (fallback when no PNG exists). `seed`
/// varies the noise; built-ins use their registry index.
pub fn pixel(name: &str, seed: u32, x: u32, y: u32) -> [f32; 4] {
    let s = seed * 7919 + 13;
    let r = n(s, x, y);
    let mix = |a: [f32; 3], b: [f32; 3], k: f32| {
        [
            a[0] + (b[0] - a[0]) * k,
            a[1] + (b[1] - a[1]) * k,
            a[2] + (b[2] - a[2]) * k,
        ]
    };
    let vary = |c: [f32; 3], amt: f32| {
        let k = 1.0 + (r - 0.5) * amt;
        [c[0] * k, c[1] * k, c[2] * k]
    };
    let opaque = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
    let stone = || {
        vary(
            mix(rgb(118, 118, 122), rgb(96, 98, 104), n(s ^ 9, x / 3, y / 3)),
            0.22,
        )
    };
    let speck = |base: [f32; 3], col: [f32; 3], salt: u32, thr: f32| {
        let cluster = n(salt, x / 2, y / 2);
        if cluster > thr && r > 0.3 {
            vary(col, 0.3)
        } else {
            base
        }
    };
    match name {
        "grass" => opaque(vary(
            mix(rgb(92, 138, 64), rgb(110, 152, 70), n(s ^ 3, x / 2, y / 2)),
            0.25,
        )),
        "grass_side" => {
            let fringe = 3 + (n(s ^ 5, x, 0) * 3.0) as u32;
            if y < fringe {
                opaque(vary(rgb(96, 142, 66), 0.2))
            } else {
                opaque(vary(rgb(121, 88, 62), 0.25))
            }
        }
        "dirt" => opaque(vary(
            mix(rgb(121, 88, 62), rgb(104, 74, 52), n(s ^ 4, x / 2, y / 2)),
            0.25,
        )),
        "stone" => opaque(stone()),
        "sand" => opaque(vary(rgb(214, 196, 148), 0.12)),
        "water" => [0.20, 0.42, 0.62, 0.72],
        "log_side" => {
            let stripe = ((x as f32 * 1.7 + n(s, x, y / 4) * 2.0).sin() * 0.5 + 0.5) * 0.25;
            opaque(vary(
                mix(rgb(102, 74, 48), rgb(70, 50, 33), stripe + 0.2),
                0.12,
            ))
        }
        "log_top" => {
            let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.5).powi(2)).sqrt();
            let ring = ((d * 1.6).sin() * 0.5 + 0.5) * 0.3;
            if d > 7.2 {
                opaque(rgb(80, 58, 38))
            } else {
                opaque(vary(mix(rgb(178, 142, 96), rgb(140, 106, 68), ring), 0.08))
            }
        }
        "leaves" => {
            let dark = n(s ^ 11, x, y) > 0.7;
            opaque(vary(
                if dark {
                    rgb(48, 92, 46)
                } else {
                    rgb(70, 118, 58)
                },
                0.25,
            ))
        }
        "iron_ore" => opaque(speck(stone(), rgb(196, 150, 120), s ^ 21, 0.68)),
        "copper_ore" => opaque(speck(stone(), rgb(206, 120, 70), s ^ 22, 0.7)),
        "coal_ore" => opaque(speck(stone(), rgb(36, 36, 40), s ^ 23, 0.62)),
        "aether" => {
            // Faceted crystal with bright seams.
            let facet = ((x + y) % 6 == 0 || (x + 16 - y) % 7 == 0) as u8 as f32;
            opaque(mix(
                vary(rgb(70, 190, 220), 0.25),
                rgb(220, 255, 255),
                facet * 0.8,
            ))
        }
        "agitated_aether" => {
            let crack = ((x * 3 + y * 5) % 9 == 0 || n(s ^ 31, x, y) > 0.9) as u8 as f32;
            opaque(mix(
                vary(rgb(180, 60, 140), 0.3),
                rgb(255, 200, 120),
                crack * 0.85,
            ))
        }
        "ancient" => {
            let rune =
                (x == 4 || x == 11) && (3..13).contains(&y) || (y == 8 && (4..12).contains(&x));
            if rune {
                opaque(rgb(120, 230, 210))
            } else {
                opaque(vary(rgb(62, 78, 84), 0.25))
            }
        }
        "planks" => {
            let seam = y % 4 == 0 || (x + (y / 4) * 5) % 16 == 0;
            if seam {
                opaque(rgb(110, 78, 46))
            } else {
                opaque(vary(rgb(176, 132, 82), 0.12))
            }
        }
        "bricks" => {
            let row = y / 4;
            let off = if row % 2 == 0 { 0 } else { 4 };
            let mortar = y % 4 == 0 || (x + off) % 8 == 0;
            if mortar {
                opaque(rgb(150, 146, 136))
            } else {
                opaque(vary(rgb(122, 120, 126), 0.15))
            }
        }
        "glass" => {
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            let glint = x + y == 6 || x + y == 8;
            if edge {
                [0.75, 0.88, 0.92, 0.9]
            } else if glint {
                [1.0, 1.0, 1.0, 0.5]
            } else {
                [0.7, 0.85, 0.9, 0.15]
            }
        }
        "metal" => {
            let rivet = (x == 2 || x == 13) && (y == 2 || y == 13);
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            if rivet {
                opaque(rgb(210, 214, 220))
            } else if edge {
                opaque(rgb(110, 116, 124))
            } else {
                opaque(vary(rgb(150, 156, 164), 0.06))
            }
        }
        "gravel" => opaque(vary(
            mix(rgb(130, 124, 118), rgb(90, 86, 84), n(s ^ 41, x / 2, y / 2)),
            0.3,
        )),
        "bedrock" => opaque(vary(
            mix(rgb(52, 52, 56), rgb(28, 28, 30), n(s ^ 51, x / 2, y / 2)),
            0.3,
        )),
        "clay" => opaque(vary(rgb(150, 156, 170), 0.1)),
        "moss" => {
            let moss = n(s ^ 61, x / 2, y / 2) > 0.35;
            opaque(if moss {
                vary(rgb(84, 120, 64), 0.25)
            } else {
                stone()
            })
        }
        "tall_grass" => {
            // Thin tapering blades of varying height.
            let blade = x % 3 == 1;
            let top = 2.0 + n(s ^ 71, x, 0) * 9.0;
            let width_ok = (y as f32) > top + (x % 2) as f32 * 2.0;
            if blade && width_ok {
                let k = y as f32 / 16.0;
                [0.30 + 0.12 * (1.0 - k), 0.52 + 0.15 * (1.0 - k), 0.22, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            }
        }
        "berry" => {
            let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 9.0).powi(2) * 1.4).sqrt();
            if d < 7.0 {
                let berry = n(s ^ 81, x / 2, y / 2) > 0.72;
                if berry {
                    [0.75, 0.15, 0.25, 1.0]
                } else {
                    [0.24, 0.45, 0.24, 1.0]
                }
            } else {
                [0.0, 0.0, 0.0, 0.0]
            }
        }
        // Unknown pattern: neutral checker so missing art is obvious but calm.
        _ => {
            let c = if (x / 4 + y / 4) % 2 == 0 {
                rgb(150, 150, 160)
            } else {
                rgb(110, 110, 120)
            };
            opaque(c)
        }
    }
}

/// RGBA8 pixels of the procedural version of a texture.
pub fn procedural_rgba(def: &TextureDef, seed: u32) -> Vec<u8> {
    let pattern = def.procedural.as_deref().unwrap_or(&def.name);
    let mut data = Vec::with_capacity((TEX_SIZE * TEX_SIZE * 4) as usize);
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            for c in pixel(pattern, seed, x, y) {
                data.push((c.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }
    data
}

/// Builds a tiling, nearest-filtered sRGB image from RGBA8 data.
pub fn image_from_rgba(width: u32, height: u32, data: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    let mut sampler = ImageSamplerDescriptor::nearest();
    sampler.set_address_mode(ImageAddressMode::Repeat);
    image.sampler = ImageSampler::Descriptor(sampler);
    image
}

/// Path of the PNG that overrides a texture, if any.
pub fn texture_file(name: &str) -> Option<PathBuf> {
    crate::content::find_asset(&format!("textures/blocks/{name}.png"))
}

/// Reads a PNG as RGBA8.
pub fn read_png(path: &std::path::Path) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    Ok((img.width(), img.height(), img.into_raw()))
}

/// Writes RGBA8 pixels as PNG (creating folders).
pub fn write_png(
    path: &std::path::Path,
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    image::save_buffer(path, data, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())
}

/// Texture image for a registry entry: PNG if present and valid, else procedural.
fn load_texture(def: &TextureDef, index: u16) -> (Image, Option<(PathBuf, SystemTime)>) {
    if let Some(path) = texture_file(&def.name) {
        match read_png(&path) {
            Ok((w, h, data)) => {
                let mtime = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                return (image_from_rgba(w, h, data), Some((path, mtime)));
            }
            Err(e) => warn!(
                "texture {}: cannot read {}: {e}; using procedural",
                def.name,
                path.display()
            ),
        }
    }
    (
        image_from_rgba(TEX_SIZE, TEX_SIZE, procedural_rgba(def, index as u32)),
        None,
    )
}

fn material_for(def: &TextureDef, image: Handle<Image>) -> StandardMaterial {
    let (er, eg, eb) = def.emissive;
    StandardMaterial {
        base_color_texture: Some(image),
        perceptual_roughness: def.roughness,
        metallic: def.metallic,
        reflectance: def.reflectance,
        emissive: LinearRgba::rgb(er, eg, eb),
        alpha_mode: match def.alpha {
            AlphaKind::Opaque => AlphaMode::Opaque,
            AlphaKind::Blend => AlphaMode::Blend,
            AlphaKind::Mask => AlphaMode::Mask(0.5),
        },
        cull_mode: if def.double_sided {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
        double_sided: def.double_sided,
        ..default()
    }
}

fn setup_block_materials(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut by_tex = Vec::new();
    let mut imgs = Vec::new();
    let mut files = Vec::new();
    for (i, def) in registry().textures.iter().enumerate() {
        let (image, file) = load_texture(def, i as u16);
        let img = images.add(image);
        by_tex.push(materials.add(material_for(def, img.clone())));
        imgs.push(img);
        files.push(file);
    }
    commands.insert_resource(BlockMaterials {
        by_tex,
        images: imgs,
    });
    commands.insert_resource(TextureFiles(files));
}

/// Re-reads textures whose PNG appeared or changed (checked twice a second).
fn hot_reload(
    time: Res<Time>,
    mut timer: Local<f32>,
    mats: Res<BlockMaterials>,
    mut files: ResMut<TextureFiles>,
    mut images: ResMut<Assets<Image>>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 0.5;
    for (i, def) in registry().textures.iter().enumerate() {
        let Some(path) = texture_file(&def.name) else {
            continue;
        };
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let known = files.0.get(i).cloned().flatten();
        if known
            .as_ref()
            .is_some_and(|(p, t)| *p == path && *t == mtime)
        {
            continue;
        }
        match read_png(&path) {
            Ok((w, h, data)) => {
                if let Some(mut img) = images.get_mut(&mats.images[i]) {
                    *img = image_from_rgba(w, h, data);
                    info!("texture {} reloaded from {}", def.name, path.display());
                }
                if let Some(slot) = files.0.get_mut(i) {
                    *slot = Some((path, mtime));
                }
            }
            Err(e) => warn!("texture {}: {e}", def.name),
        }
    }
}

/// `--export-textures`: writes every texture (current look) as PNG into
/// `assets/textures/blocks/` so it can be edited.
pub fn export_textures(dir: &std::path::Path) -> Result<usize, String> {
    let mut n = 0;
    for (i, def) in registry().textures.iter().enumerate() {
        let path = dir.join(format!("{}.png", def.name));
        if path.exists() {
            continue;
        }
        write_png(&path, TEX_SIZE, TEX_SIZE, &procedural_rgba(def, i as u32))?;
        n += 1;
    }
    Ok(n)
}

pub struct BlockMaterialsPlugin;

impl Plugin for BlockMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_block_materials)
            .add_systems(Update, hot_reload);
    }
}
