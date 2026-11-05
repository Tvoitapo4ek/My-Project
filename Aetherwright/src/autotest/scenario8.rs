//! Stage 8 checks: hand crafting + crafting UI, smelter with a hopper crate
//! (automation), grinder driven by an engine through a hinge.

use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{step, teleport},
    scenario3::{inv, with_inv},
    scenario5::field,
    screenshot,
};
use crate::{
    core::states::InputFocus,
    crafting::{
        recipes::recipes,
        stations::{CrateStorage, Machine},
    },
    items::{ItemId, ItemStack, drops::SpawnDrop},
    machinery::components::{Engine, PartAssets, PartKind, spawn_part},
    physics::joints::{Connection, JointKind, LocalFrame, spawn_connection},
    ui::inventory::CraftingDock,
};

fn spawn(world: &mut World, kind: PartKind, at: Vec3) -> Entity {
    let assets = world.remove_resource::<PartAssets>().unwrap();
    let mut c = world.commands();
    let e = spawn_part(&mut c, &assets, kind, Transform::from_translation(at));
    world.flush();
    world.insert_resource(assets);
    e
}

fn crate_count(world: &World, c: Entity, key: &str) -> u32 {
    world
        .get::<CrateStorage>(c)
        .map(|s| s.0.count(ItemId::k(key)))
        .unwrap_or(0)
}

fn drop_into(world: &mut World, c: Entity, key: &str, n: u32) {
    let p = world.get::<Transform>(c).map(|t| t.translation).unwrap();
    for i in 0..n {
        world.write_message(SpawnDrop {
            stack: ItemStack::new(ItemId::k(key), 1),
            position: p + Vec3::new(0.0, 1.0 + i as f32 * 0.35, 0.0),
            velocity: Vec3::ZERO,
        });
    }
}

