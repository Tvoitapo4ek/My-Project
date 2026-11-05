//! Health and damage shared by the player and creatures.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    core::{events::Notify, resources::SpawnPoint, rules::ModeRules},
    player::{Player, movement::CharacterState},
};

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub hp: f32,
    pub max: f32,
    /// Seconds of invulnerability left (after a hit).
    #[serde(skip)]
    pub invulnerable: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self {
            hp: max,
            max,
            invulnerable: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageSource {
    Fall,
    Creature,
    Weapon,
    Magic,
    Starvation,
    Cold,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct Damage {
    pub target: Entity,
    pub amount: f32,
    pub source: DamageSource,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct Died {
    pub entity: Entity,
}

fn apply_damage(
    mut reader: MessageReader<Damage>,
    rules: Res<ModeRules>,
    mut q: Query<(&mut Health, Has<Player>)>,
    mut died: MessageWriter<Died>,
) {
    for d in reader.read() {
        let Ok((mut h, is_player)) = q.get_mut(d.target) else {
            continue;
        };
        if is_player && !rules.player_damage {
            continue;
        }
        if h.hp <= 0.0
            || (h.invulnerable > 0.0
                && d.source != DamageSource::Starvation
                && d.source != DamageSource::Cold)
        {
            continue;
        }
        h.hp = (h.hp - d.amount).max(0.0);
        if matches!(d.source, DamageSource::Creature | DamageSource::Weapon) {
            h.invulnerable = 0.4;
        }
        if h.hp <= 0.0 {
            died.write(Died { entity: d.target });
        }
    }
}

fn tick_invulnerability(time: Res<Time>, mut q: Query<&mut Health>) {
    for mut h in &mut q {
        h.invulnerable = (h.invulnerable - time.delta_secs()).max(0.0);
    }
}

/// Landing faster than ~13 m/s hurts (≈ a 4.5-block fall with g = 19).
pub fn fall_damage(speed: f32) -> f32 {
    ((speed - 13.0) * 6.0).max(0.0)
}

fn fall(mut q: Query<(Entity, &mut CharacterState)>, mut out: MessageWriter<Damage>) {
    for (e, mut s) in &mut q {
        if let Some(v) = s.landed_with_speed.take() {
            let dmg = fall_damage(v);
            if dmg > 0.0 {
                out.write(Damage {
                    target: e,
                    amount: dmg,
                    source: DamageSource::Fall,
                });
            }
        }
    }
}

/// Player death: respawn at the spawn point with half the needs restored.
fn player_death(
    mut died: MessageReader<Died>,
    spawn: Res<SpawnPoint>,
    mut q: Query<
        (
            &mut Health,
            &mut avian3d::prelude::Position,
            &mut avian3d::prelude::LinearVelocity,
            Option<&mut super::needs::Vitals>,
        ),
        With<Player>,
    >,
    mut notify: MessageWriter<Notify>,
) {
    for d in died.read() {
        if let Ok((mut h, mut p, mut v, vitals)) = q.get_mut(d.entity) {
            h.hp = h.max;
            p.0 = spawn.0;
            v.0 = Vec3::ZERO;
            if let Some(mut vit) = vitals {
                vit.hunger = vit.hunger.max(50.0);
                vit.thirst = vit.thirst.max(50.0);
                vit.body_temp = 37.0;
            }
            notify.write(Notify::warn("Вы погибли и возродились у точки появления"));
        }
    }
}

pub struct DamagePlugin;

impl Plugin for DamagePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Damage>()
            .add_message::<Died>()
            .add_systems(
                Update,
                (fall, apply_damage, player_death, tick_invulnerability).chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fall_damage_threshold() {
        assert_eq!(super::fall_damage(10.0), 0.0);
        assert!(super::fall_damage(20.0) > 30.0);
    }
}
