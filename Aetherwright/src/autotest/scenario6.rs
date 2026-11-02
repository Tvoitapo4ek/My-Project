//! Stage 6 checks: aether field, accumulators, transfer, dynamo → energy,
//! spells (impulse, levitation, discharge) and the agitated anomaly.

#[allow(unused_imports)]
use avian3d::prelude::LinearVelocity as _Lv;
use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{step, teleport},
    scenario3::{aim_at, mouse, select},
    scenario4::pos_of,
    scenario5::field,
    screenshot,
};
use crate::{
    core::resources::SpawnPoint,
    machinery::components::{PartAssets, PartKind, PowerCell, spawn_part},
    magic::{
        anomalies::Anomaly,
        sources::{AetherSources, AetherStore},
        spells::{AetherReserve, Levitating, Spell, SpellState},
    },
    physics::joints::{Connection, JointKind, LocalFrame, spawn_connection},
    player::Player,
    world::{chunks::VoxelWorld, gen_params, generation::sites},
};

fn spawn(world: &mut World, kind: PartKind, at: Vec3) -> Entity {
    let assets = world.remove_resource::<PartAssets>().unwrap();
    let mut c = world.commands();
    let e = spawn_part(&mut c, &assets, kind, Transform::from_translation(at));
    world.flush();
    world.insert_resource(assets);
    e
}

fn spire_ground(world: &World) -> Vec3 {
    let w = world.resource::<VoxelWorld>();
    let s = sites(&gen_params(w)).spire;
    Vec3::new(s.x + 5.0, (w.sea_level + 7) as f32, s.y + 0.5)
}

fn reserve(world: &mut World) -> f32 {
    let mut q = world.query_filtered::<&AetherReserve, With<Player>>();
    q.iter(world).next().map(|r| r.amount).unwrap_or(0.0)
}

