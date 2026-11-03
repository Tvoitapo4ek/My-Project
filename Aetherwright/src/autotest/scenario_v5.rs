//! V5 checks — steps 9 and 10 of the integration scenario on the running
//! V1 drivetrain: an overload wears parts until they break (cascade: the
//! grinder stops), automation raises an alarm (rotation sensor →
//! comparator → lamp), the engineering panel names the fault, the wrench
//! repairs it (materials), the system runs again and the alarm goes out.

use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{step, teleport},
    scenario3::{aim_at, inv, mouse, select, with_inv},
    scenario5::field,
    screenshot,
};
use crate::{
    items::ItemId,
    kinetics::{KineticBlock, KineticConsumer, KineticDrive, KineticGrid, assets_ref, network::Axis, spawn_kinetic},
    logic::{LogicNode, spawn_signal},
    wear::{Integrity, repair},
};

fn base(world: &World) -> IVec3 {
    field(world).floor().as_ivec3() + IVec3::new(-5, 1, -5)
}

fn broken_blocks(world: &mut World) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &Integrity, &KineticBlock)>();
    q.iter(world).filter(|(_, i, _)| i.broken()).map(|(e, _, _)| e).collect()
}

pub fn stage_v5(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("overload wears parts until the drivetrain breaks", |world, ctx| {
        let b = base(world);
        let g = ctx.entities["v1_grinder"];
        if ctx.frames == 1 {
            // Alarm: rotation of the last shaft < 0.1 → lamp.
            let a = assets_ref(world).unwrap();
            let mut cm = world.commands();
            let sensor = spawn_kinetic(&mut cm, &a, "sensor_rotation", b + IVec3::new(4, 2, 0), Axis::Y).unwrap();
            let comp = spawn_kinetic(&mut cm, &a, "logic_comparator", b + IVec3::new(4, 3, 0), Axis::Y).unwrap();
            let lamp = spawn_kinetic(&mut cm, &a, "signal_lamp", b + IVec3::new(4, 4, 0), Axis::Y).unwrap();
            spawn_signal(&mut cm, sensor, comp);
            spawn_signal(&mut cm, comp, lamp);
            world.flush();
            ctx.entities.insert("v5_comp".into(), comp);
            ctx.entities.insert("v5_lamp".into(), lamp);
            // Overload: 7 SU/rpm · 64 rpm = 448 > shaft strength 384.
            world.get_mut::<KineticConsumer>(g).unwrap().impact = 7.0;
            world.resource_mut::<KineticGrid>().dirty = true;
            teleport(world, b.as_vec3() + Vec3::new(2.5, 2.0, 5.5));
            aim_at(world, b.as_vec3() + Vec3::new(2.5, 1.0, 0.5));
        }
        if ctx.frames == 4 {
            if let Some(mut n) = world.get_mut::<LogicNode>(ctx.entities["v5_comp"]) {
                n.gate.mode = 1; // less than 0.1
                n.gate.param = 0;
            }
        }
        if ctx.frames == 12 {
            let lamp = world.get::<LogicNode>(ctx.entities["v5_lamp"]).map(|n| n.gate.out).unwrap_or(-1.0);
            ctx.check("running drivetrain: alarm lamp off", lamp == 0.0, format!("{lamp}"));
        }
        let broken = broken_blocks(world);
        if (ctx.frames > 12 && !broken.is_empty() && ctx.vars.contains_key("seen")) || ctx.t > 45.0 {
            ctx.check("overload broke parts", !broken.is_empty(), format!("{} broken after {:.1} s", broken.len(), ctx.t));
            let drive = world.get::<KineticDrive>(g).map(|d| d.0).unwrap_or(-1.0);
            ctx.check("cascade: the grinder stopped", drive == 0.0, format!("{drive}"));
            let lamp = world.get::<LogicNode>(ctx.entities["v5_lamp"]).map(|n| n.gate.out).unwrap_or(-1.0);
            ctx.check("automation raises the alarm", lamp > 0.5, format!("{lamp}"));
            let e = broken.first().copied();
            let text = e.and_then(|e| crate::holo::engineering::describe(world, e));
            let ok = text.as_ref().is_some_and(|(_, _, d)| d.contains("Сломано")) && text.as_ref().is_some_and(|(_, l, _)| l.contains("перегрузка"));
            ctx.check("diagnostics name the fault and its cause", ok, format!("{text:?}"));
            screenshot(world, "38_breakdown");
            return Step::Next;
        }
        if !broken.is_empty() {
            // Let the network re-solve after the first break.
            ctx.vars.insert("seen".into(), 1.0);
        }
        Step::Wait
    }));
    steps.push(step("wrench repairs the drivetrain, it runs again", |world, ctx| {
        let b = base(world);
        let g = ctx.entities["v1_grinder"];
        if ctx.frames == 1 {
            // Survival: repairs cost materials (creative repairs are free).
            world.resource_mut::<NextState<crate::core::states::GameMode>>().set(crate::core::states::GameMode::Survival);
            // Lighter load, so it does not happen again.
            world.get_mut::<KineticConsumer>(g).unwrap().impact = 2.0;
            world.resource_mut::<KineticGrid>().dirty = true;
            with_inv(world, |i| {
                i.add(ItemId::k("repair_wrench"), 1);
                i.add(ItemId::k("metal"), 30);
            });
            select(world, "repair_wrench");
            ctx.vars.insert("metal0".into(), inv(world).count(ItemId::k("metal")) as f32);
            teleport(world, b.as_vec3() + Vec3::new(1.5, 1.6, 2.5));
        }
        let shaft = world.resource::<KineticGrid>().blocks.get(&(b + IVec3::X)).copied().unwrap();
        if ctx.frames == 4 {
            // Make sure the target is broken for the tool test.
            if let Some(mut i) = world.get_mut::<Integrity>(shaft) {
                i.value = 0.0;
            }
            aim_at(world, b.as_vec3() + Vec3::new(1.5, 0.5, 0.5));
        }
        if ctx.frames == 8 {
            mouse(world, MouseButton::Left, true);
            ctx.vars.insert("t_hold".into(), ctx.t);
        }
        let held = ctx.vars.get("t_hold").is_some_and(|t| ctx.t > t + 5.0);
        if held && !ctx.vars.contains_key("released") {
            mouse(world, MouseButton::Left, false);
            ctx.vars.insert("released".into(), ctx.frames as f32);
            let v = world.get::<Integrity>(shaft).map(|i| i.value).unwrap_or(0.0);
            let used = ctx.vars["metal0"] - inv(world).count(ItemId::k("metal")) as f32;
            ctx.check("wrench restores the shaft", v >= 0.999, format!("{:.0}%", v * 100.0));
            ctx.check("repair costs materials", used == 4.0, format!("{used} metal"));
            // The other broken parts: same repair rules (materials paid).
            for e in broken_blocks(world) {
                let mut i = *world.get::<Integrity>(e).unwrap();
                let mut q = world.query_filtered::<&mut crate::items::inventory::Inventory, With<crate::player::Player>>();
                let mut p = q.iter_mut(world).next().unwrap();
                repair(&mut i, 1.0, Some(&mut p), ItemId::k("metal"));
                world.entity_mut(e).insert(i);
            }
            world.resource_mut::<KineticGrid>().dirty = true;
        }
        if ctx.vars.get("released").is_some_and(|f| ctx.frames as f32 > f + 12.0) {
            let drive = world.get::<KineticDrive>(g).map(|d| d.0).unwrap_or(0.0);
            ctx.check("repaired drivetrain turns the grinder again", drive == 64.0, format!("{drive}"));
            let lamp = world.get::<LogicNode>(ctx.entities["v5_lamp"]).map(|n| n.gate.out).unwrap_or(-1.0);
            ctx.check("alarm goes out", lamp == 0.0, format!("{lamp}"));
            ctx.check("no broken parts left", broken_blocks(world).is_empty(), "");
            screenshot(world, "39_repaired");
            return Step::Next;
        }
        Step::Wait
    }));
}
