//! Stage 4 checks: loose blocks and the physics gun (grab, mass limit,
//! freeze, weld, hinge, unlink, collisions while held).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    Ctx, Step, StepFn,
    scenario::{player_pos, step, teleport},
    scenario3::{aim_at, mouse, select},
    screenshot,
};
use crate::{
    core::resources::SpawnPoint,
    physics::{
        Frozen,
        joints::{Connection, JointKind},
        physics_gun::{GunMode, Held, PhysGun},
        rigid_bodies::{LooseBlock, loose_block_mesh, spawn_loose_block},
    },
    world::{
        materials::BlockMaterials,
        voxel::{BlockId, blocks},
    },
};

fn spawn_block(world: &mut World, block: BlockId, at: Vec3) -> Entity {
    let mesh = loose_block_mesh(world);
    let mats = BlockMaterials {
        by_tex: world.resource::<BlockMaterials>().by_tex.clone(),
        images: Vec::new(),
    };
    let mut commands = world.commands();
    let e = spawn_loose_block(
        &mut commands,
        mesh,
        &mats,
        block,
        Transform::from_translation(at),
        false,
    );
    world.flush();
    e
}

pub(super) fn pos_of(world: &mut World, e: Entity) -> Vec3 {
    world.get::<Position>(e).map(|p| p.0).unwrap_or(Vec3::NAN)
}

fn ent(ctx: &Ctx, k: &str) -> Entity {
    ctx.entities[k]
}

/// Ground-level point in front of the spawn (east), clear of the test hole.
fn arena(world: &World) -> Vec3 {
    let s = world.resource::<SpawnPoint>().0;
    Vec3::new(s.x, s.y - 1.0, s.z - 3.0)
}

pub(super) fn gun(world: &mut World, mode: GunMode) {
    world.resource_mut::<PhysGun>().mode = mode;
    world.resource_mut::<PhysGun>().pending = None;
}

/// Clicks LMB at a world point over a few frames (frame offset `f0`).
pub(super) fn click_at(world: &mut World, ctx: &Ctx, f0: u32, point: Vec3) -> bool {
    if ctx.frames >= f0 && ctx.frames < f0 + 3 {
        aim_at(world, point);
    }
    if ctx.frames == f0 + 2 {
        mouse(world, MouseButton::Left, true);
    }
    if ctx.frames == f0 + 3 {
        mouse(world, MouseButton::Left, false);
        return true;
    }
    false
}

