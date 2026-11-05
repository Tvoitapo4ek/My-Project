//! Damage, diagnostics and repair (MASTER PROMPT §15).
//!
//! Machines have an `Integrity` (0..1). It drops for concrete reasons and
//! changes what the part does:
//! * **overload** — a kinetic part carrying more power than its strength
//!   wears; worn parts rub (more friction), a broken one stops transmitting
//!   and the network splits (cascade: the machine downstream stops);
//! * **distorted magic** inside a magical device corrodes it;
//! * a **stabilizer without power** holding unstable magic degrades.
//! Worn motors / engines give less power, a worn stabilizer calms less, a
//! broken magical device neither sends nor receives.
//! Broken parts stay in place: they can be diagnosed (engineering panel)
//! and repaired with the wrench (materials by kind) — or replaced.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    arcana::MagicNode,
    content::MagicRole,
    core::{events::Notify, rules::ModeRules, states::gameplay_active},
    electric::ElectricState,
    items::{ItemId, inventory::Inventory},
    kinetics::{KineticBlock, KineticGrid},
    machinery::components::Part,
    player::Player,
    save::world_save::{LoadedEntityIds, SaveEntityIds, SaveHook, SaveHookExt},
    tools::{ActiveTool, EntityTarget},
};

/// Integrity lost per second per 100 % of overload.
pub const OVERLOAD_WEAR: f32 = 0.5;
/// Integrity restored per second with the wrench.
pub const REPAIR_RATE: f32 = 0.25;
/// One material item per this much integrity restored.
pub const REPAIR_PER_ITEM: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WearCause {
    Overload,
    Distortion,
    Unpowered,
}

