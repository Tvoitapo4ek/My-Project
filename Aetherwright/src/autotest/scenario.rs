//! The end-to-end scenario. Steps are appended per development stage.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{Ctx, Scenario, Step, StepFn, screenshot};
use crate::{
    core::states::AppState,
    player::{
        Player,
        camera::{CameraMode, CameraRig},
        input::PlayerIntent,
        movement::CharacterState,
    },
};

fn step(
    name: &str,
    f: impl FnMut(&mut World, &mut Ctx) -> Step + Send + Sync + 'static,
) -> (String, StepFn) {
    (name.to_string(), Box::new(f))
}

fn player_pos(world: &mut World) -> Option<Vec3> {
    let mut q = world.query_filtered::<&Position, With<Player>>();
    q.iter(world).next().map(|p| p.0)
}

fn player_state(world: &mut World) -> Option<CharacterState> {
    let mut q = world.query_filtered::<&CharacterState, With<Player>>();
    q.iter(world).next().cloned()
}

fn set_intent(world: &mut World, f: impl FnOnce(&mut PlayerIntent)) {
    let mut intent = world.resource_mut::<PlayerIntent>();
    f(&mut intent);
}

pub fn build() -> Scenario {
    let mut steps: Vec<(String, StepFn)> = Vec::new();

    // ---- Stage 1: boot, movement, camera ------------------------------
    steps.push(step("wait for world", |world, ctx| {
        world.resource_mut::<PlayerIntent>().scripted = true;
        let in_game = *world.resource::<State<AppState>>().get() == AppState::InGame;
        if in_game && player_pos(world).is_some() && ctx.t > 2.0 {
            let s = player_state(world).unwrap();
            ctx.check(
                "player grounded after spawn",
                s.grounded,
                format!("grounded={}", s.grounded),
            );
            let p = player_pos(world).unwrap();
            ctx.vars.insert("x0".into(), p.x);
            ctx.vars.insert("z0".into(), p.z);
            screenshot(world, "01_spawn");
            Step::Next
        } else if ctx.t > 120.0 {
            ctx.check("world loaded", false, "timeout");
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("walk forward", |world, ctx| {
        world.resource_mut::<CameraRig>().yaw = 0.0;
        set_intent(world, |i| i.movement = Vec2::Y);
        if ctx.t > 1.5 {
            set_intent(world, |i| i.movement = Vec2::ZERO);
            let p = player_pos(world).unwrap();
            let moved = Vec2::new(p.x - ctx.vars["x0"], p.z - ctx.vars["z0"]).length();
            ctx.check(
                "walking moves the player",
                moved > 3.0,
                format!("moved {moved:.2} m"),
            );
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("jump", |world, ctx| {
        if ctx.frames == 1 {
            let y = player_pos(world).unwrap().y;
            ctx.vars.insert("y0".into(), y);
            ctx.vars.insert("ymax".into(), y);
            set_intent(world, |i| i.jump_pressed = true);
        }
        let y = player_pos(world).unwrap().y;
        let m = ctx.vars["ymax"].max(y);
        ctx.vars.insert("ymax".into(), m);
        if ctx.t > 1.5 {
            let h = ctx.vars["ymax"] - ctx.vars["y0"];
            ctx.check(
                "jump height ~1.2-1.8 m",
                (0.9..2.2).contains(&h),
                format!("{h:.2} m"),
            );
            let s = player_state(world).unwrap();
            ctx.check(
                "lands again",
                s.grounded,
                format!("grounded={}", s.grounded),
            );
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("crouch", |world, ctx| {
        set_intent(world, |i| i.crouch = true);
        if ctx.t > 0.5 {
            let s = player_state(world).unwrap();
            ctx.check(
                "crouch shrinks capsule",
                s.crouching && s.height < 1.5,
                format!("height {:.2}", s.height),
            );
            set_intent(world, |i| i.crouch = false);
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("stand up + external impulse", |world, ctx| {
        if ctx.frames == 1 {
            let mut q = world.query_filtered::<&mut LinearVelocity, With<Player>>();
            for mut v in q.iter_mut(world) {
                v.0 += Vec3::new(12.0, 6.0, 0.0);
            }
            let p = player_pos(world).unwrap();
            ctx.vars.insert("ix".into(), p.x);
        }
        if ctx.t > 1.0 {
            let s = player_state(world).unwrap();
            ctx.check(
                "stands up",
                !s.crouching,
                format!("crouching={}", s.crouching),
            );
            let dx = player_pos(world).unwrap().x - ctx.vars["ix"];
            ctx.check("impulse moves the player", dx > 2.0, format!("dx {dx:.2}"));
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("third person camera", |world, ctx| {
        if ctx.frames == 1 {
            let mut rig = world.resource_mut::<CameraRig>();
            rig.mode = CameraMode::ThirdPerson;
            rig.pitch = -0.35;
        }
        if ctx.t > 1.0 {
            let rig = world.resource::<CameraRig>().clone();
            ctx.check(
                "third person distance > 0",
                rig.current_distance > 0.5,
                format!("{:.2}", rig.current_distance),
            );
            screenshot(world, "02_third_person");
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("back to first person", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<CameraRig>().mode = CameraMode::FirstPerson;
        }
        if ctx.t > 0.5 { Step::Next } else { Step::Wait }
    }));

    Scenario {
        steps,
        index: 0,
        ctx: Ctx::default(),
        elapsed_total: 0.0,
    }
}
