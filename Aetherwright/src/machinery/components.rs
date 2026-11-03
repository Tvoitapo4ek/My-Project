//! Functional parts: definitions, visuals (built from primitives in the
//! game's art style) and spawning. A part is a real rigid body tagged
//! `Prop::Part(key)` + `Part`; kind-specific behaviour lives in other
//! components (`Engine`, `PowerCell`, `Gear`, `Station`, ...).

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{core::layers::Layer, physics::Prop, tools::scanning::Scannable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartKind {
    Frame,
    Crate,
    Engine,
    PowerCell,
    Shaft,
    Gear,
    Wheel,
    Connector,
    AetherJar,
    AetherDynamo,
    Workbench,
    Smelter,
    Grinder,
}

impl PartKind {
    pub const ALL: [PartKind; 13] = [
        PartKind::Frame,
        PartKind::Crate,
        PartKind::Engine,
        PartKind::PowerCell,
        PartKind::Shaft,
        PartKind::Gear,
        PartKind::Wheel,
        PartKind::Connector,
        PartKind::AetherJar,
        PartKind::AetherDynamo,
        PartKind::Workbench,
        PartKind::Smelter,
        PartKind::Grinder,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PartKind::Frame => "frame",
            PartKind::Crate => "crate",
            PartKind::Engine => "engine",
            PartKind::PowerCell => "power_cell",
            PartKind::Shaft => "shaft",
            PartKind::Gear => "gear",
            PartKind::Wheel => "wheel",
            PartKind::Connector => "connector",
            PartKind::AetherJar => "aether_jar",
            PartKind::AetherDynamo => "aether_dynamo",
            PartKind::Workbench => "workbench",
            PartKind::Smelter => "smelter",
            PartKind::Grinder => "grinder",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == k)
    }

    /// Stations are installed fixed in the world (furniture), not free bodies.
    pub fn is_station(self) -> bool {
        matches!(
            self,
            PartKind::Workbench | PartKind::Smelter | PartKind::Grinder
        )
    }

    pub fn mass(self) -> f32 {
        crate::items::ItemId::from_key(self.key())
            .map(|i| i.def().mass)
            .unwrap_or(10.0)
    }

    pub fn collider(self) -> Collider {
        match self {
            PartKind::Frame | PartKind::Engine | PartKind::AetherDynamo => {
                Collider::cuboid(1.0, 1.0, 1.0)
            }
            PartKind::Workbench | PartKind::Smelter | PartKind::Grinder => {
                Collider::cuboid(1.0, 1.0, 1.0)
            }
            PartKind::Crate => Collider::cuboid(0.9, 0.9, 0.9),
            PartKind::PowerCell => Collider::cylinder(0.3, 1.0),
            PartKind::Shaft => Collider::cylinder(0.12, 2.0),
            PartKind::Gear => Collider::cylinder(GEAR_RADIUS, 0.25),
            PartKind::Wheel => Collider::cylinder(0.6, 0.35),
            PartKind::Connector => Collider::cuboid(0.4, 0.4, 0.4),
            PartKind::AetherJar => Collider::cylinder(0.3, 0.7),
        }
    }

    /// Approximate half extent along local Y (for placement on surfaces).
    pub fn half_height(self) -> f32 {
        match self {
            PartKind::Crate => 0.45,
            PartKind::PowerCell => 0.5,
            PartKind::Shaft => 1.0,
            PartKind::Gear => 0.125,
            PartKind::Wheel => 0.175,
            PartKind::Connector => 0.2,
            PartKind::AetherJar => 0.35,
            _ => 0.5,
        }
    }
}

pub const GEAR_RADIUS: f32 = 0.5;

/// Every functional part has this.
#[derive(Component, Debug, Clone, Copy)]
pub struct Part {
    pub kind: PartKind,
}

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Engine {
    pub on: bool,
    /// 0..=1
    pub throttle: f32,
    pub reverse: bool,
    /// rad/s at full throttle
    pub max_speed: f32,
    /// N·m
    pub max_torque: f32,
    /// energy units per second at full throttle
    pub draw: f32,
    /// Was powered on the last tick (for UI).
    #[serde(skip)]
    pub powered: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            on: false,
            throttle: 1.0,
            reverse: false,
            max_speed: 9.0,
            max_torque: 900.0,
            draw: 6.0,
            powered: false,
        }
    }
}

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct PowerCell {
    pub energy: f32,
    pub capacity: f32,
}