impl WearCause {
    pub fn label(self) -> &'static str {
        match self {
            WearCause::Overload => "перегрузка сверх прочности",
            WearCause::Distortion => "искажённая магия внутри",
            WearCause::Unpowered => "стабилизатор без питания держит нестабильную магию",
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Integrity {
    pub value: f32,
    pub cause: Option<WearCause>,
    /// Integrity already paid for by consumed materials (prepaid credit).
    pub repaired: f32,
}

impl Default for Integrity {
    fn default() -> Self {
        Self { value: 1.0, cause: None, repaired: 0.0 }
    }
}

impl Integrity {
    pub fn broken(&self) -> bool {
        self.value <= 0.0
    }
}

/// Wear of a magical device per second (pure, tested).
pub fn magic_wear(role: MagicRole, amount: f32, distortion: f32, power: f32) -> (f32, Option<WearCause>) {
    if amount < 0.5 {
        return (0.0, None);
    }
    if role == MagicRole::Stabilizer && power < 0.5 && distortion > 0.3 {
        return (0.03, Some(WearCause::Unpowered));
    }
    if distortion > 0.6 {
        return ((distortion - 0.6) * 0.12, Some(WearCause::Distortion));
    }
    (0.0, None)
}

/// Material that repairs a part.
pub fn repair_material(world: &World, e: Entity) -> &'static str {
    if world.get::<MagicNode>(e).is_some() {
        return "aether_shard";
    }
    if let Some(k) = world.get::<KineticBlock>(e)
        && (crate::content::registry().logic(&k.key).is_some() || k.def().is_some_and(|d| d.energy_per_su > 0.0))
    {
        return "copper";
    }
    "metal"
}

fn attach(mut commands: Commands, added: Query<Entity, (Or<(Added<KineticBlock>, Added<Part>)>, Without<Integrity>)>) {
    for e in &added {
        commands.entity(e).insert(Integrity::default());
    }
}

fn name_of(world_name: Option<&Name>, k: Option<&KineticBlock>) -> String {
    if let Some(def) = k.and_then(|k| k.def()) {
        return ItemId::from_key(&def.item).map(|i| i.def().name.clone()).unwrap_or_default();
    }
    world_name.map(|n| n.as_str().trim_start_matches("Part ").to_string()).unwrap_or_else(|| "деталь".into())
}

/// Applies wear, announces thresholds, re-solves networks on breaks.
#[allow(clippy::type_complexity)]
fn apply_wear(
    time: Res<Time>,
    mut grid: ResMut<KineticGrid>,
    mut parts: Query<(Entity, &mut Integrity, Option<&KineticBlock>, Option<&MagicNode>, Option<&ElectricState>, Option<&Name>)>,
    mut notify: MessageWriter<Notify>,
) {
    let dt = time.delta_secs();
    let overloaded: Vec<(IVec3, f32)> = grid.solution.overloaded.clone();
    let mut resolve = false;
    for (_, mut integ, k, m, el, name) in &mut parts {
        if integ.broken() {
            continue;
        }
        let mut rate = 0.0;
        let mut cause = None;
        if let Some(k) = k
            && let Some((_, ratio)) = overloaded.iter().find(|(c, _)| *c == k.cell)
        {
            rate += (ratio - 1.0) * OVERLOAD_WEAR;
            cause = Some(WearCause::Overload);
        }
        if let Some(m) = m
            && let Some(def) = m.def()
        {
            let (r, c) = magic_wear(def.role, m.amount, m.quality.distortion, el.map(|s| s.satisfaction).unwrap_or(0.0));
            if r > 0.0 {
                rate += r;
                cause = cause.or(c);
            }
        }
        if rate <= 0.0 {
            continue;
        }
        let before = integ.value;
        integ.value = (integ.value - rate * dt).max(0.0);
        integ.cause = cause;
        let n = name_of(name, k);
        if before > 0.5 && integ.value <= 0.5 {
            notify.write(Notify::warn(format!("Износ 50 %: {n} ({})", cause.map(|c| c.label()).unwrap_or(""))));
        }
        if integ.broken() {
            notify.write(Notify::warn(format!("Авария: {n} сломан(а) — {}", cause.map(|c| c.label()).unwrap_or(""))));
            resolve = true;
        }
        // Friction grows with wear: refresh the kinetic network now and then.
        if (before * 10.0).floor() != (integ.value * 10.0).floor() {
            resolve = true;
        }
    }
    if resolve {
        grid.dirty = true;
    }
}

/// Repairs `e` by `amount`, paying materials from `inv` (None = free).
/// Returns how much was repaired (0 if materials are missing).
pub fn repair(integ: &mut Integrity, amount: f32, inv: Option<&mut Inventory>, material: ItemId) -> f32 {
    let amount = amount.min(1.0 - integ.value);
    if amount <= 0.0 {
        return 0.0;
    }
    if let Some(inv) = inv {
        // Each item pays for REPAIR_PER_ITEM of integrity in advance.
        if integ.repaired < amount {
            let need = ((amount - integ.repaired) / REPAIR_PER_ITEM).ceil() as u32;
            if !inv.remove(material, need) {
                return 0.0;
            }
            integ.repaired += need as f32 * REPAIR_PER_ITEM;
        }
        integ.repaired -= amount;
    }
    integ.value = (integ.value + amount).min(1.0);
    if integ.value >= 1.0 {
        integ.cause = None;
        integ.repaired = 0.0;
    }
    amount
}

/// Wrench: hold LMB on a damaged part.
fn wrench(world: &mut World) {
    let tool = *world.resource::<ActiveTool>();
    let ActiveTool::Repair(_) = tool else { return };
    if !world.resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left) {
        return;
    }
    let Some(e) = world.resource::<EntityTarget>().entity else { return };
    let Some(integ) = world.get::<Integrity>(e).copied() else { return };
    if integ.value >= 1.0 {
        return;
    }
    let dt = world.resource::<Time>().delta_secs();
    let consume = world.resource::<ModeRules>().consume_items;
    let material = ItemId::k(repair_material(world, e));
    let was_broken = integ.broken();
    let mut inv = {
        let mut q = world.query_filtered::<&Inventory, With<Player>>();
        q.iter(world).next().cloned()
    };
    let mut i = integ;
    let done = repair(&mut i, REPAIR_RATE * dt, if consume { inv.as_mut() } else { None }, material);
    if done <= 0.0 {
        world.write_message(Notify::warn(format!("Для ремонта нужно: {}", material.def().name)));
        return;
    }
    if let Some(mut x) = world.get_mut::<Integrity>(e) {
        *x = i;
    }
    if consume && let Some(inv) = inv {
        let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
        if let Some(mut p) = q.iter_mut(world).next() {
            *p = inv;
        }
    }
    if was_broken || (integ.value * 10.0).floor() != (i.value * 10.0).floor() {
        world.resource_mut::<KineticGrid>().dirty = true;
    }
    if i.value >= 1.0 {
        world.write_message(Notify::ok("Отремонтировано"));
    }
}

