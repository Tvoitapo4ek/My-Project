//! Player interaction with machines (manipulator "Mechanics" mode) and the
//! live scanner descriptions of machine parts.

use bevy::prelude::*;

use super::{
    components::{AetherDynamo, Engine, Part, PartKind, PowerCell, describe},
    power::PowerStats,
    transmission::GearMeshes,
};
use crate::{
    core::events::Notify,
    physics::joints::{Connection, JointKind},
    tools::{manipulator::MechanicsUse, scanning::Scannable},
};

/// A station was activated (opens its UI; handled by `crafting`).
#[derive(Message, Debug, Clone, Copy)]
pub struct OpenStation {
    pub entity: Entity,
    pub kind: PartKind,
}

const THROTTLES: [f32; 3] = [0.25, 0.5, 1.0];

fn use_mechanics(
    mut reader: MessageReader<MechanicsUse>,
    parts: Query<&Part>,
    mut engines: Query<&mut Engine>,
    cells: Query<&PowerCell>,
    stats: Res<PowerStats>,
    mut open: MessageWriter<OpenStation>,
    mut notify: MessageWriter<Notify>,
) {
    for u in reader.read() {
        let Ok(part) = parts.get(u.entity) else {
            notify.write(Notify::warn("Это не механизм"));
            continue;
        };
        match part.kind {
            PartKind::Engine => {
                let Ok(mut e) = engines.get_mut(u.entity) else {
                    continue;
                };
                if u.secondary {
                    // Cycle throttle; after full throttle flip direction.
                    let i = THROTTLES
                        .iter()
                        .position(|t| (*t - e.throttle).abs() < 1e-3)
                        .unwrap_or(2);
                    if i == THROTTLES.len() - 1 {
                        e.reverse = !e.reverse;
                        e.throttle = THROTTLES[0];
                    } else {
                        e.throttle = THROTTLES[i + 1];
                    }
                    notify.write(Notify::info(format!(
                        "Двигатель: тяга {:.0}%{}",
                        e.throttle * 100.0,
                        if e.reverse { ", реверс" } else { "" }
                    )));
                } else {
                    e.on = !e.on;
                    let (energy, _) = stats.by_entity.get(&u.entity).copied().unwrap_or_default();
                    if e.on && energy <= 0.0 {
                        notify.write(Notify::warn(
                            "Двигатель включён, но нет энергии: соедините его с энергоячейкой",
                        ));
                    } else {
                        notify.write(Notify::info(if e.on {
                            "Двигатель включён"
                        } else {
                            "Двигатель выключен"
                        }));
                    }
                }
            }
            PartKind::PowerCell => {
                if let Ok(c) = cells.get(u.entity) {
                    notify.write(Notify::info(format!(
                        "Энергоячейка: {:.0}/{:.0}",
                        c.energy, c.capacity
                    )));
                }
            }
            PartKind::Crate => {}
            k if k.is_station() => {
                open.write(OpenStation {
                    entity: u.entity,
                    kind: k,
                });
            }
            _ => {
                notify.write(Notify::info("У этой детали нет настроек"));
            }
        }
    }
}

/// Keeps scanner texts of machine parts current.
#[allow(clippy::too_many_arguments)]
fn update_descriptions(
    time: Res<Time>,
    mut timer: Local<f32>,
    stats: Res<PowerStats>,
    gears: Res<GearMeshes>,
    joints: Query<&Connection>,
    mut q: Query<(
        Entity,
        &Part,
        &mut Scannable,
        Option<&Engine>,
        Option<&PowerCell>,
        Option<&AetherDynamo>,
    )>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 0.4;
    for (e, part, mut s, engine, cell, dynamo) in &mut q {
        let base = describe(part.kind);
        let mut info = base.info.clone();
        let links = joints.iter().filter(|c| c.a == e || c.b == e).count();
        let hinges = joints
            .iter()
            .filter(|c| (c.a == e || c.b == e) && c.kind == JointKind::Hinge)
            .count();
        if let Some(en) = engine {
            info += &format!(
                "\nСостояние: {}{}  ·  тяга {:.0}%{}\nПриводных шарниров: {hinges}",
                if en.on { "вкл" } else { "выкл" },
                if en.on && !en.powered {
                    " (нет энергии!)"
                } else {
                    ""
                },
                en.throttle * 100.0,
                if en.reverse { " (реверс)" } else { "" }
            );
        }
        if let Some(c) = cell {
            info += &format!("\nЗаряд: {:.0} / {:.0}", c.energy, c.capacity);
        }
        if let Some(d) = dynamo {
            info += &format!("\nВыработка: {:.1} ед/с", d.output);
        }
        if let Some((energy, cap)) = stats.by_entity.get(&e)
            && *cap > 0.0
        {
            info += &format!("\nЭнергосеть: {energy:.0} / {cap:.0}");
        }
        if part.kind == PartKind::Gear {
            let meshing = gears.0.iter().filter(|(a, b)| *a == e || *b == e).count();
            info += &format!("\nЗацеплений: {meshing}");
        }
        info += &format!("\nСоединений: {links}");
        s.title = base.title;
        s.info = info;
    }
}

pub struct ConnectionsPlugin;

impl Plugin for ConnectionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenStation>()
            .add_systems(Update, (use_mechanics, update_descriptions));
    }
}
