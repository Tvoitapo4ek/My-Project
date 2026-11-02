//! Procedural block textures and materials.
//!
//! All block art is generated at startup from small rules (base palette,
//! per-pixel variation, a pattern per material). This keeps the art style
//! coherent — muted natural tones, bright accents only on magical blocks —
//! and avoids mixing third-party assets. Textures are 16×16, nearest
//! filtered and tiled per block (UVs are in block units).

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::{generation::hash3, voxel::Tex};

pub const TEX_SIZE: u32 = 16;

#[derive(Resource)]
pub struct BlockMaterials {
    pub by_tex: Vec<Handle<StandardMaterial>>,
    pub images: Vec<Handle<Image>>,
}

impl BlockMaterials {
    pub fn get(&self, t: Tex) -> Handle<StandardMaterial> {
        self.by_tex[t as usize].clone()
    }
}

fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn n(seed: u32, x: u32, y: u32) -> f32 {
    (hash3(seed, x as i32, y as i32, 0) & 0xFFFF) as f32 / 65535.0
}

/// Returns RGBA for pixel (x, y) of a texture (y = 0 is the top row).
fn pixel(t: Tex, x: u32, y: u32) -> [f32; 4] {
    let s = t as u32 * 7919 + 13;
    let r = n(s, x, y);
    let mix = |a: [f32; 3], b: [f32; 3], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
    let vary = |c: [f32; 3], amt: f32| {
        let k = 1.0 + (r - 0.5) * amt;
        [c[0] * k, c[1] * k, c[2] * k]
    };
    let opaque = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
    let stone = || vary(mix(rgb(118, 118, 122), rgb(96, 98, 104), n(s ^ 9, x / 3, y / 3)), 0.22);
    let speck = |base: [f32; 3], col: [f32; 3], salt: u32, thr: f32| {
        let cluster = n(salt, x / 2, y / 2);
        if cluster > thr && r > 0.3 { vary(col, 0.3) } else { base }
    };
    match t {
        Tex::Grass => opaque(vary(mix(rgb(92, 138, 64), rgb(110, 152, 70), n(s ^ 3, x / 2, y / 2)), 0.25)),
        Tex::GrassSide => {
            let fringe = 3 + (n(s ^ 5, x, 0) * 3.0) as u32;
            if y < fringe { opaque(vary(rgb(96, 142, 66), 0.2)) } else { opaque(vary(rgb(121, 88, 62), 0.25)) }
        }
        Tex::Dirt => opaque(vary(mix(rgb(121, 88, 62), rgb(104, 74, 52), n(s ^ 4, x / 2, y / 2)), 0.25)),
        Tex::Stone => opaque(stone()),
        Tex::Sand => opaque(vary(rgb(214, 196, 148), 0.12)),
        Tex::Water => [0.20, 0.42, 0.62, 0.72],
        Tex::LogSide => {
            let stripe = ((x as f32 * 1.7 + n(s, x, y / 4) * 2.0).sin() * 0.5 + 0.5) * 0.25;
            opaque(vary(mix(rgb(102, 74, 48), rgb(70, 50, 33), stripe + 0.2), 0.12))
        }
        Tex::LogTop => {
            let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.5).powi(2)).sqrt();
            let ring = ((d * 1.6).sin() * 0.5 + 0.5) * 0.3;
            if d > 7.2 { opaque(rgb(80, 58, 38)) } else { opaque(vary(mix(rgb(178, 142, 96), rgb(140, 106, 68), ring), 0.08)) }
        }
        Tex::Leaves => {
            let dark = n(s ^ 11, x, y) > 0.7;
            opaque(vary(if dark { rgb(48, 92, 46) } else { rgb(70, 118, 58) }, 0.25))
        }
        Tex::IronOre => opaque(speck(stone(), rgb(196, 150, 120), s ^ 21, 0.68)),
        Tex::CopperOre => opaque(speck(stone(), rgb(206, 120, 70), s ^ 22, 0.7)),
        Tex::CoalOre => opaque(speck(stone(), rgb(36, 36, 40), s ^ 23, 0.62)),
        Tex::Aether => {
            // Faceted crystal with bright seams.
            let facet = ((x + y) % 6 == 0 || (x + 16 - y) % 7 == 0) as u8 as f32;
            opaque(mix(vary(rgb(70, 190, 220), 0.25), rgb(220, 255, 255), facet * 0.8))
        }
        Tex::AgitatedAether => {
            let crack = ((x * 3 + y * 5) % 9 == 0 || n(s ^ 31, x, y) > 0.9) as u8 as f32;
            opaque(mix(vary(rgb(180, 60, 140), 0.3), rgb(255, 200, 120), crack * 0.85))
        }
        Tex::Ancient => {
            let rune = (x == 4 || x == 11) && (3..13).contains(&y) || (y == 8 && (4..12).contains(&x));
            if rune { opaque(rgb(120, 230, 210)) } else { opaque(vary(rgb(62, 78, 84), 0.25)) }
        }
        Tex::Planks => {
            let seam = y % 4 == 0 || (x + (y / 4) * 5) % 16 == 0;
            if seam { opaque(rgb(110, 78, 46)) } else { opaque(vary(rgb(176, 132, 82), 0.12)) }
        }
        Tex::Bricks => {
            let row = y / 4;
            let off = if row % 2 == 0 { 0 } else { 4 };
            let mortar = y % 4 == 0 || (x + off) % 8 == 0;
            if mortar { opaque(rgb(150, 146, 136)) } else { opaque(vary(rgb(122, 120, 126), 0.15)) }
        }
        Tex::Glass => {
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            let glint = x + y == 6 || x + y == 8;
            if edge { [0.75, 0.88, 0.92, 0.9] } else if glint { [1.0, 1.0, 1.0, 0.5] } else { [0.7, 0.85, 0.9, 0.15] }
        }
        Tex::Metal => {
            let rivet = (x == 2 || x == 13) && (y == 2 || y == 13);
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            if rivet { opaque(rgb(210, 214, 220)) } else if edge { opaque(rgb(110, 116, 124)) } else { opaque(vary(rgb(150, 156, 164), 0.06)) }
        }
        Tex::Gravel => opaque(vary(mix(rgb(130, 124, 118), rgb(90, 86, 84), n(s ^ 41, x / 2, y / 2)), 0.3)),
        Tex::Bedrock => opaque(vary(mix(rgb(52, 52, 56), rgb(28, 28, 30), n(s ^ 51, x / 2, y / 2)), 0.3)),
        Tex::Clay => opaque(vary(rgb(150, 156, 170), 0.1)),
        Tex::Moss => {
            let moss = n(s ^ 61, x / 2, y / 2) > 0.35;
            opaque(if moss { vary(rgb(84, 120, 64), 0.25) } else { stone() })
        }
        Tex::TallGrass => {
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
        Tex::Berry => {
            let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 9.0).powi(2) * 1.4).sqrt();
            if d < 7.0 {
                let berry = n(s ^ 81, x / 2, y / 2) > 0.72;
                if berry { [0.75, 0.15, 0.25, 1.0] } else { [0.24, 0.45, 0.24, 1.0] }
            } else {
                [0.0, 0.0, 0.0, 0.0]
            }
        }
    }
}

