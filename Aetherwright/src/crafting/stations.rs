//! Production stations and simple automation.
//!
//! * **Smelter** — processes one input at a time; 1 coal = 4 operations.
//! * **Grinder** — processes only while a body hinged to it spins faster
//!   than 2 rad/s relative to it (speed scales the rate). Drive it with an
//!   engine → this is where engineering and production meet.
//! * **Hopper crate** — a crate within 2 m of a station: items thrown into
//!   it are stored; the station pulls inputs/fuel from it and pushes its
//!   output back into it.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::recipes::{Station, machine_recipe};
use crate::{
    items::{ItemId, ItemStack, SavedStack, drops::ItemDrop, inventory::Inventory},
    machinery::components::{Part, PartKind},
    physics::joints::{Connection, JointKind},
    tools::scanning::Scannable,
};

#[derive(Component, Debug, Clone, Default)]
pub struct Machine {
    pub input: Option<ItemStack>,
    pub output: Option<ItemStack>,
    /// Remaining fuel operations (smelter).
    pub fuel_ops: u32,
    pub progress: f32,
    /// Grinder: current input shaft speed (rad/s).
    pub drive_speed: f32,
}

/// Storage of a crate (9 slots).
#[derive(Component, Debug, Clone)]
pub struct CrateStorage(pub Inventory);

impl Default for CrateStorage {
    fn default() -> Self {
        let mut inv = Inventory::default();
        inv.slots.truncate(9);
        Self(inv)
    }
}

/// Serializable machine/crate state (see save).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedStorage {
    pub slots: Vec<Option<SavedStack>>,
    pub fuel_ops: u32,
    pub progress: f32,
}

pub const GRINDER_MIN_SPEED: f32 = 2.0;
const HOPPER_RANGE: f32 = 2.0;

fn station_of(kind: PartKind) -> Option<Station> {
    match kind {
        PartKind::Smelter => Some(Station::Smelter),
        PartKind::Grinder => Some(Station::Grinder),
        _ => None,
    }
}

fn add_components(mut commands: Commands, added: Query<(Entity, &Part), Added<Part>>) {
    for (e, p) in &added {
        match p.kind {
            PartKind::Smelter | PartKind::Grinder => {
                commands.entity(e).insert(Machine::default());
            }
            PartKind::Crate => {
                commands.entity(e).insert(CrateStorage::default());
            }
            _ => {}
        }
    }
}

/// Dropped items that land in (on top of) a crate are stored inside.
fn crate_intake(
    mut commands: Commands,
    drops: Query<(Entity, &Position, &ItemDrop)>,
    mut crates: Query<(&Position, &mut CrateStorage)>,
) {
    for (de, dp, d) in &drops {
        for (cp, mut storage) in &mut crates {
            let rel = dp.0 - cp.0;
            if rel.y > 0.2 && rel.y < 1.1 && rel.x.abs() < 0.6 && rel.z.abs() < 0.6 {
                let left = storage.0.add(d.stack.item, d.stack.count);
                if left == 0 {
                    commands.entity(de).despawn();
                }
                break;
            }
        }
    }
}

/// Relative spin of bodies hinged to the grinder.
fn grinder_drive(
    joints: Query<&Connection>,
    bodies: Query<&AngularVelocity>,
    mut machines: Query<(Entity, &Part, &mut Machine, Option<&crate::kinetics::KineticDrive>)>,
) {
    for (e, part, mut m, kinetic) in &mut machines {
        if part.kind != PartKind::Grinder {
            continue;
        }
        // Rotation from the kinetic network (RPM → rad/s).
        let from_network = kinetic.map(|k| k.0 / 60.0 * std::f32::consts::TAU).unwrap_or(0.0);
        let mine = bodies.get(e).map(|a| a.0).unwrap_or_default();
        m.drive_speed = joints
            .iter()
            .filter(|c| c.kind == JointKind::Hinge && (c.a == e || c.b == e))
            .filter_map(|c| bodies.get(if c.a == e { c.b } else { c.a }).ok())
            .map(|w| (w.0 - mine).length())
            .fold(from_network, f32::max);
    }
}

fn take_from(storage: &mut Inventory, pred: impl Fn(ItemId) -> bool) -> Option<ItemId> {
    let slot = storage
        .slots
        .iter()
        .position(|s| s.is_some_and(|s| pred(s.item)))?;
    storage.take_one(slot)
}

