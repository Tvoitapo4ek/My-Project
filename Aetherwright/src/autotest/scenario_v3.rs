//! V3 checks: the whole magical pipeline, built from real devices:
//! crystals → collector → conductor → accumulator → rune channel →
//! stabilizer (electric) → conductor → resonator-transmitter ⇢ receiver →
//! conductor → magic engine → shaft → grinder. Then a filter rune (with the
//! real tool), a wall in the channel's way, and the save file.

use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{edit, step, teleport},
    scenario3::{aim_at, mouse, select, with_inv},
    scenario5::field,
    screenshot,
};
use crate::{
    arcana::{MagicLink, MagicNode, Resonance, network::Refusal, spawn_link},
    content::{AetherKind, RuneKind},
    electric::{ElectricState, spawn_wire},
    items::ItemId,
    kinetics::{KineticDrive, KineticGrid, KineticState, assets_ref, network::Axis, spawn_kinetic},
    machinery::components::{PartAssets, PartKind, spawn_part},
    tools::manipulator::MechanicsUse,
    world::{chunks::EditCause, voxel::blocks},
};

fn base(world: &World) -> IVec3 {
    field(world).floor().as_ivec3() + IVec3::new(1, 1, -4)
}

fn ent(ctx: &super::Ctx, k: &str) -> Entity {
    ctx.entities[k]
}

fn link_between(world: &mut World, a: Entity, b: Entity) -> Option<MagicLink> {
    let mut q = world.query::<&MagicLink>();
    q.iter(world).find(|l| (l.a == a && l.b == b) || (l.a == b && l.b == a)).cloned()
}

