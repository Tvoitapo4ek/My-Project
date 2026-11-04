//! Your own models from Blockbench (free, blockbench.net).
//!
//! Put `<block key>.bbmodel` into `assets/models/` and the block uses it
//! instead of the built-in model (in the world, the hand and the icon);
//! `player.bbmodel` replaces the player. The game reads cubes only:
//!
//! * a cube's texture picks the game material by name (`brass.png` →
//!   brass, any texture of `assets/textures`); a cube without a texture is
//!   steel. Face UVs are not used: every face shows the whole 32×32
//!   texture, like the built-in models;
//! * 16 Blockbench pixels = 1 block; the block is −8..8 on X/Z and 0..16 on
//!   Y («Generic model»; «Java block» 0..16 is recognised too);
//! * a group's name says what moves: `rotor` (spins with the shaft speed,
//!   about the block's axis), `rotor_x`, `rotor_z`, `steer`, `mount`,
//!   `pole`, `port0`..`port5`; anything else is the static body;
//! * player groups are hung on the animated limbs by name: `head`,
//!   `body`, `arm…` and `leg…` (left / right by the side of X), so walking,
//!   crouching and the swing keep working. 32 pixels = player height.

use std::{collections::HashMap, path::Path, sync::OnceLock};

use bevy::prelude::*;
use serde_json::Value;

use crate::{
    content::{Mech, registry},
    style::{Layer, Part},
};

/// Bones of the player rig, with pivots in model pixels.
pub const PLAYER_BONES: [(&str, [f32; 3]); 6] = [
    ("head", [0.0, 24.0, 0.0]),
    ("body", [0.0, 12.0, 0.0]),
    ("arm_r", [6.0, 22.0, 0.0]),
    ("arm_l", [-6.0, 22.0, 0.0]),
    ("leg_r", [2.0, 12.0, 0.0]),
    ("leg_l", [-2.0, 12.0, 0.0]),
];

/// One imported box (model pixels, before scaling).
#[derive(Debug, Clone)]
pub struct Cube {
    pub center: Vec3,
    pub size: Vec3,
    pub rot: Quat,
    pub texture: String,
    /// Name of the outermost named group (or the cube's own name).
    pub group: String,
    /// Origin of that group.
    pub group_origin: Vec3,
}

fn v3(v: Option<&Value>) -> Vec3 {
    let a = v.and_then(|v| v.as_array());
    let f = |i: usize| a.and_then(|a| a.get(i)).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
    Vec3::new(f(0), f(1), f(2))
}

fn euler(v: Option<&Value>) -> Quat {
    let r = v3(v);
    // Blockbench rotates Z, then Y, then X (three.js order 'ZYX').
    Quat::from_euler(EulerRot::ZYX, r.z.to_radians(), r.y.to_radians(), r.x.to_radians())
}