#[allow(clippy::type_complexity)]
fn run_machines(
    time: Res<Time>,
    mut machines: Query<(&Part, &Position, &mut Machine)>,
    mut crates: Query<(&Position, &mut CrateStorage), Without<Machine>>,
) {
    let dt = time.delta_secs();
    let coal = ItemId::k("coal");
    for (part, pos, mut m) in &mut machines {
        let Some(station) = station_of(part.kind) else {
            continue;
        };
        // Automation: hoppers nearby.
        let mut hopper = crates
            .iter_mut()
            .find(|(cp, _)| cp.0.distance(pos.0) < HOPPER_RANGE)
            .map(|(_, s)| s);
        if m.input.is_none()
            && let Some(h) = hopper.as_mut()
            && let Some(item) = take_from(&mut h.0, |i| machine_recipe(station, i).is_some())
        {
            m.input = Some(ItemStack::new(item, 1));
        }
        if station == Station::Smelter
            && m.fuel_ops == 0
            && let Some(h) = hopper.as_mut()
            && take_from(&mut h.0, |i| i == coal).is_some()
        {
            m.fuel_ops += 4;
        }
        if let (Some(out), Some(h)) = (m.output, hopper.as_mut()) {
            let left = h.0.add(out.item, out.count);
            m.output = (left > 0).then(|| ItemStack::new(out.item, left));
        }

        // Processing.
        let Some(input) = m.input else {
            m.progress = 0.0;
            continue;
        };
        let Some(recipe) = machine_recipe(station, input.item) else {
            m.input = None;
            continue;
        };
        let rate = match station {
            Station::Smelter if m.fuel_ops > 0 => 1.0,
            Station::Grinder if m.drive_speed >= GRINDER_MIN_SPEED => {
                (m.drive_speed / 6.0).clamp(0.3, 2.0)
            }
            _ => 0.0,
        };
        if rate == 0.0
            || m.output
                .is_some_and(|o| o.item != recipe.output_item() || o.count + recipe.output.1 > 64)
        {
            continue;
        }
        m.progress += dt * rate;
        if m.progress >= recipe.time {
            m.progress = 0.0;
            // One input item per operation; the rest of the stack waits.
            m.input = (input.count > 1).then(|| ItemStack::new(input.item, input.count - 1));
            if station == Station::Smelter {
                m.fuel_ops -= 1;
            }
            let out = recipe.output_item();
            let n = m.output.map(|o| o.count).unwrap_or(0) + recipe.output.1;
            m.output = Some(ItemStack::new(out, n));
        }
    }
}

fn describe(
    mut q: Query<(&Part, &Machine, &mut Scannable)>,
    crates: Query<(&CrateStorage, &mut Scannable), Without<Machine>>,
) {
    for (part, m, mut s) in &mut q {
        let item = |st: Option<ItemStack>| {
            st.map(|s| format!("{} ×{}", s.item.def().name, s.count))
                .unwrap_or("—".into())
        };
        let mut info = format!(
            "Вход: {}\nВыход: {}\nПрогресс: {:.0}%",
            item(m.input),
            item(m.output),
            m.progress * 25.0
        );
        if part.kind == PartKind::Smelter {
            info += &format!("\nТопливо: {} операций (уголь)", m.fuel_ops);
        } else {
            info += &format!(
                "\nВал: {:.1} рад/с (нужно ≥ {GRINDER_MIN_SPEED})",
                m.drive_speed
            );
        }
        info += "\nЯщик рядом (< 2 м) работает как бункер.";
        s.info = info;
    }
    let mut crates = crates;
    for (c, mut s) in &mut crates {
        let items: Vec<String> =
            c.0.slots
                .iter()
                .flatten()
                .map(|s| format!("{} ×{}", s.item.def().name, s.count))
                .collect();
        s.info = if items.is_empty() {
            "Пусто. Бросьте предметы сверху (G).".into()
        } else {
            items.join("\n")
        };
    }
}

pub struct StationsPlugin;

impl Plugin for StationsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_components, crate_intake, describe))
            .add_systems(
                FixedUpdate,
                (grinder_drive, run_machines)
                    .chain()
                    .after(crate::machinery::MachinerySet::Drive),
            );
    }
}
