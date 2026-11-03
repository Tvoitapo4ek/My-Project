//! V2 checks: the electric network. Rotation → electric generator → wire
//! → power cell (charging); an overloaded wire heats up and burns; the wire
//! tool lays a wire; wires survive save → load.

use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{edit, step, teleport},
    scenario3::{aim_at, mouse, select, with_inv},
    scenario5::field,
    screenshot,
};
use crate::{
    core::states::AppState,
    electric::{ElectricState, WIRE_BURN_TIME, Wire, spawn_wire},
    items::ItemId,
    kinetics::{KineticBlock, assets_ref, network::Axis, spawn_kinetic},
    machinery::components::{Engine, PartAssets, PartKind, PowerCell, spawn_part},
    save::world_save::{PendingLoad, save_to, start_load},
    tools::manipulator::MechanicsUse,
    world::{chunks::EditCause, voxel::blocks},
};

fn base(world: &World) -> IVec3 {
    field(world).floor().as_ivec3() + IVec3::new(-5, 1, 2)
}

fn wires(world: &mut World) -> Vec<(Entity, Wire)> {
    let mut q = world.query::<(Entity, &Wire)>();
    q.iter(world).map(|(e, w)| (e, w.clone())).collect()
}

pub fn stage_v2(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("rotation charges a cell through a generator and a wire", |world, ctx| {
        let b = base(world);
        if ctx.frames == 1 {
            for x in -1..6 {
                for y in 0..3 {
                    for z in -1..3 {
                        edit(world, b + IVec3::new(x, y, z), blocks::AIR, EditCause::Script);
                    }
                }
            }
        }
        if ctx.frames == 3 {
            let a = assets_ref(world).unwrap();
            let parts = world.remove_resource::<PartAssets>().unwrap();
            let mut c = world.commands();
            let crank = spawn_kinetic(&mut c, &a, "hand_crank", b, Axis::X).unwrap();
            spawn_kinetic(&mut c, &a, "shaft", b + IVec3::X, Axis::X);
            let generator = spawn_kinetic(&mut c, &a, "electric_generator", b + IVec3::X * 2, Axis::X).unwrap();
            let cell = spawn_part(&mut c, &parts, PartKind::PowerCell, Transform::from_translation(b.as_vec3() + Vec3::new(4.5, 0.6, 1.5)));
            spawn_wire(&mut c, generator, cell, "copper_wire");
            world.flush();
            world.insert_resource(parts);
            if let Some(mut pc) = world.get_mut::<PowerCell>(cell) {
                pc.energy = 100.0;
            }
            ctx.entities.insert("v2_crank".into(), crank);
            ctx.entities.insert("v2_gen".into(), generator);
            ctx.entities.insert("v2_cell".into(), cell);
            // The strip is cleared up to z+2: look along it from its end.
            teleport(world, b.as_vec3() + Vec3::new(6.5, 1.6, 0.5));
            aim_at(world, b.as_vec3() + Vec3::new(1.5, 0.6, 0.5));
        }
        // Keep turning the crank.
        if ctx.frames >= 6 && ctx.frames % 8 == 6 {
            world.write_message(MechanicsUse { entity: ctx.entities["v2_crank"], secondary: false });
        }
        if ctx.frames == 10 {
            ctx.vars.insert("e0".into(), world.get::<PowerCell>(ctx.entities["v2_cell"]).unwrap().energy);
            ctx.vars.insert("t0".into(), ctx.t);
        }
        if ctx.frames > 10 && ctx.t > ctx.vars["t0"] + 6.0 {
            let g = world.get::<ElectricState>(ctx.entities["v2_gen"]).copied().unwrap_or_default();
            let e1 = world.get::<PowerCell>(ctx.entities["v2_cell"]).unwrap().energy;
            ctx.check("generator feeds the network", g.generated > 0.1, format!("{:.2} W", g.generated));
            ctx.check("cell charges from rotation", e1 > ctx.vars["e0"] + 0.5, format!("{:.2} → {e1:.2}", ctx.vars["e0"]));
            screenshot(world, "34_generator");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("overloaded wire heats up and burns", |world, ctx| {
        let b = base(world);
        if ctx.frames == 1 {
            let parts = world.remove_resource::<PartAssets>().unwrap();
            let mut c = world.commands();
            let engine = spawn_part(&mut c, &parts, PartKind::Engine, Transform::from_translation(b.as_vec3() + Vec3::new(4.5, 0.6, -0.5)));
            world.flush();
            world.insert_resource(parts);
            let cell = ctx.entities["v2_cell"];
            let mut c = world.commands();
            let w = spawn_wire(&mut c, cell, engine, "copper_wire").unwrap();
            world.flush();
            // A draw far beyond a copper wire (12 W).
            if let Some(mut en) = world.get_mut::<Engine>(engine) {
                en.on = true;
                en.draw = 25.0;
                en.throttle = 1.0;
            }
            ctx.entities.insert("v2_engine".into(), engine);
            ctx.entities.insert("v2_hot_wire".into(), w);
        }
        let w = ctx.entities["v2_hot_wire"];
        if ctx.frames == 12 {
            let st = world.get::<ElectricState>(ctx.entities["v2_engine"]).copied().unwrap_or_default();
            let load = world.get::<Wire>(w).map(|w| w.load()).unwrap_or(0.0);
            ctx.check("wire limits the power", (st.served - 12.0).abs() < 0.5 && load >= 0.98, format!("served {:.1} W, load {:.0}%", st.served, load * 100.0));
        }
        let gone = world.get::<Wire>(w).is_none();
        if gone && !ctx.vars.contains_key("gone_at") {
            ctx.vars.insert("gone_at".into(), ctx.frames as f32);
        }
        // Give the network a few ticks to notice the missing wire.
        let settled = ctx.vars.get("gone_at").is_some_and(|f| ctx.frames as f32 > f + 4.0);
        if (ctx.frames > 12 && settled) || ctx.t > WIRE_BURN_TIME + 8.0 {
            ctx.check("overloaded wire burns", gone, format!("after {:.1} s", ctx.t));
            let st = world.get::<ElectricState>(ctx.entities["v2_engine"]).copied().unwrap_or_default();
            ctx.check("engine without a wire gets nothing", st.served == 0.0, format!("{:.1}", st.served));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("wire tool lays a wire between devices", |world, ctx| {
        let cell = ctx.entities["v2_cell"];
        let engine = ctx.entities["v2_engine"];
        if ctx.frames == 1 {
            if let Some(mut en) = world.get_mut::<Engine>(engine) {
                en.draw = 6.0;
            }
            // The overloaded engine drained the cell; top it up.
            if let Some(mut pc) = world.get_mut::<PowerCell>(cell) {
                pc.energy = 600.0;
            }
            with_inv(world, |i| {
                i.add(ItemId::k("power_cable"), 8);
            });
            select(world, "power_cable");
            ctx.vars.insert("n0".into(), wires(world).len() as f32);
            // Stand to the side so both devices are in plain view.
            let pc = world.get::<Transform>(cell).unwrap().translation;
            let pe = world.get::<Transform>(engine).unwrap().translation;
            teleport(world, (pc + pe) / 2.0 + Vec3::new(3.0, 0.8, 0.0));
        }
        if ctx.frames == 6 {
            let p = world.get::<Transform>(cell).unwrap().translation;
            aim_at(world, p);
        }
        if ctx.frames == 10 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 11 {
            mouse(world, MouseButton::Left, false);
        }
        if ctx.frames == 14 {
            let p = world.get::<Transform>(engine).unwrap().translation;
            aim_at(world, p);
        }
        if ctx.frames == 18 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 19 {
            mouse(world, MouseButton::Left, false);
        }
        if ctx.frames == 30 {
            let ws = wires(world);
            let made = ws.iter().any(|(_, w)| w.item == "power_cable" && ((w.a == cell && w.b == engine) || (w.a == engine && w.b == cell)));
            ctx.check("tool connected the cell and the engine", made, format!("{} wires", ws.len()));
            let st = world.get::<ElectricState>(engine).copied().unwrap_or_default();
            ctx.check("engine is powered over the cable", st.satisfaction > 0.99, format!("{:.0}%", st.satisfaction * 100.0));
            screenshot(world, "35_wires");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("wires survive save and load", |world, ctx| {
        let path = std::path::PathBuf::from("target/autotest_v2.json");
        if ctx.frames == 1 {
            let before: Vec<(String, bool)> = wires(world)
                .iter()
                .map(|(_, w)| (w.item.clone(), world.get::<KineticBlock>(w.a).is_some() || world.get::<KineticBlock>(w.b).is_some()))
                .collect();
            ctx.vars.insert("wires".into(), before.len() as f32);
            ctx.vars.insert("kin_wires".into(), before.iter().filter(|w| w.1).count() as f32);
            let r = save_to(world, &path);
            ctx.check("saved", r.is_ok(), format!("{r:?}"));
            for (e, _) in wires(world) {
                world.despawn(e);
            }
        }
        if ctx.frames == 4 {
            let r = start_load(world, &path);
            ctx.check("load accepted", r.is_ok(), format!("{r:?}"));
        }
        let in_game = *world.resource::<State<AppState>>().get() == AppState::InGame;
        if ctx.frames > 8 && in_game && !world.contains_resource::<PendingLoad>() && ctx.t > 3.0 {
            let after = wires(world);
            let kin = after.iter().filter(|(_, w)| world.get::<KineticBlock>(w.a).is_some() || world.get::<KineticBlock>(w.b).is_some()).count();
            ctx.check(
                "all wires restored with their ends",
                after.len() as f32 == ctx.vars["wires"] && kin as f32 == ctx.vars["kin_wires"],
                format!("{} / {} (kinetic ends {kin} / {})", after.len(), ctx.vars["wires"], ctx.vars["kin_wires"]),
            );
            let _ = std::fs::remove_file(&path);
            return Step::Next;
        }
        Step::Wait
    }));
}
