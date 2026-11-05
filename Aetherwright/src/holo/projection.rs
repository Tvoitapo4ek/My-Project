//! 3D holograms projected in front of the centre panel: the character
//! figure (equipment / character), terrain (map), blueprint models.
//! Everything here is on `HOLO_LAYER`, unlit and translucent.
//!
//! Views (rotation, tilt, zoom, pan) live in `HoloViews`, keyed by section,
//! so they survive UI rebuilds. A UI node with `DragView` turns pointer
//! drags / wheel over it into view changes.

use std::collections::HashMap;

use bevy::{camera::visibility::RenderLayers, light::NotShadowCaster, prelude::*};

use super::panels::HOLO_LAYER;
use crate::{
    content::EquipSlot,
    items::equipment::{Equipment, SlotRef},
};

#[derive(Resource, Clone)]
pub struct HoloMats {
    pub line: Handle<StandardMaterial>,
    pub glass: Handle<StandardMaterial>,
    pub bright: Handle<StandardMaterial>,
    pub violet: Handle<StandardMaterial>,
    pub amber: Handle<StandardMaterial>,
    pub highlight: Handle<StandardMaterial>,
}

/// Unlit translucent material for holograms.
pub fn holo_material(color: Color, additive: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        unlit: true,
        alpha_mode: if additive { AlphaMode::Add } else { AlphaMode::Blend },
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

fn init_mats(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(HoloMats {
        line: materials.add(holo_material(Color::srgba(0.45, 0.92, 1.0, 0.75), false)),
        glass: materials.add(holo_material(Color::srgba(0.3, 0.8, 1.0, 0.22), false)),
        bright: materials.add(holo_material(Color::srgba(0.85, 1.0, 1.0, 0.9), false)),
        violet: materials.add(holo_material(Color::srgba(0.78, 0.55, 1.0, 0.7), false)),
        amber: materials.add(holo_material(Color::srgba(1.0, 0.76, 0.35, 0.8), false)),
        highlight: materials.add(holo_material(Color::srgba(1.0, 1.0, 1.0, 0.95), true)),
    });
}

/// Bundle for a hologram mesh part.
pub fn holo_mesh(mesh: Handle<Mesh>, mat: Handle<StandardMaterial>, t: Transform) -> impl Bundle {
    (Mesh3d(mesh), MeshMaterial3d(mat), t, NotShadowCaster, RenderLayers::layer(HOLO_LAYER))
}

/// Bundle for an empty hologram group node.
pub fn holo_group(t: Transform) -> impl Bundle {
    (t, Visibility::Inherited, RenderLayers::layer(HOLO_LAYER))
}

#[derive(Debug, Clone, Copy)]
pub struct ViewState {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: Vec2,
    /// Rotate slowly by itself until the user drags.
    pub spin: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { yaw: 0.4, pitch: 0.0, zoom: 1.0, pan: Vec2::ZERO, spin: true }
    }
}