pub fn make_texture(t: Tex) -> Image {
    let mut data = Vec::with_capacity((TEX_SIZE * TEX_SIZE * 4) as usize);
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let p = pixel(t, x, y);
            for c in p {
                data.push((c.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }
    let mut image = Image::new(
        Extent3d { width: TEX_SIZE, height: TEX_SIZE, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut sampler = ImageSamplerDescriptor::nearest();
    sampler.set_address_mode(ImageAddressMode::Repeat);
    image.sampler = ImageSampler::Descriptor(sampler);
    image
}

fn material_for(t: Tex, image: Handle<Image>) -> StandardMaterial {
    let mut m = StandardMaterial {
        base_color_texture: Some(image),
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        ..default()
    };
    match t {
        Tex::Water => {
            m.alpha_mode = AlphaMode::Blend;
            m.perceptual_roughness = 0.08;
            m.reflectance = 0.5;
            m.cull_mode = None;
        }
        Tex::Glass => {
            m.alpha_mode = AlphaMode::Blend;
            m.perceptual_roughness = 0.05;
            m.reflectance = 0.6;
        }
        Tex::Metal => {
            m.metallic = 0.75;
            m.perceptual_roughness = 0.4;
        }
        Tex::Aether => {
            m.emissive = LinearRgba::rgb(0.6, 2.4, 3.0);
            m.perceptual_roughness = 0.2;
        }
        Tex::AgitatedAether => {
            m.emissive = LinearRgba::rgb(2.8, 0.7, 1.6);
            m.perceptual_roughness = 0.25;
        }
        Tex::Ancient => {
            m.emissive = LinearRgba::rgb(0.05, 0.25, 0.22);
        }
        Tex::TallGrass | Tex::Berry => {
            m.alpha_mode = AlphaMode::Mask(0.5);
            m.cull_mode = None;
            m.double_sided = true;
        }
        _ => {}
    }
    m
}

fn setup_block_materials(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut by_tex = Vec::new();
    let mut imgs = Vec::new();
    for t in Tex::all() {
        let img = images.add(make_texture(t));
        by_tex.push(materials.add(material_for(t, img.clone())));
        imgs.push(img);
    }
    commands.insert_resource(BlockMaterials { by_tex, images: imgs });
}

pub struct BlockMaterialsPlugin;

impl Plugin for BlockMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_block_materials);
    }
}