pub fn stage8(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("hand crafting", |world, ctx| {
        if ctx.frames == 1 {
            with_inv(world, |i| {
                i.add(ItemId::k("wood"), 2);
            });
            let p0 = inv(world).count(ItemId::k("planks"));
            let mut q = world.query_filtered::<&mut crate::items::inventory::Inventory, With<crate::player::Player>>();
            let ok = q.iter_mut(world).next().map(|mut i| recipes()[0].craft(&mut i)).unwrap_or(false);
            let p1 = inv(world).count(ItemId::k("planks"));
            ctx.check("wood → planks by hand", ok && p1 == p0 + 4, format!("{p0} → {p1}"));
            world.resource_mut::<NextState<InputFocus>>().set(InputFocus::Inventory);
        }
        if ctx.t > 1.5 {
            let mut q = world.query_filtered::<&Children, With<CraftingDock>>();
            let n = q.iter(world).next().map(|c| c.len()).unwrap_or(0);
            ctx.check("crafting panel lists recipes", n > 10, format!("{n} entries"));
            screenshot(world, "17_crafting");
            world.resource_mut::<NextState<InputFocus>>().set(InputFocus::Gameplay);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("smelter with a hopper crate", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world);
            teleport(world, f + Vec3::new(-3.0, 2.0, 4.0));
            let smelter = spawn(world, PartKind::Smelter, f + Vec3::new(-3.0, 1.5, 0.0));
            let hopper = spawn(world, PartKind::Crate, f + Vec3::new(-1.6, 1.5, 0.0));
            ctx.entities.insert("smelter".into(), smelter);
            ctx.entities.insert("hopper".into(), hopper);
        }
        if ctx.frames == 20 {
            let h = ctx.entities["hopper"];
            drop_into(world, h, "iron_ore", 2);
            drop_into(world, h, "coal", 1);
        }
        if ctx.frames > 20 && ctx.t > 14.0 {
            let h = ctx.entities["hopper"];
            let metal = crate_count(world, h, "metal");
            let m = world
                .get::<Machine>(ctx.entities["smelter"])
                .cloned()
                .unwrap_or_default();
            ctx.check(
                "smelter turned ore into metal automatically",
                metal >= 1,
                format!("metal in crate {metal}, fuel {} ops", m.fuel_ops),
            );
            screenshot(world, "18_smelter");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("grinder driven by an engine", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world);
            teleport(world, f + Vec3::new(3.0, 2.0, 5.0));
            let grinder = spawn(world, PartKind::Grinder, f + Vec3::new(3.0, 1.5, 0.0));
            let engine = spawn(world, PartKind::Engine, f + Vec3::new(3.0, 2.55, 0.0));
            let cell = spawn(world, PartKind::PowerCell, f + Vec3::new(3.0, 3.6, 0.0));
            let hopper = spawn(world, PartKind::Crate, f + Vec3::new(4.7, 1.5, 0.0));
            let up = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
            let id = Quat::IDENTITY;
            let mut c = world.commands();
            spawn_connection(
                &mut c,
                Connection {
                    a: grinder,
                    b: engine,
                    kind: JointKind::Hinge,
                    frame_a: LocalFrame {
                        anchor: Vec3::Y * 0.5,
                        basis: up,
                    },
                    frame_b: LocalFrame {
                        anchor: -Vec3::Y * 0.52,
                        basis: up,
                    },
                },
            );
            spawn_connection(
                &mut c,
                Connection {
                    a: engine,
                    b: cell,
                    kind: JointKind::Weld,
                    frame_a: LocalFrame {
                        anchor: Vec3::Y * 0.5,
                        basis: id,
                    },
                    frame_b: LocalFrame {
                        anchor: -Vec3::Y * 0.55,
                        basis: id,
                    },
                },
            );
            world.flush();
            if let Some(mut e) = world.get_mut::<Engine>(engine) {
                e.on = true;
                e.throttle = 0.5;
            }
            ctx.entities.insert("grinder".into(), grinder);
            ctx.entities.insert("gengine".into(), engine);
            ctx.entities.insert("ghopper".into(), hopper);
        }
        if ctx.frames == 20 {
            let h = ctx.entities["ghopper"];
            drop_into(world, h, "stone", 2);
        }
        if ctx.frames > 20 && ctx.t > 10.0 {
            let h = ctx.entities["ghopper"];
            let gravel = crate_count(world, h, "gravel");
            let m = world
                .get::<Machine>(ctx.entities["grinder"])
                .cloned()
                .unwrap_or_default();
            ctx.check(
                "engine spins the grinder shaft",
                m.drive_speed >= 2.0,
                format!("{:.1} rad/s", m.drive_speed),
            );
            ctx.check(
                "grinder produces while driven",
                gravel >= 1,
                format!("gravel {gravel}"),
            );
            screenshot(world, "19_grinder");
            if let Some(mut e) = world.get_mut::<Engine>(ctx.entities["gengine"]) {
                e.on = false;
            }
            ctx.vars.insert("gravel".into(), gravel as f32);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("grinder stops without drive", |world, ctx| {
        if ctx.frames == 1 {
            let h = ctx.entities["ghopper"];
            drop_into(world, h, "stone", 2);
        }
        let g = ctx.entities["grinder"];
        if ctx.t > 10.0 && !ctx.vars.contains_key("p10") {
            let m = world.get::<Machine>(g).cloned().unwrap_or_default();
            ctx.vars.insert("p10".into(), m.progress);
        }
        if ctx.t > 13.0 {
            let m = world.get::<Machine>(g).cloned().unwrap_or_default();
            ctx.check(
                "rotor spins down without the engine",
                m.drive_speed < 2.0,
                format!("{:.2} rad/s", m.drive_speed),
            );
            ctx.check(
                "no rotation → no processing",
                (m.progress - ctx.vars["p10"]).abs() < 1e-4,
                format!("progress {:.2} → {:.2}", ctx.vars["p10"], m.progress),
            );
            return Step::Next;
        }
        Step::Wait
    }));
}
