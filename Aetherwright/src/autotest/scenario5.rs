//! Stage 5 checks: placing parts with preview, assembling a working cart
//! from separate parts with the player's tools, gear transmission and group
//! operations (copy / paste / rotate / delete).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    Ctx, Step, StepFn,
    scenario::{block_at, edit, step, teleport},
    scenario3::{aim_at, key, mouse, select, with_inv, with_manip},
    scenario4::{gun, pos_of},
    screenshot,
};
use crate::{
    building::{
        construction::{Area, paste},
        preview::{LastGhosts, Validity},
        selection::{GroupOp, PendingOps, Selection},
    },
    core::resources::SpawnPoint,
    items::ItemId,
    machinery::components::{Engine, Part, PartAssets, PartKind, spawn_part},
    physics::{
        joints::{Connection, JointKind, LocalFrame, spawn_connection},
        physics_gun::GunMode,
    },
    tools::manipulator::ManipulatorMode,
    world::{chunks::EditCause, voxel::blocks},
};

/// Flat empty test field west of spawn (cleared by the first step).
pub(super) fn field(world: &World) -> Vec3 {
    let s = world.resource::<SpawnPoint>().0;
    Vec3::new(s.x - 8.0, s.y - 1.0, s.z + 4.0)
}

fn parts_of(world: &mut World, kind: PartKind) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &Part)>();
    q.iter(world)
        .filter(|(_, p)| p.kind == kind)
        .map(|(e, _)| e)
        .collect()
}

/// Test marker: part existed before the current placement.
#[derive(Component)]
struct Seen;

/// Places a part item by aiming at `ground` and pressing RMB.
fn place(world: &mut World, ctx: &mut Ctx, key_name: &str, ground: Vec3, name: &str) -> Step {
    let kind = PartKind::from_key(key_name).unwrap();
    if ctx.frames == 1 {
        with_inv(world, |i| {
            i.add(ItemId::k(key_name), 1);
        });
        select(world, key_name);
        let before = parts_of(world, kind);
        ctx.vars.insert("before_n".into(), before.len() as f32);
        for e in before {
            world.entity_mut(e).insert(Seen);
        }
        ctx.entities.retain(|k, _| k != "__tmp");
        // Stand next to the target so nothing else is in the line of sight.
        teleport(world, ground + Vec3::new(0.0, 1.6, 2.6));
        hover(world);
    }
    if ctx.frames < 12 {
        aim_at(world, ground);
    }
    if ctx.frames == 12 {
        mouse(world, MouseButton::Right, true);
    }
    if ctx.frames == 13 {
        mouse(world, MouseButton::Right, false);
    }
    if ctx.frames == 18 {
        let all = parts_of(world, kind);
        let ok = all.len() as f32 == ctx.vars["before_n"] + 1.0;
        ctx.check(
            &format!("{name} placed"),
            ok,
            format!("{} parts", all.len()),
        );
        // The new part is the one that was not there before (entity
        // indices are reused, so "highest index" is not reliable).
        if let Some(e) = all
            .iter()
            .copied()
            .find(|e| world.get::<Seen>(*e).is_none())
        {
            ctx.entities.insert(name.into(), e);
        }
        return Step::Next;
    }
    Step::Wait
}

/// Keeps the player hovering in place (creative flight) so viewpoints are stable.
fn hover(world: &mut World) {
    let mut q = world.query_filtered::<&mut crate::player::movement::CharacterState, With<crate::player::Player>>();
    for mut s in q.iter_mut(world) {
        s.flying = true;
    }
}

/// Clicks entity `e` from whichever of several viewpoints actually has it
/// under the crosshair (checked via `EntityTarget`). Returns true once clicked.
fn click_entity(world: &mut World, ctx: &mut Ctx, e: Entity) -> bool {
    const VIEWS: [Vec3; 6] = [
        Vec3::NEG_X,
        Vec3::X,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, -1.0),
    ];
    let k = ((ctx.frames / 12) as usize) % VIEWS.len();
    let phase = ctx.frames % 12;
    let p = pos_of(world, e);
    if phase == 0 {
        teleport(world, p + VIEWS[k].normalize() * 3.0 - Vec3::Y * 0.4);
        hover(world);
        mouse(world, MouseButton::Left, false);
    }
    if phase >= 6 {
        aim_at(world, p);
    }
    if phase == 10 && world.resource::<crate::tools::EntityTarget>().entity == Some(e) {
        mouse(world, MouseButton::Left, true);
        ctx.vars.insert("clicked_at".into(), ctx.frames as f32);
    }
    if phase == 11 {
        mouse(world, MouseButton::Left, false);
        return ctx
            .vars
            .get("clicked_at")
            .is_some_and(|f| *f as u32 + 1 == ctx.frames);
    }
    false
}

