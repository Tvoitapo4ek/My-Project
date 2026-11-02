//! Hunger, thirst and a simple body temperature.
//!
//! Tuned so needs nudge the player without interrupting exploration: a full
//! stomach lasts ~40 minutes, water ~30; empty needs hurt slowly. Eating
//! uses held food (RMB); E while looking at water drinks.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::damage::{Damage, DamageSource, Health};
use crate::{
    core::{config::GameConfig, events::Notify, resources::{EyeRay, WorldClock}, rules::ModeRules, states::gameplay_active},
    items::{inventory::Inventory, Use},
    machinery::components::{Part, PartKind},
    player::{input::PlayerIntent, movement::CharacterState, Player},
    tools::ActiveTool,
    world::{chunks::VoxelWorld, interaction::voxel_raycast},
};

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Vitals {
    /// 0..100
    pub hunger: f32,
    /// 0..100
    pub thirst: f32,
    /// °C
    pub body_temp: f32,
}

impl Default for Vitals {
    fn default() -> Self {
        Self { hunger: 90.0, thirst: 90.0, body_temp: 37.0 }
    }
}

pub const HUNGER_PER_SEC: f32 = 100.0 / (40.0 * 60.0);
pub const THIRST_PER_SEC: f32 = 100.0 / (30.0 * 60.0);

/// Ambient temperature (°C) at a point: altitude, night, water, heat sources.
pub fn ambient_temperature(height_above_sea: f32, daylight: f32, in_water: bool, near_heat: bool) -> f32 {
    let mut t = 12.0 + daylight * 10.0 - (height_above_sea.max(0.0) * 0.25);
    if in_water {
        t -= 8.0;
    }
    if near_heat {
        t += 15.0;
    }
    t
}

#[allow(clippy::too_many_arguments)]
fn tick_needs(
    time: Res<Time>,
    rules: Res<ModeRules>,
    clock: Res<WorldClock>,
    world: Res<VoxelWorld>,
    intent: Res<PlayerIntent>,
    heat: Query<(&Transform, &Part)>,
    mut q: Query<(Entity, &Transform, &CharacterState, &mut Vitals, &mut Health), With<Player>>,
    mut damage: MessageWriter<Damage>,
) {
    if !rules.needs {
        return;
    }
    let dt = time.delta_secs();
    for (e, t, state, mut v, mut h) in &mut q {
        let effort = if intent.sprint && intent.movement != Vec2::ZERO { 1.6 } else { 1.0 };
        v.hunger = (v.hunger - HUNGER_PER_SEC * effort * dt).max(0.0);
        v.thirst = (v.thirst - THIRST_PER_SEC * effort * dt).max(0.0);

        let near_heat = heat.iter().any(|(ht, p)| p.kind == PartKind::Smelter && ht.translation.distance(t.translation) < 5.0);
        let ambient = ambient_temperature(t.translation.y - world.sea_level as f32, clock.daylight(), state.submerged > 0.3, near_heat);
        // Body drifts towards 37 °C when the ambient is comfortable, and
        // slowly towards cold/heat outside 8..30 °C.
        let target = if ambient < 8.0 { 37.0 - (8.0 - ambient) * 0.5 } else if ambient > 30.0 { 37.0 + (ambient - 30.0) * 0.2 } else { 37.0 };
        v.body_temp += (target - v.body_temp) * (dt / 60.0).min(1.0);

        if v.hunger <= 0.0 {
            damage.write(Damage { target: e, amount: 0.4 * dt, source: DamageSource::Starvation });
        }
        if v.thirst <= 0.0 {
            damage.write(Damage { target: e, amount: 0.5 * dt, source: DamageSource::Starvation });
        }
        if v.body_temp < 35.0 {
            damage.write(Damage { target: e, amount: 0.3 * dt, source: DamageSource::Cold });
        }
        // Regeneration when well fed.
        if v.hunger > 50.0 && v.thirst > 50.0 && h.hp < h.max {
            h.hp = (h.hp + 0.6 * dt).min(h.max);
        }
    }
}

/// RMB with food: eat one.
fn eat(
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    mut q: Query<(&mut Inventory, &mut Vitals, &mut Health), With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    let ActiveTool::Consume(item) = *tool else { return };
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Use::Eat(food, water) = item.def().use_ else { return };
    let Ok((mut inv, mut v, mut h)) = q.single_mut() else { return };
    if v.hunger > 98.0 && water <= 0.0 {
        notify.write(Notify::info("Вы не голодны"));
        return;
    }
    if inv.remove(item, 1) {
        v.hunger = (v.hunger + food).min(100.0);
        v.thirst = (v.thirst + water).min(100.0);
        h.hp = (h.hp + food * 0.2).min(h.max);
        notify.write(Notify::ok(format!("Съедено: {}", item.def().name)));
    }
}

/// E while looking at water: drink.
fn drink(
    keys: Res<ButtonInput<KeyCode>>,
    eye: Res<EyeRay>,
    cfg: Res<GameConfig>,
    world: Res<VoxelWorld>,
    mut q: Query<&mut Vitals, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let hit = voxel_raycast(&world, eye.origin, eye.direction, cfg.player.reach, |b| b.is_liquid() || b.is_solid());
    if hit.is_some_and(|h| h.block.is_liquid())
        && let Ok(mut v) = q.single_mut()
    {
        v.thirst = (v.thirst + 30.0).min(100.0);
        notify.write(Notify::ok("Вы попили воды"));
    }
}

pub struct NeedsPlugin;

impl Plugin for NeedsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tick_needs.run_if(crate::core::states::in_game))
            .add_systems(Update, (eat, drink).run_if(gameplay_active));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_are_slow() {
        // 10 minutes of exploration costs a quarter of the stomach at most.
        assert!(HUNGER_PER_SEC * 600.0 <= 25.0);
        assert!(THIRST_PER_SEC * 600.0 <= 35.0);
    }

    #[test]
    fn ambient_temperature_reacts() {
        let day = ambient_temperature(5.0, 1.0, false, false);
        let night = ambient_temperature(5.0, 0.0, false, false);
        let peak = ambient_temperature(40.0, 0.0, false, false);
        let fire = ambient_temperature(40.0, 0.0, false, true);
        assert!(day > night && night > peak && fire > peak);
    }
}