/// Converts stored aether into energy (see `magic`).
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct AetherDynamo {
    /// Energy produced last second (UI).
    #[serde(skip)]
    pub output: f32,
    /// Produced energy waiting to be taken by the electric network
    /// (small: the dynamo has no real storage).
    #[serde(skip)]
    pub buffer: f32,
}

/// Max energy a dynamo holds before production is wasted.
pub const DYNAMO_BUFFER: f32 = 8.0;

#[derive(Component, Debug, Clone, Copy)]
pub struct Gear {
    pub radius: f32,
}

/// Shared visual assets for parts.
#[derive(Resource)]
pub struct PartAssets {
    pub meshes: Vec<(
        PartKind,
        Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)>,
    )>,
}

impl PartAssets {
    fn visuals(&self, kind: PartKind) -> &[(Handle<Mesh>, Handle<StandardMaterial>, Transform)] {
        self.meshes
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| v.as_slice())
            .unwrap_or(&[])
    }
}

fn mat(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
    metallic: f32,
    rough: f32,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        metallic,
        perceptual_roughness: rough,
        ..default()
    })
}

fn glow(materials: &mut Assets<StandardMaterial>, c: Color, k: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        emissive: c.to_linear() * k,
        ..default()
    })
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    style: Res<crate::style::StyleMaterials>,
    blocks: Option<Res<crate::world::materials::BlockMaterials>>,
) {
    // Style (docs/STYLE_GUIDE.md): Create-like parts from pixel boxes —
    // andesite, brass, copper, wood; magic parts in rune stone and gold.
    let st = |k: &str| style.get(k);
    let block_mat = |name: &str| {
        let i = crate::content::registry().texture_id(name).map(|t| t as usize);
        blocks.as_ref().and_then(|b| i.and_then(|i| b.by_tex.get(i).cloned())).unwrap_or_else(|| style.get("cogwood"))
    };
    let glow_cyan = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(1.0, 3.0, 4.0)), unlit: true, ..default() });
    let glow_amber = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(4.0, 1.8, 0.4)), unlit: true, ..default() });
    let glow_violet = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(2.4, 1.0, 3.6)), unlit: true, ..default() });
    let mut bx = |x: f32, y: f32, z: f32| meshes.add(crate::style::pixel_box(Vec3::new(x, y, z)));
    let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);
    let turn = |t: Transform, q: Quat| t.with_rotation(q);
    let q45y = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let mut list: Vec<(PartKind, Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)>)> = Vec::new();

    // Octagonal disk (two squares) in the XZ plane + teeth around it.
    let mut cog = |radius: f32, teeth: usize, thick: f32| {
        let side = radius * 1.5;
        let mut v = vec![
            (bx(side, thick, side), st("cogwood"), at(0.0, 0.0, 0.0)),
            (bx(side, thick, side), st("cogwood"), turn(at(0.0, 0.0, 0.0), q45y)),
            (bx(0.3, thick + 0.06, 0.3), st("andesite"), at(0.0, 0.0, 0.0)),
        ];
        for i in 0..teeth {
            let a = i as f32 / teeth as f32 * std::f32::consts::TAU;
            v.push((bx(0.2, thick + 0.02, 0.18), st("cogwood"), turn(at(a.cos() * radius, 0.0, a.sin() * radius), Quat::from_rotation_y(-a))));
        }
        v
    };
    list.push((PartKind::Gear, cog(GEAR_RADIUS - 0.05, 8, 0.24)));

    // Frame: andesite edges around a plank core.
    let mut frame = vec![(bx(0.9, 0.9, 0.9), block_mat("planks"), at(0.0, 0.0, 0.0))];
    for (a, b) in [(-0.43f32, -0.43f32), (-0.43, 0.43), (0.43, -0.43), (0.43, 0.43)] {
        frame.push((bx(0.16, 1.0, 0.16), st("andesite"), at(a, 0.0, b)));
        frame.push((bx(1.0, 0.16, 0.16), st("andesite"), at(0.0, a, b)));
        frame.push((bx(0.16, 0.16, 1.0), st("andesite"), at(a, b, 0.0)));
    }
    list.push((PartKind::Frame, frame));

    list.push((
        PartKind::Crate,
        vec![
            (bx(0.88, 0.88, 0.88), block_mat("planks"), at(0.0, 0.0, 0.0)),
            (bx(0.92, 0.1, 0.92), st("andesite"), at(0.0, 0.4, 0.0)),
            (bx(0.92, 0.1, 0.92), st("andesite"), at(0.0, -0.4, 0.0)),
            (bx(0.2, 0.14, 0.06), st("brass"), at(0.0, 0.25, 0.46)),
        ],
    ));
    list.push((
        PartKind::Engine,
        vec![
            (bx(0.98, 0.98, 0.98), st("copper_casing"), at(0.0, 0.0, 0.0)),
            (bx(0.8, 0.2, 1.02), st("brass"), at(0.0, 0.3, 0.0)),
            (bx(0.08, 0.5, 0.5), st("andesite"), at(0.52, 0.0, 0.0)),
            (bx(0.08, 0.5, 0.5), st("andesite"), turn(at(0.52, 0.0, 0.0), Quat::from_rotation_x(std::f32::consts::FRAC_PI_4))),
            (bx(0.08, 0.5, 0.5), st("andesite"), at(-0.52, 0.0, 0.0)),
            (bx(0.08, 0.5, 0.5), st("andesite"), turn(at(-0.52, 0.0, 0.0), Quat::from_rotation_x(std::f32::consts::FRAC_PI_4))),
            (bx(0.3, 0.08, 0.3), glow_amber.clone(), at(0.0, 0.5, 0.0)),
        ],
    ));
    list.push((
        PartKind::PowerCell,
        vec![
            (bx(0.56, 0.96, 0.56), st("copper_casing"), at(0.0, 0.0, 0.0)),
            (bx(0.6, 0.1, 0.6), st("brass"), at(0.0, 0.42, 0.0)),
            (bx(0.6, 0.1, 0.6), st("brass"), at(0.0, -0.42, 0.0)),
            (bx(0.16, 0.6, 0.58), glow_cyan.clone(), at(0.0, 0.0, 0.0)),
            (bx(0.16, 0.12, 0.16), st("andesite"), at(0.0, 0.53, 0.0)),
        ],
    ));
    list.push((PartKind::Shaft, vec![(bx(0.24, 2.0, 0.24), st("shaft"), at(0.0, 0.0, 0.0))]));
    let side = 0.85;
    list.push((
        PartKind::Wheel,
        vec![
            (bx(side, 0.35, side), st("rubber"), at(0.0, 0.0, 0.0)),
            (bx(side, 0.35, side), st("rubber"), turn(at(0.0, 0.0, 0.0), q45y)),
            (bx(0.5, 0.37, 0.5), st("cogwood"), at(0.0, 0.0, 0.0)),
            (bx(0.5, 0.37, 0.5), st("cogwood"), turn(at(0.0, 0.0, 0.0), q45y)),
            (bx(0.22, 0.4, 0.22), st("andesite"), at(0.0, 0.0, 0.0)),
        ],
    ));
    list.push((
        PartKind::Connector,
        vec![
            (bx(0.4, 0.4, 0.4), st("brass_casing"), at(0.0, 0.0, 0.0)),
            (bx(0.2, 0.42, 0.2), st("andesite"), at(0.0, 0.0, 0.0)),
        ],
    ));
    list.push((
        PartKind::AetherJar,
        vec![
            (bx(0.5, 0.56, 0.5), st("crystal"), at(0.0, 0.0, 0.0)),
            (bx(0.3, 0.36, 0.3), glow_violet.clone(), at(0.0, -0.02, 0.0)),
            (bx(0.56, 0.08, 0.56), st("gold_trim"), at(0.0, 0.31, 0.0)),
            (bx(0.56, 0.08, 0.56), st("gold_trim"), at(0.0, -0.31, 0.0)),
        ],
    ));
    list.push((
        PartKind::AetherDynamo,
        vec![
            (bx(0.98, 0.3, 0.98), st("arcane_stone"), at(0.0, -0.15, 0.0)),
            (bx(1.0, 0.07, 1.0), st("gold_trim"), at(0.0, 0.02, 0.0)),
            (bx(0.6, 0.2, 0.6), st("copper"), at(0.0, 0.12, 0.0)),
            (bx(0.3, 0.3, 0.3), glow_cyan.clone(), turn(at(0.0, 0.32, 0.0), q45y)),
        ],
    ));
    list.push((
        PartKind::Workbench,
        vec![
            (bx(1.0, 0.16, 1.0), block_mat("planks"), at(0.0, 0.42, 0.0)),
            (bx(0.84, 0.82, 0.84), st("andesite_casing"), at(0.0, -0.08, 0.0)),
            (bx(0.3, 0.12, 0.2), st("andesite"), at(-0.2, 0.56, 0.1)),
            (bx(0.08, 0.06, 0.4), st("brass"), at(0.25, 0.53, -0.1)),
        ],
    ));
    list.push((
        PartKind::Smelter,
        vec![
            (bx(1.0, 1.0, 1.0), block_mat("bricks"), at(0.0, 0.0, 0.0)),
            (bx(0.5, 0.32, 0.06), glow_amber.clone(), at(0.0, -0.15, 0.49)),
            (bx(0.3, 0.4, 0.3), st("copper"), at(0.25, 0.68, -0.2)),
            (bx(1.02, 0.1, 1.02), st("copper"), at(0.0, 0.45, 0.0)),
        ],
    ));
    let mut grinder = vec![
        (bx(1.0, 0.66, 1.0), st("andesite_casing"), at(0.0, -0.17, 0.0)),
        (bx(0.4, 0.06, 0.8), st("andesite"), at(0.0, 0.17, 0.0)),
    ];
    for z in [-0.18f32, 0.18] {
        let r = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        grinder.push((bx(0.5, 0.3, 0.5), st("cogwood"), turn(at(0.0, 0.2, z), r)));
        grinder.push((bx(0.5, 0.3, 0.5), st("cogwood"), turn(at(0.0, 0.2, z), r * q45y)));
    }
    list.push((PartKind::Grinder, grinder));
    let _ = (mat, glow);
    commands.insert_resource(PartAssets { meshes: list });
}

