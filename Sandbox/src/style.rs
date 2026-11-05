//! Shared visual style (docs/STYLE.md).
//!
//! * `Materials` — PBR materials made from `assets/textures/<name>.png`
//!   (steel, brass, wood, rubber …) plus glowing cores.
//! * `pixel_box` — a box whose UVs keep one texture per metre, so a small
//!   part shows a piece of the texture at the same pixel density as a block:
//!   the chunky Create / Hytale look.
//! * `Part` / `spawn_parts` — models are lists of boxes (data), built once.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::grid::render::{load_texture, pixel_image, TILE};

/// (name, metallic, roughness)
pub const MATERIALS: [(&str, f32, f32); 19] = [
    ("white", 0.0, 0.9),
    ("concrete", 0.0, 0.92),
    ("steel", 0.55, 0.55),
    ("steel_casing", 0.4, 0.6),
    ("brass_casing", 0.5, 0.5),
    ("brass", 0.85, 0.32),
    ("copper", 0.8, 0.38),
    ("andesite", 0.05, 0.85),
    ("shaft", 0.2, 0.6),
    ("cogwood", 0.0, 0.85),
    ("planks", 0.0, 0.85),
    ("rubber", 0.0, 0.95),
    ("red_grip", 0.0, 0.7),
    ("leather", 0.0, 0.8),
    ("player", 0.0, 0.9),
    ("glass", 0.0, 0.1),
    ("copper_casing", 0.45, 0.55),
    ("water", 0.0, 0.15),
    ("diesel", 0.0, 0.2),
];

#[derive(Resource, Clone, Default)]
pub struct Materials(pub HashMap<String, Handle<StandardMaterial>>);

impl Materials {
    pub fn get(&self, k: &str) -> Handle<StandardMaterial> {
        self.0.get(k).cloned().unwrap_or_default()
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut m = HashMap::new();
    for (name, metallic, rough) in MATERIALS {
        let tex = images.add(pixel_image(TILE, TILE, load_texture(name)));
        let blend = name == "glass";
        m.insert(
            name.to_string(),
            mats.add(StandardMaterial {
                base_color_texture: Some(tex),
                metallic,
                perceptual_roughness: rough,
                alpha_mode: if blend { AlphaMode::Mask(0.5) } else { AlphaMode::Opaque },
                ..default()
            }),
        );
    }
    // Every other texture as a plain material too (custom models use any).
    for name in &crate::content::registry().textures {
        if !m.contains_key(name) {
            let tex = images.add(pixel_image(TILE, TILE, load_texture(name)));
            m.insert(name.clone(), mats.add(StandardMaterial { base_color_texture: Some(tex), perceptual_roughness: 0.85, alpha_mode: AlphaMode::Mask(0.5), ..default() }));
        }
    }
    // Glowing cores (HDR colour → bloom).
    for (name, c) in [("glow_amber", LinearRgba::rgb(4.0, 1.9, 0.3)), ("glow_cyan", LinearRgba::rgb(0.5, 2.6, 4.0)), ("glow_green", LinearRgba::rgb(0.4, 3.0, 0.8)), ("glow_red", LinearRgba::rgb(3.5, 0.4, 0.3))] {
        m.insert(name.to_string(), mats.add(StandardMaterial { base_color: Color::LinearRgba(c), unlit: true, ..default() }));
    }
    // Screens: dark glass background and unlit segments.
    m.insert("screen".into(), mats.add(StandardMaterial { base_color: Color::srgb(0.05, 0.09, 0.1), perceptual_roughness: 0.25, reflectance: 0.4, ..default() }));
    m.insert("blueprint".into(), mats.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(0.25, 0.7, 1.6)), unlit: true, ..default() }));
    m.insert("glass_lens".into(), mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.6, 0.58), perceptual_roughness: 0.15, reflectance: 0.6, ..default() }));
    m.insert("glow_lamp".into(), mats.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(6.0, 5.2, 3.4)), unlit: true, ..default() }));
    // Wires: a dull colour, and a bright one while powered.
    for (i, (_, c)) in crate::logic::COLORS.iter().enumerate() {
        let col = Color::srgb(c[0], c[1], c[2]);
        m.insert(crate::logic::WIRE_MATS[i].into(), mats.add(StandardMaterial { base_color: col.darker(0.25), perceptual_roughness: 0.6, ..default() }));
        let l = col.to_linear();
        m.insert(crate::logic::WIRE_MATS_ON[i].into(), mats.add(StandardMaterial { base_color: col, emissive: LinearRgba::rgb(l.red * 3.0, l.green * 3.0, l.blue * 3.0), ..default() }));
    }
    m.insert("smoke_black".into(), mats.add(StandardMaterial { base_color: Color::srgba(0.07, 0.07, 0.08, 0.85), alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() }));
    m.insert("smoke".into(), mats.add(StandardMaterial { base_color: Color::srgba(0.32, 0.32, 0.34, 0.7), alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() }));
    m.insert("seg_off".into(), mats.add(StandardMaterial { base_color: Color::srgb(0.13, 0.16, 0.17), perceptual_roughness: 0.6, ..default() }));
    commands.insert_resource(Materials(m));
}

/// A box of `size` (m) centred on the origin with world-scale UVs.
pub fn pixel_box(size: Vec3) -> Mesh {
    let h = size / 2.0;
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::NEG_Z, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::NEG_Y),
        (Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::Z, Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::NEG_Y),
    ];
    let mut pos = Vec::new();
    let mut nor = Vec::new();
    let mut uv = Vec::new();
    let mut col = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    for (n, u, v) in faces {
        let (du, dv) = ((u * size).abs().max_element(), (v * size).abs().max_element());
        let centre = n * h;
        let start = pos.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            pos.push((centre + u * (su * du / 2.0) + v * (sv * dv / 2.0)).to_array());
            nor.push(n.to_array());
            uv.push([(su + 1.0) / 2.0 * du, (sv + 1.0) / 2.0 * dv]);
            col.push(bake(n, centre + u * (su * du / 2.0) + v * (sv * dv / 2.0), h));
        }
        if u.cross(v).dot(n) > 0.0 {
            idx.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
        } else {
            idx.extend_from_slice(&[start, start + 2, start + 1, start, start + 3, start + 2]);
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    m.insert_indices(Indices::U32(idx));
    m
}

