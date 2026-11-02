//! Stage 9 checks: creative mode and tools, save → mutate → load roundtrip,
//! corrupt / incompatible save handling.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    Ctx, Step, StepFn,
    scenario::{block_at, edit, player_pos, step, teleport},
    scenario3::{inv, key, select, with_inv, with_manip},
    scenario5::field,
    screenshot,
};
use crate::{
    building::{
        construction::Area,
        selection::{GroupOp, PendingOps, Selection},
    },
    core::{
        resources::WorldClock,
        rules::ModeRules,
        states::{AppState, GameMode},
    },
    items::ItemId,
    machinery::components::{Engine, Part, PartAssets, PartKind, PowerCell, spawn_part},
    physics::{
        Prop,
        joints::{Connection, JointKind, LocalFrame, spawn_connection},
    },
    player::{Player, movement::CharacterState},
    save::world_save::{PendingLoad, save_to, start_load},
    survival::damage::{Damage, DamageSource, Health},
    tools::manipulator::ManipulatorMode,
    world::{chunks::EditCause, voxel::blocks},
};

fn player_e(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<Player>>()
        .iter(world)
        .next()
        .unwrap()
}

fn set_mode(world: &mut World, m: GameMode) {
    world.resource_mut::<NextState<GameMode>>().set(m);
}

fn press(world: &mut World, ctx: &Ctx, at: u32, k: KeyCode) {
    if ctx.frames == at {
        key(world, k, true);
    }
    if ctx.frames == at + 1 {
        key(world, k, false);
    }
}

fn save_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("target").join(name)
}

