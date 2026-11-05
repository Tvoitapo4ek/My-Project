//! Dropped items: small physical bodies in the world. Mined blocks drop
//! their item; players can throw items; auto-pickup collects ordinary
//! resources (toggle with P), anything can be picked up with E.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{ItemId, ItemStack, block_drop, inventory::Inventory};
use crate::{
    core::{
        config::GameConfig,
        events::Notify,
        layers::Layer,
        rules::ModeRules,
        states::{AppState, gameplay_active},
    },
    world::{chunks::EditCause, interaction::BlockChanged},
};

#[derive(Component, Debug, Clone)]
pub struct ItemDrop {
    pub stack: ItemStack,
    /// Seconds since spawn (pickup delay, despawn).
    pub age: f32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnDrop {
    pub stack: ItemStack,
    pub position: Vec3,
    pub velocity: Vec3,
}

#[derive(Resource, Debug)]
pub struct PickupSettings {
    pub auto: bool,
}

/// Cached render assets for drops (one material per item).
#[derive(Resource, Default)]
struct DropAssets {
    mesh: Handle<Mesh>,
    materials: HashMap<ItemId, Handle<StandardMaterial>>,
}

#[derive(Component)]
pub struct Picker;

const DESPAWN_AFTER: f32 = 600.0;
const PICKUP_DELAY: f32 = 0.6;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, cfg: Res<GameConfig>) {
    commands.insert_resource(DropAssets {
        mesh: meshes.add(Cuboid::new(0.28, 0.28, 0.28)),
        ..default()
    });
    commands.insert_resource(PickupSettings {
        auto: cfg.player.auto_pickup,
    });
}

fn drops_from_blocks(
    rules: Res<ModeRules>,
    mut changed: MessageReader<BlockChanged>,
    mut spawn: MessageWriter<SpawnDrop>,
) {
    for c in changed.read() {
        if c.cause != EditCause::Mined || !rules.drops {
            continue;
        }
        if let Some(item) = block_drop(c.old) {
            spawn.write(SpawnDrop {
                stack: ItemStack::new(item, 1),
                position: c.pos.as_vec3() + Vec3::splat(0.5),
                velocity: Vec3::new(0.0, 2.5, 0.0),
            });
        }
    }
}

fn spawn_drops(
    mut commands: Commands,
    mut reader: MessageReader<SpawnDrop>,
    mut assets: ResMut<DropAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for d in reader.read() {
        let mat = assets
            .materials
            .entry(d.stack.item)
            .or_insert_with(|| {
                let def = d.stack.item.def();
                let glow = matches!(def.category, super::Category::Magic);
                materials.add(StandardMaterial {
                    base_color: d.stack.item.color(),
                    emissive: if glow {
                        d.stack.item.color().to_linear() * 2.0
                    } else {
                        LinearRgba::BLACK
                    },
                    perceptual_roughness: 0.6,
                    ..default()
                })
            })
            .clone();
        commands.spawn((
            Name::new(format!("Drop {}", d.stack.item.def().key)),
            ItemDrop {
                stack: d.stack,
                age: 0.0,
            },
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(d.position),
            RigidBody::Dynamic,
            Collider::cuboid(0.28, 0.28, 0.28),
            LinearVelocity(d.velocity),
            AngularVelocity(Vec3::new(1.0, 2.0, 0.5)),
            (
                Mass(d.stack.item.def().mass.max(0.2)),
                Friction::new(0.8),
                LinearDamping(0.3),
                GravityScale(1.0),
            ),
            CollisionLayers::new(Layer::Item, [Layer::Terrain, Layer::Prop, Layer::Item]),
        ));
    }
}

/// Auto-pickup (magnet + collect) of ordinary resources, manual pickup with E.
#[allow(clippy::too_many_arguments)]
fn pickup(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<PickupSettings>,
    focus_ok: Option<Res<State<crate::core::states::InputFocus>>>,
    mut pickers: Query<(&Transform, &mut Inventory), With<Picker>>,
    mut drops: Query<(
        Entity,
        &Transform,
        &mut ItemDrop,
        &mut LinearVelocity,
        &mut GravityScale,
    )>,
    mut notify: MessageWriter<Notify>,
) {
    let dt = time.delta_secs();
    let manual = keys.just_pressed(KeyCode::KeyE)
        && focus_ok.is_some_and(|f| *f.get() == crate::core::states::InputFocus::Gameplay);
    let Ok((ptf, mut inv)) = pickers.single_mut() else {
        return;
    };
    let center = ptf.translation;
    let mut picked_any = false;
    for (e, tf, mut drop, mut vel, mut gravity) in &mut drops {
        drop.age += dt;
        if drop.age > DESPAWN_AFTER {
            commands.entity(e).despawn();
            continue;
        }
        if drop.age < PICKUP_DELAY {
            continue;
        }
        let to_player = center - tf.translation;
        let dist = to_player.length();
        let auto = settings.auto && drop.stack.item.def().category.auto_pickup();
        // Magnet: weightless flight towards the player (slides along walls).
        let magnet = auto && dist < 4.0;
        gravity.0 = if magnet { 0.0 } else { 1.0 };
        if magnet {
            vel.0 = to_player.normalize() * 7.0;
        }
        if (auto && dist < 1.6) || (manual && dist < 3.0) {
            let left = inv.add(drop.stack.item, drop.stack.count);
            if left == 0 {
                commands.entity(e).despawn();
                picked_any = true;
            } else if left < drop.stack.count {
                drop.stack.count = left;
                picked_any = true;
            } else if manual {
                notify.write(Notify::warn("Инвентарь полон"));
            }
        }
    }
    let _ = picked_any;
}

fn toggle_auto(
    keys: Res<ButtonInput<KeyCode>>,
    mut s: ResMut<PickupSettings>,
    mut notify: MessageWriter<Notify>,
) {
    if keys.just_pressed(KeyCode::KeyP) {
        s.auto = !s.auto;
        notify.write(Notify::info(if s.auto {
            "Автоподбор: вкл"
        } else {
            "Автоподбор: выкл"
        }));
    }
}

/// G throws one item of the selected stack.
fn throw_item(
    keys: Res<ButtonInput<KeyCode>>,
    eye: Res<crate::core::resources::EyeRay>,
    mut pickers: Query<&mut Inventory, With<Picker>>,
    mut spawn: MessageWriter<SpawnDrop>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Ok(mut inv) = pickers.single_mut() else {
        return;
    };
    let slot = inv.selected;
    if let Some(item) = inv.take_one(slot) {
        spawn.write(SpawnDrop {
            stack: ItemStack::new(item, 1),
            position: eye.origin + eye.direction * 0.8,
            velocity: eye.direction * 7.0 + Vec3::Y * 1.5,
        });
    }
}

pub struct DropsPlugin;

impl Plugin for DropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnDrop>()
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                (drops_from_blocks, spawn_drops)
                    .chain()
                    .after(crate::world::interaction::apply_block_edits)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, pickup.run_if(in_state(AppState::InGame)))
            .add_systems(Update, (toggle_auto, throw_item).run_if(gameplay_active));
    }
}
