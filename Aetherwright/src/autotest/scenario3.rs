//! Stage 3 checks: inventory, drops, pickup, manipulator modes and modules.

use bevy::prelude::*;

use super::{
    Ctx, Step, StepFn,
    scenario::{block_at, edit, player_pos, step, teleport},
    screenshot,
};
use crate::{
    core::{
        resources::{EyeRay, SpawnPoint},
        states::InputFocus,
    },
    items::{
        ItemId,
        drops::{ItemDrop, PickupSettings},
        inventory::Inventory,
    },
    player::{Player, camera::CameraRig},
    tools::{
        manipulator::{Manipulator, ManipulatorMode},
        scanning::ScanResult,
    },
    world::{
        chunks::{EditCause, VoxelWorld},
        voxel::{BlockId, blocks},
    },
};

pub(super) fn inv(world: &mut World) -> Inventory {
    let mut q = world.query_filtered::<&Inventory, With<Player>>();
    q.iter(world).next().cloned().unwrap()
}

pub(super) fn with_inv(world: &mut World, f: impl FnOnce(&mut Inventory)) {
    let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
    if let Some(mut i) = q.iter_mut(world).next() {
        f(&mut i);
    }
}

pub(super) fn with_manip(world: &mut World, f: impl FnOnce(&mut Manipulator)) {
    let mut q = world.query_filtered::<&mut Manipulator, With<Player>>();
    if let Some(mut m) = q.iter_mut(world).next() {
        f(&mut m);
    }
}

pub(super) fn select(world: &mut World, key: &str) {
    let item = ItemId::k(key);
    with_inv(world, |inv| {
        if let Some(i) = inv
            .slots
            .iter()
            .position(|s| s.is_some_and(|s| s.item == item))
        {
            if i >= 9 {
                inv.slots.swap(i, 8);
                inv.selected = 8;
            } else {
                inv.selected = i;
            }
        } else {
            inv.slots[8] = Some(crate::items::ItemStack::new(item, 1));
            inv.selected = 8;
        }
    });
}

/// Points the camera at a world point (from the current eye position).
pub(super) fn aim_at(world: &mut World, target: Vec3) {
    let eye = world.resource::<EyeRay>().origin;
    let d = (target - eye).normalize();
    let mut rig = world.resource_mut::<CameraRig>();
    rig.yaw = (-d.x).atan2(-d.z);
    rig.pitch = d.y.asin();
}

pub(super) fn mouse(world: &mut World, button: MouseButton, down: bool) {
    let mut m = world.resource_mut::<ButtonInput<MouseButton>>();
    if down {
        m.press(button);
    } else {
        m.release(button);
    }
}

pub(super) fn key(world: &mut World, k: KeyCode, down: bool) {
    let mut m = world.resource_mut::<ButtonInput<KeyCode>>();
    if down {
        m.press(k);
    } else {
        m.release(k);
    }
}

/// Test block two cells east of spawn, at ground level.
fn test_cell(world: &World) -> IVec3 {
    let s = world.resource::<SpawnPoint>().0;
    let w = world.resource::<VoxelWorld>();
    let x = s.x.floor() as i32 + 2;
    let z = s.z.floor() as i32;
    let y = (0..w.size_blocks().y)
        .rev()
        .find(|&y| w.get(IVec3::new(x, y, z)).is_solid())
        .unwrap();
    IVec3::new(x, y, z)
}

fn count(world: &mut World, key: &str) -> u32 {
    inv(world).count(ItemId::k(key))
}

/// Mines `cell` with the current tool: aims, holds LMB until it is gone or
/// the timeout passes. Returns Some(success) when finished.
fn mine(world: &mut World, ctx: &mut Ctx, cell: IVec3, timeout: f32) -> Option<bool> {
    aim_at(world, cell.as_vec3() + Vec3::new(0.5, 0.95, 0.5));
    if ctx.frames > 3 {
        mouse(world, MouseButton::Left, true);
    }
    let gone = block_at(world, cell).is_air();
    if gone || ctx.t > timeout {
        mouse(world, MouseButton::Left, false);
        return Some(gone);
    }
    None
}