/// Lines for engineering panels.
pub fn describe(world: &World, e: Entity) -> Option<(Vec<String>, Vec<String>)> {
    let i = world.get::<Integrity>(e)?;
    let mut lines = vec![format!(
        "Целостность: {:.0} %{}",
        i.value * 100.0,
        i.cause.map(|c| format!(" (износ: {})", c.label())).unwrap_or_default()
    )];
    let mut diag = Vec::new();
    if i.broken() {
        diag.push("Сломано: не работает. Ремонтный ключ (удерживать ЛКМ) или замена".into());
    } else if i.value < 0.5 {
        diag.push("Сильный износ: работает хуже, скоро откажет".into());
    }
    if i.value < 1.0 {
        let m = ItemId::k(repair_material(world, e));
        lines.push(format!("Ремонт: {} (1 шт. на 25 %)", m.def().name));
    }
    Some((std::mem::take(&mut lines), diag))
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
enum End {
    Prop(u32),
    Kinetic(IVec3),
}

fn capture(world: &mut World) -> serde_json::Value {
    let ids = world.get_resource::<SaveEntityIds>().cloned().unwrap_or_default();
    let mut q = world.query::<(Entity, &Integrity, Option<&KineticBlock>)>();
    let v: Vec<(End, f32)> = q
        .iter(world)
        .filter(|(_, i, _)| i.value < 1.0)
        .filter_map(|(e, i, k)| {
            let end = match k {
                Some(k) => End::Kinetic(k.cell),
                None => End::Prop(*ids.0.get(&e)?),
            };
            Some((end, i.value))
        })
        .collect();
    serde_json::to_value(v).unwrap_or_default()
}

fn restore(world: &mut World, v: Option<&serde_json::Value>) {
    let saved: Vec<(End, f32)> = v.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let ids = world.get_resource::<LoadedEntityIds>().cloned().unwrap_or_default();
    let mut kq = world.query::<(Entity, &KineticBlock)>();
    let cells: std::collections::HashMap<IVec3, Entity> = kq.iter(world).map(|(e, k)| (k.cell, e)).collect();
    for (end, value) in saved {
        let e = match end {
            End::Prop(id) => ids.0.get(&id).copied(),
            End::Kinetic(c) => cells.get(&c).copied(),
        };
        if let Some(e) = e {
            world.entity_mut(e).insert(Integrity { value, cause: None, repaired: 0.0 });
        }
    }
    world.resource_mut::<KineticGrid>().dirty = true;
}

pub struct WearPlugin;

impl Plugin for WearPlugin {
    fn build(&self, app: &mut App) {
        app.add_save_hook(SaveHook { key: "integrity", capture, restore })
            .add_systems(Update, (attach, apply_wear).chain().run_if(crate::core::states::in_game))
            .add_systems(Update, wrench.run_if(gameplay_active));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_wear_causes() {
        assert_eq!(magic_wear(MagicRole::Accumulator, 10.0, 0.2, 0.0), (0.0, None));
        let (r, c) = magic_wear(MagicRole::Accumulator, 10.0, 0.9, 0.0);
        assert!(r > 0.0 && c == Some(WearCause::Distortion));
        let (_, c) = magic_wear(MagicRole::Stabilizer, 10.0, 0.5, 0.0);
        assert_eq!(c, Some(WearCause::Unpowered));
        assert_eq!(magic_wear(MagicRole::Stabilizer, 10.0, 0.5, 1.0).1, None);
    }

    #[test]
    fn repair_pays_materials() {
        let metal = ItemId::k("metal");
        let mut inv = Inventory::default();
        inv.add(metal, 2);
        let mut i = Integrity { value: 0.2, cause: Some(WearCause::Overload), repaired: 0.0 };
        let mut total = 0.0;
        for _ in 0..40 {
            total += repair(&mut i, 0.05, Some(&mut inv), metal);
        }
        // Two items pay for 0.5 of integrity; then it stops.
        assert!((total - 0.5).abs() < 1e-3, "{total}");
        assert_eq!(inv.count(metal), 0);
        assert!(i.value < 1.0);
        // Free repair (creative) finishes it.
        repair(&mut i, 1.0, None, metal);
        assert_eq!(i.value, 1.0);
        assert_eq!(i.cause, None);
    }
}