/// Reads the cubes of a `.bbmodel` (JSON).
pub fn parse(text: &str) -> Result<Vec<Cube>, String> {
    let j: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let textures: Vec<String> = j["textures"]
        .as_array()
        .map(|t| t.iter().map(|t| t["name"].as_str().unwrap_or("").trim_end_matches(".png").to_lowercase()).collect())
        .unwrap_or_default();
    let shift = if j["meta"]["model_format"].as_str() == Some("java_block") { Vec3::new(8.0, 0.0, 8.0) } else { Vec3::ZERO };
    let mut elements: HashMap<String, &Value> = HashMap::new();
    for e in j["elements"].as_array().into_iter().flatten() {
        if e["type"].as_str().unwrap_or("cube") == "cube" && e["visibility"].as_bool() != Some(false) {
            elements.insert(e["uuid"].as_str().unwrap_or("").to_string(), e);
        }
    }
    let cube = |e: &Value, group: &str, origin: Vec3, parent: (Vec3, Quat)| -> Cube {
        let (from, to) = (v3(e.get("from")) - shift, v3(e.get("to")) - shift);
        let o = v3(e.get("origin")) - shift;
        let r = euler(e.get("rotation"));
        let c = (from + to) * 0.5;
        let local = o + r * (c - o);
        let (pt, pq) = parent;
        let tex = e["faces"].as_object().and_then(|f| f.values().find_map(|f| f["texture"].as_u64())).and_then(|i| textures.get(i as usize)).cloned().unwrap_or_default();
        Cube {
            center: pt + pq * local,
            size: (to - from).abs().max(Vec3::splat(0.05)),
            rot: pq * r,
            texture: tex,
            group: if group.is_empty() { e["name"].as_str().unwrap_or("").to_lowercase() } else { group.to_string() },
            group_origin: if group.is_empty() { pt + pq * o } else { origin },
        }
    };
    let mut out = Vec::new();
    // Groups (the outliner) carry rotations and names.
    fn walk<'a>(node: &'a Value, elements: &HashMap<String, &'a Value>, group: &str, origin: Vec3, parent: (Vec3, Quat), shift: Vec3, out: &mut Vec<Cube>, cube: &dyn Fn(&Value, &str, Vec3, (Vec3, Quat)) -> Cube, seen: &mut Vec<String>) {
        if let Some(id) = node.as_str() {
            if let Some(e) = elements.get(id) {
                out.push(cube(e, group, origin, parent));
                seen.push(id.to_string());
            }
            return;
        }
        if node["visibility"].as_bool() == Some(false) {
            return;
        }
        let name = node["name"].as_str().unwrap_or("").to_lowercase();
        let o = v3(node.get("origin")) - shift;
        let r = euler(node.get("rotation"));
        // The group turns its children about its origin, inside the parent:
        // transforms are x → t + q·x, composed parent ∘ group.
        let (pt, pq) = parent;
        let (gt, gq) = (o - r * o, r);
        let here = (pt + pq * gt, pq * gq);
        let (g, go) = if group.is_empty() { (name, pt + pq * o) } else { (group.to_string(), origin) };
        for c in node["children"].as_array().into_iter().flatten() {
            walk(c, elements, &g, go, here, shift, out, cube, seen);
        }
    }
    let mut seen = Vec::new();
    for n in j["outliner"].as_array().into_iter().flatten() {
        walk(n, &elements, "", Vec3::ZERO, (Vec3::ZERO, Quat::IDENTITY), shift, &mut out, &cube, &mut seen);
    }
    // Cubes outside the outliner (older files).
    for (id, e) in &elements {
        if !seen.contains(id) {
            out.push(cube(e, "", Vec3::ZERO, (Vec3::ZERO, Quat::IDENTITY)));
        }
    }
    if out.is_empty() {
        return Err("в модели нет кубов".into());
    }
    Ok(out)
}

pub fn layer_of(group: &str) -> Layer {
    match group.replace([' ', '-'], "_").as_str() {
        "rotor" | "rotor_y" => Layer::Rotor,
        "rotor_x" => Layer::RotorX,
        "rotor_z" => Layer::RotorZ,
        "steer" => Layer::Steer,
        "mount" => Layer::Mount,
        "pole" => Layer::Pole,
        g => match g.strip_prefix("port").and_then(|i| i.parse::<u8>().ok()) {
            Some(i) if i < 6 => Layer::Port(i),
            _ => Layer::Body,
        },
    }
}

/// Untextured materials of the style (name → colour for Blockbench).
pub const PLAIN: [(&str, [u8; 3]); 11] = [
    ("glow_amber", [255, 190, 70]),
    ("glow_cyan", [120, 220, 255]),
    ("glow_green", [110, 240, 140]),
    ("glow_red", [255, 90, 70]),
    ("glow_lamp", [255, 240, 200]),
    ("screen", [13, 23, 26]),
    ("blueprint", [90, 170, 255]),
    ("glass_lens", [140, 153, 148]),
    ("seg_off", [33, 41, 43]),
    ("smoke", [82, 82, 87]),
    ("smoke_black", [18, 18, 20]),
];

/// A material name the game has (leaked once per name: models load once).
fn material(tex: &str) -> &'static str {
    static NAMES: OnceLock<std::sync::Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let known = crate::paint::texture_names().iter().any(|t| t == tex) || PLAIN.iter().any(|(n, _)| *n == tex) || wire_color(tex).is_some();
    let tex = if known { tex } else { "steel" };
    let mut m = NAMES.get_or_init(Default::default).lock().unwrap();
    m.entry(tex.to_string()).or_insert_with(|| Box::leak(tex.to_string().into_boxed_str()))
}

