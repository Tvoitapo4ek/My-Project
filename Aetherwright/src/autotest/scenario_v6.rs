//! V6 checks: the anomaly generator needs unstable surroundings and fuel,
//! shapes a gravity anomaly (with spread), an unheld anomaly wanders, a
//! powered stabilizer holds it (predictable, tracked), objects inside lose
//! weight but keep inertia, the anomaly sensor sees it, cutting the
//! stabilizer's power loses control (message + registered consequences),
//! stopping the generator releases the anomaly, and it survives a save.

use std::sync::atomic::{AtomicU32, Ordering};

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{edit, step, teleport},
    scenario3::aim_at,
    scenario5::field,
    screenshot,
};
use crate::{
    anomaly::{AnomalyConsequences, AnomalyControlLost, AnomalyField, AnomalyGenerator, GenStatus, LossCause},
    arcana::{MagicNode, network::Quality, spawn_link},
    content::AetherKind,
    electric::spawn_wire,
    kinetics::{assets_ref, network::Axis, spawn_kinetic},
    logic::LogicNode,
    machinery::components::{PartAssets, PartKind, spawn_part},
    tools::manipulator::MechanicsUse,
    world::{chunks::EditCause, voxel::blocks},
};

fn base(world: &World) -> IVec3 {
    field(world).floor().as_ivec3() + IVec3::new(9, 1, 7)
}

static LOSSES_SEEN: AtomicU32 = AtomicU32::new(0);

fn test_consequence(_: &mut World, _: &AnomalyControlLost) {
    LOSSES_SEEN.fetch_add(1, Ordering::SeqCst);
}

fn fields(world: &mut World) -> Vec<(Entity, AnomalyField)> {
    let mut q = world.query::<(Entity, &AnomalyField)>();
    q.iter(world).map(|(e, f)| (e, f.clone())).collect()
}

fn gen_state(world: &World, ctx: &super::Ctx) -> AnomalyGenerator {
    world.get::<AnomalyGenerator>(ctx.entities["v6_gen"]).cloned().unwrap()
}

