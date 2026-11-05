//! V4 checks: logic controls the machines built in V1 and V3.
//! aether sensor (on the accumulator) → comparator → controller → magic
//! engine; lever → clutch; presence sensor → lamp; the cable tool; save.

use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{edit, step, teleport},
    scenario3::{aim_at, mouse, select, with_inv},
    scenario5::field,
    screenshot,
};
use crate::{
    arcana::MagicNode,
    items::ItemId,
    kinetics::{KineticBlock, KineticGrid, assets_ref, network::Axis, spawn_kinetic},
    logic::{LogicNode, SignalLink, spawn_signal},
    tools::manipulator::MechanicsUse,
    world::{chunks::EditCause, voxel::blocks},
};

/// Same origin as the V3 magic setup.
fn base(world: &World) -> IVec3 {
    field(world).floor().as_ivec3() + IVec3::new(1, 1, -4)
}

fn block(world: &World, cell: IVec3) -> Option<Entity> {
    world.resource::<KineticGrid>().blocks.get(&cell).copied()
}

pub fn stage_v4(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("sensor and controller run the magic engine", |world, ctx| {
        let c = base(world);
        if ctx.frames == 1 {
            // Reopen the rune channel blocked in V3.
            for y in 0..2 {
                edit(world, c + IVec3::new(2, y, 2), blocks::AIR, EditCause::Script);
            }
            let a = assets_ref(world).unwrap();
            let mut cm = world.commands();
            let sensor = spawn_kinetic(&mut cm, &a, "sensor_aether", c + IVec3::new(2, 1, 0), Axis::Y).unwrap();
            let comp = spawn_kinetic(&mut cm, &a, "logic_comparator", c + IVec3::new(3, 1, 0), Axis::Y).unwrap();
            let ctrl = spawn_kinetic(&mut cm, &a, "logic_controller", c + IVec3::new(4, 1, 0), Axis::Y).unwrap();
            spawn_signal(&mut cm, sensor, comp);
            spawn_signal(&mut cm, comp, ctrl);
            world.flush();
            ctx.entities.insert("v4_sensor".into(), sensor);
            ctx.entities.insert("v4_comp".into(), comp);
            ctx.entities.insert("v4_ctrl".into(), ctrl);
            teleport(world, c.as_vec3() + Vec3::new(3.0, 2.2, 6.0));
            aim_at(world, c.as_vec3() + Vec3::new(3.5, 1.0, 0.5));
        }
        if ctx.frames == 4 {
            // Comparator: on above 0.3 (param 2).
            if let Some(mut n) = world.get_mut::<LogicNode>(ctx.entities["v4_comp"]) {
                n.gate.param = 2;
            }
            let acc = ctx.entities["v3_acc"];
            if let Some(mut m) = world.get_mut::<MagicNode>(acc) {
                m.amount = 100.0;
            }
        }
        if ctx.frames == 5 {
            // Controller → magic engine, laid with the real cable tool.
            with_inv(world, |i| {
                i.add(ItemId::k("signal_cable"), 4);
            });
            select(world, "signal_cable");
        }
        if ctx.frames == 8 {
            aim_at(world, c.as_vec3() + Vec3::new(4.5, 1.5, 0.5));
        }
        if ctx.frames == 12 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 13 {
            mouse(world, MouseButton::Left, false);
        }
        if ctx.frames == 14 {
            // Step to the engine's free east side for the second end.
            teleport(world, c.as_vec3() + Vec3::new(6.6, 1.4, -0.5));
        }
        if ctx.frames == 16 {
            aim_at(world, c.as_vec3() + Vec3::new(5.5, 0.5, -0.5));
        }
        if ctx.frames == 20 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 21 {
            mouse(world, MouseButton::Left, false);
        }
        if ctx.frames == 40 {
            let eng = ctx.entities["v3_eng"];
            let ctrl = ctx.entities["v4_ctrl"];
            let mut q = world.query::<&SignalLink>();
            let cabled = q.iter(world).any(|l| l.from == ctrl && l.to == eng);
            ctx.check("cable tool connected controller → engine", cabled, "");
            let on = world.get::<KineticBlock>(eng).is_some_and(|k| k.on);
            ctx.check("full accumulator keeps the engine on", on, "");
            // Drain the accumulator: below the threshold the engine stops.
            if let Some(mut m) = world.get_mut::<MagicNode>(ctx.entities["v3_acc"]) {
                m.amount = 2.0;
            }
        }
        if ctx.frames == 52 {
            let eng = ctx.entities["v3_eng"];
            let off = world.get::<KineticBlock>(eng).is_some_and(|k| !k.on);
            let out = world.get::<LogicNode>(ctx.entities["v4_ctrl"]).map(|n| n.gate.out).unwrap_or(-1.0);
            ctx.check("low aether: controller switches the engine off", off && out == 0.0, format!("controller out {out}"));
            teleport(world, c.as_vec3() + Vec3::new(3.0, 2.4, 5.0));
            aim_at(world, c.as_vec3() + Vec3::new(3.5, 0.8, 0.0));
        }
        if ctx.frames == 56 {
            screenshot(world, "37_logic");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("lever drives a clutch, presence lights a lamp", |world, ctx| {
        // V1 drivetrain: motor at b, clutch at b + 2X.
        let b = field(world).floor().as_ivec3() + IVec3::new(-5, 1, -5);
        if ctx.frames == 1 {
            let a = assets_ref(world).unwrap();
            let clutch = block(world, b + IVec3::X * 2).unwrap();
            let mut cm = world.commands();
            let lever = spawn_kinetic(&mut cm, &a, "logic_lever", b + IVec3::new(2, 1, 1), Axis::Y).unwrap();
            let presence = spawn_kinetic(&mut cm, &a, "sensor_presence", b + IVec3::new(0, 2, 1), Axis::Y).unwrap();
            let lamp = spawn_kinetic(&mut cm, &a, "signal_lamp", b + IVec3::new(1, 2, 1), Axis::Y).unwrap();
            spawn_signal(&mut cm, lever, clutch);
            spawn_signal(&mut cm, presence, lamp);
            world.flush();
            ctx.entities.insert("v4_lever".into(), lever);
            ctx.entities.insert("v4_lamp".into(), lamp);
            ctx.entities.insert("v4_clutch".into(), clutch);
            teleport(world, b.as_vec3() + Vec3::new(1.5, 1.6, 3.0));
        }
        if ctx.frames == 8 {
            // Lever is off: the command disengages the clutch.
            let open = world.get::<KineticBlock>(ctx.entities["v4_clutch"]).is_some_and(|k| k.toggled);
            ctx.check("lever off: clutch disengaged by command", open, "");
            world.write_message(MechanicsUse { entity: ctx.entities["v4_lever"], secondary: false });
        }
        if ctx.frames == 18 {
            let closed = world.get::<KineticBlock>(ctx.entities["v4_clutch"]).is_some_and(|k| !k.toggled);
            ctx.check("lever on: clutch engaged", closed, "");
            let lamp = world.get::<LogicNode>(ctx.entities["v4_lamp"]).map(|n| n.gate.out).unwrap_or(0.0);
            ctx.check("presence sensor lights the lamp", lamp > 0.5, format!("{lamp}"));
            let save = crate::save::world_save::capture(world);
            let l = save.extensions.get("logic");
            let gates = l.and_then(|v| v.get("gates")).and_then(|v| v.as_array()).map(|v| v.len()).unwrap_or(0);
            let cables = l.and_then(|v| v.get("cables")).and_then(|v| v.as_array()).map(|v| v.len()).unwrap_or(0);
            ctx.check("logic and cables are saved", gates == 6 && cables == 5, format!("{gates} gates, {cables} cables"));
            return Step::Next;
        }
        Step::Wait
    }));
}