/// Block parts: 16 px = 1 block, centred on the cell (Y 0..16 → −0.5..0.5).
pub fn block_parts(cubes: &[Cube]) -> Vec<Part> {
    cubes
        .iter()
        .map(|c| Part { size: c.size / 16.0, mat: material(&c.texture), at: (c.center - Vec3::new(0.0, 8.0, 0.0)) / 16.0, rot: c.rot, layer: layer_of(&c.group) })
        .collect()
}

/// Which player bone a group belongs to.
pub fn bone_of(c: &Cube) -> usize {
    let g = c.group.replace([' ', '-', '_'], "");
    let side_r = c.group_origin.x > 0.0 || (c.group_origin.x == 0.0 && c.center.x > 0.0);
    if g.contains("head") || g.contains("hat") {
        0
    } else if g.contains("arm") {
        if side_r { 2 } else { 3 }
    } else if g.contains("leg") {
        if side_r { 4 } else { 5 }
    } else {
        1
    }
}

/// Player parts per bone, relative to the bone pivot, in model pixels.
pub fn player_parts(cubes: &[Cube]) -> [Vec<Part>; 6] {
    let mut out: [Vec<Part>; 6] = Default::default();
    for c in cubes {
        let b = bone_of(c);
        let pivot = Vec3::from(PLAYER_BONES[b].1);
        out[b].push(Part { size: c.size, mat: material(if c.texture.is_empty() { "player" } else { &c.texture }), at: c.center - pivot, rot: c.rot, layer: Layer::Body });
    }
    out
}

fn load(path: &Path) -> Option<Vec<Cube>> {
    let text = std::fs::read_to_string(path).ok()?;
    match parse(&text) {
        Ok(c) => {
            info!("model {}: {} cubes", path.display(), c.len());
            Some(c)
        }
        Err(e) => {
            warn!("model {}: {e}", path.display());
            None
        }
    }
}

struct Custom {
    blocks: HashMap<Mech, Vec<Part>>,
    player: Option<[Vec<Part>; 6]>,
}

fn custom() -> &'static Custom {
    static C: OnceLock<Custom> = OnceLock::new();
    C.get_or_init(|| {
        let dir = Path::new("assets/models");
        let mut blocks = HashMap::new();
        for b in &registry().blocks {
            if let Some(m) = b.mech
                && !blocks.contains_key(&m)
                && let Some(c) = load(&dir.join(format!("{}.bbmodel", b.key)))
            {
                blocks.insert(m, block_parts(&c));
            }
        }
        let player = load(&dir.join("player.bbmodel")).map(|c| player_parts(&c));
        Custom { blocks, player }
    })
}

/// The model from `assets/models/<key>.bbmodel`, if there is one.
pub fn block_override(m: Mech) -> Option<Vec<Part>> {
    custom().blocks.get(&m).cloned()
}

pub fn player_override() -> Option<&'static [Vec<Part>; 6]> {
    custom().player.as_ref()
}


