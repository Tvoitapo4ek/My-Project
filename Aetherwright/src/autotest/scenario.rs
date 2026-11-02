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
    world::{
        chunks::{EditCause, VoxelWorld},
        interaction::{BlockEdit, BlockTarget},
        voxel::{blocks, BlockId},
        ChunkEntities, LoadProgress,
    },
};

pub(super) fn step(
    name: &str,
    f: impl FnMut(&mut World, &mut Ctx) -> Step + Send + Sync + 'static,
) -> (String, StepFn) {
    (name.to_string(), Box::new(f))
}

pub(super) fn player_pos(world: &mut World) -> Option<Vec3> {
    let mut q = world.query_filtered::<&Position, With<Player>>();
    q.iter(world).next().map(|p| p.0)
}

pub(super) fn player_state(world: &mut World) -> Option<CharacterState> {
    let mut q = world.query_filtered::<&CharacterState, With<Player>>();
    q.iter(world).next().cloned()
}

pub(super) fn set_intent(world: &mut World, f: impl FnOnce(&mut PlayerIntent)) {
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
            // Back to the (open, flat) spawn so terrain does not interfere.
            let spawn = world.resource::<crate::core::resources::SpawnPoint>().0;
            teleport(world, spawn);
        }
        if ctx.frames == 30 {
            let y = player_pos(world).unwrap().y;
            ctx.vars.insert("y0".into(), y);
            ctx.vars.insert("ymax".into(), y);
            ctx.vars.insert("tj".into(), ctx.t);
            set_intent(world, |i| i.jump_pressed = true);
        }
        if ctx.frames < 30 {
            return Step::Wait;
        }
        let y = player_pos(world).unwrap().y;
        let m = ctx.vars["ymax"].max(y);
        ctx.vars.insert("ymax".into(), m);
        if ctx.t > ctx.vars["tj"] + 1.6 {
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

    stage2(&mut steps);
    super::scenario3::stage3(&mut steps);
    super::scenario4::stage4(&mut steps);
    super::scenario5::stage5(&mut steps);
    super::scenario6::stage6(&mut steps);

    Scenario {
        steps,
        index: 0,
        ctx: Ctx::default(),
        elapsed_total: 0.0,
    }
}

pub(super) fn edit(world: &mut World, pos: IVec3, block: BlockId, cause: EditCause) {
    world.write_message(BlockEdit { pos, block, cause });
}

pub(super) fn block_at(world: &World, pos: IVec3) -> BlockId {
    world.resource::<VoxelWorld>().get(pos)
}

/// Physics ray straight down; returns the hit distance.
fn ray_down(world: &mut World, from: Vec3) -> Option<f32> {
    let mut state = bevy::ecs::system::SystemState::<SpatialQuery>::new(world);
    let spatial = state.get(world).expect("spatial query");
    let filter = SpatialQueryFilter::from_mask(crate::core::layers::Layer::Terrain);
    spatial.cast_ray(from, Dir3::NEG_Y, 100.0, true, &filter).map(|h| h.distance)
}

pub(super) fn teleport(world: &mut World, p: Vec3) {
    let mut q = world.query_filtered::<(&mut Position, &mut LinearVelocity), With<Player>>();
    for (mut pos, mut vel) in q.iter_mut(world) {
        pos.0 = p;
        vel.0 = Vec3::ZERO;
    }
}