pub fn stage_v6(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("anomaly generator needs unstable surroundings", |world, ctx| {
        let c = base(world);
        if ctx.frames == 1 {
            for x in -4..6 {
                for z in -4..6 {
                    edit(world, c + IVec3::new(x, -1, z), blocks::PLANKS, EditCause::Script);
                    for y in 0..7 {
                        edit(world, c + IVec3::new(x, y, z), blocks::AIR, EditCause::Script);
                    }
                }
            }
        }
        if ctx.frames == 3 {
            let a = assets_ref(world).unwrap();
            let parts = world.remove_resource::<PartAssets>().unwrap();
            let mut cm = world.commands();
            let g = spawn_kinetic(&mut cm, &a, "anomaly_generator", c, Axis::Y).unwrap();
            let acc = spawn_kinetic(&mut cm, &a, "crystal_accumulator", c + IVec3::new(3, 0, 0), Axis::Y).unwrap();
            let sensor = spawn_kinetic(&mut cm, &a, "sensor_anomaly", c + IVec3::new(0, 0, 1), Axis::Y).unwrap();
            let cell = spawn_part(&mut cm, &parts, PartKind::PowerCell, Transform::from_translation(c.as_vec3() + Vec3::new(-2.5, 0.6, 4.5)));
            spawn_link(&mut cm, acc, g, "crystal_conductor");
            world.flush();
            world.insert_resource(parts);
            for (n, e) in [("gen", g), ("acc", acc), ("sensor", sensor), ("cell", cell)] {
                ctx.entities.insert(format!("v6_{n}"), e);
            }
            teleport(world, c.as_vec3() + Vec3::new(4.5, 1.6, 5.0));
        }
        if ctx.frames == 6 {
            let ents = ctx.entities.clone();
            // Fuel: agitated aether in the accumulator.
            if let Some(mut n) = world.get_mut::<MagicNode>(ents["v6_acc"]) {
                n.amount = 100.0;
                n.quality = Quality::raw(AetherKind::Agitated);
            }
            let placed_off = world.get::<crate::kinetics::KineticBlock>(ents["v6_gen"]).is_some_and(|k| !k.on);
            ctx.check("generator is placed switched off", placed_off, "");
            world.write_message(MechanicsUse { entity: ents["v6_gen"], secondary: false });
            ctx.vars.insert("t0".into(), ctx.t);
        }
        let t0 = ctx.vars.get("t0").copied().unwrap_or(f32::MAX);
        if ctx.t > t0 + 3.0 {
            let g = gen_state(world, ctx);
            ctx.check("calm surroundings: generator refuses to work", g.status == GenStatus::NoEnvironment && fields(world).is_empty(), format!("{:?}, instability {:.2}", g.status, g.instability));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("generator shapes a gravity anomaly", |world, ctx| {
        let c = base(world);
        let ge = ctx.entities["v6_gen"];
        if ctx.frames == 1 {
            // Agitated crystals make the place unstable.
            for p in [IVec3::new(-2, 0, -1), IVec3::new(-2, 0, 0), IVec3::new(-2, 0, 1), IVec3::new(-2, 1, 0), IVec3::new(-1, 0, -2), IVec3::new(1, 0, -2)] {
                edit(world, c + p, blocks::AGITATED_CRYSTAL, EditCause::Script);
            }
            // RMB cycles the radius.
            ctx.vars.insert("r_idx".into(), gen_state(world, ctx).radius_idx as f32);
            world.write_message(MechanicsUse { entity: ge, secondary: true });
        }
        if ctx.frames == 4 {
            let before = ctx.vars["r_idx"] as usize;
            let g = gen_state(world, ctx);
            ctx.check("RMB changes the generator radius", g.radius_idx == before + 1, format!("{before} → {}", g.radius_idx));
            // Smallest radius, inverted pull (Shift+RMB in the game).
            if let Some(mut g) = world.get_mut::<AnomalyGenerator>(ge) {
                g.radius_idx = 0;
                g.mode_idx = 2;
            }
            ctx.vars.insert("t1".into(), ctx.t);
        }
        let t1 = ctx.vars.get("t1").copied().unwrap_or(f32::MAX);
        let g = gen_state(world, ctx);
        if g.status == GenStatus::Sustaining || ctx.t > t1 + 40.0 {
            let fs = fields(world);
            let fuel_used = world.get::<MagicNode>(ctx.entities["v6_acc"]).map(|n| n.amount).unwrap_or(100.0) < 99.0;
            ctx.check("unstable surroundings: anomaly shaped", g.status == GenStatus::Sustaining && fs.len() == 1, format!("{:?}, instability {:.2}, fields {}", g.status, g.instability, fs.len()));
            ctx.check("generator burns agitated aether (not made from nothing)", fuel_used, "");
            if let Some((_, f)) = fs.first() {
                // Setting: 4 m, ×−0.6; spread 25 % of the deviation.
                ctx.check("shaped result spreads around the setting", (3.0..=5.0).contains(&f.radius) && (-1.0..=-0.2).contains(&f.gravity), format!("r {:.2}, g {:.2}", f.radius, f.gravity));
                ctx.vars.insert("gmin".into(), f32::MAX);
                ctx.vars.insert("gmax".into(), f32::MIN);
            }
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("unheld anomaly wanders", |world, ctx| {
        let fs = fields(world);
        let Some((_, f)) = fs.first() else {
            ctx.check("anomaly exists", false, "");
            return Step::Next;
        };
        let (lo, hi) = (ctx.vars["gmin"].min(f.now_gravity), ctx.vars["gmax"].max(f.now_gravity));
        ctx.vars.insert("gmin".into(), lo);
        ctx.vars.insert("gmax".into(), hi);
        if ctx.t > 5.0 {
            ctx.check("without a stabilizer the anomaly is unpredictable", hi - lo > 0.1 && f.containment == 0.0, format!("g {lo:.2}..{hi:.2}, containment {:.2}", f.containment));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("powered stabilizer holds the anomaly", |world, ctx| {
        let c = base(world);
        if ctx.frames == 1 {
            let a = assets_ref(world).unwrap();
            let mut cm = world.commands();
            let s = spawn_kinetic(&mut cm, &a, "magic_stabilizer", c + IVec3::new(-2, 0, 3), Axis::Y).unwrap();
            spawn_wire(&mut cm, s, ctx.entities["v6_cell"], "copper_wire");
            world.flush();
            ctx.entities.insert("v6_stab".into(), s);
            world.resource_mut::<AnomalyConsequences>().0.push(("autotest", test_consequence));
            LOSSES_SEEN.store(0, Ordering::SeqCst);
        }
        // The window starts once the stabilizer has power and full hold.
        let full = fields(world).first().is_some_and(|(_, f)| f.containment >= 0.999);
        if ctx.frames > 1 && (full || ctx.t > 20.0) && ctx.vars.get("h0").is_none() {
            ctx.vars.insert("h0".into(), ctx.t);
            ctx.vars.insert("gmin".into(), f32::MAX);
            ctx.vars.insert("gmax".into(), f32::MIN);
        }
        if let Some(&h0) = ctx.vars.get("h0") {
            let fs = fields(world);
            let Some((_, f)) = fs.first() else { return Step::Next };
            let (lo, hi) = (ctx.vars["gmin"].min(f.now_gravity), ctx.vars["gmax"].max(f.now_gravity));
            ctx.vars.insert("gmin".into(), lo);
            ctx.vars.insert("gmax".into(), hi);
            if ctx.t > h0 + 3.0 {
                ctx.check("stabilizer holds it (containment ≥ 60 %)", f.containment >= 0.6 && f.held, format!("{:.2}", f.containment));
                ctx.check("held anomaly is predictable and keeps its size", hi - lo < 0.02 && (f.now_radius - f.radius).abs() < 0.01, format!("g {lo:.3}..{hi:.3}, r {:.2}/{:.2}", f.now_radius, f.radius));
                ctx.check("stabilizer does not cancel the anomaly", (f.now_gravity - f.gravity).abs() < 0.01 && f.gravity < 0.0, format!("{:.2}", f.now_gravity));
                let e = ctx.entities["v6_stab"];
                let text = crate::holo::engineering::describe(world, e).map(|(_, l, _)| l).unwrap_or_default();
                ctx.check("stabilizer panel tracks the anomaly", text.contains("Удерживает аномалию"), text.clone());
                return Step::Next;
            }
        }
        Step::Wait
    }));
    steps.push(step("objects inside lose weight but keep inertia", |world, ctx| {
        let c = base(world);
        let center = c.as_vec3() + Vec3::new(0.5, 1.0, 0.5);
        if ctx.frames == 1 {
            let parts = world.remove_resource::<PartAssets>().unwrap();
            let mut cm = world.commands();
            let b = spawn_part(&mut cm, &parts, PartKind::Crate, Transform::from_translation(center + Vec3::new(0.0, -0.1, -1.4)));
            world.flush();
            world.insert_resource(parts);
            ctx.entities.insert("v6_box".into(), b);
            aim_at(world, center);
        }
        if ctx.frames == 2 {
            let y = world.get::<Position>(ctx.entities["v6_box"]).map(|p| p.0.y).unwrap_or(0.0);
            ctx.vars.insert("y_spawn".into(), y);
        }
        if ctx.frames == 5 {
            let b = ctx.entities["v6_box"];
            let y = world.get::<Position>(b).map(|p| p.0.y).unwrap_or(0.0);
            ctx.check("inverted pull lifts the crate", y > ctx.vars["y_spawn"] + 0.5, format!("y {:.2} → {y:.2}", ctx.vars["y_spawn"]));
            // A sideways push: nothing but gravity changed, so it keeps going.
            if let Some(mut v) = world.get_mut::<LinearVelocity>(b) {
                v.0 = Vec3::new(0.6, 0.0, 0.0);
            }
            ctx.vars.insert("t0".into(), ctx.t);
        }
        if ctx.frames == 12 {
            screenshot(world, "60_gravity_anomaly");
        }
        let t0 = ctx.vars.get("t0").copied().unwrap_or(f32::MAX);
        if ctx.t > t0 + 1.5 {
            let b = ctx.entities["v6_box"];
            let p = world.get::<Position>(b).map(|p| p.0).unwrap_or_default();
            let v = world.get::<LinearVelocity>(b).map(|v| v.0).unwrap_or_default();
            // It may bounce off something; it must not stop sideways.
            ctx.check("inertia stays: sideways motion continues", Vec2::new(v.x, v.z).length() > 0.3, format!("v {v:.2}"));
            // It rises to where the pull fades and floats there.
            let fs = fields(world);
            let floats = fs.first().is_some_and(|(fe, f)| world.get::<Transform>(*fe).is_some_and(|t| p.distance(t.translation) < f.now_radius + 1.0));
            ctx.check("lifted crate floats near the anomaly edge", floats, format!("{p:.2}"));
            let s = world.get::<LogicNode>(ctx.entities["v6_sensor"]).map(|n| n.gate.out).unwrap_or(0.0);
            ctx.check("anomaly sensor sees it", s > 0.5, format!("{s:.2}"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("losing stabilizer power loses control", |world, ctx| {
        if ctx.frames == 1 {
            let cell = ctx.entities["v6_cell"];
            world.entity_mut(cell).despawn();
        }
        let fs = fields(world);
        let lost = LOSSES_SEEN.load(Ordering::SeqCst);
        if lost > 0 || ctx.t > 8.0 {
            let f = fs.first().map(|(_, f)| f.clone());
            ctx.check("control lost when the stabilizer has no power", lost == 1 && f.as_ref().is_some_and(|f| !f.held), format!("losses {lost}"));
            // The cause is reported with the message.
            let mut msgs = world.resource_mut::<Messages<AnomalyControlLost>>();
            let cause = msgs.drain().last().map(|m| m.cause);
            ctx.check("loss cause: no power", cause.is_none() || cause == Some(LossCause::NoPower), format!("{cause:?}"));
            ctx.check("no consequences yet (concept open): anomaly unchanged", f.is_some_and(|f| f.generator.is_some()), "");
            world.resource_mut::<AnomalyConsequences>().0.retain(|(n, _)| *n != "autotest");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("stopped generator releases the anomaly; it is saved", |world, ctx| {
        let ge = ctx.entities["v6_gen"];
        if ctx.frames == 1 {
            world.write_message(MechanicsUse { entity: ge, secondary: false });
        }
        if ctx.frames == 6 {
            let g = gen_state(world, ctx);
            let fs = fields(world);
            let released = fs.first().is_some_and(|(_, f)| f.generator.is_none() && f.life.is_some());
            ctx.check("generator off: anomaly released (lingers or fades)", g.status == GenStatus::Off && released, format!("{:?} {:?}", g.status, fs.first().map(|(_, f)| f.life)));
            // Save round trip of the anomaly extension.
            let saved = crate::anomaly::capture(world);
            crate::anomaly::restore(world, Some(&saved));
            let after = fields(world);
            let same = after.len() == fs.len()
                && after.iter().zip(&fs).all(|((_, a), (_, b))| (a.radius - b.radius).abs() < 1e-4 && (a.gravity - b.gravity).abs() < 1e-4 && a.life.is_some());
            ctx.check("anomaly survives save and load", same, format!("{} → {}", fs.len(), after.len()));
            let g2 = gen_state(world, ctx);
            ctx.check("generator settings survive save and load", g2.radius_idx == 0 && g2.mode_idx == 2, format!("{} {}", g2.radius_idx, g2.mode_idx));
        }
        if ctx.frames == 10 {
            // Clean up: later stages must not float.
            for (e, _) in fields(world) {
                world.entity_mut(e).despawn();
            }
            let c = base(world);
            for p in [IVec3::new(-2, 0, -1), IVec3::new(-2, 0, 0), IVec3::new(-2, 0, 1), IVec3::new(-2, 1, 0), IVec3::new(-1, 0, -2), IVec3::new(1, 0, -2)] {
                edit(world, c + p, blocks::AIR, EditCause::Script);
            }
            return Step::Next;
        }
        Step::Wait
    }));
}
