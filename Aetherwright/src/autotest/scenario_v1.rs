//! V1 checks: the kinetic network. A drivetrain is built on the grid
//! (motor → shaft → clutch → cogs → shaft → grinder), then speed, gear
//! reversal, power use, real grinding, the clutch, overload, removal,
//! saving and placement with the real tool are verified.

use bevy::prelude::*;

use super::{
    Step, StepFn,
    scenario::{edit, step, teleport},
    scenario3::{aim_at, mouse, select, with_inv},
    scenario5::field,
    screenshot,
};
use crate::{
    crafting::stations::Machine,
    items::{ItemId, ItemStack},
    kinetics::{KineticBlock, KineticConsumer, KineticDrive, KineticGrid, KineticState, assets_ref, describe_block, network::Axis, spawn_kinetic},
    machinery::components::{PartAssets, PartKind, PowerCell, spawn_part},
    physics::rigid_bodies::CollectProp,
    tools::manipulator::MechanicsUse,
    world::{chunks::EditCause, voxel::blocks},
};

fn base(world: &World) -> IVec3 {
    field(world).floor().as_ivec3() + IVec3::new(-5, 1, -5)
}

fn rpm_at(world: &mut World, cell: IVec3) -> f32 {
    let e = world.resource::<KineticGrid>().blocks.get(&cell).copied();
    e.and_then(|e| world.get::<KineticState>(e)).map(|s| s.rpm).unwrap_or(f32::NAN)
}

fn block_at(world: &World, cell: IVec3) -> Option<Entity> {
    world.resource::<KineticGrid>().blocks.get(&cell).copied()
}

