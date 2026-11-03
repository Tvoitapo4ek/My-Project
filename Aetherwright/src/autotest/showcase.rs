//! Style showcase (`--showcase`): builds a gallery next to the spawn —
//! block samples, every kinetic machine, every magical device and logic
//! element — then photographs it from a few angles and exits. A quick way
//! to judge the visual style (docs/STYLE_GUIDE.md) without the full test.

use bevy::prelude::*;

use super::{Ctx, Scenario, Step, StepFn, scenario::{edit, step, teleport}, scenario3::aim_at, screenshot};
use crate::{
    arcana::MagicNode,
    content::registry,
    core::{resources::WorldClock, states::{AppState, InputFocus}},
    kinetics::{KineticBlock, KineticGrid, assets_ref, network::{Axis, Role}, spawn_kinetic},
    player::{Player, camera::{CameraMode, CameraRig}, input::PlayerIntent},
    world::{chunks::{EditCause, VoxelWorld}, voxel::BlockId},
};

fn origin(world: &mut World) -> IVec3 {
    let mut q = world.query_filtered::<&Transform, With<Player>>();
    let p = q.iter(world).next().map(|t| t.translation).unwrap_or_default();
    let mut o = p.floor().as_ivec3() + IVec3::new(3, 0, -6);
    // Stand the gallery on the ground at that spot.
    if let Some(y) = world.resource::<VoxelWorld>().surface_y(o.x, o.z) {
        o.y = y + 1;
    }
    o
}