pub fn stage_v3(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("magic pipeline drives a machine", |world, ctx| {
        let c = base(world);
        if ctx.frames == 1 {
            for x in -1..7 {
                for y in 0..3 {
                    for z in -2..5 {
                        edit(world, c + IVec3::new(x, y, z), blocks::AIR, EditCause::Script);
                    }
                }
            }
            // A small crystal cluster: a natural source of calm magic.
            for p in [IVec3::new(-1, 0, 1), IVec3::new(0, 0, 1), IVec3::new(-1, 0, 2)] {
                edit(world, c + p, blocks::AETHER_CRYSTAL, EditCause::Script);
            }
        }
        if ctx.frames == 3 {
            let a = assets_ref(world).unwrap();
            let parts = world.remove_resource::<PartAssets>().unwrap();
            let mut cm = world.commands();
            let mut k = |key: &str, p: IVec3, axis: Axis| spawn_kinetic(&mut cm, &a, key, c + p, axis).unwrap();
            let col = k("magic_collector", IVec3::ZERO, Axis::Y);
            let acc = k("crystal_accumulator", IVec3::new(2, 0, 0), Axis::Y);
            let stab = k("magic_stabilizer", IVec3::new(2, 0, 3), Axis::Y);
            let tx = k("resonator_tx", IVec3::new(4, 0, 3), Axis::Y);
            let rx = k("resonator_rx", IVec3::new(4, 0, -1), Axis::Y);
            let eng = k("magic_engine", IVec3::new(5, 0, -1), Axis::Z);
            k("shaft", IVec3::new(5, 0, 0), Axis::Z);
            let g = spawn_part(&mut cm, &parts, PartKind::Grinder, Transform::from_translation((c + IVec3::new(5, 0, 1)).as_vec3() + Vec3::splat(0.5)));
            let cell = spawn_part(&mut cm, &parts, PartKind::PowerCell, Transform::from_translation(c.as_vec3() + Vec3::new(3.5, 0.6, 4.5)));
            spawn_link(&mut cm, col, acc, "crystal_conductor");
            spawn_link(&mut cm, acc, stab, "rune_chisel");
            spawn_link(&mut cm, stab, tx, "crystal_conductor");
            spawn_link(&mut cm, rx, eng, "crystal_conductor");
            spawn_wire(&mut cm, stab, cell, "copper_wire");
            world.flush();
            world.insert_resource(parts);
            for (n, e) in [("col", col), ("acc", acc), ("stab", stab), ("tx", tx), ("rx", rx), ("eng", eng), ("grinder", g)] {
                ctx.entities.insert(format!("v3_{n}"), e);
            }
            teleport(world, c.as_vec3() + Vec3::new(2.5, 2.2, 8.5));
            aim_at(world, c.as_vec3() + Vec3::new(2.5, 0.5, 1.5));
        }
        if ctx.frames == 8 {
            // Seed the accumulator so the far end does not wait for minutes.
            if let Some(mut n) = world.get_mut::<MagicNode>(ent(ctx, "v3_acc")) {
                n.amount = 40.0;
            }
            ctx.vars.insert("t0".into(), ctx.t);
        }
        let t0 = ctx.vars.get("t0").copied().unwrap_or(f32::MAX);
        if ctx.t > t0 + 10.0 {
            let ents = ctx.entities.clone();
            let get = |w: &World, k: &str| w.get::<MagicNode>(ents[&format!("v3_{k}")]).cloned();
            let col = get(world, "col").unwrap();
            ctx.check("collector gathers natural magic", col.amount > 0.1 && col.quality.kind == AetherKind::Calm, format!("{:.2} {:?}", col.amount, col.quality.kind));
            let ch = link_between(world, ent(ctx, "v3_acc"), ent(ctx, "v3_stab")).unwrap();
            ctx.check("rune channel carries magic", ch.flow > 0.0 && !ch.blocked, format!("{:.2}/s blocked={}", ch.flow, ch.blocked));
            let power = world.get::<ElectricState>(ent(ctx, "v3_stab")).map(|s| s.satisfaction).unwrap_or(0.0);
            ctx.check("stabilizer runs on electricity", power >= 0.5, format!("{power:.2}"));
            let res = world.resource::<Resonance>().0.iter().any(|(a, b, f)| *a == ent(ctx, "v3_tx") && *b == ent(ctx, "v3_rx") && *f > 0.0);
            ctx.check("resonance transmits wirelessly", res, format!("{:?}", world.resource::<Resonance>().0));
            let eng = get(world, "eng").unwrap();
            ctx.check("magic engine runs on aether", eng.output > 0.05, format!("{:.2}", eng.output));
            let rpm = world.resource::<KineticGrid>().blocks.get(&(c + IVec3::new(5, 0, 0))).and_then(|e| world.get::<KineticState>(*e)).map(|s| s.rpm).unwrap_or(0.0);
            let drive = world.get::<KineticDrive>(ent(ctx, "v3_grinder")).map(|d| d.0).unwrap_or(0.0);
            ctx.check("magic turns the shaft and the grinder", rpm.abs() > 1.0 && drive > 1.0, format!("shaft {rpm:.1} rpm, grinder {drive:.1}"));
            screenshot(world, "36_magic_pipeline");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("filter rune and a blocked channel stop the flow", |world, ctx| {
        let c = base(world);
        let stab = ent(ctx, "v3_stab");
        if ctx.frames == 1 {
            with_inv(world, |i| {
                i.add(ItemId::k("rune_filter"), 1);
            });
            select(world, "rune_filter");
            // From the west: nothing stands between the player and it.
            teleport(world, c.as_vec3() + Vec3::new(-0.5, 1.4, 3.5));
        }
        if ctx.frames == 5 {
            aim_at(world, c.as_vec3() + Vec3::new(2.5, 0.5, 3.5));
        }
        if ctx.frames == 9 {
            mouse(world, MouseButton::Right, true);
        }
        if ctx.frames == 10 {
            mouse(world, MouseButton::Right, false);
        }
        if ctx.frames == 14 {
            let has = world.get::<MagicNode>(stab).is_some_and(|n| n.rune(RuneKind::Filter).is_some());
            let aimed = world.resource::<crate::tools::EntityTarget>().entity;
            let name = aimed.and_then(|e| world.get::<Name>(e)).map(|n| n.as_str().to_string());
            ctx.check("rune inscribed with the tool", has, format!("aimed at {name:?}"));
            // Mechanics LMB: filter → agitated only (calm refused).
            world.write_message(MechanicsUse { entity: stab, secondary: false });
        }
        if ctx.frames == 24 {
            let ch = link_between(world, ent(ctx, "v3_acc"), stab).unwrap();
            ctx.check("filter refuses the wrong kind", ch.refused == Some(Refusal::Kind), format!("{:?}", ch.refused));
            // Remove the rune effect again, then put a wall in the channel.
            if let Some(mut n) = world.get_mut::<MagicNode>(stab) {
                n.runes.clear();
            }
            for y in 0..2 {
                edit(world, c + IVec3::new(2, y, 2), blocks::STONE, EditCause::Script);
            }
        }
        if ctx.frames == 40 {
            let ch = link_between(world, ent(ctx, "v3_acc"), stab).unwrap();
            ctx.check("wall interrupts the rune channel", ch.blocked && ch.flow == 0.0, format!("blocked={} flow={:.2}", ch.blocked, ch.flow));
            let save = crate::save::world_save::capture(world);
            let a = save.extensions.get("arcana");
            let nodes = a.and_then(|v| v.get("nodes")).and_then(|v| v.as_array()).map(|v| v.len()).unwrap_or(0);
            let links = a.and_then(|v| v.get("links")).and_then(|v| v.as_array()).map(|v| v.len()).unwrap_or(0);
            ctx.check("magic devices and links are saved", nodes == 6 && links == 4, format!("{nodes} nodes, {links} links"));
            return Step::Next;
        }
        Step::Wait
    }));
}