/// Builds the scanner description for a part.
pub fn describe(kind: PartKind) -> Scannable {
    let item = crate::items::ItemId::from_key(kind.key());
    Scannable {
        title: item
            .map(|i| i.def().name.as_str())
            .unwrap_or("Деталь")
            .to_string(),
        info: item
            .map(|i| i.def().info.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// Spawns a part body. Stations are static; everything else dynamic.
pub fn spawn_part(
    commands: &mut Commands,
    assets: &PartAssets,
    kind: PartKind,
    transform: Transform,
) -> Entity {
    let body = if kind.is_station() {
        RigidBody::Static
    } else {
        RigidBody::Dynamic
    };
    let friction = if kind == PartKind::Wheel { 1.4 } else { 0.6 };
    let mut e = commands.spawn((
        Name::new(format!("Part {}", kind.key())),
        Part { kind },
        Prop::Part(kind.key().to_string()),
        describe(kind),
        transform,
        Visibility::default(),
        body,
        kind.collider(),
        Mass(kind.mass()),
        // Bearing/air losses: free rotors spin down instead of forever.
        AngularDamping(0.35),
        Friction::new(friction),
        CollisionLayers::new(Layer::Prop, LayerMask::ALL),
    ));
    match kind {
        PartKind::Engine => {
            e.insert((Engine::default(), SleepingDisabled));
        }
        PartKind::PowerCell => {
            e.insert(PowerCell {
                energy: 600.0,
                capacity: 1000.0,
            });
        }
        PartKind::Gear => {
            e.insert((
                Gear {
                    radius: GEAR_RADIUS,
                },
                SleepingDisabled,
            ));
        }
        PartKind::AetherDynamo => {
            e.insert(AetherDynamo::default());
        }
        PartKind::Grinder => {
            e.insert(SleepingDisabled);
        }
        _ => {}
    }
    e.insert((
        crate::models::ModelSlot {
            category: "parts",
            key: kind.key().to_string(),
        },
        crate::models::AnimationRequest("idle".into()),
    ));
    e.with_children(|p| {
        for (mesh, material, t) in assets.visuals(kind) {
            p.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                *t,
                crate::models::PrimitiveVisual,
            ));
        }
    });
    e.id()
}

pub struct ComponentsPlugin;

impl Plugin for ComponentsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_assets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_has_an_item() {
        for p in PartKind::ALL {
            let item = crate::items::ItemId::from_key(p.key())
                .unwrap_or_else(|| panic!("no item for {}", p.key()));
            assert_eq!(
                item.def().use_,
                crate::items::Use::Part(p.key().to_string())
            );
            assert_eq!(PartKind::from_key(p.key()), Some(p));
        }
    }
}