pub fn build() -> Scenario {
    let mut steps: Vec<(String, StepFn)> = Vec::new();
    steps.push(step("wait for world", |world, ctx: &mut Ctx| {
        world.resource_mut::<PlayerIntent>().scripted = true;
        let in_game = *world.resource::<State<AppState>>().get() == AppState::InGame;
        if in_game && ctx.t > 2.0 {
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("build the gallery", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<WorldClock>().time_of_day = 0.36;
            world.resource_mut::<WorldClock>().paused = true;
            let o = origin(world);
            ctx.vars.insert("ox".into(), o.x as f32);
            ctx.vars.insert("oy".into(), o.y as f32);
            ctx.vars.insert("oz".into(), o.z as f32);
            // Clear and floor a 22 × 12 area.
            for x in -2..22 {
                for z in -3..10 {
                    edit(world, o + IVec3::new(x, -1, z), BlockId::from_key("planks").unwrap(), EditCause::Script);
                    for y in 0..5 {
                        edit(world, o + IVec3::new(x, y, z), BlockId(0), EditCause::Script);
                    }
                }
            }
            // Row 1: block samples on pedestals.
            let samples = ["grass", "dirt", "stone", "sand", "log", "leaves", "planks", "bricks", "iron_ore", "copper_ore", "coal_ore", "aether_crystal", "agitated_crystal", "ancient_stone", "moss_stone", "clay", "gravel", "metal_plate"];
            for (i, k) in samples.iter().enumerate() {
                if let Some(b) = BlockId::from_key(k) {
                    edit(world, o + IVec3::new(i as i32, 0, 7), b, EditCause::Script);
                }
            }
        }
        if ctx.frames == 4 {
            let o = IVec3::new(ctx.vars["ox"] as i32, ctx.vars["oy"] as i32, ctx.vars["oz"] as i32);
            let a = assets_ref(world).unwrap();
            let mut c = world.commands();
            // Row 2: machines (a running drivetrain: crank → shaft → cogs).
            let mech = ["hand_crank", "shaft", "cogwheel"];
            for (i, k) in mech.iter().enumerate() {
                spawn_kinetic(&mut c, &a, k, o + IVec3::new(i as i32, 0, 3), Axis::X);
            }
            spawn_kinetic(&mut c, &a, "cogwheel", o + IVec3::new(2, 1, 3), Axis::X);
            spawn_kinetic(&mut c, &a, "large_cogwheel", o + IVec3::new(2, 2, 4), Axis::X);
            let others = ["gearbox", "clutch", "gearshift", "electric_motor", "electric_generator", "bearing"];
            for (i, k) in others.iter().enumerate() {
                spawn_kinetic(&mut c, &a, k, o + IVec3::new(5 + i as i32 * 2, 0, 3), Axis::Y);
            }
            // Row 3: magic, row 0: logic.
            let mut mi = 0;
            let mut li = 0;
            for k in &registry().kinetics {
                if registry().magic(&k.key).is_some() {
                    spawn_kinetic(&mut c, &a, &k.key, o + IVec3::new(mi * 2, 0, -1), if k.role == Role::Motor { Axis::Z } else { Axis::Y });
                    mi += 1;
                } else if registry().logic(&k.key).is_some() && li < 10 {
                    spawn_kinetic(&mut c, &a, &k.key, o + IVec3::new(li * 2, 0, 1), Axis::Y);
                    li += 1;
                }
            }
            world.flush();
        }
        if ctx.frames == 8 {
            // Fill magic devices so they glow and spark; turn the crank.
            let mut q = world.query::<&mut MagicNode>();
            for mut n in q.iter_mut(world) {
                if let Some(d) = n.def() {
                    n.amount = d.capacity * 0.8;
                }
            }
            let mut kq = world.query::<&mut KineticBlock>();
            for mut k in kq.iter_mut(world) {
                k.crank = 1e6;
            }
            world.resource_mut::<KineticGrid>().dirty = true;
        }
        if ctx.t > 3.0 && ctx.frames > 10 {
            return Step::Next;
        }
        Step::Wait
    }));
    for (name, from, to) in [
        ("50_showcase_overview", Vec3::new(9.0, 6.5, 14.0), Vec3::new(9.0, 0.5, 2.0)),
        ("51_showcase_machines", Vec3::new(4.0, 2.4, 7.0), Vec3::new(4.0, 0.6, 3.5)),
        ("52_showcase_magic", Vec3::new(6.0, 2.2, 2.5), Vec3::new(6.0, 0.4, -1.0)),
        ("53_showcase_blocks", Vec3::new(8.0, 2.6, 11.5), Vec3::new(8.0, 0.5, 7.5)),
        ("54_showcase_logic", Vec3::new(9.0, 2.2, 4.0), Vec3::new(9.0, 0.3, 1.0)),
    ] {
        steps.push(step(name, move |world, ctx| {
            let o = Vec3::new(ctx.vars["ox"], ctx.vars["oy"], ctx.vars["oz"]);
            if ctx.frames == 1 {
                teleport(world, o + from - Vec3::Y * 0.7);
                let mut q = world.query_filtered::<&mut crate::player::movement::CharacterState, With<Player>>();
                for mut s in q.iter_mut(world) {
                    s.flying = true;
                }
            }
            if ctx.frames >= 2 && ctx.frames < 8 {
                aim_at(world, o + to);
            }
            if ctx.frames == 8 {
                screenshot(world, name);
            }
            if ctx.frames > 12 {
                return Step::Next;
            }
            Step::Wait
        }));
    }
    // Sky: horizon haze, gradient and blocky clouds.
    steps.push(step("55_showcase_sky", |world, ctx| {
        let o = Vec3::new(ctx.vars["ox"], ctx.vars["oy"], ctx.vars["oz"]);
        if ctx.frames >= 1 && ctx.frames < 8 {
            aim_at(world, o + Vec3::new(9.0, 30.0, -120.0));
        }
        if ctx.frames == 8 {
            screenshot(world, "55_showcase_sky");
        }
        if ctx.frames > 12 {
            return Step::Next;
        }
        Step::Wait
    }));
    // Character (third person) and a hollowling in front of it.
    steps.push(step("56_showcase_character", |world, ctx| {
        let o = Vec3::new(ctx.vars["ox"], ctx.vars["oy"], ctx.vars["oz"]);
        if ctx.frames == 1 {
            let mut q = world.query_filtered::<&mut crate::player::movement::CharacterState, With<Player>>();
            for mut s in q.iter_mut(world) {
                s.flying = false;
            }
            teleport(world, o + Vec3::new(10.5, 0.6, 5.6));
            let mut c = world.commands();
            crate::survival::creatures::spawn_hollowling(&mut c, o + Vec3::new(10.0, 0.8, 4.3));
            world.flush();
        }
        if ctx.frames >= 2 && ctx.frames < 10 {
            aim_at(world, o + Vec3::new(10.0, 0.2, 3.0));
            let mut rig = world.resource_mut::<CameraRig>();
            rig.mode = CameraMode::ThirdPerson;
            rig.pitch = -0.3;
        }
        if ctx.frames == 14 {
            screenshot(world, "56_showcase_character");
        }
        if ctx.frames > 18 {
            world.resource_mut::<CameraRig>().mode = CameraMode::FirstPerson;
            return Step::Next;
        }
        Step::Wait
    }));
    // Item icons: the inventory screen with the creative catalogue.
    steps.push(step("57_showcase_icons", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<NextState<InputFocus>>().set(InputFocus::Inventory);
        }
        if ctx.frames == 8 {
            screenshot(world, "57_showcase_icons");
        }
        if ctx.frames > 12 {
            world.resource_mut::<NextState<InputFocus>>().set(InputFocus::Gameplay);
            return Step::Next;
        }
        Step::Wait
    }));
    Scenario { steps, index: 0, ctx: Ctx::default(), elapsed_total: 0.0 }
}