pub fn stage3(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("prepare test area", |world, ctx| {
        if ctx.frames == 1 {
            let spawn = world.resource::<SpawnPoint>().0;
            teleport(world, spawn);
            world.resource_mut::<PickupSettings>().auto = true;
            // Clear plants around the test cells.
            let c = test_cell(world);
            for dz in -2..=2 {
                for dx in -3..=3 {
                    let p = IVec3::new(c.x + dx, c.y + 1, c.z + dz);
                    if block_at(world, p).def().render == crate::world::voxel::RenderKind::Cross {
                        edit(world, p, blocks::AIR, EditCause::Script);
                    }
                }
            }
            let i = inv(world);
            ctx.check(
                "starter inventory",
                i.count(ItemId::k("manipulator")) == 1 && i.count(ItemId::k("planks")) == 16,
                "manipulator + planks",
            );
        }
        if ctx.t > 0.8 { Step::Next } else { Step::Wait }
    }));
    steps.push(step("mine with the manipulator", |world, ctx| {
        if ctx.frames == 1 {
            select(world, "manipulator");
            with_manip(world, |m| m.mode = ManipulatorMode::Mine);
            let c = test_cell(world);
            ctx.vars.insert("cx".into(), c.x as f32);
            ctx.vars.insert("cy".into(), c.y as f32);
            ctx.vars.insert("cz".into(), c.z as f32);
            let drop = crate::items::block_drop(block_at(world, c))
                .map(|i| i.def().key)
                .unwrap_or("dirt");
            ctx.vars.insert("before".into(), count(world, drop) as f32);
            ctx.vars.insert("drop".into(), ItemId::k(drop).0 as f32);
        }
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32,
            ctx.vars["cz"] as i32,
        );
        match mine(world, ctx, c, 6.0) {
            Some(ok) => {
                ctx.check(
                    "hold-to-mine removes the block",
                    ok,
                    format!("after {:.2}s", ctx.t),
                );
                ctx.check(
                    "mining is not instant in survival",
                    ctx.t > 0.3,
                    format!("{:.2}s", ctx.t),
                );
                Step::Next
            }
            None => Step::Wait,
        }
    }));
    steps.push(step("auto-pickup of the drop", |world, ctx| {
        let item = ItemId(ctx.vars["drop"] as u16);
        let n = inv(world).count(item) as f32;
        if n > ctx.vars["before"] || ctx.t > 4.0 {
            ctx.check(
                "drop collected automatically",
                n > ctx.vars["before"],
                format!("{} {} → {}", item.def().name, ctx.vars["before"], n),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("place a block from the hotbar", |world, ctx| {
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32,
            ctx.vars["cz"] as i32,
        );
        if ctx.frames == 1 {
            select(world, "planks");
            ctx.vars
                .insert("planks".into(), count(world, "planks") as f32);
        }
        aim_at(world, c.as_vec3() + Vec3::new(0.5, 0.05, 0.5));
        if ctx.frames == 5 {
            // Whatever face the crosshair is on decides the cell.
            let Some(hit) = world.resource::<crate::world::interaction::BlockTarget>().0 else {
                ctx.check("has a target for placement", false, "none");
                return Step::Next;
            };
            let p = if hit.block.is_replaceable() {
                hit.pos
            } else {
                hit.place_pos()
            };
            ctx.vars.insert("cx".into(), p.x as f32);
            ctx.vars.insert("cy".into(), p.y as f32);
            ctx.vars.insert("cz".into(), p.z as f32);
            mouse(world, MouseButton::Right, true);
        }
        if ctx.frames == 6 {
            mouse(world, MouseButton::Right, false);
        }
        if ctx.frames == 14 {
            ctx.check(
                "block placed at the targeted face",
                block_at(world, c) == blocks::PLANKS,
                format!("{:?}", block_at(world, c).def().key),
            );
            let now = count(world, "planks") as f32;
            ctx.check(
                "placing consumes the item",
                now == ctx.vars["planks"] - 1.0,
                format!("{} → {now}", ctx.vars["planks"]),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("scanner", |world, ctx| {
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32,
            ctx.vars["cz"] as i32,
        );
        if ctx.frames == 1 {
            select(world, "manipulator");
            with_manip(world, |m| m.mode = ManipulatorMode::Scan);
            world.insert_resource(ScanResult::default());
        }
        aim_at(world, c.as_vec3() + Vec3::new(0.5, 0.95, 0.5));
        if ctx.frames == 5 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 6 {
            mouse(world, MouseButton::Left, false);
        }
        if ctx.frames == 12 {
            let r = world.resource::<ScanResult>().clone();
            ctx.check(
                "scan describes the block",
                r.title == "Доски" && r.lines.len() >= 2,
                format!("{} / {} lines", r.title, r.lines.len()),
            );
            screenshot(world, "05_scan");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("special material needs a module", |world, ctx| {
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32,
            ctx.vars["cz"] as i32,
        );
        if ctx.frames == 1 {
            edit(world, c, blocks::ANCIENT_STONE, EditCause::Script);
            with_manip(world, |m| m.mode = ManipulatorMode::Mine);
        }
        if ctx.frames < 4 {
            return Step::Wait;
        }
        match mine(world, ctx, c, 2.0) {
            Some(gone) => {
                ctx.check(
                    "tier-2 block resists the base drill",
                    !gone,
                    block_at(world, c).def().key,
                );
                Step::Next
            }
            None => Step::Wait,
        }
    }));
    steps.push(step("install drill module II", |world, ctx| {
        if ctx.frames == 1 {
            with_inv(world, |i| {
                i.add(ItemId::k("drill_module_2"), 1);
            });
            select(world, "drill_module_2");
        }
        if ctx.frames == 4 {
            mouse(world, MouseButton::Right, true);
        }
        if ctx.frames == 5 {
            mouse(world, MouseButton::Right, false);
        }
        if ctx.frames == 10 {
            let mut q = world.query_filtered::<&Manipulator, With<Player>>();
            let tier = q.iter(world).next().unwrap().drill_tier;
            ctx.check(
                "module installed",
                tier == 2 && count(world, "drill_module_2") == 0,
                format!("tier {tier}"),
            );
            select(world, "manipulator");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("mine ancient stone with module", |world, ctx| {
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32,
            ctx.vars["cz"] as i32,
        );
        match mine(world, ctx, c, 6.0) {
            Some(gone) => {
                ctx.check(
                    "module unlocks special material",
                    gone,
                    format!("{:.2}s", ctx.t),
                );
                Step::Next
            }
            None => Step::Wait,
        }
    }));
    steps.push(step("cutting needs the cutter module", |world, ctx| {
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32 - 1,
            ctx.vars["cz"] as i32,
        );
        if ctx.frames == 1 {
            with_manip(world, |m| m.mode = ManipulatorMode::Cut);
            ctx.vars
                .insert("cut_block".into(), block_at(world, c).0 as f32);
        }
        if ctx.frames < 3 {
            return Step::Wait;
        }
        match mine(world, ctx, c, 1.5) {
            Some(gone) => {
                ctx.check("no cutting without module", !gone, "block intact");
                Step::Next
            }
            None => Step::Wait,
        }
    }));
    steps.push(step("cut a block out", |world, ctx| {
        let c = IVec3::new(
            ctx.vars["cx"] as i32,
            ctx.vars["cy"] as i32 - 1,
            ctx.vars["cz"] as i32,
        );
        if ctx.frames == 1 {
            with_manip(world, |m| m.cutter = true);
        }
        match mine(world, ctx, c, 6.0) {
            Some(gone) => {
                ctx.check(
                    "cutter removes block from the terrain",
                    gone,
                    format!("{:.2}s", ctx.t),
                );
                Step::Next
            }
            None => Step::Wait,
        }
    }));
    steps.push(step("manual pickup with auto-pickup off", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<PickupSettings>().auto = false;
            let p = player_pos(world).unwrap();
            world.write_message(crate::items::drops::SpawnDrop {
                stack: crate::items::ItemStack::new(ItemId::k("coal"), 3),
                position: p + Vec3::new(1.0, 0.0, 0.0),
                velocity: Vec3::ZERO,
            });
            ctx.vars.insert("coal".into(), count(world, "coal") as f32);
        }
        if ctx.frames == 1 + 60 {
            let mut q = world.query::<&ItemDrop>();
            let lying = q.iter(world).any(|d| d.stack.item == ItemId::k("coal"));
            ctx.check(
                "item stays when auto-pickup is off",
                lying && count(world, "coal") as f32 == ctx.vars["coal"],
                "coal on the ground",
            );
            key(world, KeyCode::KeyE, true);
        }
        if ctx.frames == 62 {
            key(world, KeyCode::KeyE, false);
        }
        if ctx.frames == 70 {
            let now = count(world, "coal") as f32;
            ctx.check(
                "E picks the item up",
                now == ctx.vars["coal"] + 3.0,
                format!("coal {now}"),
            );
            world.resource_mut::<PickupSettings>().auto = true;
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("inventory screen", |world, ctx| {
        if ctx.frames == 1 {
            world
                .resource_mut::<NextState<InputFocus>>()
                .set(InputFocus::Inventory);
        }
        if ctx.t > 1.0 && ctx.vars.get("inv_shot").is_none() {
            ctx.vars.insert("inv_shot".into(), 1.0);
            screenshot(world, "06_inventory");
        }
        if ctx.t > 2.0 {
            world
                .resource_mut::<NextState<InputFocus>>()
                .set(InputFocus::Gameplay);
            let _: BlockId = blocks::AIR;
            return Step::Next;
        }
        Step::Wait
    }));
}