fn stage2(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("world generated", |world, ctx| {
        let progress = world.resource::<LoadProgress>().clone();
        let chunks = world.resource::<ChunkEntities>().0.len();
        ctx.check("all chunks generated + meshed", progress.total > 0 && chunks == progress.total, format!("{chunks}/{}", progress.total));
        let w = world.resource::<VoxelWorld>();
        let mut kinds = std::collections::HashSet::new();
        for c in w.chunks.values() {
            for b in &c.blocks {
                kinds.insert(*b);
            }
        }
        ctx.check("several materials present", kinds.len() >= 15, format!("{} block types", kinds.len()));
        Step::Next
    }));
    steps.push(step("break a block", |world, ctx| {
        if ctx.frames == 1 {
            let p = player_pos(world).unwrap();
            let x = p.x.floor() as i32 + 3;
            let z = p.z.floor() as i32;
            let w = world.resource::<VoxelWorld>();
            let y = (0..w.size_blocks().y).rev().find(|&y| w.get(IVec3::new(x, y, z)).is_solid()).unwrap();
            ctx.vars.insert("bx".into(), x as f32);
            ctx.vars.insert("by".into(), y as f32);
            ctx.vars.insert("bz".into(), z as f32);
            let d = ray_down(world, Vec3::new(x as f32 + 0.5, y as f32 + 5.0, z as f32 + 0.5)).unwrap_or(-1.0);
            ctx.vars.insert("ray_before".into(), d);
            edit(world, IVec3::new(x, y, z), blocks::AIR, EditCause::Mined);
        }
        if ctx.frames == 6 {
            let pos = IVec3::new(ctx.vars["bx"] as i32, ctx.vars["by"] as i32, ctx.vars["bz"] as i32);
            ctx.check("block removed from voxel data", block_at(world, pos).is_air(), format!("{:?}", block_at(world, pos)));
            let d = ray_down(world, pos.as_vec3() + Vec3::new(0.5, 5.0, 0.5)).unwrap_or(-1.0);
            let before = ctx.vars["ray_before"];
            ctx.check("collider updated after break", d > before + 0.9, format!("ray {before:.2} -> {d:.2}"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("place a block", |world, ctx| {
        let pos = IVec3::new(ctx.vars["bx"] as i32, ctx.vars["by"] as i32 + 3, ctx.vars["bz"] as i32);
        if ctx.frames == 1 {
            edit(world, pos, blocks::BRICKS, EditCause::Placed);
        }
        if ctx.frames == 6 {
            ctx.check("block placed", block_at(world, pos) == blocks::BRICKS, format!("{:?}", block_at(world, pos)));
            let d = ray_down(world, pos.as_vec3() + Vec3::new(0.5, 5.0, 0.5)).unwrap_or(-1.0);
            ctx.check("collider has the new block", (d - 4.0).abs() < 0.05, format!("ray {d:.2}"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("placement into the player is rejected", |world, ctx| {
        let p = player_pos(world).unwrap().floor().as_ivec3();
        if ctx.frames == 1 {
            ctx.vars.insert("before".into(), block_at(world, p).0 as f32);
            edit(world, p, blocks::STONE, EditCause::Placed);
        }
        if ctx.frames == 6 {
            ctx.check("occupied cell not filled", block_at(world, p).0 as f32 == ctx.vars["before"], "player cell");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("chunk border edit", |world, ctx| {
        if ctx.frames == 1 {
            let w = world.resource::<VoxelWorld>();
            let n = w.chunk_size;
            let x = n * 3; // first block of chunk x=3
            let z = n * 3 + 5;
            let y = w.surface_y(x, z).unwrap();
            ctx.vars.insert("cy".into(), y as f32);
            edit(world, IVec3::new(x, y, z), blocks::AIR, EditCause::Mined);
            edit(world, IVec3::new(x - 1, y, z), blocks::AIR, EditCause::Mined);
        }
        if ctx.frames == 6 {
            let w = world.resource::<VoxelWorld>();
            let n = w.chunk_size;
            let y = ctx.vars["cy"] as i32;
            let ok = w.get(IVec3::new(n * 3, y, n * 3 + 5)).is_air() && w.get(IVec3::new(n * 3 - 1, y, n * 3 + 5)).is_air();
            ctx.check("edits on both sides of a chunk border", ok && w.dirty.is_empty(), format!("dirty left {}", w.dirty.len()));
            let d = ray_down(world, Vec3::new(n as f32 * 3.0 - 0.5, y as f32 + 5.0, n as f32 * 3.0 + 5.5)).unwrap_or(-1.0);
            ctx.check("neighbour chunk collider updated", (d - 5.0).abs() < 0.05, format!("ray {d:.2} (expected 5.00)"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("look at a block", |world, ctx| {
        world.resource_mut::<CameraRig>().pitch = -1.2;
        if ctx.t > 0.4 {
            let t = world.resource::<BlockTarget>().0;
            ctx.check("crosshair targets a block", t.is_some(), format!("{:?}", t.map(|h| h.block)));
            world.resource_mut::<CameraRig>().pitch = -0.1;
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("landscape screenshot", |world, ctx| {
        if ctx.frames == 1 {
            let w = world.resource::<VoxelWorld>();
            let size = w.size_blocks();
            let spire = crate::world::generation::sites(&crate::world::gen_params(w)).spire;
            let from = Vec3::new(size.x as f32 * 0.45, w.sea_level as f32 + 26.0, size.z as f32 * 0.55);
            let sea = w.sea_level as f32;
            teleport(world, from);
            let mut q = world.query_filtered::<&mut CharacterState, With<Player>>();
            for mut s in q.iter_mut(world) {
                s.flying = true;
            }
            let dir = Vec3::new(spire.x, sea + 6.0, spire.y) - from;
            let mut rig = world.resource_mut::<CameraRig>();
            rig.yaw = (-dir.x).atan2(-dir.z);
            rig.pitch = -0.25;
        }
        if ctx.t > 2.5 {
            screenshot(world, "03_landscape");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("swim", |world, ctx| {
        if ctx.frames == 1 {
            let mut q = world.query_filtered::<&mut CharacterState, With<Player>>();
            for mut s in q.iter_mut(world) {
                s.flying = false;
            }
            // Find a column with ≥ 3 blocks of water.
            let w = world.resource::<VoxelWorld>();
            let size = w.size_blocks();
            let sea = w.sea_level;
            let mut found = None;
            'outer: for z in (8..size.z - 8).step_by(3) {
                for x in (8..size.x - 8).step_by(3) {
                    if (0..3).all(|d| w.get(IVec3::new(x, sea - d, z)).is_liquid()) {
                        found = Some(Vec3::new(x as f32 + 0.5, sea as f32 - 1.0, z as f32 + 0.5));
                        break 'outer;
                    }
                }
            }
            match found {
                Some(p) => {
                    teleport(world, p);
                    ctx.vars.insert("wy".into(), p.y);
                }
                None => {
                    ctx.check("found water", false, "no lake");
                    return Step::Next;
                }
            }
        }
        if (1.0..1.1).contains(&ctx.t) {
            let s = player_state(world).unwrap();
            ctx.check("player is submerged", s.submerged > 0.3, format!("{:.2}", s.submerged));
            ctx.vars.insert("wy".into(), player_pos(world).unwrap().y);
        }
        if ctx.t > 1.1 {
            set_intent(world, |i| i.jump_held = true);
        }
        if ctx.t > 2.6 {
            set_intent(world, |i| i.jump_held = false);
            let dy = player_pos(world).unwrap().y - ctx.vars["wy"];
            ctx.check("swimming up works", dy > 0.8, format!("rose {dy:.2} m"));
            screenshot(world, "04_water");
            return Step::Next;
        }
        Step::Wait
    }));
}