pub fn stage4(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("cut block became a rigid body", |world, ctx| {
        let mut q = world.query::<(&LooseBlock, &RigidBody)>();
        let n = q.iter(world).filter(|(_, rb)| rb.is_dynamic()).count();
        ctx.check(
            "cut block is a dynamic body",
            n >= 1,
            format!("{n} loose blocks"),
        );
        Step::Next
    }));
    steps.push(step("spawn test bodies", |world, ctx| {
        if ctx.frames == 1 {
            let spawn = world.resource::<SpawnPoint>().0;
            teleport(world, spawn);
            let a = arena(world);
            // Guarantee an open arena: clear everything above the ground.
            let base = a.floor().as_ivec3();
            for y in 1..8 {
                for z in -7..=3 {
                    for x in -6..=5 {
                        let p = base + IVec3::new(x, y, z);
                        if !super::scenario::block_at(world, p).is_air() {
                            super::scenario::edit(
                                world,
                                p,
                                blocks::AIR,
                                crate::world::chunks::EditCause::Script,
                            );
                        }
                    }
                }
            }
            let light = spawn_block(world, blocks::PLANKS, a + Vec3::new(-1.5, 0.6, 0.0));
            let heavy = spawn_block(world, blocks::ANCIENT_STONE, a + Vec3::new(1.5, 0.6, 0.0));
            ctx.entities.insert("light".into(), light);
            ctx.entities.insert("heavy".into(), heavy);
            select(world, "physgun");
            gun(world, GunMode::Grab);
        }
        if ctx.t > 1.5 { Step::Next } else { Step::Wait }
    }));
    steps.push(step("grab and lift a light object", |world, ctx| {
        let light = ent(ctx, "light");
        if ctx.frames == 1 {
            ctx.vars.insert("ly0".into(), pos_of(world, light).y);
        }
        if ctx.frames < 4 {
            {
                let target = pos_of(world, light);
                aim_at(world, target);
            }
        }
        if ctx.frames == 4 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 8 {
            ctx.check(
                "object is held",
                world.get::<Held>(light).is_some(),
                "Held marker",
            );
        }
        if ctx.frames > 8 {
            // Look up: the object should follow.
            world
                .resource_mut::<crate::player::camera::CameraRig>()
                .pitch = 0.3;
        }
        if ctx.t > 2.5 {
            let dy = pos_of(world, light).y - ctx.vars["ly0"];
            ctx.check("held object lifted", dy > 1.0, format!("rose {dy:.2} m"));
            screenshot(world, "07_physgun_hold");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("freeze in mid-air", |world, ctx| {
        let light = ent(ctx, "light");
        if ctx.frames == 2 {
            mouse(world, MouseButton::Right, true);
        }
        if ctx.frames == 3 {
            mouse(world, MouseButton::Right, false);
            mouse(world, MouseButton::Left, false);
        }
        if ctx.frames > 3 && !ctx.vars.contains_key("fy") && ctx.t > 0.3 {
            ctx.vars.insert("fy".into(), pos_of(world, light).y);
        }
        if ctx.t > 1.5 && ctx.vars.contains_key("fy") {
            let rb = world.get::<RigidBody>(light).copied();
            let drift = (pos_of(world, light).y - ctx.vars["fy"]).abs();
            ctx.check(
                "frozen body is static and hangs in the air",
                rb == Some(RigidBody::Static)
                    && world.get::<Frozen>(light).is_some()
                    && drift < 0.01,
                format!("{rb:?}, drift {drift:.3}"),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("heavy object cannot be lifted", |world, ctx| {
        let heavy = ent(ctx, "heavy");
        if ctx.frames == 1 {
            world
                .resource_mut::<crate::player::camera::CameraRig>()
                .pitch = -0.3;
            ctx.vars.insert("hy0".into(), pos_of(world, heavy).y);
        }
        if ctx.frames < 5 {
            {
                let target = pos_of(world, heavy);
                aim_at(world, target);
            }
        }
        if ctx.frames == 5 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames > 8 {
            world
                .resource_mut::<crate::player::camera::CameraRig>()
                .pitch = 0.3;
        }
        if ctx.t > 2.5 {
            mouse(world, MouseButton::Left, false);
            let mass = world
                .get::<ComputedMass>(heavy)
                .map(|m| m.value())
                .unwrap_or(0.0);
            let dy = pos_of(world, heavy).y - ctx.vars["hy0"];
            ctx.check(
                "mass limits the gun (heavy stays down)",
                dy < 0.5,
                format!("{mass:.0} kg rose {dy:.2} m"),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step(
        "held object still collides with terrain",
        |world, ctx| {
            let light = ent(ctx, "light");
            if ctx.frames == 1 {
                // Unfreeze by grabbing; aim into the ground.
                mouse(world, MouseButton::Left, false);
            }
            if ctx.frames < 6 {
                {
                    let target = pos_of(world, light);
                    aim_at(world, target);
                }
            }
            if ctx.frames == 6 {
                mouse(world, MouseButton::Left, true);
            }
            if ctx.frames > 10 {
                world
                    .resource_mut::<crate::player::camera::CameraRig>()
                    .pitch = -1.2;
                if let Some(h) = world.resource_mut::<PhysGun>().held.as_mut() {
                    h.distance = 5.0;
                }
            }
            if ctx.t > 2.5 {
                let ground = world.resource::<SpawnPoint>().0.y - 1.0;
                let y = pos_of(world, light).y;
                ctx.check(
                    "body is not pushed through the ground",
                    y > ground - 0.6,
                    format!("y {y:.2}, ground top {ground:.2}"),
                );
                mouse(world, MouseButton::Left, false);
                return Step::Next;
            }
            Step::Wait
        },
    ));
    steps.push(step("weld two bodies", |world, ctx| {
        if ctx.frames == 1 {
            let a = arena(world);
            let w1 = spawn_block(world, blocks::PLANKS, a + Vec3::new(-0.5, 0.6, -2.0));
            let w2 = spawn_block(world, blocks::PLANKS, a + Vec3::new(0.52, 0.6, -2.0));
            ctx.entities.insert("w1".into(), w1);
            ctx.entities.insert("w2".into(), w2);
            gun(world, GunMode::Weld);
        }
        if ctx.frames < 40 {
            return Step::Wait;
        }
        let (w1, w2) = (ent(ctx, "w1"), ent(ctx, "w2"));
        {
            let target = pos_of(world, w1) + Vec3::Y * 0.3;
            click_at(world, ctx, 40, target);
        }
        {
            let target = pos_of(world, w2) + Vec3::Y * 0.3;
            click_at(world, ctx, 50, target);
        }
        if ctx.frames == 60 {
            let mut q = world.query::<&Connection>();
            let ok = q.iter(world).any(|c| {
                c.kind == JointKind::Weld && ((c.a == w1 && c.b == w2) || (c.a == w2 && c.b == w1))
            });
            ctx.check("weld joint created", ok, "Connection(Weld)");
            ctx.vars
                .insert("wd".into(), pos_of(world, w1).distance(pos_of(world, w2)));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("welded bodies move together", |world, ctx| {
        let (w1, w2) = (ent(ctx, "w1"), ent(ctx, "w2"));
        if ctx.frames == 1 {
            gun(world, GunMode::Grab);
            ctx.vars.insert("w2y".into(), pos_of(world, w2).y);
        }
        if ctx.frames < 5 {
            let target = (pos_of(world, w1) + pos_of(world, w2)) * 0.5 + Vec3::Y * 0.45;
            aim_at(world, target);
        }
        if ctx.frames == 5 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames > 6 && ctx.t > 0.5 {
            world
                .resource_mut::<crate::player::camera::CameraRig>()
                .pitch = 0.3;
        }
        if ctx.t > 3.5 {
            let d = pos_of(world, w1).distance(pos_of(world, w2));
            let rise = pos_of(world, w2).y - ctx.vars["w2y"];
            ctx.check(
                "partner lifted through the weld",
                rise > 0.8,
                format!("rose {rise:.2}"),
            );
            ctx.check(
                "weld keeps the distance",
                (d - ctx.vars["wd"]).abs() < 0.15,
                format!("{:.2} → {d:.2}", ctx.vars["wd"]),
            );
            screenshot(world, "08_weld");
            mouse(world, MouseButton::Left, false);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("hinge snaps a part onto a base", |world, ctx| {
        if ctx.frames == 1 {
            let a = arena(world);
            // Base frozen in the air; wheel-like block lying on the ground.
            let base = spawn_block(world, blocks::METAL_PLATE, a + Vec3::new(-3.0, 2.5, -1.0));
            world.entity_mut(base).insert((RigidBody::Static, Frozen));
            let part = spawn_block(world, blocks::PLANKS, a + Vec3::new(-3.0, 0.6, 1.0));
            ctx.entities.insert("base".into(), base);
            ctx.entities.insert("part".into(), part);
            gun(world, GunMode::Hinge);
            world
                .resource_mut::<crate::player::camera::CameraRig>()
                .pitch = -0.3;
        }
        if ctx.frames < 40 {
            return Step::Wait;
        }
        let (base, part) = (ent(ctx, "base"), ent(ctx, "part"));
        // Part: its top face. Base: its south (+Z) side face.
        {
            let target = pos_of(world, part) + Vec3::Y * 0.49;
            click_at(world, ctx, 40, target);
        }
        {
            let target = pos_of(world, base) + Vec3::new(0.0, 0.0, 0.49);
            click_at(world, ctx, 50, target);
        }
        if ctx.frames == 56 {
            let mut q = world.query::<&Connection>();
            let c = q
                .iter(world)
                .find(|c| c.kind == JointKind::Hinge && c.a == base && c.b == part)
                .cloned();
            ctx.check(
                "hinge joint created (part → base)",
                c.is_some(),
                "Connection(Hinge)",
            );
        }
        if ctx.t > 3.5 && ctx.frames > 56 {
            let pb = pos_of(world, base);
            let pp = pos_of(world, part);
            let off = pp - pb;
            ctx.check(
                "part snapped against the base face",
                off.z > 0.8 && off.z < 1.3 && off.y.abs() < 0.6,
                format!("offset {off:.2}"),
            );
            // Spin about the hinge axis (+Z): it must keep rotating.
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step(
        "hinge rotates freely about its axis only",
        |world, ctx| {
            let part = ent(ctx, "part");
            if ctx.frames == 1 {
                world
                    .entity_mut(part)
                    .insert(AngularVelocity(Vec3::new(4.0, 4.0, 6.0)));
            }
            if ctx.frames == 20 {
                let w = world
                    .get::<AngularVelocity>(part)
                    .map(|a| a.0)
                    .unwrap_or_default();
                ctx.check(
                    "spin survives about the hinge axis",
                    w.z.abs() > 1.0,
                    format!("ω {w:.2}"),
                );
                ctx.check(
                    "no spin across the axis",
                    Vec2::new(w.x, w.y).length() < 0.6,
                    format!("ω {w:.2}"),
                );
                screenshot(world, "09_hinge");
                return Step::Next;
            }
            Step::Wait
        },
    ));
    steps.push(step("unlink", |world, ctx| {
        if ctx.frames == 1 {
            gun(world, GunMode::Unlink);
        }
        let part = ent(ctx, "part");
        if ctx.frames >= 5 && ctx.frames < 8 {
            {
                let target = pos_of(world, part);
                aim_at(world, target);
            }
        }
        if ctx.frames == 7 {
            mouse(world, MouseButton::Left, true);
        }
        if ctx.frames == 8 {
            mouse(world, MouseButton::Left, false);
        }
        if ctx.t > 1.5 && ctx.frames > 12 {
            let mut q = world.query::<&Connection>();
            let left = q.iter(world).filter(|c| c.a == part || c.b == part).count();
            ctx.check(
                "unlink removes the joints",
                left == 0,
                format!("{left} joints left"),
            );
            let y = pos_of(world, part).y;
            let by = pos_of(world, ent(ctx, "base")).y;
            ctx.check(
                "released part falls",
                y < by - 0.5,
                format!("part y {y:.2} base y {by:.2}"),
            );
            let _ = player_pos(world);
            return Step::Next;
        }
        Step::Wait
    }));
}