// ------------------------------------------------------------ export

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `wire_3` / `wire_3_on` → that wire colour.
fn wire_color(name: &str) -> Option<[u8; 3]> {
    let i = crate::logic::WIRE_MATS.iter().chain(crate::logic::WIRE_MATS_ON.iter()).position(|w| *w == name)? % 8;
    let c = crate::logic::COLORS[i].1;
    Some([(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8])
}

fn png_data_uri(name: &str) -> String {
    let n = crate::grid::render::TILE;
    let plain = PLAIN.iter().find(|(k, _)| *k == name).map(|(_, c)| *c).or_else(|| wire_color(name));
    let px = match plain {
        Some(c) => (0..n * n).flat_map(|_| [c[0], c[1], c[2], 255]).collect(),
        None => crate::grid::render::load_texture(name),
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    let _ = image::write_buffer_with_format(&mut buf, &px, n, n, image::ExtendedColorType::Rgba8, image::ImageFormat::Png);
    format!("data:image/png;base64,{}", base64(buf.get_ref()))
}

fn layer_name(l: Layer) -> String {
    match l {
        Layer::Body => "body".into(),
        Layer::Rotor => "rotor".into(),
        Layer::RotorX => "rotor_x".into(),
        Layer::RotorZ => "rotor_z".into(),
        Layer::Steer => "steer".into(),
        Layer::Mount => "mount".into(),
        Layer::Pole => "pole".into(),
        Layer::Port(i) => format!("port{i}"),
    }
}

/// Boxes (model pixels, Y up from the floor) grouped by name → `.bbmodel`.
fn write(groups: &[(String, Vec3, Vec<(Vec3, Vec3, Quat, &str)>)]) -> String {
    let mut textures: Vec<&str> = Vec::new();
    let mut elements = Vec::new();
    let mut outliner = Vec::new();
    let mut id = 0;
    for (name, origin, cubes) in groups {
        let mut children = Vec::new();
        for (center, size, rot, mat) in cubes {
            let t = textures.iter().position(|t| t == mat).unwrap_or_else(|| {
                textures.push(mat);
                textures.len() - 1
            });
            let (z, y, x) = rot.to_euler(EulerRot::ZYX);
            let r = |v: f32| (v.to_degrees() * 1000.0).round() / 1000.0;
            let (from, to) = (*center - *size * 0.5, *center + *size * 0.5);
            let face = serde_json::json!({ "uv": [0, 0, 16, 16], "texture": t });
            let uuid = format!("00000000-0000-4000-8000-{id:012}");
            id += 1;
            elements.push(serde_json::json!({
                "name": mat, "type": "cube", "uuid": uuid,
                "from": from.to_array(), "to": to.to_array(), "origin": center.to_array(),
                "rotation": [r(x), r(y), r(z)],
                "faces": { "north": face, "east": face, "south": face, "west": face, "up": face, "down": face },
            }));
            children.push(serde_json::Value::String(uuid));
        }
        outliner.push(serde_json::json!({ "name": name, "origin": origin.to_array(), "children": children, "uuid": format!("00000000-0000-4000-9000-{id:012}") }));
        id += 1;
    }
    let textures: Vec<_> = textures.iter().enumerate().map(|(i, t)| serde_json::json!({ "name": format!("{t}.png"), "id": i.to_string(), "width": 32, "height": 32, "uv_width": 16, "uv_height": 16, "source": png_data_uri(t) })).collect();
    let j = serde_json::json!({
        "meta": { "format_version": "4.10", "model_format": "free", "box_uv": false },
        "name": "sandbox", "resolution": { "width": 16, "height": 16 },
        "elements": elements, "outliner": outliner, "textures": textures,
    });
    serde_json::to_string_pretty(&j).unwrap_or_default()
}

/// The built-in model of a block as a Blockbench file.
pub fn export_block(m: Mech) -> String {
    let parts = crate::models::builtin_model(m);
    let mut groups: Vec<(String, Vec3, Vec<(Vec3, Vec3, Quat, &str)>)> = Vec::new();
    for p in &parts {
        let name = layer_name(p.layer);
        let cube = (p.at * 16.0 + Vec3::new(0.0, 8.0, 0.0), p.size * 16.0, p.rot, p.mat);
        match groups.iter_mut().find(|g| g.0 == name) {
            Some(g) => g.2.push(cube),
            None => groups.push((name, Vec3::new(0.0, 8.0, 0.0), vec![cube])),
        }
    }
    write(&groups)
}

/// The default player (Minecraft proportions) as a Blockbench file.
pub fn export_player() -> String {
    let c = |x: f32, y: f32, w: f32, h: f32, d: f32| (Vec3::new(x, y, 0.0), Vec3::new(w, h, d), Quat::IDENTITY, "player");
    let bone = |i: usize, cubes| (PLAYER_BONES[i].0.to_string(), Vec3::from(PLAYER_BONES[i].1), cubes);
    write(&[
        bone(0, vec![c(0.0, 28.0, 8.0, 8.0, 8.0)]),
        bone(1, vec![c(0.0, 18.0, 8.0, 12.0, 4.0)]),
        bone(2, vec![c(6.0, 18.0, 4.0, 12.0, 4.0)]),
        bone(3, vec![c(-6.0, 18.0, 4.0, 12.0, 4.0)]),
        bone(4, vec![c(2.0, 6.0, 4.0, 12.0, 4.0)]),
        bone(5, vec![c(-2.0, 6.0, 4.0, 12.0, 4.0)]),
    ])
}

/// Console: `model export <block key | player>`.
pub fn export_cmd(key: &str) -> String {
    let text = if key == "player" {
        export_player()
    } else {
        match registry().blocks.iter().find(|b| b.key == key).and_then(|b| b.mech) {
            Some(m) => export_block(m),
            None => return format!("Нет механизма «{key}» (у простых блоков модели нет — у них текстура)"),
        }
    };
    let path = format!("assets/models/{key}.bbmodel");
    let _ = std::fs::create_dir_all("assets/models");
    match std::fs::write(&path, text) {
        Ok(()) => format!("Модель записана: {path} — откройте в Blockbench, после правки перезапустите игру"),
        Err(e) => format!("Не удалось записать {path}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "meta": {"format_version": "4.10", "model_format": "free"},
        "textures": [{"name": "brass.png"}, {"name": "steel.png"}],
        "elements": [
            {"name": "base", "type": "cube", "from": [-8, 0, -8], "to": [8, 4, 8], "origin": [0, 0, 0], "faces": {"north": {"texture": 1}}, "uuid": "a"},
            {"name": "blade", "type": "cube", "from": [-1, 4, -6], "to": [1, 12, 6], "origin": [0, 8, 0], "rotation": [0, 45, 0], "faces": {"up": {"texture": 0}}, "uuid": "b"}
        ],
        "outliner": ["a", {"name": "rotor", "origin": [0, 8, 0], "children": ["b"]}]
    }"#;

    #[test]
    fn reads_blockbench_cubes_and_groups() {
        let c = parse(SAMPLE).unwrap();
        assert_eq!(c.len(), 2);
        let p = block_parts(&c);
        let base = p.iter().find(|p| p.layer == Layer::Body).unwrap();
        assert_eq!(base.mat, "steel");
        assert!((base.size - Vec3::new(1.0, 0.25, 1.0)).length() < 1e-5, "16 px = 1 block");
        assert!((base.at - Vec3::new(0.0, -0.375, 0.0)).length() < 1e-5, "Y 0..16 → cell −0.5..0.5");
        let blade = p.iter().find(|p| p.layer == Layer::Rotor).unwrap();
        assert_eq!(blade.mat, "brass");
        assert!((blade.rot.to_euler(EulerRot::YXZ).0 - std::f32::consts::FRAC_PI_4).abs() < 1e-4);
        assert_eq!(layer_of("port3"), Layer::Port(3));
        assert!(parse("{}").is_err());
    }

    #[test]
    fn player_groups_follow_the_limbs() {
        let text = r#"{"meta": {"model_format": "free"}, "elements": [
            {"name": "c", "from": [4, 12, -2], "to": [8, 24, 2], "uuid": "1"},
            {"name": "c", "from": [-4, 24, -4], "to": [4, 32, 4], "uuid": "2"}],
          "outliner": [{"name": "RightArm", "origin": [5, 22, 0], "children": ["1"]}, {"name": "Head", "origin": [0, 24, 0], "children": ["2"]}]}"#;
        let parts = player_parts(&parse(text).unwrap());
        assert_eq!(parts[2].len(), 1, "the arm on +X is the right arm");
        assert!((parts[2][0].at - Vec3::new(0.0, -4.0, 0.0)).length() < 1e-5, "relative to the shoulder");
        assert!((parts[0][0].at - Vec3::new(0.0, 4.0, 0.0)).length() < 1e-5);
        assert_eq!(parts[0][0].mat, "player");
    }

    #[test]
    fn export_round_trip() {
        // A built-in model exported and read back is the same model.
        for m in registry().blocks.iter().filter_map(|b| b.mech) {
            let orig = crate::models::builtin_model(m);
            if orig.is_empty() {
                continue;
            }
            let back = block_parts(&parse(&export_block(m)).unwrap());
            assert_eq!(orig.len(), back.len(), "{m:?}");
            for p in &orig {
                let near = back.iter().any(|b| b.mat == p.mat && b.layer == p.layer && (b.at - p.at).length() < 1e-3 && (b.size - p.size).length() < 1e-3 && b.rot.angle_between(p.rot) < 1e-3);
                assert!(near, "{m:?}: {p:?}");
            }
        }
        let player = player_parts(&parse(&export_player()).unwrap());
        assert!(player.iter().all(|b| b.len() == 1), "one box per limb");
        assert!((player[4][0].at - Vec3::new(0.0, -6.0, 0.0)).length() < 1e-4, "leg hangs from the hip");
        assert_eq!(&base64(b"Man"), "TWFu");
        assert_eq!(&base64(b"Ma"), "TWE=");
    }
}