pub fn stage9(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("creative mode rules", |world, ctx| {
        if ctx.frames == 1 {
            set_mode(world, GameMode::Creative);
        }
        if ctx.frames == 4 {
            let rules = *world.resource::<ModeRules>();
            ctx.check(
                "creative rules applied",
                rules == ModeRules::CREATIVE,
                format!("{rules:?}"),
            );
            let e = player_e(world);
            world.get_mut::<Health>(e).unwrap().hp = 100.0;
            world.write_message(Damage {
                target: e,
                amount: 50.0,
                source: DamageSource::Creature,
            });
        }
        press(world, ctx, 8, KeyCode::KeyF);
        if ctx.frames == 14 {
            let e = player_e(world);
            ctx.check(
                "no damage in creative",
                world.get::<Health>(e).unwrap().hp == 100.0,
                "",
            );
            let flying = world.get::<CharacterState>(e).unwrap().flying;
            ctx.check("flight toggles in creative", flying, "");
        }
        press(world, ctx, 16, KeyCode::KeyF);
        if ctx.frames == 22 {
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("creative: time of day and gravity", |world, ctx| {
        if ctx.frames == 1 {
            ctx.vars
                .insert("tod".into(), world.resource::<WorldClock>().time_of_day);
            ctx.vars
                .insert("g".into(), world.resource::<Gravity>().0.length());
        }
        press(world, ctx, 3, KeyCode::BracketRight);
        press(world, ctx, 6, KeyCode::Equal);
        if ctx.frames == 10 {
            let dt = (world.resource::<WorldClock>().time_of_day - ctx.vars["tod"]).rem_euclid(1.0);
            ctx.check(
                "] advances time by an hour",
                (dt - 1.0 / 24.0).abs() < 0.01,
                format!("{dt:.3}"),
            );
            let g = world.resource::<Gravity>().0.length();
            ctx.check(
                "= doubles gravity",
                (g - ctx.vars["g"] * 2.0).abs() < 0.01,
                format!("{g:.1}"),
            );
        }
        press(world, ctx, 12, KeyCode::Digit0);
        if ctx.frames == 16 {
            let g = world.resource::<Gravity>().0.length();
            ctx.check(
                "0 resets gravity",
                (g - ctx.vars["g"]).abs() < 0.01,
                format!("{g:.1}"),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("creative: mass fill and mass delete", |world, ctx| {
        let base = field(world).floor().as_ivec3() + IVec3::new(-5, 1, -5);
        let area = Area::new(base, base + IVec3::new(2, 0, 2));
        if ctx.frames == 1 {
            select(world, "planks");
        }
        if ctx.frames == 4 {
            select(world, "manipulator");
            with_manip(world, |m| m.mode = ManipulatorMode::Select);
            let mut sel = world.resource_mut::<Selection>();
            sel.a = Some(area.min);
            sel.b = Some(area.max);
        }
        press(world, ctx, 8, KeyCode::Enter);
        if ctx.frames == 14 {
            let n = area
                .cells()
                .filter(|c| block_at(world, *c) == blocks::PLANKS)
                .count();
            ctx.check("Enter fills the selection", n == 9, format!("{n}/9"));
            world.resource_mut::<PendingOps>().0.push(GroupOp::Delete);
        }
        if ctx.frames == 20 {
            let n = area
                .cells()
                .filter(|c| !block_at(world, *c).is_air())
                .count();
            ctx.check("Delete clears it", n == 0, format!("{n} left"));
            screenshot(world, "20_creative");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("prepare a world state to save", |world, ctx| {
        if ctx.frames == 1 {
            set_mode(world, GameMode::Survival);
            let f = field(world);
            let mark = f.floor().as_ivec3() + IVec3::new(-4, 1, 4);
            edit(world, mark, blocks::BRICKS, EditCause::Script);
            let assets = world.remove_resource::<PartAssets>().unwrap();
            let mut c = world.commands();
            let engine = spawn_part(
                &mut c,
                &assets,
                PartKind::Engine,
                Transform::from_translation(f + Vec3::new(-2.0, 1.6, 4.0)),
            );
            let cell = spawn_part(
                &mut c,
                &assets,
                PartKind::PowerCell,
                Transform::from_translation(f + Vec3::new(-2.0, 2.7, 4.0)),
            );
            let wheel = spawn_part(
                &mut c,
                &assets,
                PartKind::Wheel,
                Transform::from_translation(f + Vec3::new(-2.0, 1.6, 4.7)),
            );
            let up = Quat::from_rotation_arc(Vec3::Z, Vec3::Z);
            spawn_connection(
                &mut c,
                Connection {
                    a: engine,
                    b: wheel,
                    kind: JointKind::Hinge,
                    frame_a: LocalFrame {
                        anchor: Vec3::Z * 0.5,
                        basis: up,
                    },
                    frame_b: LocalFrame {
                        anchor: Vec3::NEG_Y * 0.18,
                        basis: Quat::from_rotation_arc(Vec3::Z, Vec3::NEG_Y),
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
                        basis: Quat::IDENTITY,
                    },
                    frame_b: LocalFrame {
                        anchor: Vec3::NEG_Y * 0.5,
                        basis: Quat::IDENTITY,
                    },
                },
            );
            world.flush();
            world.insert_resource(assets);
            if let Some(mut e) = world.get_mut::<Engine>(engine) {
                e.throttle = 0.5;
                e.reverse = true;
            }
            if let Some(mut p) = world.get_mut::<PowerCell>(cell) {
                p.energy = 123.0;
            }
            with_inv(world, |i| {
                i.add(ItemId::k("copper"), 7);
            });
            world.resource_mut::<WorldClock>().time_of_day = 0.6;
            teleport(world, f + Vec3::new(-4.0, 2.2, 6.0));
            ctx.vars.insert("mx".into(), mark.x as f32);
            ctx.vars.insert("my".into(), mark.y as f32);
            ctx.vars.insert("mz".into(), mark.z as f32);
        }
        if ctx.t > 1.5 && ctx.frames > 10 {
            let mut q = world.query::<&Prop>();
            ctx.vars
                .insert("props".into(), q.iter(world).count() as f32);
            ctx.vars.insert(
                "copper".into(),
                inv(world).count(ItemId::k("copper")) as f32,
            );
            let p = player_pos(world).unwrap();
            ctx.vars.insert("px".into(), p.x);
            ctx.vars.insert("pz".into(), p.z);
            let path = save_path("autotest_save.json");
            let r = save_to(world, &path);
            ctx.check("save written", r.is_ok() && path.exists(), format!("{r:?}"));
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            info!("[AUTOTEST] save size {size} bytes");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("mutate the world after saving", |world, ctx| {
        if ctx.frames == 1 {
            let mark = IVec3::new(
                ctx.vars["mx"] as i32,
                ctx.vars["my"] as i32,
                ctx.vars["mz"] as i32,
            );
            edit(world, mark, blocks::AIR, EditCause::Script);
            let mut q = world.query_filtered::<Entity, With<Part>>();
            let parts: Vec<Entity> = q.iter(world).collect();
            for p in parts {
                world.despawn(p);
            }
            with_inv(world, |i| {
                i.remove(ItemId::k("copper"), 7);
            });
            world.resource_mut::<WorldClock>().time_of_day = 0.1;
            set_mode(world, GameMode::Creative);
            let spawn = world.resource::<crate::core::resources::SpawnPoint>().0;
            teleport(world, spawn);
        }
        if ctx.frames == 6 {
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("load restores everything", |world, ctx| {
        if ctx.frames == 1 {
            let r = start_load(world, &save_path("autotest_save.json"));
            ctx.check("load accepted", r.is_ok(), format!("{r:?}"));
        }
        let in_game = *world.resource::<State<AppState>>().get() == AppState::InGame;
        if ctx.frames > 5 && in_game && !world.contains_resource::<PendingLoad>() && ctx.t > 3.0 {
            let mark = IVec3::new(
                ctx.vars["mx"] as i32,
                ctx.vars["my"] as i32,
                ctx.vars["mz"] as i32,
            );
            ctx.check(
                "voxel edit restored",
                block_at(world, mark) == blocks::BRICKS,
                block_at(world, mark).def().key.as_str(),
            );
            let mut q = world.query::<&Prop>();
            let props = q.iter(world).count() as f32;
            ctx.check(
                "objects restored",
                props == ctx.vars["props"],
                format!("{props} vs {}", ctx.vars["props"]),
            );
            let mut eq = world.query::<(Entity, &Engine)>();
            let engines: Vec<(Entity, f32, bool)> = eq
                .iter(world)
                .map(|(e, en)| (e, en.throttle, en.reverse))
                .collect();
            ctx.check(
                "engine state restored",
                engines
                    .iter()
                    .any(|(_, t, r)| (*t - 0.5).abs() < 1e-3 && *r),
                format!("{engines:?}"),
            );
            let mut cq = world.query::<&PowerCell>();
            ctx.check(
                "power cell charge restored",
                cq.iter(world).any(|c| (c.energy - 123.0).abs() < 1.0),
                "",
            );
            let mut jq = world.query::<&Connection>();
            let hinges = jq
                .iter(world)
                .filter(|c| c.kind == JointKind::Hinge && engines.iter().any(|(e, ..)| *e == c.a))
                .count();
            ctx.check(
                "joints restored",
                hinges >= 1,
                format!("{hinges} engine hinges"),
            );
            let copper = inv(world).count(ItemId::k("copper")) as f32;
            ctx.check(
                "inventory restored",
                copper == ctx.vars["copper"],
                format!("copper {copper}"),
            );
            let p = player_pos(world).unwrap();
            let d = Vec2::new(p.x - ctx.vars["px"], p.z - ctx.vars["pz"]).length();
            ctx.check("player position restored", d < 0.5, format!("{d:.2} m off"));
            let tod = world.resource::<WorldClock>().time_of_day;
            ctx.check(
                "time of day restored",
                (tod - 0.6).abs() < 0.01,
                format!("{tod:.3}"),
            );
            ctx.check(
                "game mode restored",
                *world.resource::<State<GameMode>>().get() == GameMode::Survival,
                "",
            );
            screenshot(world, "21_after_load");
            return Step::Next;
        }
        if ctx.t > 200.0 {
            ctx.check("load finished", false, "timeout");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step(
        "corrupt and incompatible saves are rejected safely",
        |world, ctx| {
            let bad = save_path("autotest_bad.json");
            let _ = std::fs::write(
                &bad,
                "{\"format\":\"aetherwright-save\",\"version\":1,\"world\":{ broken",
            );
            let r = start_load(world, &bad);
            ctx.check("corrupt save rejected", r.is_err(), format!("{r:?}"));
            let newer = save_path("autotest_newer.json");
            let text = std::fs::read_to_string(save_path("autotest_save.json"))
                .unwrap_or_default()
                .replacen(
                    &format!("\"version\":{}", crate::save::serialization::SAVE_VERSION),
                    "\"version\":99",
                    1,
                );
            let _ = std::fs::write(&newer, text);
            let r2 = start_load(world, &newer);
            ctx.check(
                "newer-version save rejected",
                matches!(r2, Err(crate::save::serialization::LoadError::TooNew(99))),
                format!("{r2:?}"),
            );
            let mark = IVec3::new(
                ctx.vars["mx"] as i32,
                ctx.vars["my"] as i32,
                ctx.vars["mz"] as i32,
            );
            let in_game = *world.resource::<State<AppState>>().get() == AppState::InGame;
            ctx.check(
                "current world untouched after failed loads",
                in_game
                    && block_at(world, mark) == blocks::BRICKS
                    && !world.contains_resource::<PendingLoad>(),
                "",
            );
            let _ = world.query::<&RigidBody>();
            Step::Next
        },
    ));
}