pub fn stage6(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("aether field", |world, ctx| {
        let n = world.resource::<AetherSources>().0.len();
        ctx.check("aether sources indexed", n > 20, format!("{n} sources"));
        let spawn = world.resource::<SpawnPoint>().0;
        let at_spire = world
            .resource::<AetherSources>()
            .sample(spire_ground(world));
        let at_spawn = world.resource::<AetherSources>().sample(spawn);
        ctx.check(
            "field is stronger near crystals",
            at_spire.strength > at_spawn.strength * 3.0,
            format!("{:.2} vs {:.2}", at_spire.strength, at_spawn.strength),
        );
        let mut q = world.query::<&Anomaly>();
        ctx.check("agitated anomaly exists", q.iter(world).count() >= 1, "");
        Step::Next
    }));
    steps.push(step("jars charge and share", |world, ctx| {
        if ctx.frames == 1 {
            let g = spire_ground(world);
            let near = spawn(world, PartKind::AetherJar, g + Vec3::Y * 0.4);
            let far = field(world) + Vec3::new(4.0, 1.5, -4.0);
            let remote = spawn(world, PartKind::AetherJar, far);
            let remote2 = spawn(world, PartKind::AetherJar, far + Vec3::new(2.0, 0.0, 0.0));
            ctx.entities.insert("jar_near".into(), near);
            ctx.entities.insert("jar_a".into(), remote);
            ctx.entities.insert("jar_b".into(), remote2);
            ctx.vars.insert(
                "sim0".into(),
                world.resource::<Time<Virtual>>().elapsed_secs(),
            );
        }
        if ctx.frames == 3 {
            if let Some(mut s) = world.get_mut::<AetherStore>(ctx.entities["jar_a"]) {
                s.amount = 80.0;
            }
        }
        if ctx.t > 3.0 {
            let near = world
                .get::<AetherStore>(ctx.entities["jar_near"])
                .map(|s| s.amount)
                .unwrap_or(0.0);
            let a = world
                .get::<AetherStore>(ctx.entities["jar_a"])
                .map(|s| s.amount)
                .unwrap_or(0.0);
            let b = world
                .get::<AetherStore>(ctx.entities["jar_b"])
                .map(|s| s.amount)
                .unwrap_or(0.0);
            ctx.check(
                "jar near the spire charges from the field",
                near > 1.0,
                format!("{near:.1}"),
            );
            ctx.check(
                "aether flows between linked jars",
                b > 10.0 && a < 75.0,
                format!("a {a:.1}, b {b:.1}"),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("dynamo turns aether into energy", |world, ctx| {
        if ctx.frames == 1 {
            let base = field(world) + Vec3::new(4.0, 1.6, -2.0);
            let dynamo = spawn(world, PartKind::AetherDynamo, base);
            let cell = spawn(world, PartKind::PowerCell, base + Vec3::new(1.2, 0.0, 0.0));
            let id = Quat::IDENTITY;
            let mut c = world.commands();
            spawn_connection(
                &mut c,
                Connection {
                    a: dynamo,
                    b: cell,
                    kind: JointKind::Weld,
                    frame_a: LocalFrame {
                        anchor: Vec3::X * 0.6,
                        basis: id,
                    },
                    frame_b: LocalFrame {
                        anchor: -Vec3::X * 0.6,
                        basis: id,
                    },
                },
            );
            world.flush();
            if let Some(mut p) = world.get_mut::<PowerCell>(cell) {
                p.energy = 0.0;
            }
            for k in ["jar_a", "jar_b"] {
                if let Some(mut s) = world.get_mut::<AetherStore>(ctx.entities[k]) {
                    s.amount = 100.0;
                }
            }
            ctx.entities.insert("dyn_cell".into(), cell);
        }
        if ctx.t > 3.0 {
            let e = world
                .get::<PowerCell>(ctx.entities["dyn_cell"])
                .map(|c| c.energy)
                .unwrap_or(0.0);
            ctx.check(
                "power cell charged by the aether dynamo",
                e > 20.0,
                format!("energy {e:.1}"),
            );
            screenshot(world, "13_aether_dynamo");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("discharge charges a power cell", |world, ctx| {
        let cell = ctx.entities["dyn_cell"];
        if ctx.frames == 1 {
            select(world, "aether_focus");
            let pe = world
                .query_filtered::<Entity, With<Player>>()
                .iter(world)
                .next()
                .unwrap();
            if let Some(mut r) = world.get_mut::<AetherReserve>(pe) {
                r.amount = r.max;
            }
            world.resource_mut::<SpellState>().selected = Spell::Discharge;
            world.resource_mut::<SpellState>().cooldown = 0.0;
            let p = pos_of(world, cell);
            teleport(world, p + Vec3::new(0.0, 1.5, 3.0));
            if let Some(mut c) = world.get_mut::<PowerCell>(cell) {
                c.energy = 0.0;
            }
            // Detach it from the dynamo so only the spell can charge it.
            let mut q = world.query::<(Entity, &Connection)>();
            let joints: Vec<Entity> = q
                .iter(world)
                .filter(|(_, c)| c.a == cell || c.b == cell)
                .map(|(e, _)| e)
                .collect();
            for j in joints {
                world.despawn(j);
            }
        }
        if ctx.frames < 10 || ctx.t < 1.0 {
            let p = pos_of(world, cell);
            aim_at(world, p);
            if let Some(mut c) = world.get_mut::<PowerCell>(cell) {
                c.energy = 0.0;
            }
            return Step::Wait;
        }
        if !ctx.vars.contains_key("cast") {
            ctx.vars.insert("cast".into(), ctx.t);
            mouse(world, MouseButton::Left, true);
            return Step::Wait;
        }
        mouse(world, MouseButton::Left, false);
        if ctx.t > ctx.vars["cast"] + 0.6 {
            let e = world
                .get::<PowerCell>(cell)
                .map(|c| c.energy)
                .unwrap_or(0.0);
            ctx.check(
                "discharge transfers magic into a machine",
                e > 20.0,
                format!("energy {e:.1}"),
            );
            ctx.vars.remove("cast");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("clear the test field", |world, ctx| {
        let f = field(world);
        let mut q = world.query::<(Entity, &crate::physics::Prop, &Position)>();
        let junk: Vec<Entity> = q
            .iter(world)
            .filter(|(_, _, p)| p.0.distance(f) < 12.0)
            .map(|(e, _, _)| e)
            .collect();
        for e in junk {
            world.despawn(e);
        }
        if ctx.t > 0.5 { Step::Next } else { Step::Wait }
    }));
    steps.push(step("impulse spell pushes objects", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world);
            teleport(world, f + Vec3::new(0.0, 2.0, -3.0));
            // The field floor top is at f.y + 1.
            let crate_e = spawn(world, PartKind::Crate, f + Vec3::new(0.0, 1.6, -5.0));
            ctx.entities.insert("crate".into(), crate_e);
            select(world, "aether_focus");
            world.resource_mut::<SpellState>().selected = Spell::Impulse;
            world.resource_mut::<SpellState>().cooldown = 0.0;
        }
        if ctx.frames < 10 || ctx.t < 0.8 {
            let p = pos_of(world, ctx.entities["crate"]);
            aim_at(world, p);
            ctx.vars.insert("c0".into(), p.z);
            return Step::Wait;
        }
        if !ctx.vars.contains_key("cast") {
            ctx.vars.insert("cast".into(), ctx.t);
            ctx.vars.insert("r0".into(), reserve(world));
            mouse(world, MouseButton::Left, true);
            return Step::Wait;
        }
        mouse(world, MouseButton::Left, false);
        if ctx.t > ctx.vars["cast"] + 1.5 {
            let dz = pos_of(world, ctx.entities["crate"]).z - ctx.vars["c0"];
            ctx.check(
                "impulse pushes the crate away",
                dz < -1.0,
                format!("dz {dz:.2}"),
            );
            let spent = ctx.vars["r0"] - reserve(world);
            ctx.check(
                "spell costs aether",
                spent > 10.0,
                format!("spent {spent:.1}"),
            );
            ctx.vars.remove("cast");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("levitation", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world);
            let e = spawn(world, PartKind::Crate, f + Vec3::new(1.5, 1.6, -5.0));
            ctx.entities.insert("crate2".into(), e);
        }
        let c = ctx.entities["crate2"];
        if ctx.frames == 1 {
            world.resource_mut::<SpellState>().selected = Spell::Levitate;
            world.resource_mut::<SpellState>().cooldown = 0.0;
            let pe = world
                .query_filtered::<Entity, With<Player>>()
                .iter(world)
                .next()
                .unwrap();
            if let Some(mut r) = world.get_mut::<AetherReserve>(pe) {
                r.amount = r.max;
            }
        }
        if ctx.frames < 10 || ctx.t < 1.0 {
            let p = pos_of(world, c);
            aim_at(world, p);
            ctx.vars.insert("ly".into(), p.y);
            return Step::Wait;
        }
        if !ctx.vars.contains_key("cast") {
            ctx.vars.insert("cast".into(), ctx.t);
            mouse(world, MouseButton::Left, true);
            return Step::Wait;
        }
        mouse(world, MouseButton::Left, false);
        if ctx.t > ctx.vars["cast"] + 2.0 {
            let dy = pos_of(world, c).y - ctx.vars["ly"];
            ctx.check(
                "levitating object floats up",
                world.get::<Levitating>(c).is_some() && dy > 0.8,
                format!("dy {dy:.2}"),
            );
            screenshot(world, "14_levitation");
            ctx.vars.remove("cast");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("agitated anomaly pushes objects", |world, ctx| {
        if ctx.frames == 1 {
            let mut q = world.query::<(&Transform, &Anomaly)>();
            let center = q.iter(world).next().map(|(t, _)| t.translation).unwrap();
            let px = (center.x - 3.5).floor() as i32;
            let pz = center.z.floor() as i32;
            let ground = world
                .resource::<VoxelWorld>()
                .surface_y(px, pz)
                .unwrap_or(0) as f32;
            let probe = spawn(
                world,
                PartKind::Crate,
                Vec3::new(px as f32 + 0.5, ground + 1.6, pz as f32 + 0.5),
            );
            info!(
                "[AUTOTEST] anomaly centre {center:.1}, probe {probe:?} at {:.1}",
                Vec3::new(px as f32 + 0.5, ground + 1.6, pz as f32 + 0.5)
            );
            ctx.entities.insert("probe".into(), probe);
            teleport(world, center + Vec3::new(14.0, 4.0, 0.0));
        }
        if ctx.frames == 20 {
            ctx.vars
                .insert("p0x".into(), pos_of(world, ctx.entities["probe"]).x);
            ctx.vars
                .insert("p0z".into(), pos_of(world, ctx.entities["probe"]).z);
            let mut q = world.query::<(&Transform, &Anomaly)>();
            let center = q.iter(world).next().map(|(t, _)| t.translation).unwrap();
            aim_at(world, center + Vec3::Y * 2.0);
        }
        if ctx.frames > 20 {
            let v = world
                .get::<LinearVelocity>(ctx.entities["probe"])
                .map(|v| v.0.length())
                .unwrap_or(0.0);
            let mv = ctx.vars.get("maxv").copied().unwrap_or(0.0).max(v);
            ctx.vars.insert("maxv".into(), mv);
            let p = pos_of(world, ctx.entities["probe"]);
            let p0 = Vec3::new(ctx.vars["p0x"], p.y, ctx.vars["p0z"]);
            let d = p.distance(p0) + (p.y - ctx.vars.get("p0y").copied().unwrap_or(p.y)).abs();
            let m = ctx.vars.get("maxd").copied().unwrap_or(0.0).max(d);
            ctx.vars.insert("maxd".into(), m);
        }
        if ctx.frames == 20 {
            let y = pos_of(world, ctx.entities["probe"]).y;
            ctx.vars.insert("p0y".into(), y);
        }
        if ctx.frames > 20 && ctx.t > 15.0 {
            let moved = ctx.vars.get("maxd").copied().unwrap_or(0.0);
            let mv = ctx.vars.get("maxv").copied().unwrap_or(0.0);
            ctx.check(
                "anomaly pulse pushed the object",
                mv > 1.5 || moved > 0.5,
                format!("peak speed {mv:.2} m/s, max displacement {moved:.2} m"),
            );
            screenshot(world, "15_anomaly");
            return Step::Next;
        }
        Step::Wait
    }));
}