/// Baked light of a box vertex (Hytale-like readable volume): top faces
/// brightest, bottoms darkest, side faces fade from light at the top to
/// darker at the bottom.
fn bake(n: Vec3, p: Vec3, h: Vec3) -> [f32; 4] {
    let k = if n.y > 0.5 {
        1.06
    } else if n.y < -0.5 {
        0.8
    } else {
        let t = if h.y > 1e-4 { (p.y / h.y).clamp(-1.0, 1.0) } else { 0.0 };
        0.9 + 0.09 * t
    };
    [k, k, k, 1.0]
}

/// Meshes shared by size, and merged model meshes shared by content.
#[derive(Resource, Default)]
pub struct BoxCache(pub HashMap<[i32; 3], Handle<Mesh>>, pub HashMap<u64, Handle<Mesh>>);

impl BoxCache {
    pub fn get(&mut self, meshes: &mut Assets<Mesh>, size: Vec3) -> Handle<Mesh> {
        let key = (size * 1000.0).round().as_ivec3().to_array();
        self.0.entry(key).or_insert_with(|| meshes.add(pixel_box(size))).clone()
    }

    /// One mesh for several boxes (same material, same layer): far fewer
    /// entities and draw calls than one per box.
    pub fn merged(&mut self, meshes: &mut Assets<Mesh>, parts: &[&Part]) -> Handle<Mesh> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for p in parts {
            let q = |v: f32| (v * 10000.0).round() as i32;
            for v in [p.size.x, p.size.y, p.size.z, p.at.x, p.at.y, p.at.z, p.rot.x, p.rot.y, p.rot.z, p.rot.w] {
                q(v).hash(&mut h);
            }
        }
        let key = h.finish();
        self.1
            .entry(key)
            .or_insert_with(|| {
                let mut m = pixel_box(parts[0].size).transformed_by(Transform::from_translation(parts[0].at).with_rotation(parts[0].rot));
                for p in &parts[1..] {
                    let _ = m.merge(&pixel_box(p.size).transformed_by(Transform::from_translation(p.at).with_rotation(p.rot)));
                }
                meshes.add(m)
            })
            .clone()
    }
}

/// Which sub-node a part belongs to (animated separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Layer {
    /// Static body.
    #[default]
    Body,
    /// Spins with the block's kinetic speed about local Y.
    Rotor,
    /// Spins with the kinetic speed about local X (side gear rings).
    RotorX,
    /// Spins about local Z (the output of upright engines).
    RotorZ,
    /// A gearbox port stub: placed at the port `DIRS[i]` (block-local),
    /// turning about it.
    Port(u8),
    /// Turned by steering (suspension knuckle + wheel).
    Steer,
    /// Follows a bearing's angle / a piston's extension (`mounts`).
    Mount,
    /// Piston pole: stretched along local Y with the extension.
    Pole,
    /// End caps of a modular block (engines): hidden where the next module
    /// joins on that side (local +Z front / −Z back).
    CapFront,
    CapBack,
    /// Suspension spring: stretched with the wheel's travel.
    Spring,
    /// Suspension arm: moves down with the wheel.
    Arm,
}

/// One box of a model.
#[derive(Debug, Clone)]
pub struct Part {
    pub size: Vec3,
    pub mat: &'static str,
    pub at: Vec3,
    pub rot: Quat,
    pub layer: Layer,
}

pub fn part(size: [f32; 3], mat: &'static str, at: [f32; 3]) -> Part {
    Part { size: Vec3::from(size), mat, at: Vec3::from(at), rot: Quat::IDENTITY, layer: Layer::Body }
}

impl Part {
    pub fn rot(mut self, r: Quat) -> Self {
        self.rot = r;
        self
    }
    pub fn layer(mut self, l: Layer) -> Self {
        self.layer = l;
        self
    }
}

/// Marks the node that carries a layer's parts.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerNode(pub Layer);

/// Spawns a model under `parent`: one node per used layer, parts inside.
/// Parts of one layer and material become one merged mesh.
pub fn spawn_parts(commands: &mut Commands, parent: Entity, parts: &[Part], meshes: &mut Assets<Mesh>, cache: &mut BoxCache, mats: &Materials, shadows: bool) {
    let mut nodes: HashMap<Layer, Entity> = HashMap::new();
    let mut groups: Vec<((Layer, &str), Vec<&Part>)> = Vec::new();
    for p in parts {
        match groups.iter_mut().find(|(k, _)| *k == (p.layer, p.mat)) {
            Some((_, v)) => v.push(p),
            None => groups.push(((p.layer, p.mat), vec![p])),
        }
    }
    for ((layer, mat), group) in groups {
        let node = *nodes.entry(layer).or_insert_with(|| commands.spawn((LayerNode(layer), Transform::default(), Visibility::default(), ChildOf(parent))).id());
        let mesh = cache.merged(meshes, &group);
        let mut e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(mats.get(mat)), Transform::default(), ChildOf(node)));
        if !shadows || mat.starts_with("glow") {
            e.insert(NotShadowCaster);
        }
    }
}

pub struct StylePlugin;

impl Plugin for StylePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoxCache>().add_systems(PreStartup, setup);
    }
}
