//! The player model: Minecraft proportions (8 × 32 "pixels"), no skin —
//! plain light grey boxes. Animated in code: walking / running swings legs
//! and arms, crouching leans the torso, using swings the right arm, the
//! head follows the look pitch. Hidden in first person.

use bevy::prelude::*;

use super::{Body, Player, Seated, camera::{CameraView, View}};
use crate::style::{BoxCache, Materials};

/// One model pixel (m): 32 px = 1.8 m.
const PX: f32 = 1.8 / 32.0;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Limb {
    Root,
    Torso,
    Head,
    ArmR,
    ArmL,
    LegR,
    LegL,
}

/// Worn equipment on the head.
#[derive(Component)]
struct HeadGear;

fn show_gear(players: Query<&crate::inventory::Inventory, With<Player>>, mut gear: Query<&mut Visibility, With<HeadGear>>) {
    let Ok(inv) = players.single() else { return };
    let on = inv.worn(0).is_some();
    for mut v in &mut gear {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
}

#[derive(Component, Default)]
struct Walk {
    phase: f32,
    amount: f32,
}

fn attach(mut commands: Commands, players: Query<Entity, Added<Player>>, mut meshes: ResMut<Assets<Mesh>>, mut cache: ResMut<BoxCache>, mats: Res<Materials>) {
    for p in &players {
        let m = mats.get("player");
        let mut bx = |w: f32, h: f32, d: f32| cache.get(&mut meshes, Vec3::new(w, h, d) * PX);
        let (head, body, limb) = (bx(8.0, 8.0, 8.0), bx(8.0, 12.0, 4.0), bx(4.0, 12.0, 4.0));
        let root = commands.spawn((Limb::Root, Walk::default(), Transform::default(), Visibility::default(), ChildOf(p))).id();
        // Legs hang from the hips.
        for (limb_kind, x) in [(Limb::LegR, 2.0), (Limb::LegL, -2.0)] {
            let pivot = commands.spawn((limb_kind, Transform::from_xyz(x * PX, 12.0 * PX, 0.0), Visibility::default(), ChildOf(root))).id();
            commands.spawn((Mesh3d(limb.clone()), MeshMaterial3d(m.clone()), Transform::from_xyz(0.0, -6.0 * PX, 0.0), ChildOf(pivot)));
        }
        // Torso pivots at the hips (crouch lean).
        let torso = commands.spawn((Limb::Torso, Transform::from_xyz(0.0, 12.0 * PX, 0.0), Visibility::default(), ChildOf(root))).id();
        commands.spawn((Mesh3d(body), MeshMaterial3d(m.clone()), Transform::from_xyz(0.0, 6.0 * PX, 0.0), ChildOf(torso)));
        let neck = commands.spawn((Limb::Head, Transform::from_xyz(0.0, 12.0 * PX, 0.0), Visibility::default(), ChildOf(torso))).id();
        commands.spawn((Mesh3d(head), MeshMaterial3d(m.clone()), Transform::from_xyz(0.0, 4.0 * PX, 0.0), ChildOf(neck)));
        // Engineer's goggles on the forehead (shown while worn).
        let gear = commands.spawn((HeadGear, Transform::from_xyz(0.0, 5.5 * PX, -4.0 * PX - 0.02), Visibility::Hidden, ChildOf(neck))).id();
        crate::style::spawn_parts(&mut commands, gear, &crate::models::goggles_parts(), &mut meshes, &mut cache, &mats, true);
        for (limb_kind, x) in [(Limb::ArmR, 6.0), (Limb::ArmL, -6.0)] {
            let pivot = commands.spawn((limb_kind, Transform::from_xyz(x * PX, 10.0 * PX, 0.0), Visibility::default(), ChildOf(torso))).id();
            commands.spawn((Mesh3d(limb.clone()), MeshMaterial3d(m.clone()), Transform::from_xyz(0.0, -4.0 * PX, 0.0), ChildOf(pivot)));
        }
    }
}

#[allow(clippy::type_complexity)]
fn animate(
    time: Res<Time>,
    view: Res<CameraView>,
    players: Query<(&Body, &Children, Has<Seated>), With<Player>>,
    mut roots: Query<(&mut Walk, &mut Transform, &mut Visibility, &Children), With<Limb>>,
    mut limbs: Query<(&Limb, &mut Transform), Without<Walk>>,
    kids: Query<&Children>,
) {
    let dt = time.delta_secs();
    for (body, children, seated) in &players {
        for c in children.iter() {
            let Ok((mut walk, mut root_t, mut vis, _)) = roots.get_mut(c) else { continue };
            *vis = if view.0 == View::First { Visibility::Hidden } else { Visibility::Inherited };
            // Model stands on the capsule's feet and faces the look yaw.
            // Seated: hips on the cushion.
            root_t.translation.y = -body.height() * 0.5 - if seated { 0.3 } else { 0.0 };
            root_t.rotation = Quat::from_rotation_y(body.yaw);
            let target = if body.flying { 0.0 } else { (body.speed / super::movement::WALK).min(1.6) };
            walk.amount += (target - walk.amount) * (dt * 10.0).min(1.0);
            walk.phase += body.speed * dt * 2.2;
            let (p, a) = (walk.phase, walk.amount);
            let crouch = body.crouching as i32 as f32;
            let sit = if seated { 1.0 } else { 0.0 };
            let (p, a) = (p, a * (1.0 - sit));
            for l in kids.iter_descendants(c) {
                let Ok((kind, mut t)) = limbs.get_mut(l) else { continue };
                match kind {
                    Limb::LegR => t.rotation = Quat::from_rotation_x(p.cos() * 0.9 * a + sit * 1.45),
                    Limb::LegL => t.rotation = Quat::from_rotation_x(-p.cos() * 0.9 * a + sit * 1.45),
                    Limb::ArmL => t.rotation = Quat::from_rotation_x(p.cos() * 0.8 * a + crouch * 0.4 + sit * 0.6),
                    Limb::ArmR => {
                        let swing = (body.swing * std::f32::consts::PI).sin();
                        t.rotation = Quat::from_rotation_x(-p.cos() * 0.8 * a + crouch * 0.4 + sit * 0.6 + swing * 1.3) * Quat::from_rotation_z(swing * 0.3);
                    }
                    // Crouch: lean the torso forward from the hips.
                    Limb::Torso => t.rotation = Quat::from_rotation_x(-0.5 * crouch),
                    // The head keeps looking where the player looks.
                    Limb::Head => t.rotation = Quat::from_rotation_x(body.pitch + 0.5 * crouch),
                    Limb::Root => {}
                }
            }
        }
    }
}

pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach, animate, show_gear));
    }
}
