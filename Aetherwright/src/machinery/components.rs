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
) {
    let steel = mat(&mut materials, Color::srgb(0.55, 0.57, 0.6), 0.7, 0.4);
    let dark = mat(&mut materials, Color::srgb(0.22, 0.23, 0.25), 0.5, 0.6);
    let brass = mat(&mut materials, Color::srgb(0.75, 0.6, 0.3), 0.85, 0.3);
    let wood = mat(&mut materials, Color::srgb(0.6, 0.43, 0.26), 0.0, 0.85);
    let rust = mat(&mut materials, Color::srgb(0.62, 0.28, 0.18), 0.4, 0.6);
    let rubber = mat(&mut materials, Color::srgb(0.12, 0.12, 0.13), 0.0, 0.95);
    let stone = mat(&mut materials, Color::srgb(0.42, 0.4, 0.38), 0.0, 0.9);
    let cyan = glow(&mut materials, Color::srgb(0.3, 0.9, 1.0), 3.0);
    let green = glow(&mut materials, Color::srgb(0.3, 1.0, 0.55), 2.5);
    let ember = glow(&mut materials, Color::srgb(1.0, 0.45, 0.15), 3.0);
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.9, 1.0, 0.3),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let mut m = |mesh: Mesh| meshes.add(mesh);
    let t = Transform::from_xyz;
    let rx = |tr: Transform| tr.with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));

    let mut gear_teeth = vec![(
        m(Cylinder::new(GEAR_RADIUS - 0.08, 0.25).into()),
        brass.clone(),
        Transform::IDENTITY,
    )];
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        gear_teeth.push((
            m(Cuboid::new(0.14, 0.24, 0.12).into()),
            brass.clone(),
            Transform::from_xyz(
                a.cos() * (GEAR_RADIUS - 0.04),
                0.0,
                a.sin() * (GEAR_RADIUS - 0.04),
            )
            .with_rotation(Quat::from_rotation_y(-a)),
        ));
    }
    gear_teeth.push((
        m(Cylinder::new(0.08, 0.3).into()),
        dark.clone(),
        Transform::IDENTITY,
    ));

    let list = vec![
        (
            PartKind::Frame,
            vec![
                (
                    m(Cuboid::new(1.0, 1.0, 1.0).into()),
                    steel.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cuboid::new(1.02, 0.12, 1.02).into()),
                    dark.clone(),
                    t(0.0, 0.0, 0.0),
                ),
            ],
        ),
        (
            PartKind::Crate,
            vec![
                (
                    m(Cuboid::new(0.9, 0.9, 0.9).into()),
                    wood.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cuboid::new(0.94, 0.1, 0.94).into()),
                    dark.clone(),
                    t(0.0, 0.3, 0.0),
                ),
                (
                    m(Cuboid::new(0.94, 0.1, 0.94).into()),
                    dark.clone(),
                    t(0.0, -0.3, 0.0),
                ),
            ],
        ),
        (
            PartKind::Engine,
            vec![
                (
                    m(Cuboid::new(1.0, 1.0, 1.0).into()),
                    rust.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cuboid::new(0.8, 0.2, 1.04).into()),
                    dark.clone(),
                    t(0.0, 0.25, 0.0),
                ),
                (
                    m(Cylinder::new(0.18, 0.12).into()),
                    ember.clone(),
                    t(0.0, 0.53, 0.0),
                ),
                (
                    m(Cylinder::new(0.25, 0.08).into()),
                    brass.clone(),
                    rx(t(0.0, 0.0, 0.52)),
                ),
                (
                    m(Cylinder::new(0.25, 0.08).into()),
                    brass.clone(),
                    rx(t(0.0, 0.0, -0.52)),
                ),
            ],
        ),
        (
            PartKind::PowerCell,
            vec![
                (
                    m(Cylinder::new(0.3, 1.0).into()),
                    dark.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cylinder::new(0.31, 0.12).into()),
                    green.clone(),
                    t(0.0, 0.2, 0.0),
                ),
                (
                    m(Cylinder::new(0.31, 0.12).into()),
                    green.clone(),
                    t(0.0, -0.2, 0.0),
                ),
                (
                    m(Cylinder::new(0.12, 0.1).into()),
                    brass.clone(),
                    t(0.0, 0.53, 0.0),
                ),
            ],
        ),
        (
            PartKind::Shaft,
            vec![(
                m(Cylinder::new(0.12, 2.0).into()),
                steel.clone(),
                Transform::IDENTITY,
            )],
        ),
        (PartKind::Gear, gear_teeth),
        (
            PartKind::Wheel,
            vec![
                (
                    m(Cylinder::new(0.6, 0.35).into()),
                    rubber.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cylinder::new(0.32, 0.37).into()),
                    steel.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cuboid::new(0.5, 0.38, 0.08).into()),
                    dark.clone(),
                    Transform::IDENTITY,
                ),
            ],
        ),
        (
            PartKind::Connector,
            vec![
                (
                    m(Cuboid::new(0.4, 0.4, 0.4).into()),
                    brass.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Sphere::new(0.12).mesh().ico(2).unwrap()),
                    dark.clone(),
                    t(0.0, 0.0, 0.0),
                ),
            ],
        ),
        (
            PartKind::AetherJar,
            vec![
                (
                    m(Cylinder::new(0.3, 0.7).into()),
                    glass.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Sphere::new(0.17).mesh().ico(3).unwrap()),
                    cyan.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cylinder::new(0.32, 0.08).into()),
                    brass.clone(),
                    t(0.0, 0.36, 0.0),
                ),
                (
                    m(Cylinder::new(0.32, 0.08).into()),
                    brass.clone(),
                    t(0.0, -0.36, 0.0),
                ),
            ],
        ),
        (
            PartKind::AetherDynamo,
            vec![
                (
                    m(Cuboid::new(1.0, 0.6, 1.0).into()),
                    steel.clone(),
                    t(0.0, -0.2, 0.0),
                ),
                (
                    m(Torus::new(0.25, 0.38).into()),
                    brass.clone(),
                    t(0.0, 0.25, 0.0),
                ),
                (
                    m(Sphere::new(0.2).mesh().ico(3).unwrap()),
                    cyan.clone(),
                    t(0.0, 0.3, 0.0),
                ),
            ],
        ),
        (
            PartKind::Workbench,
            vec![
                (
                    m(Cuboid::new(1.0, 0.15, 1.0).into()),
                    wood.clone(),
                    t(0.0, 0.42, 0.0),
                ),
                (
                    m(Cuboid::new(0.9, 0.85, 0.9).into()),
                    dark.clone(),
                    t(0.0, -0.08, 0.0),
                ),
                (
                    m(Cuboid::new(0.3, 0.1, 0.2).into()),
                    steel.clone(),
                    t(0.2, 0.55, 0.1),
                ),
            ],
        ),
        (
            PartKind::Smelter,
            vec![
                (
                    m(Cuboid::new(1.0, 1.0, 1.0).into()),
                    stone.clone(),
                    Transform::IDENTITY,
                ),
                (
                    m(Cuboid::new(0.5, 0.35, 0.05).into()),
                    ember.clone(),
                    t(0.0, -0.15, 0.5),
                ),
                (
                    m(Cylinder::new(0.15, 0.4).into()),
                    dark.clone(),
                    t(0.25, 0.65, -0.25),
                ),
            ],
        ),
        (
            PartKind::Grinder,
            vec![
                (
                    m(Cuboid::new(1.0, 0.7, 1.0).into()),
                    steel.clone(),
                    t(0.0, -0.15, 0.0),
                ),
                (
                    m(Cylinder::new(0.42, 0.3).into()),
                    stone.clone(),
                    t(0.0, 0.35, 0.0),
                ),
                (
                    m(Cylinder::new(0.1, 0.4).into()),
                    brass.clone(),
                    t(0.0, 0.5, 0.0),
                ),
            ],
        ),
    ];
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