#[derive(Resource, Default, Debug)]
pub struct HoloViews(pub HashMap<&'static str, ViewState>);

impl HoloViews {
    pub fn get(&mut self, k: &'static str) -> &mut ViewState {
        self.0.entry(k).or_default()
    }
}

/// Hologram root driven by a view.
#[derive(Component, Debug, Clone, Copy)]
pub struct Turntable {
    pub view: &'static str,
    pub base_scale: f32,
    /// Tilt is applied (map) or ignored (figures stay upright).
    pub tilt: bool,
}

/// UI node that controls a view with drag (LMB rotate, MMB pan) and wheel.
#[derive(Component, Debug, Clone, Copy)]
pub struct DragView(pub &'static str);

fn on_drag(ev: On<bevy::picking::events::PointerDrag>, q: Query<&DragView>, mut views: ResMut<HoloViews>) {
    let Ok(d) = q.get(ev.event_target()) else {
        return;
    };
    let v = views.get(d.0);
    v.spin = false;
    match ev.button {
        bevy::picking::pointer::PointerButton::Middle => v.pan += ev.delta * 0.002 / v.zoom,
        _ => {
            v.yaw += ev.delta.x * 0.01;
            v.pitch = (v.pitch + ev.delta.y * 0.006).clamp(-0.2, 1.2);
        }
    }
}

fn on_wheel(ev: On<bevy::picking::events::PointerScroll>, q: Query<&DragView>, mut views: ResMut<HoloViews>) {
    let Ok(d) = q.get(ev.event_target()) else {
        return;
    };
    let v = views.get(d.0);
    let step = if ev.unit == bevy::input::mouse::MouseScrollUnit::Line { ev.y * 0.12 } else { ev.y * 0.002 };
    v.zoom = (v.zoom * (1.0 + step)).clamp(0.4, 4.0);
}

fn apply_turntables(time: Res<Time>, mut views: ResMut<HoloViews>, mut q: Query<(&Turntable, &mut Transform)>) {
    let dt = time.delta_secs();
    for v in views.0.values_mut() {
        if v.spin {
            v.yaw += dt * 0.35;
        }
    }
    for (tt, mut t) in &mut q {
        let v = views.get(tt.view);
        let tilt = if tt.tilt { v.pitch } else { 0.0 };
        t.rotation = Quat::from_rotation_x(tilt) * Quat::from_rotation_y(v.yaw);
        t.scale = Vec3::splat(tt.base_scale * if tt.tilt { 1.0 } else { v.zoom });
    }
}

/// Marks the figure part that belongs to an equipment slot.
#[derive(Component)]
#[allow(dead_code)]
pub struct FigurePart(pub Option<EquipSlot>);

/// Character figure (~0.5 m tall) with worn items as coloured overlays.
/// `highlight` makes the inspected slot glow.
pub fn spawn_figure(
    commands: &mut Commands,
    parent: Entity,
    view: &'static str,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    mats: &HoloMats,
    eq: Option<&Equipment>,
    highlight: Option<SlotRef>,
) -> Entity {
    let root = commands
        .spawn((
            holo_group(Transform::from_xyz(0.0, -0.04, 0.0)),
            Turntable { view, base_scale: 0.27, tilt: false },
            ChildOf(parent),
        ))
        .id();
    // Body from the same primitives as the player model (metres ×1.8 tall).
    let body = [
        (meshes.add(Capsule3d::new(0.28, 0.55)), Vec3::new(0.0, 0.05, 0.0), None),
        (meshes.add(Sphere::new(0.21).mesh().ico(2).unwrap()), Vec3::new(0.0, 0.66, 0.0), None),
        (meshes.add(Capsule3d::new(0.1, 0.5)), Vec3::new(-0.13, -0.6, 0.0), None),
        (meshes.add(Capsule3d::new(0.1, 0.5)), Vec3::new(0.13, -0.6, 0.0), None),
        (meshes.add(Capsule3d::new(0.07, 0.45)), Vec3::new(-0.36, 0.05, 0.0), None),
        (meshes.add(Capsule3d::new(0.07, 0.45)), Vec3::new(0.36, 0.05, 0.0), None),
    ];
    for (m, p, _slot) in body.into_iter().map(|(m, p, s): (Handle<Mesh>, Vec3, Option<EquipSlot>)| (m, p, s)) {
        commands.spawn((holo_mesh(m, mats.glass.clone(), Transform::from_translation(p)), FigurePart(None), ChildOf(root)));
    }
    // Device on the right wrist.
    commands.spawn((
        holo_mesh(meshes.add(Torus::new(0.06, 0.085)), mats.bright.clone(), Transform::from_xyz(0.36, -0.2, 0.0)),
        FigurePart(None),
        ChildOf(root),
    ));
    // Worn items: overlay shapes per slot, tinted with the item colour.
    let overlay = |slot: EquipSlot, index: u8, meshes: &mut Assets<Mesh>| -> Vec<(Handle<Mesh>, Vec3)> {
        match (slot, index) {
            (EquipSlot::Head, _) => vec![(meshes.add(Sphere::new(0.235).mesh().uv(16, 8)), Vec3::new(0.0, 0.7, 0.0))],
            (EquipSlot::Body, _) => vec![(meshes.add(Capsule3d::new(0.31, 0.5)), Vec3::new(0.0, 0.08, 0.0))],
            (EquipSlot::Hands, _) => vec![
                (meshes.add(Sphere::new(0.085).mesh().ico(1).unwrap()), Vec3::new(-0.36, -0.25, 0.0)),
                (meshes.add(Sphere::new(0.085).mesh().ico(1).unwrap()), Vec3::new(0.36, -0.25, 0.0)),
            ],
            (EquipSlot::Legs, _) => vec![
                (meshes.add(Cuboid::new(0.16, 0.2, 0.26)), Vec3::new(-0.13, -0.92, -0.03)),
                (meshes.add(Cuboid::new(0.16, 0.2, 0.26)), Vec3::new(0.13, -0.92, -0.03)),
            ],
            (EquipSlot::Accessory, i) => vec![(
                meshes.add(Sphere::new(0.05).mesh().ico(1).unwrap()),
                Vec3::new(if i == 0 { -0.08 } else { 0.08 }, 0.38, -0.27),
            )],
        }
    };
    if let Some(eq) = eq {
        for (slot, stack) in &eq.worn {
            let c = stack.item.color().to_srgba();
            let hl = highlight == Some(*slot);
            let mat = if hl {
                mats.highlight.clone()
            } else {
                materials.add(holo_material(Color::srgba(c.red, c.green, c.blue, 0.55), false))
            };
            for (m, p) in overlay(slot.slot, slot.index, meshes) {
                commands.spawn((holo_mesh(m, mat.clone(), Transform::from_translation(p)), FigurePart(Some(slot.slot)), ChildOf(root)));
            }
        }
    }
    // Empty inspected slot: show where it is with an outline marker.
    if let Some(h) = highlight
        && eq.is_none_or(|e| e.get(h).is_none())
    {
        for (m, p) in overlay(h.slot, h.index, meshes) {
            commands.spawn((holo_mesh(m, mats.line.clone(), Transform::from_translation(p)), FigurePart(Some(h.slot)), ChildOf(root)));
        }
    }
    // Base ring.
    commands.spawn((
        holo_mesh(meshes.add(Torus::new(0.5, 0.52)), mats.line.clone(), Transform::from_xyz(0.0, -1.0, 0.0)),
        ChildOf(root),
    ));
    root
}

/// Decorative rotating core (home screen).
pub fn spawn_core(commands: &mut Commands, parent: Entity, meshes: &mut Assets<Mesh>, mats: &HoloMats) {
    let root = commands
        .spawn((holo_group(Transform::from_xyz(0.0, 0.43, -0.12)), Turntable { view: "home", base_scale: 0.09, tilt: false }, ChildOf(parent)))
        .id();
    commands.spawn((holo_mesh(meshes.add(Sphere::new(0.35).mesh().ico(1).unwrap()), mats.glass.clone(), Transform::default()), ChildOf(root)));
    commands.spawn((holo_mesh(meshes.add(Sphere::new(0.12).mesh().ico(2).unwrap()), mats.bright.clone(), Transform::default()), ChildOf(root)));
    for (i, axis) in [Vec3::X, Vec3::Z, Vec3::new(1.0, 0.0, 1.0)].into_iter().enumerate() {
        let r = 0.6 + i as f32 * 0.12;
        commands.spawn((
            holo_mesh(
                meshes.add(Torus::new(r - 0.012, r)),
                if i == 1 { mats.violet.clone() } else { mats.line.clone() },
                Transform::from_rotation(Quat::from_axis_angle(axis.normalize(), 1.1 + i as f32 * 0.6)),
            ),
            ChildOf(root),
        ));
    }
}

pub struct ProjectionPlugin;

impl Plugin for ProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloViews>()
            .add_systems(Startup, init_mats)
            .add_observer(on_drag)
            .add_observer(on_wheel)
            .add_systems(Update, apply_turntables.run_if(super::device_shown));
    }
}