/// Clicks the face of `e` whose outward normal is `n` (world axis): first
/// stands in front of the face along its normal so nothing else is in the
/// way, then aims at the face centre and clicks LMB. Spans 14 frames from
/// `f0`; returns true on the last one.
fn click_face(world: &mut World, ctx: &Ctx, f0: u32, e: Entity, n: Vec3, half: f32) -> bool {
    if ctx.frames == f0 {
        let p = pos_of(world, e);
        let from = p
            + n * 3.0
            + Vec3::Y * if n.y.abs() > 0.5 { 0.0 } else { 0.3 }
            + if n.y > 0.5 { Vec3::Z * 1.0 } else { Vec3::ZERO };
        teleport(world, from - Vec3::Y * 0.7);
        hover(world);
    }
    if ctx.frames >= f0 + 8 && ctx.frames < f0 + 12 {
        let t = pos_of(world, e) + n * half;
        aim_at(world, t);
    }
    if ctx.frames == f0 + 11 {
        mouse(world, MouseButton::Left, true);
    }
    if ctx.frames == f0 + 12 {
        mouse(world, MouseButton::Left, false);
    }
    ctx.frames == f0 + 13
}

pub fn stage5(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("prepare engineering field", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world).floor().as_ivec3();
            for y in 0..8 {
                for z in -6..=6 {
                    for x in -6..=6 {
                        let p = f + IVec3::new(x, y, z);
                        let want = if y == 0 { blocks::STONE } else { blocks::AIR };
                        if block_at(world, p) != want {
                            edit(world, p, want, EditCause::Script);
                        }
                    }
                }
            }
            // Fill holes under the floor so it is solid.
            for z in -6..=6 {
                for x in -6..=6 {
                    let p = f + IVec3::new(x, -1, z);
                    if block_at(world, p).is_air() {
                        edit(world, p, blocks::STONE, EditCause::Script);
                    }
                }
            }
            let f = field(world);
            teleport(world, f + Vec3::new(0.0, 2.0, 5.0));
            world.resource_mut::<crate::player::camera::CameraRig>().yaw = 0.0;
        }
        if ctx.t > 1.5 { Step::Next } else { Step::Wait }
    }));

    // --- preview colours --------------------------------------------------
    steps.push(step("placement preview shows validity", |world, ctx| {
        if ctx.frames == 1 {
            with_inv(world, |i| {
                i.add(ItemId::k("crate"), 1);
            });
            select(world, "crate");
        }
        let f = field(world);
        let last = |world: &World| world.resource::<LastGhosts>().0.first().map(|g| g.2);
        if ctx.frames < 10 {
            aim_at(world, f + Vec3::new(0.0, 1.0, 0.0));
            return Step::Wait;
        }
        if ctx.frames == 10 {
            let v = last(world);
            ctx.check(
                "free ground → green preview",
                v == Some(Validity::Ok),
                format!("{v:?}"),
            );
            // Stand on the spot: the preview now overlaps the player.
            teleport(world, f + Vec3::new(0.0, 2.0, 0.6));
        }
        if ctx.frames > 10 && ctx.t > 1.5 {
            aim_at(world, f + Vec3::new(0.0, 1.0, 0.0));
            if ctx.frames % 4 == 0 {
                let v = last(world);
                if v == Some(Validity::Obstructed) || ctx.t > 4.0 {
                    ctx.check(
                        "overlap with the player → orange preview",
                        v == Some(Validity::Obstructed),
                        format!("{v:?}"),
                    );
                    teleport(world, f + Vec3::new(0.0, 2.0, 5.0));
                    return Step::Next;
                }
            }
        }
        Step::Wait
    }));

    // --- group operations ---------------------------------------------------
    steps.push(step("select, copy, paste, rotate, delete", |world, ctx| {
        let f = field(world).floor().as_ivec3() + IVec3::new(3, 1, -6);
        if ctx.frames == 1 {
            // An L-shaped bricks wall with a frame part on it.
            for p in [
                IVec3::ZERO,
                IVec3::X,
                IVec3::new(2, 0, 0),
                IVec3::new(0, 1, 0),
                IVec3::new(0, 0, 1),
            ] {
                edit(world, f + p, blocks::BRICKS, EditCause::Script);
            }
            with_manip(world, |m| m.mode = ManipulatorMode::Select);
            select(world, "manipulator");
        }
        if ctx.frames == 6 {
            let mut sel = world.resource_mut::<Selection>();
            sel.a = Some(f);
            sel.b = Some(f + IVec3::new(2, 1, 1));
            world.resource_mut::<PendingOps>().0.push(GroupOp::Copy);
        }
        if ctx.frames == 10 {
            let n = world
                .resource::<Selection>()
                .clipboard
                .as_ref()
                .map(|c| c.blocks.len())
                .unwrap_or(0);
            ctx.check("copy captures blocks", n == 5, format!("{n} blocks"));
            // Paste rotated 90° next to the original (creative-free path: survival with items).
            with_inv(world, |i| {
                i.add(ItemId::k("bricks"), 10);
            });
            let bp = world
                .resource::<Selection>()
                .clipboard
                .clone()
                .unwrap()
                .rotated_y90();
            let origin = f + IVec3::new(0, 0, 2);
            let r = paste(world, &bp, origin);
            ctx.check("paste rotated copy", r.is_ok(), format!("{r:?}"));
            ctx.vars.insert("ox".into(), origin.x as f32);
        }
        if ctx.frames == 16 {
            let origin = f + IVec3::new(0, 0, 2);
            // Rotated L: original (2,0,0) → (0,0,s.x-1-2)=(0,0,0); original (0,0,0) → (0,0,2).
            let ok = block_at(world, origin + IVec3::new(0, 0, 2)) == blocks::BRICKS
                && block_at(world, origin + IVec3::new(1, 0, 2)) == blocks::BRICKS
                && block_at(world, origin + IVec3::new(0, 1, 2)) == blocks::BRICKS;
            let mut found = Vec::new();
            for c in Area::new(origin - IVec3::ONE, origin + IVec3::new(3, 2, 3)).cells() {
                if block_at(world, c) == blocks::BRICKS {
                    found.push(c - origin);
                }
            }
            ctx.check(
                "pasted blocks are rotated correctly",
                ok,
                format!("bricks at {found:?}"),
            );
            let mut sel = world.resource_mut::<Selection>();
            sel.a = Some(origin);
            sel.b = Some(origin + IVec3::new(1, 1, 2));
            world.resource_mut::<PendingOps>().0.push(GroupOp::Delete);
        }
        if ctx.frames == 22 {
            let origin = f + IVec3::new(0, 0, 2);
            let area = Area::new(origin, origin + IVec3::new(1, 1, 2));
            let left = area
                .cells()
                .filter(|c| !block_at(world, *c).is_air())
                .count();
            ctx.check(
                "delete clears the selection",
                left == 0,
                format!("{left} left"),
            );
            key(world, KeyCode::KeyV, false);
            return Step::Next;
        }
        Step::Wait
    }));

    // --- the cart -----------------------------------------------------------
    steps.push(step(
        "assembly session (creative flight for stable viewpoints)",
        |world, ctx| {
            if ctx.frames == 1 {
                world
                    .resource_mut::<NextState<crate::core::states::GameMode>>()
                    .set(crate::core::states::GameMode::Creative);
            }
            if ctx.frames > 3 {
                Step::Next
            } else {
                Step::Wait
            }
        },
    ));
    steps.push(step("place engine", |world, ctx| {
        let f = field(world);
        place(world, ctx, "engine", f + Vec3::new(0.0, 1.0, 0.0), "engine")
    }));
    steps.push(step("place frame skid", |world, ctx| {
        let f = field(world);
        place(world, ctx, "frame", f + Vec3::new(1.65, 1.0, 0.0), "skid")
    }));
    steps.push(step("place wheel 1", |world, ctx| {
        let f = field(world);
        place(world, ctx, "wheel", f + Vec3::new(2.5, 1.0, 1.5), "wheel1")
    }));
    steps.push(step("place wheel 2", |world, ctx| {
        let f = field(world);
        place(world, ctx, "wheel", f + Vec3::new(-2.5, 1.0, 1.5), "wheel2")
    }));
    steps.push(step("place wheel 3", |world, ctx| {
        let f = field(world);
        place(world, ctx, "wheel", f + Vec3::new(5.0, 1.0, 4.5), "wheel3")
    }));
    steps.push(step("place wheel 4", |world, ctx| {
        let f = field(world);
        place(world, ctx, "wheel", f + Vec3::new(3.5, 1.0, -3.0), "wheel4")
    }));
    steps.push(step(
        "place power cell on top of the engine",
        |world, ctx| {
            let e = ctx.entities["engine"];
            let top = pos_of(world, e) + Vec3::Y * 0.5;
            place(world, ctx, "power_cell", top, "cell")
        },
    ));
    for (wheel, side) in [("wheel1", 1.0f32), ("wheel2", -1.0f32)] {
        steps.push(step(
            &format!("hinge {wheel} to the engine"),
            move |world, ctx| {
                let (w, e) = (ctx.entities[wheel], ctx.entities["engine"]);
                if ctx.frames == 1 {
                    select(world, "physgun");
                    gun(world, GunMode::Hinge);
                }
                click_face(world, ctx, 2, w, Vec3::Y, 0.17);
                if click_face(world, ctx, 20, e, Vec3::Z * side, 0.49) {
                    let mut q = world.query::<&Connection>();
                    let ok = q
                        .iter(world)
                        .any(|c| c.kind == JointKind::Hinge && c.a == e && c.b == w);
                    ctx.check(&format!("{wheel} hinged to engine"), ok, "");
                    return Step::Next;
                }
                Step::Wait
            },
        ));
    }
    steps.push(step("weld power cell and skid", |world, ctx| {
        let (c, e, k) = (
            ctx.entities["cell"],
            ctx.entities["engine"],
            ctx.entities["skid"],
        );
        if ctx.frames == 1 {
            gun(world, GunMode::Weld);
        }
        click_face(world, ctx, 2, c, Vec3::Y, 0.5);
        click_face(world, ctx, 18, e, Vec3::NEG_X, 0.49);
        click_face(world, ctx, 34, k, Vec3::Y, 0.49);
        if click_face(world, ctx, 50, e, Vec3::NEG_X, 0.49) {
            let mut q = world.query::<&Connection>();
            let welds: Vec<_> = q
                .iter(world)
                .filter(|c| c.kind == JointKind::Weld)
                .map(|c| (c.a, c.b))
                .collect();
            ctx.check("power cell welded", welds.contains(&(e, c)), "");
            ctx.check("skid welded", welds.contains(&(e, k)), "");
            screenshot(world, "10_cart_assembled");
            return Step::Next;
        }
        Step::Wait
    }));
    for (wheel, side) in [("wheel3", 1.0f32), ("wheel4", -1.0f32)] {
        steps.push(step(
            &format!("free {wheel} on the frame"),
            move |world, ctx| {
                let (k, w) = (ctx.entities["skid"], ctx.entities[wheel]);
                if ctx.frames == 1 {
                    gun(world, GunMode::Hinge);
                }
                click_face(world, ctx, 2, w, Vec3::Y, 0.17);
                if click_face(world, ctx, 20, k, Vec3::Z * side, 0.49) {
                    let mut q = world.query::<&Connection>();
                    let ok = q
                        .iter(world)
                        .any(|c| c.kind == JointKind::Hinge && c.a == k && c.b == w);
                    ctx.check(&format!("{wheel} hinged to the frame (free wheel)"), ok, "");
                    return Step::Next;
                }
                Step::Wait
            },
        ));
    }
    steps.push(step("go to the engine", |world, ctx| {
        if ctx.frames == 1 {
            let p = pos_of(world, ctx.entities["engine"]);
            teleport(world, p + Vec3::new(-3.5, 1.5, 0.8));
        }
        if ctx.frames > 5 {
            Step::Next
        } else {
            Step::Wait
        }
    }));
    steps.push(step("switch the engine on", |world, ctx| {
        let e = ctx.entities["engine"];
        if ctx.frames == 1 {
            select(world, "manipulator");
            with_manip(world, |m| m.mode = ManipulatorMode::Mechanics);
            ctx.vars.insert("ex".into(), pos_of(world, e).x);
            ctx.vars.insert("ez".into(), pos_of(world, e).z);
            ctx.vars.insert(
                "sim0".into(),
                world.resource::<Time<Virtual>>().elapsed_secs(),
            );
        }
        if ctx.frames > 2 && !ctx.vars.contains_key("engine_clicked") && click_entity(world, ctx, e)
        {
            ctx.vars.insert("engine_clicked".into(), ctx.frames as f32);
        }
        let done = ctx
            .vars
            .get("engine_clicked")
            .is_some_and(|f| ctx.frames == *f as u32 + 8)
            || ctx.frames > 200;
        if done {
            ctx.vars.remove("engine_clicked");
            let on = world.get::<Engine>(e).is_some_and(|en| en.on && en.powered);
            ctx.check(
                "engine on and powered",
                on,
                format!("{:?}", world.get::<Engine>(e).map(|e| (e.on, e.powered))),
            );
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("the cart drives", |world, ctx| {
        let (e, w) = (ctx.entities["engine"], ctx.entities["wheel1"]);
        if ctx.t > 5.0 {
            let p = pos_of(world, e);
            let moved = Vec2::new(p.x - ctx.vars["ex"], p.z - ctx.vars["ez"]).length();
            let rot = world.get::<Rotation>(e).map(|r| r.0).unwrap_or_default();
            let wheel_w = world
                .get::<AngularVelocity>(w)
                .map(|a| a.0)
                .unwrap_or_default();
            let rel = (wheel_w
                - world
                    .get::<AngularVelocity>(e)
                    .map(|a| a.0)
                    .unwrap_or_default())
            .length();
            ctx.check(
                "wheel spins relative to the engine",
                rel > 2.0,
                format!("{rel:.1} rad/s"),
            );
            let sim = world.resource::<Time<Virtual>>().elapsed_secs() - ctx.vars["sim0"];
            let speed = moved / sim.max(0.01);
            ctx.check(
                "self-built cart moves under its own power",
                moved > 0.3 && rel > 2.0,
                format!("moved {moved:.2} m in {sim:.1} s of simulation ({speed:.2} m/s)"),
            );
            let upright = (rot * Vec3::Y).y;
            info!("[AUTOTEST] cart upright factor {upright:.2}");
            for wk in ["wheel1", "wheel2", "wheel3", "wheel4"] {
                let we = ctx.entities[wk];
                let r = world.get::<Rotation>(we).map(|r| r.0).unwrap_or_default();
                let w = world
                    .get::<AngularVelocity>(we)
                    .map(|a| a.0)
                    .unwrap_or_default();
                let p = pos_of(world, we);
                info!(
                    "[AUTOTEST] {wk}: pos {p:.2} axis {:.2} ω·axis {:.2}",
                    r * Vec3::Y,
                    w.dot(r * Vec3::Y)
                );
            }
            let skid = pos_of(world, ctx.entities["skid"]);
            info!("[AUTOTEST] engine {p:.2} skid {skid:.2}");
            for wk in ["wheel1", "wheel2"] {
                let we = ctx.entities[wk];
                let d = pos_of(world, we).distance(p);
                let axis = world
                    .get::<Rotation>(we)
                    .map(|r| r.0 * Vec3::Y)
                    .unwrap_or_default();
                let along = axis.dot(rot * Vec3::Z).abs();
                ctx.check(
                    &format!("{wk} mounted on the engine side"),
                    (d - 0.68).abs() < 0.15 && along > 0.9,
                    format!("dist {d:.2}, axis·Z {along:.2}"),
                );
            }
            let cam_target = p;
            teleport(world, p + Vec3::new(4.0, 1.0, 4.0));
            ctx.vars.insert("look".into(), 1.0);
            let _ = cam_target;
            screenshot(world, "11_cart_driving");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("engine off stops driving", |world, ctx| {
        let e = ctx.entities["engine"];
        if ctx.frames == 1 {
            if let Some(mut en) = world.get_mut::<Engine>(e) {
                en.on = false;
            }
        }
        if ctx.t > 1.0 {
            let powered = world.get::<Engine>(e).is_some_and(|en| en.powered);
            ctx.check("engine stops when switched off", !powered, "");
            return Step::Next;
        }
        Step::Wait
    }));

    // --- gear transmission (assembled via the API) -------------------------
    steps.push(step("gear transmission", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world) + Vec3::new(-3.0, 3.0, -3.0);
            let assets = world.remove_resource::<PartAssets>().unwrap();
            let mut c = world.commands();
            let engine = spawn_part(
                &mut c,
                &assets,
                PartKind::Engine,
                Transform::from_translation(f),
            );
            let cell = spawn_part(
                &mut c,
                &assets,
                PartKind::PowerCell,
                Transform::from_translation(f + Vec3::new(0.0, 0.0, -1.2)),
            );
            let g1 = spawn_part(
                &mut c,
                &assets,
                PartKind::Gear,
                Transform::from_translation(f + Vec3::new(0.0, 0.65, 0.0)),
            );
            let anchor = spawn_part(
                &mut c,
                &assets,
                PartKind::Connector,
                Transform::from_translation(f + Vec3::new(1.0, 0.25, 0.0)),
            );
            let g2 = spawn_part(
                &mut c,
                &assets,
                PartKind::Gear,
                Transform::from_translation(f + Vec3::new(1.0, 0.65, 0.0)),
            );
            let id = Quat::IDENTITY;
            let up = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
            spawn_connection(
                &mut c,
                Connection {
                    a: engine,
                    b: cell,
                    kind: JointKind::Weld,
                    frame_a: LocalFrame {
                        anchor: Vec3::new(0.0, 0.0, -0.5),
                        basis: id,
                    },
                    frame_b: LocalFrame {
                        anchor: Vec3::new(0.0, 0.0, 0.7),
                        basis: id,
                    },
                },
            );
            spawn_connection(
                &mut c,
                Connection {
                    a: engine,
                    b: g1,
                    kind: JointKind::Hinge,
                    frame_a: LocalFrame {
                        anchor: Vec3::new(0.0, 0.5, 0.0),
                        basis: up,
                    },
                    frame_b: LocalFrame {
                        anchor: Vec3::new(0.0, -0.15, 0.0),
                        basis: up,
                    },
                },
            );
            spawn_connection(
                &mut c,
                Connection {
                    a: anchor,
                    b: g2,
                    kind: JointKind::Hinge,
                    frame_a: LocalFrame {
                        anchor: Vec3::new(0.0, 0.2, 0.0),
                        basis: up,
                    },
                    frame_b: LocalFrame {
                        anchor: Vec3::new(0.0, -0.2, 0.0),
                        basis: up,
                    },
                },
            );
            world.flush();
            world.insert_resource(assets);
            for e in [engine, anchor] {
                world.entity_mut(e).insert(RigidBody::Static);
            }
            if let Some(mut en) = world.get_mut::<Engine>(engine) {
                en.on = true;
                en.throttle = 0.5;
            }
            ctx.entities.insert("g1".into(), g1);
            ctx.entities.insert("g2".into(), g2);
            ctx.entities.insert("genine".into(), engine);
        }
        if ctx.t > 3.0 {
            let w1 = world
                .get::<AngularVelocity>(ctx.entities["g1"])
                .map(|a| a.0.y)
                .unwrap_or(0.0);
            let w2 = world
                .get::<AngularVelocity>(ctx.entities["g2"])
                .map(|a| a.0.y)
                .unwrap_or(0.0);
            ctx.check("driver gear turns", w1.abs() > 1.5, format!("ω1 {w1:.2}"));
            ctx.check(
                "driven gear turns the opposite way at 1:1",
                w2.abs() > 1.0
                    && w1.signum() != w2.signum()
                    && (w1.abs() - w2.abs()).abs() < 0.35 * w1.abs(),
                format!("ω1 {w1:.2}, ω2 {w2:.2}"),
            );
            screenshot(world, "12_gears");
            if let Some(mut en) = world.get_mut::<Engine>(ctx.entities["genine"]) {
                en.on = false;
            }
            return Step::Next;
        }
        Step::Wait
    }));

    steps.push(step("back to survival", |world, ctx| {
        if ctx.frames == 1 {
            world
                .resource_mut::<NextState<crate::core::states::GameMode>>()
                .set(crate::core::states::GameMode::Survival);
        }
        if ctx.frames > 3 {
            Step::Next
        } else {
            Step::Wait
        }
    }));
}