pub fn stage_v1(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("kinetic drivetrain turns a grinder", |world, ctx| {
        let b = base(world);
        if ctx.frames == 1 {
            // Clear a strip on the stone floor.
            for x in -1..8 {
                for y in 0..4 {
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
            spawn_kinetic(&mut c, &a, "electric_motor", b, Axis::X);
            spawn_kinetic(&mut c, &a, "shaft", b + IVec3::X, Axis::X);
            spawn_kinetic(&mut c, &a, "clutch", b + IVec3::X * 2, Axis::X);
            spawn_kinetic(&mut c, &a, "cogwheel", b + IVec3::X * 3, Axis::X);
            spawn_kinetic(&mut c, &a, "cogwheel", b + IVec3::new(3, 1, 0), Axis::X);
            spawn_kinetic(&mut c, &a, "shaft", b + IVec3::new(4, 1, 0), Axis::X);
            let g = spawn_part(&mut c, &parts, PartKind::Grinder, Transform::from_translation((b + IVec3::new(5, 1, 0)).as_vec3() + Vec3::splat(0.5)));
            let cell = spawn_part(&mut c, &parts, PartKind::PowerCell, Transform::from_translation(b.as_vec3() + Vec3::new(0.5, 0.6, 1.5)));
            world.flush();
            world.insert_resource(parts);
            ctx.entities.insert("v1_grinder".into(), g);
            ctx.entities.insert("v1_cell".into(), cell);
            teleport(world, b.as_vec3() + Vec3::new(2.5, 2.2, 4.5));
            aim_at(world, b.as_vec3() + Vec3::new(2.8, 1.0, 0.5));
        }
        if ctx.frames == 20 {
            let g = ctx.entities["v1_grinder"];
            if let Some(mut m) = world.get_mut::<Machine>(g) {
                m.input = Some(ItemStack::new(ItemId::k("stone"), 1));
                m.output = None;
            }
            ctx.vars.insert("e0".into(), world.get::<PowerCell>(ctx.entities["v1_cell"]).map(|c| c.energy).unwrap_or(0.0));
            ctx.vars.insert("t_in".into(), ctx.t);
        }
        // One grinding operation takes ~2 s of game time at 64 RPM.
        let ground = ctx.frames > 20
            && world.get::<Machine>(ctx.entities["v1_grinder"]).is_some_and(|m| m.input.is_none());
        if ctx.frames > 30 && (ground || ctx.vars.get("t_in").is_some_and(|t| ctx.t > t + 8.0)) {
            let m1 = rpm_at(world, b + IVec3::X);
            let c1 = rpm_at(world, b + IVec3::X * 3);
            let c2 = rpm_at(world, b + IVec3::new(3, 1, 0));
            ctx.check("motor drives the shaft", m1 == 64.0, format!("{m1}"));
            ctx.check("meshing cogs reverse", c1 == 64.0 && c2 == -64.0, format!("{c1} / {c2}"));
            let g = ctx.entities["v1_grinder"];
            let drive = world.get::<KineticDrive>(g).map(|d| d.0).unwrap_or(0.0);
            ctx.check("grinder receives rotation", drive == 64.0, format!("{drive}"));
            // The result may already have been moved into a crate nearby
            // (hopper automation): the stone being processed is the proof.
            let m = world.get::<Machine>(g).cloned();
            let worked = m.as_ref().is_some_and(|m| {
                m.input.is_none() || m.output.is_some_and(|o| o.item == ItemId::k("gravel"))
            });
            ctx.check(
                "grinder works from the network",
                worked,
                format!("{:?}", m.map(|m| (m.input, m.output, m.progress))),
            );
            let e1 = world.get::<PowerCell>(ctx.entities["v1_cell"]).map(|c| c.energy).unwrap_or(0.0);
            ctx.check("motor uses energy for the work", e1 < ctx.vars["e0"], format!("{:.2} → {e1:.2}", ctx.vars["e0"]));
            screenshot(world, "33_kinetics");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("clutch, overload and diagnostics", |world, ctx| {
        let b = base(world);
        let g = ctx.entities["v1_grinder"];
        if ctx.frames == 1 {
            let clutch = block_at(world, b + IVec3::X * 2).unwrap();
            world.write_message(MechanicsUse { entity: clutch, secondary: false });
        }
        if ctx.frames == 8 {
            let drive = world.get::<KineticDrive>(g).map(|d| d.0).unwrap_or(-1.0);
            ctx.check("open clutch stops the grinder", drive == 0.0, format!("{drive}"));
            let clutch = block_at(world, b + IVec3::X * 2).unwrap();
            world.write_message(MechanicsUse { entity: clutch, secondary: false });
            // A very heavy machine: the motor cannot carry it.
            world.get_mut::<KineticConsumer>(g).unwrap().impact = 60.0;
            world.resource_mut::<KineticGrid>().dirty = true;
        }
        if ctx.frames == 16 {
            let shaft = block_at(world, b + IVec3::X).unwrap();
            let (_, _, diag) = describe_block(world, shaft).unwrap_or_default();
            let stalled = rpm_at(world, b + IVec3::X) == 0.0;
            ctx.check("overloaded network stalls", stalled, format!("{}", rpm_at(world, b + IVec3::X)));
            ctx.check("diagnostics explain the overload", diag.iter().any(|d| d.contains("Перегрузка")), format!("{diag:?}"));
            world.get_mut::<KineticConsumer>(g).unwrap().impact = 2.0;
            world.resource_mut::<KineticGrid>().dirty = true;
        }
        if ctx.frames == 24 {
            let r = rpm_at(world, b + IVec3::X);
            ctx.check("normal load runs again", r == 64.0, format!("{r}"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("kinetic blocks are saved, removed and placed with the tool", |world, ctx| {
        let b = base(world);
        if ctx.frames == 1 {
            let save = crate::save::world_save::capture(world);
            let n = save.extensions.get("kinetics").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            ctx.check("drivetrain in the save file", n == 6, format!("{n}"));
            // Mining the upper cog splits the network.
            let cog = block_at(world, b + IVec3::new(3, 1, 0)).unwrap();
            world.write_message(CollectProp(cog));
        }
        if ctx.frames == 8 {
            let gone = block_at(world, b + IVec3::new(3, 1, 0)).is_none();
            let drive = world.get::<KineticDrive>(ctx.entities["v1_grinder"]).map(|d| d.0).unwrap_or(-1.0);
            ctx.check("removed cog leaves the network", gone && drive == 0.0, format!("gone={gone} drive={drive}"));
            // Place a drive shaft with the real tool on top of the motor.
            with_inv(world, |i| {
                i.add(ItemId::k("drive_shaft"), 1);
            });
            select(world, "drive_shaft");
            teleport(world, b.as_vec3() + Vec3::new(1.5, 1.6, 3.0));
        }
        if ctx.frames == 10 {
            aim_at(world, b.as_vec3() + Vec3::new(0.5, 0.95, 0.5));
        }
        if ctx.frames == 14 {
            mouse(world, MouseButton::Right, true);
        }
        if ctx.frames == 15 {
            mouse(world, MouseButton::Right, false);
        }
        if ctx.frames == 22 {
            let e = block_at(world, b + IVec3::Y);
            let ok = e.and_then(|e| world.get::<KineticBlock>(e)).is_some_and(|k| k.key == "shaft");
            ctx.check("tool places a kinetic block on the grid", ok, format!("{e:?}"));
            return Step::Next;
        }
        Step::Wait
    }));
}
