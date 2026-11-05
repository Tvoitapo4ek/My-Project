//! Scripted end-to-end test: `sandbox --autotest [--shots DIR]` plays the
//! game through a list of steps (real input resources, real systems),
//! checks results, takes screenshots and exits with 0 / 1.

use avian3d::prelude::*;
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::{
    content::BlockId,
    core::{Scripted, Ui},
    debug::Console,
    grid::{Cell, VoxelGrid},
    interact::Target,
    inventory::Inventory,
    player::{Body, Player, camera::{CameraView, View}, movement::MoveInput},
    sky::Clock,
    world::WorldEntity,
};

pub enum Step {
    Wait,
    Next,
}

pub type StepFn = Box<dyn FnMut(&mut World, &mut Ctx) -> Step + Send + Sync>;

#[derive(Default)]
pub struct Ctx {
    /// Seconds / frames in the current step.
    pub t: f32,
    pub frames: u32,
    pub passed: usize,
    pub failed: Vec<String>,
    pub vars: std::collections::HashMap<String, f32>,
}

impl Ctx {
    pub fn check(&mut self, name: &str, ok: bool, detail: impl std::fmt::Display) {
        if ok {
            info!("[AUTOTEST] PASS {name}: {detail}");
            self.passed += 1;
        } else {
            error!("[AUTOTEST] FAIL {name}: {detail}");
            self.failed.push(format!("{name}: {detail}"));
        }
    }
}

#[derive(Resource)]
pub struct Scenario {
    pub steps: Vec<(String, StepFn)>,
    pub index: usize,
    pub ctx: Ctx,
    pub total: f32,
}

#[derive(Resource, Clone, Default)]
pub struct Shots(pub Option<String>);

pub fn step(name: &str, f: impl FnMut(&mut World, &mut Ctx) -> Step + Send + Sync + 'static) -> (String, StepFn) {
    (name.to_string(), Box::new(f))
}

pub fn screenshot(world: &mut World, name: &str) {
    if let Some(dir) = world.resource::<Shots>().0.clone() {
        let _ = std::fs::create_dir_all(&dir);
        world.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("{dir}/{name}.png")));
    }
}

// ------------------------------------------------------------ helpers

pub fn player(world: &mut World) -> (Vec3, Body) {
    let mut q = world.query_filtered::<(&Position, &Body), With<Player>>();
    q.iter(world).next().map(|(p, b)| (p.0, b.clone())).unwrap_or_default()
}

pub fn teleport(world: &mut World, feet: Vec3) {
    let mut q = world.query_filtered::<(&mut Position, &mut LinearVelocity, &Body), With<Player>>();
    for (mut p, mut v, b) in q.iter_mut(world) {
        p.0 = feet + Vec3::Y * (b.height() * 0.5 + 0.02);
        v.0 = Vec3::ZERO;
    }
}

/// Look at a world point from the player's eye.
pub fn look_at(world: &mut World, point: Vec3) {
    let (pos, body) = player(world);
    let eye = pos - Vec3::Y * body.height() * 0.5 + Vec3::Y * body.eye();
    let d = (point - eye).normalize_or(Vec3::NEG_Z);
    let mut q = world.query_filtered::<&mut Body, With<Player>>();
    for mut b in q.iter_mut(world) {
        b.yaw = (-d.x).atan2(-d.z);
        b.pitch = d.y.asin();
    }
}

pub fn input(world: &mut World, f: impl FnOnce(&mut MoveInput)) {
    let mut m = world.resource_mut::<MoveInput>();
    m.scripted = true;
    f(&mut m);
}

pub fn click(world: &mut World, button: MouseButton, down: bool) {
    let mut m = world.resource_mut::<ButtonInput<MouseButton>>();
    if down { m.press(button) } else { m.release(button) }
}

pub fn world_grid(world: &World) -> Entity {
    world.resource::<WorldEntity>().0
}

pub fn set_block(world: &mut World, c: IVec3, block: &str, facing: u8) {
    let g = world_grid(world);
    let id = if block == "air" { BlockId::AIR } else { BlockId::k(block) };
    world.get_mut::<VoxelGrid>(g).unwrap().set(c, Cell::new(id, facing));
}

pub fn get_block(world: &World, c: IVec3) -> Cell {
    world.get::<VoxelGrid>(world_grid(world)).unwrap().get(c)
}

/// Removes every block in a box (later stages need free space).
pub fn clear_area(world: &mut World, lo: IVec3, hi: IVec3) {
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                if !get_block(world, IVec3::new(x, y, z)).is_air() {
                    set_block(world, IVec3::new(x, y, z), "air", 2);
                }
            }
        }
    }
}

pub fn select(world: &mut World, item: &str) {
    let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
    for mut inv in q.iter_mut(world) {
        inv.pick(crate::content::ItemId::k(item));
    }
}

pub fn console(world: &mut World, line: &str) {
    world.resource_mut::<Console>().queue.push(line.into());
}

fn run(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let Some(mut s) = world.remove_resource::<Scenario>() else { return };
    s.total += dt;
    if s.total > 1800.0 {
        s.ctx.failed.push("timeout".into());
        s.index = s.steps.len();
    }
    if s.index < s.steps.len() {
        s.ctx.t += dt;
        s.ctx.frames += 1;
        let i = s.index;
        if s.ctx.frames == 1 {
            info!("[AUTOTEST] step {}: {}", i + 1, s.steps[i].0);
        }
        let r = (s.steps[i].1)(world, &mut s.ctx);
        if matches!(r, Step::Next) {
            s.index += 1;
            s.ctx.t = 0.0;
            s.ctx.frames = 0;
        }
    } else if s.ctx.t < 1.5 {
        s.ctx.t += dt;
    } else {
        info!("[AUTOTEST] finished: {} passed, {} failed", s.ctx.passed, s.ctx.failed.len());
        for f in &s.ctx.failed {
            error!("[AUTOTEST]   failed: {f}");
        }
        std::process::exit(if s.ctx.failed.is_empty() { 0 } else { 1 });
    }
    world.insert_resource(s);
}

/// Extra scenario stages register here (one per feature module).
pub fn build() -> Scenario {
    let mut steps = Vec::new();
    // `AUTOTEST_ONLY=icons` etc. runs selected stages (development).
    let only = std::env::var("AUTOTEST_ONLY").ok();
    let want = |s: &str| only.as_ref().is_none_or(|o| o.split(',').any(|x| x == s));
    warmup(&mut steps);
    if want("icons") {
        steps.push(step("icon atlas", |world, ctx| {
            if ctx.frames == 60 {
                if let Some(icons) = world.get_resource::<crate::icons::ItemIcons>() {
                    let h = icons.image.clone();
                let e = world.spawn((ImageNode::new(h), Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: px(512), height: px(256), ..default() }, GlobalZIndex(100))).id();
                    let _ = e;
                }
            }
            if ctx.frames == 64 {
                screenshot(world, "00_icon_atlas_ui");
            }
            if ctx.frames == 66 {
                let mut q = world.query_filtered::<Entity, (With<ImageNode>, With<GlobalZIndex>)>();
                let nodes: Vec<Entity> = q.iter(world).collect();
                for e in nodes {
                    if world.get::<GlobalZIndex>(e).is_some_and(|z| z.0 == 100) {
                        world.entity_mut(e).despawn();
                    }
                }
            }
            if ctx.frames > 68 { Step::Next } else { Step::Wait }
        }));
    }
    if want("basics") {
        basics(&mut steps);
    }
    if want("mech") {
        mechanisms(&mut steps);
    }
    if want("car") {
        car(&mut steps);
    }
    if want("guns") {
        guns(&mut steps);
    }
    if want("build2") {
        build2(&mut steps);
    }
    if want("ground") {
        ground(&mut steps);
    }
    if want("mounts") {
        mounts(&mut steps);
    }
    if want("fluids") {
        fluids(&mut steps);
    }
    if want("logic") {
        logic(&mut steps);
    }
    if want("menu") {
        menu(&mut steps);
        help(&mut steps);
        visuals(&mut steps);
    }
    if want("torque") {
        torque(&mut steps);
    }
    if want("links") {
        links(&mut steps);
    }
    // Performance measurement: only on request (`AUTOTEST_ONLY=perf`).
    if only.as_ref().is_some_and(|o| o.split(',').any(|x| x == "perf")) {
        perf(&mut steps);
    }
    if want("shapes") {
        shapes(&mut steps);
        showcase(&mut steps);
        doors(&mut steps);
        goggles(&mut steps);
    }
    if want("items") {
        items(&mut steps);
        workshop(&mut steps);
        hand_tools(&mut steps);
    }
    if only.as_ref().is_some_and(|o| o.split(',').any(|x| x == "visuals")) {
        visuals(&mut steps);
    }
    if want("roundtrip") {
        roundtrip(&mut steps);
    }
    // Last: it replaces the whole world.
    if want("saves") {
        saves(&mut steps);
    }
    Scenario { steps, index: 0, ctx: Ctx::default(), total: 0.0 }
}

fn basics(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("world and player", |world, ctx| {
        world.resource_mut::<Scripted>().0 = true;
        input(world, |m| *m = MoveInput { scripted: true, ..default() });
        if ctx.t < 2.5 {
            return Step::Wait;
        }
        let (p, b) = player(world);
        ctx.check("concrete proving ground under the spawn", get_block(world, IVec3::new(0, -1, 0)).id == BlockId::k("concrete_line"), "");
        ctx.check("player stands on the platform", b.grounded && (p.y - 0.9).abs() < 0.15, format!("{p:.2}"));
        screenshot(world, "01_spawn");
        Step::Next
    }));
    steps.push(step("walk, sprint", |world, ctx| {
        if ctx.frames == 1 {
            let (p, _) = player(world);
            ctx.vars.insert("x0".into(), p.z);
            input(world, |m| m.axis = Vec2::Y);
        }
        if ctx.t > 1.5 && !ctx.vars.contains_key("walk") {
            let (p, _) = player(world);
            ctx.vars.insert("walk".into(), (ctx.vars["x0"] - p.z) / ctx.t);
            ctx.vars.insert("x1".into(), p.z);
            ctx.vars.insert("t1".into(), ctx.t);
            input(world, |m| m.sprint = true);
        }
        if ctx.t > 3.0 {
            let (p, b) = player(world);
            let walk = ctx.vars["walk"];
            let sprint = (ctx.vars["x1"] - p.z) / (ctx.t - ctx.vars["t1"]);
            ctx.check("walking moves forward", walk > 2.5, format!("{walk:.2} m/s"));
            ctx.check("sprint is faster", sprint > walk * 1.1 && b.sprinting, format!("{sprint:.2} m/s"));
            input(world, |m| *m = MoveInput { scripted: true, ..default() });
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("crouching does not fall off an edge", |world, ctx| {
        let base = IVec3::new(20, 3, 20);
        if ctx.frames == 1 {
            for x in 0..3 {
                for z in 0..3 {
                    set_block(world, base + IVec3::new(x, 0, z), "stone", 2);
                }
            }
            teleport(world, base.as_vec3() + Vec3::new(1.5, 1.0, 1.5));
        }
        if ctx.frames == 10 {
            input(world, |m| {
                m.crouch = true;
                m.axis = Vec2::Y;
            });
        }
        if ctx.t > 4.0 && !ctx.vars.contains_key("sneak_y") {
            let (p, b) = player(world);
            ctx.vars.insert("sneak_y".into(), p.y);
            ctx.check("crouched on the edge, still on the pillar", b.crouching && p.y > 4.5 && b.grounded, format!("{p:.2}"));
            screenshot(world, "02_sneak_edge");
            input(world, |m| m.crouch = false);
        }
        if ctx.t > 6.5 {
            let (p, _) = player(world);
            ctx.check("without crouching it walks off", p.y < 2.0, format!("{p:.2}"));
            input(world, |m| *m = MoveInput { scripted: true, ..default() });
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("creative flight (double Space)", |world, ctx| {
        match ctx.frames {
            1 => teleport(world, Vec3::new(0.5, 0.0, 0.5)),
            // Two taps: the first one "just happened", the second now.
            6 => {
                let mut q = world.query_filtered::<&mut Body, With<Player>>();
                for mut b in q.iter_mut(world) {
                    b.last_jump_press = 0.1;
                }
                input(world, |m| m.jump_pressed = true);
            }
            8 => input(world, |m| m.jump = true),
            _ => {}
        }
        if ctx.frames > 20 {
            let (p, b) = player(world);
            ctx.check("double Space toggles flight", b.flying, "");
            ctx.check("flies up", p.y > 4.0, format!("{p:.2}"));
            input(world, |m| *m = MoveInput { scripted: true, ..default() });
            console(world, "fly");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("break and place blocks", |world, ctx| {
        match ctx.frames {
            1 => {
                teleport(world, Vec3::new(0.5, 0.0, 0.5));
                select(world, "stone");
            }
            10 => look_at(world, Vec3::new(0.5, 0.0, -2.5)),
            14 => {
                let a = world.resource::<Target>().hit;
                ctx.check("crosshair finds the floor block", a.is_some_and(|a| a.cell == IVec3::new(0, -1, -3)), format!("{:?}", a.map(|a| a.cell)));
                click(world, MouseButton::Right, true);
            }
            15 => click(world, MouseButton::Right, false),
            20 => {
                ctx.check("RMB places a block on the face", get_block(world, IVec3::new(0, 0, -3)).id == BlockId::k("stone"), "");
                look_at(world, Vec3::new(0.5, 0.5, -2.5));
            }
            24 => click(world, MouseButton::Left, true),
            25 => click(world, MouseButton::Left, false),
            30 => {
                ctx.check("LMB breaks it", get_block(world, IVec3::new(0, 0, -3)).is_air(), "");
                look_at(world, Vec3::new(0.5, 0.0, -2.5));
                select(world, "glass");
            }
            34 => click(world, MouseButton::Middle, true),
            35 => click(world, MouseButton::Middle, false),
            40 => {
                let held = world.query_filtered::<&Inventory, With<Player>>().iter(world).next().and_then(|i| i.held()).map(|s| s.item.def().key.clone());
                let floor = get_block(world, IVec3::new(0, -1, -3)).id.def().key.clone();
                ctx.check("MMB picks the block", held.as_deref() == Some(floor.as_str()), format!("{held:?} / {floor}"));
                // A little build for the screenshots.
                for (i, k) in ["white_block", "stone", "planks", "steel_block", "steel_frame", "steel_casing", "glass"].iter().enumerate() {
                    set_block(world, IVec3::new(-3 + i as i32, 0, -5), k, 2);
                    set_block(world, IVec3::new(-3 + i as i32, 1, -5), k, 2);
                }
                look_at(world, Vec3::new(0.5, 0.8, -5.0));
            }
            46 => screenshot(world, "03_blocks"),
            50 => return Step::Next,
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("inventory and catalogue", |world, ctx| {
        match ctx.frames {
            1 => world.resource_mut::<NextState<Ui>>().set(Ui::Inventory),
            12 => screenshot(world, "04_inventory"),
            16 => {
                let mut c = world.resource_mut::<crate::ui::Catalog>();
                c.open = true;
                c.dirty_now();
            }
            28 => screenshot(world, "05_catalogue"),
            32 => {
                let mut c = world.resource_mut::<crate::ui::Catalog>();
                c.search = "стал".into();
                c.dirty_now();
            }
            44 => screenshot(world, "06_search"),
            48 => {
                let mut c = world.resource_mut::<crate::ui::Catalog>();
                c.search.clear();
                c.open = false;
                c.dirty_now();
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("debug menu and console", |world, ctx| {
        match ctx.frames {
            1 => world.resource_mut::<NextState<Ui>>().set(Ui::Debug),
            10 => screenshot(world, "07_debug_menu"),
            14 => {
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                console(world, "time set night");
                console(world, "time stop");
            }
            30 => {
                let c = world.resource::<Clock>().clone();
                ctx.check("console sets night", c.hours == 0.0, c.label());
                ctx.check("console stops time", c.paused, "");
                screenshot(world, "08_night");
            }
            36 => console(world, "time set 7.5"),
            40 => {
                let h = world.resource::<Clock>().hours;
                ctx.check("time stays stopped", h == 7.5, format!("{h}"));
                console(world, "time set day");
            }
            44 => return Step::Next,
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("player model and animations", |world, ctx| {
        match ctx.frames {
            1 => {
                world.resource_mut::<CameraView>().0 = View::Front;
                teleport(world, Vec3::new(4.5, 0.0, 4.5));
                input(world, |m| m.axis = Vec2::Y);
            }
            14 => screenshot(world, "09_walk_front"),
            16 => {
                input(world, |m| {
                    m.axis = Vec2::ZERO;
                    m.crouch = true;
                });
                world.resource_mut::<CameraView>().0 = View::Behind;
            }
            28 => screenshot(world, "10_crouch_behind"),
            32 => {
                input(world, |m| *m = MoveInput { scripted: true, ..default() });
                world.resource_mut::<CameraView>().0 = View::First;
                clear_area(world, IVec3::new(-4, 0, -6), IVec3::new(4, 2, -2));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// Speed of the mechanism in a world cell (RPM about the grid +axis).
pub fn rpm(world: &mut World, c: IVec3) -> f32 {
    let g = world_grid(world);
    let e = world.get::<VoxelGrid>(g).and_then(|g| g.entities.get(&c).copied());
    e.and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(f32::NAN)
}

pub fn block_entity(world: &World, c: IVec3) -> Option<Entity> {
    world.get::<VoxelGrid>(world_grid(world)).and_then(|g| g.entities.get(&c).copied())
}

pub fn set_param(world: &mut World, c: IVec3, key: &str, v: f32) {
    if let Some(e) = block_entity(world, c) {
        let id = world.get::<crate::mech::BlockEntity>(e).unwrap().id;
        if let Some((i, _)) = id.def().param(key)
            && let Some(mut p) = world.get_mut::<crate::mech::Params>(e)
        {
            p.0[i] = v;
        }
    }
}

fn mechanisms(steps: &mut Vec<(String, StepFn)>) {
    // Facings: 0 +X, 1 −X, 2 +Y, 3 −Y, 4 +Z, 5 −Z.
    steps.push(step("mechanisms: build", |world, ctx| {
        if ctx.frames == 1 {
            let z = -8;
            set_block(world, IVec3::new(0, 0, z), "creative_motor", 0);
            for x in 1..4 {
                set_block(world, IVec3::new(x, 0, z), "shaft", 0);
            }
            set_block(world, IVec3::new(4, 0, z), "gearbox", 2);
            set_block(world, IVec3::new(5, 0, z), "shaft", 0);
            set_block(world, IVec3::new(4, 0, z + 1), "shaft", 4);
            set_block(world, IVec3::new(4, 0, z + 2), "shaft", 4);
            set_block(world, IVec3::new(6, 0, z), "cogwheel", 0);
            set_block(world, IVec3::new(6, 1, z), "cogwheel", 0);
            set_block(world, IVec3::new(6, 2, z + 1), "large_cogwheel", 0);
            // Clutch, gearshift and speed-controller lines.
            for (z, k) in [(-12, "clutch"), (-14, "gearshift"), (-16, "speed_controller")] {
                set_block(world, IVec3::new(0, 0, z), "creative_motor", 0);
                set_block(world, IVec3::new(1, 0, z), k, 0);
                set_block(world, IVec3::new(2, 0, z), "shaft", 0);
                set_block(world, IVec3::new(3, 0, z), "shaft", 0);
            }
            teleport(world, Vec3::new(-2.5, 0.0, -2.5));
        }
        if ctx.frames == 6 {
            set_param(world, IVec3::new(0, 0, -16), "speed", 64.0);
        }
        if ctx.frames == 12 {
            let m = rpm(world, IVec3::new(0, 0, -8));
            let s3 = rpm(world, IVec3::new(3, 0, -8));
            ctx.check("creative motor drives a shaft line", m.abs() == 32.0 && s3 == m, format!("motor {m}, shaft {s3}"));
            let through = rpm(world, IVec3::new(5, 0, -8));
            ctx.check("gearbox: straight through reverses", through == -m, format!("{through}"));
            let corner = rpm(world, IVec3::new(4, 0, -6));
            ctx.check("gearbox turns the rotation 90°", corner.abs() == 32.0, format!("{corner}"));
            let c1 = rpm(world, IVec3::new(6, 0, -8));
            let c2 = rpm(world, IVec3::new(6, 1, -8));
            let big = rpm(world, IVec3::new(6, 2, -7));
            ctx.check("meshing cogs turn the other way", c2 == -c1 && c1 != 0.0, format!("{c1} / {c2}"));
            ctx.check("small → large cog halves the speed", (big.abs() - c1.abs() / 2.0).abs() < 1e-3, format!("{big}"));
            ctx.check("speed controller outputs its own speed", rpm(world, IVec3::new(3, 0, -16)).abs() == 32.0 && rpm(world, IVec3::new(0, 0, -16)).abs() == 64.0, format!("{}", rpm(world, IVec3::new(3, 0, -16))));
            ctx.vars.insert("gs".into(), rpm(world, IVec3::new(3, 0, -14)));
            look_at(world, Vec3::new(3.0, 0.5, -10.0));
        }
        if ctx.frames == 16 {
            console(world, "fly");
            teleport(world, Vec3::new(-2.0, 3.0, -5.0));
        }
        if ctx.frames == 20 {
            look_at(world, Vec3::new(3.0, 0.0, -11.0));
        }
        if ctx.frames == 26 {
            screenshot(world, "20_mechanisms");
        }
        if ctx.frames == 28 {
            console(world, "fly");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("seat, panel and sensors drive clutch / gearshift / speed controller", |world, ctx| {
        match ctx.frames {
            1 => {
                // Sensors sit on top of the controlled blocks, facing into them.
                for z in [-12, -14, -16] {
                    set_block(world, IVec3::new(1, 1, z), "sensor", 3);
                }
                set_block(world, IVec3::new(-2, 0, -14), "seat", 0);
                set_block(world, IVec3::new(-2, 0, -13), "control_panel", 0);
                select(world, "wrench");
                teleport(world, Vec3::new(-3.5, 0.0, -11.5));
            }
            // Link with the wrench: panel, then sensor.
            6 => look_at(world, Vec3::new(-1.5, 0.3, -12.5)),
            10 => click(world, MouseButton::Left, true),
            11 => click(world, MouseButton::Left, false),
            14 => look_at(world, Vec3::new(1.5, 1.2, -11.5)),
            18 => click(world, MouseButton::Left, true),
            19 => click(world, MouseButton::Left, false),
            26 => {
                let e = block_entity(world, IVec3::new(1, 1, -12)).unwrap();
                let linked = world.get::<crate::signal::Linked>(e).and_then(|l| l.0);
                ctx.check("wrench links a sensor to the panel", linked.is_some_and(|p| p.cell == IVec3::new(-2, 0, -13)), format!("{linked:?}"));
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                // The other two directly.
                for z in [-14, -16] {
                    let e = block_entity(world, IVec3::new(1, 1, z)).unwrap();
                    world.get_mut::<crate::signal::Linked>(e).unwrap().0 = linked;
                }
            }
            30 => {
                select(world, "white_block");
                // From the side away from the panel.
                teleport(world, Vec3::new(-3.5, 0.0, -15.5));
            }
            32 => look_at(world, Vec3::new(-1.5, 0.3, -13.5)),
            34 => {
                let a = world.resource::<Target>().hit;
                let (p, b) = player(world);
                ctx.check("aiming at the seat", a.is_some_and(|a| a.cell == IVec3::new(-2, 0, -14)), format!("{:?} from {p:.2} crouch {} ui {:?}", a.map(|a| a.cell), b.crouching, world.resource::<State<Ui>>().get()));
                click(world, MouseButton::Right, true);
            }
            35 => click(world, MouseButton::Right, false),
            40 => {
                let seated = world.query_filtered::<&crate::player::Seated, With<Player>>().iter(world).count() == 1;
                ctx.check("RMB on the seat sits down", seated, "");
                let mut pk = world.resource_mut::<crate::signal::PanelKeys>();
                pk.scripted = true;
                pk.held = vec![KeyCode::KeyW];
            }
            46 => {
                ctx.check("clutch stops the rotation on a signal", rpm(world, IVec3::new(3, 0, -12)) == 0.0, format!("{}", rpm(world, IVec3::new(3, 0, -12))));
                let gs = rpm(world, IVec3::new(3, 0, -14));
                ctx.check("gearshift reverses on a signal", gs == -ctx.vars["gs"] && gs != 0.0, format!("{gs}"));
                let sc = rpm(world, IVec3::new(3, 0, -16));
                ctx.check("speed controller switches to its signal speed", sc.abs() == 128.0, format!("{sc}"));
                screenshot(world, "22_seated_controls");
                world.resource_mut::<crate::signal::PanelKeys>().held.clear();
            }
            50 => {
                ctx.check("released key: clutch engaged again", rpm(world, IVec3::new(3, 0, -12)) != 0.0, "");
                input(world, |m| m.crouch = true);
            }
            56 => {
                input(world, |m| m.crouch = false);
                let seated = world.query_filtered::<&crate::player::Seated, With<Player>>().iter(world).count() == 1;
                ctx.check("Shift stands up", !seated, "");
                world.resource_mut::<crate::signal::PanelKeys>().scripted = false;
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("telemetry stand: digits and needle", |world, ctx| {
        let z = -20;
        let link = |world: &mut World, sensor: IVec3, display: IVec3| {
            let g = world_grid(world);
            let e = block_entity(world, sensor).unwrap();
            world.get_mut::<crate::signal::Linked>(e).unwrap().0 = Some(crate::signal::PanelRef { grid: g, cell: display });
        };
        let shown = |world: &mut World, c: IVec3| block_entity(world, c).and_then(|e| world.get::<crate::telemetry::Shown>(e)).map(|s| s.0).unwrap_or(-1.0);
        match ctx.frames {
            1 => {
                set_block(world, IVec3::new(0, 0, z), "creative_motor", 0);
                for x in 1..6 {
                    set_block(world, IVec3::new(x, 0, z), if x == 2 || x == 4 { "mechanical_display" } else { "shaft" }, if x == 2 || x == 4 { 4 } else { 0 });
                }
                set_block(world, IVec3::new(0, 1, z), "telemetry_sensor", 3);
                set_block(world, IVec3::new(3, 1, z), "telemetry_sensor", 3);
                teleport(world, Vec3::new(3.0, 0.0, z as f32 + 3.2));
            }
            4 => {
                set_param(world, IVec3::new(0, 0, z), "speed", 96.0);
                set_param(world, IVec3::new(4, 0, z), "style", 1.0);
                link(world, IVec3::new(0, 1, z), IVec3::new(2, 0, z));
                link(world, IVec3::new(3, 1, z), IVec3::new(4, 0, z));
                look_at(world, Vec3::new(3.0, 0.5, z as f32 + 0.5));
            }
            12 => {
                let a = shown(world, IVec3::new(2, 0, z));
                let b = shown(world, IVec3::new(4, 0, z));
                // On a motor the sensor reads the load of its network (%).
                let net = block_entity(world, IVec3::new(0, 0, z)).and_then(|e| world.get::<crate::mech::NetState>(e).copied()).unwrap_or_default();
                let want = net.stress / net.capacity.max(1.0) * 100.0;
                ctx.check("telemetry: motor network load (%) on the digital display", net.stress > 0.0 && (a - want).abs() < 1e-3, format!("{a:.3}% ({:.1} / {:.0} SU)", net.stress, net.capacity));
                ctx.check("telemetry: shaft RPM on the needle display", b == 96.0, format!("{b}"));
            }
            22 => {
                screenshot(world, "23_telemetry_stand");
            }
            26 => set_param(world, IVec3::new(0, 0, z), "speed", 0.0),
            30 => {
                let rpm = block_entity(world, IVec3::new(2, 0, z)).and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(1.0);
                let a = shown(world, IVec3::new(2, 0, z));
                ctx.check("motor stopped: displays lose power and show 0", rpm == 0.0 && a == 0.0, format!("rpm {rpm}, shown {a}"));
            }
            38 => {
                screenshot(world, "24_telemetry_off");
                clear_area(world, IVec3::new(0, 0, z), IVec3::new(5, 1, z));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("wrench: turn and settings", |world, ctx| {
        match ctx.frames {
            1 => {
                select(world, "wrench");
                teleport(world, Vec3::new(2.5, 0.0, -5.5));
            }
            4 => look_at(world, Vec3::new(2.5, 1.0, -7.5)),
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the top of a shaft", a.is_some_and(|a| a.cell == IVec3::new(2, 0, -8) && a.normal == IVec3::Y), format!("{:?}", a.map(|a| (a.cell, a.normal))));
                click(world, MouseButton::Right, true);
            }
            9 => click(world, MouseButton::Right, false),
            14 => {
                let f = get_block(world, IVec3::new(2, 0, -8)).facing;
                ctx.check("RMB with the wrench turns the shaft about the face", crate::grid::facing_dir(f).x == 0, format!("facing {f}"));
                ctx.check("a turned shaft breaks the line", rpm(world, IVec3::new(3, 0, -8)) == 0.0, format!("{}", rpm(world, IVec3::new(3, 0, -8))));
                look_at(world, Vec3::new(0.5, 0.5, -7.5));
            }
            18 => click(world, MouseButton::Left, true),
            19 => click(world, MouseButton::Left, false),
            26 => {
                let ui = *world.resource::<State<Ui>>().get();
                ctx.check("LMB with the wrench opens the motor settings", ui == Ui::Settings, format!("{ui:?}"));
                screenshot(world, "21_motor_settings");
            }
            30 => {
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                clear_area(world, IVec3::new(-3, 0, -17), IVec3::new(7, 3, -5));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn contraptions(world: &mut World) -> Vec<(Entity, Vec3, Quat, usize)> {
    let mut q = world.query_filtered::<(Entity, &GlobalTransform, &VoxelGrid), With<crate::contraption::Contraption>>();
    q.iter(world).map(|(e, t, g)| { let t = t.compute_transform(); (e, t.translation, t.rotation, g.cells().len()) }).collect()
}

/// Block entity in the (first) contraption's grid.
fn car_block(world: &mut World, c: IVec3) -> Option<Entity> {
    let mut q = world.query_filtered::<&VoxelGrid, With<crate::contraption::Contraption>>();
    q.iter(world).next().and_then(|g| g.entities.get(&c).copied())
}

fn world_cells(world: &World) -> usize {
    world.get::<VoxelGrid>(world_grid(world)).unwrap().cells().len()
}

fn warmup(steps: &mut Vec<(String, StepFn)>) {
    // Software rendering compiles shaders for a while: frames are black.
    steps.push(step("warm-up", |world, ctx| {
        world.resource_mut::<Scripted>().0 = true;
        input(world, |m| *m = MoveInput { scripted: true, ..default() });
        if ctx.t > 8.0 { Step::Next } else { Step::Wait }
    }));
}

fn car(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("car: build in blocks", |world, ctx| {
        match ctx.frames {
            1 => {
                for x in 0..3 {
                    for z in -30..=-26 {
                        set_block(world, IVec3::new(x, 1, z), "steel_block", 2);
                    }
                }
                for z in [-30, -26] {
                    set_block(world, IVec3::new(0, 0, z), "suspension", 1);
                    set_block(world, IVec3::new(2, 0, z), "suspension", 0);
                    set_block(world, IVec3::new(-1, 0, z), "wheel", 1);
                    set_block(world, IVec3::new(3, 0, z), "wheel", 0);
                }
                // Rear axle ← two cogs ← speed controller (throttle) ← motor.
                set_block(world, IVec3::new(1, 0, -26), "cogwheel", 0);
                set_block(world, IVec3::new(1, 0, -25), "cogwheel", 0);
                set_block(world, IVec3::new(2, 0, -25), "speed_controller", 1);
                set_block(world, IVec3::new(3, 0, -25), "creative_motor", 1);
                set_block(world, IVec3::new(2, 1, -25), "sensor", 3);
                // Steering sensors in front of the front suspensions.
                set_block(world, IVec3::new(0, 0, -31), "sensor", 4);
                set_block(world, IVec3::new(2, 0, -31), "sensor", 4);
                set_block(world, IVec3::new(1, 2, -27), "seat", 5);
                set_block(world, IVec3::new(1, 2, -28), "control_panel", 4);
                set_block(world, IVec3::new(1, 2, -26), "physics_toggle", 2);
                // Telemetry: speedometer ← telemetry sensor (from above);
                // a display facing +Z, turned from its −X side by a motor.
                set_block(world, IVec3::new(0, 2, -30), "speedometer", 4);
                set_block(world, IVec3::new(0, 3, -30), "telemetry_sensor", 3);
                set_block(world, IVec3::new(2, 2, -29), "mechanical_display", 4);
                set_block(world, IVec3::new(1, 2, -29), "creative_motor", 0);
                let g = world_grid(world);
                world.entity_mut(g).insert(crate::tools::glue::Glue(vec![(IVec3::new(-1, 0, -31), IVec3::new(3, 3, -25))]));
                select(world, "white_block");
                teleport(world, Vec3::new(7.5, 0.0, -22.5));
            }
            6 => {
                set_param(world, IVec3::new(3, 0, -25), "speed", 128.0);
                set_param(world, IVec3::new(2, 0, -25), "speed", 0.0);
                set_param(world, IVec3::new(2, 0, -25), "signal_speed", 96.0);
                set_param(world, IVec3::new(2, 1, -25), "key_neg", 2.0);
                let g = world_grid(world);
                let e = block_entity(world, IVec3::new(2, 1, -25)).unwrap();
                world.get_mut::<crate::signal::Linked>(e).unwrap().0 = Some(crate::signal::PanelRef { grid: g, cell: IVec3::new(1, 2, -28) });
                for x in [0, 2] {
                    set_param(world, IVec3::new(x, 0, -31), "key_pos", 3.0);
                    set_param(world, IVec3::new(x, 0, -31), "key_neg", 4.0);
                    let e = block_entity(world, IVec3::new(x, 0, -31)).unwrap();
                    world.get_mut::<crate::signal::Linked>(e).unwrap().0 = Some(crate::signal::PanelRef { grid: g, cell: IVec3::new(1, 2, -28) });
                }
                look_at(world, Vec3::new(1.5, 0.5, -28.0));
            }
            12 => {
                let w = rpm(world, IVec3::new(3, 0, -26));
                ctx.check("throttle at 0: wheels stand", w == 0.0 && rpm(world, IVec3::new(3, 0, -25)).abs() == 128.0, format!("{w}"));
                screenshot(world, "30_car_blocks");
                ctx.vars.insert("cells0".into(), world_cells(world) as f32);
            }
            16 => look_at(world, Vec3::new(1.5, 3.0, -25.5)),
            20 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the physics toggle", a.is_some_and(|a| a.cell == IVec3::new(1, 2, -26)), format!("{:?}", a.map(|a| (a.cell, a.normal))));
                click(world, MouseButton::Right, true);
            }
            21 => click(world, MouseButton::Right, false),
            // The lever opens: drag the knob to «Физика».
            24 => {
                let mut l = world.resource_mut::<crate::toggle_ui::Lever>();
                l.value = 1.0;
                l.dragging = false;
            }
            34 => {
                let c = contraptions(world);
                let left = world_cells(world) as f32;
                ctx.check("physics toggle: the glued car becomes one body", c.len() == 1 && c[0].3 >= 30, format!("{:?}", c.iter().map(|c| c.3).collect::<Vec<_>>()));
                ctx.check("its blocks left the world grid", ctx.vars["cells0"] - left >= 30.0, format!("{} → {left}", ctx.vars["cells0"]));
                let m = world.query::<&crate::contraption::Contraption>().iter(world).next().map(|c| c.mass).unwrap_or(0.0);
                ctx.check("mass from its blocks", m > 2000.0, format!("{m:.0} kg"));
                if let Some(c) = c.first() {
                    ctx.vars.insert("z0".into(), c.1.z);
                    ctx.vars.insert("y0".into(), c.1.y);
                }
                ctx.vars.insert("t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames > 34 && ctx.t > ctx.vars["t0"] + 4.0 {
            let c = contraptions(world);
            let Some(c) = c.first() else { return Step::Next };
            let moved = (c.1.z - ctx.vars["z0"]).abs();
            ctx.check("physical car stands still without throttle", moved < 0.3, format!("moved {moved:.2} m"));
            ctx.check("springs hold the car at its height, upright", (c.1.y - ctx.vars["y0"]).abs() < 0.3 && (c.2 * Vec3::Y).y > 0.95, format!("y {:.2} → {:.2}", ctx.vars["y0"], c.1.y));
            screenshot(world, "31_car_physical");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("car: link telemetry with the wrench", |world, ctx| {
        let Some((_, pos, rot, _)) = contraptions(world).first().copied() else {
            ctx.check("car exists", false, "");
            return Step::Next;
        };
        let at = |p: Vec3| pos + rot * p;
        match ctx.frames {
            1 => {
                select(world, "wrench");
                teleport(world, at(Vec3::new(5.5, 0.0, -29.5)) - Vec3::Y * at(Vec3::ZERO).y);
            }
            4 => look_at(world, at(Vec3::new(0.5, 3.5, -29.5))),
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the telemetry sensor", a.is_some_and(|a| a.cell == IVec3::new(0, 3, -30)), format!("{:?}", a.map(|a| a.cell)));
                click(world, MouseButton::Left, true);
            }
            9 => click(world, MouseButton::Left, false),
            12 => look_at(world, at(Vec3::new(3.0, 2.5, -28.5))),
            16 => click(world, MouseButton::Left, true),
            17 => click(world, MouseButton::Left, false),
            22 => {
                let sensor = car_block(world, IVec3::new(0, 3, -30));
                let link = sensor.and_then(|e| world.get::<crate::signal::Linked>(e)).and_then(|l| l.0);
                ctx.check("wrench links the telemetry sensor to the display", link.is_some_and(|l| l.cell == IVec3::new(2, 2, -29)), format!("{link:?}"));
                let d = car_block(world, IVec3::new(2, 2, -29)).unwrap();
                let rpm = world.get::<crate::mech::Spin>(d).map(|s| s.rpm).unwrap_or(0.0);
                ctx.check("display is powered by the motor on its side", rpm.abs() > 1.0, format!("{rpm}"));
                let shown = world.get::<crate::telemetry::Shown>(d).map(|s| s.0).unwrap_or(-1.0);
                ctx.check("standing car: the display shows 0 km/h", shown < 0.5, format!("{shown:.2}"));
                select(world, "white_block");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("car: sit, drive and steer with the panel", |world, ctx| {
        let Some((_, pos, rot, _)) = contraptions(world).first().copied() else {
            ctx.check("car exists", false, "");
            return Step::Next;
        };
        let fwd = rot * Vec3::NEG_Z;
        let keys = |world: &mut World, k: Vec<KeyCode>| {
            let mut pk = world.resource_mut::<crate::signal::PanelKeys>();
            pk.scripted = true;
            pk.held = k;
        };
        match ctx.frames {
            // Stand on the chassis next to the seat.
            1 => teleport(world, pos + rot * Vec3::new(2.5, 2.05, -26.5)),
            4 => look_at(world, pos + rot * Vec3::new(1.5, 2.4, -26.5)),
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the seat", a.is_some_and(|a| a.cell == IVec3::new(1, 2, -27)), format!("{:?}", a.map(|a| a.cell)));
                click(world, MouseButton::Right, true);
            }
            9 => click(world, MouseButton::Right, false),
            14 => {
                let seated = world.query_filtered::<&crate::player::Seated, With<Player>>().iter(world).count() == 1;
                ctx.check("sits on the car's seat", seated, "");
                keys(world, vec![KeyCode::KeyW]);
                ctx.vars.insert("p0x".into(), pos.x);
                ctx.vars.insert("p0z".into(), pos.z);
                world.resource_mut::<CameraView>().0 = View::Behind;
            }
            26 => {
                let moved = Vec2::new(pos.x - ctx.vars["p0x"], pos.z - ctx.vars["p0z"]).length();
                ctx.check("W on the panel: throttle drives the car", moved > 2.0, format!("{moved:.2} m"));
                screenshot(world, "32_car_driving");
                let d = car_block(world, IVec3::new(2, 2, -29)).unwrap();
                let shown = world.get::<crate::telemetry::Shown>(d).map(|s| s.0).unwrap_or(0.0);
                ctx.check("driving: the display shows the speed", shown > 3.0, format!("{shown:.1} km/h"));
                ctx.vars.insert("kmh".into(), shown);
                keys(world, vec![KeyCode::KeyW, KeyCode::KeyA]);
                ctx.vars.insert("f0x".into(), fwd.x);
                ctx.vars.insert("f0z".into(), fwd.z);
            }
            32 => {
                // Switch the display to the needle (wrench setting «Вид»).
                let d = car_block(world, IVec3::new(2, 2, -29)).unwrap();
                world.get_mut::<crate::mech::Params>(d).unwrap().0[0] = 1.0;
            }
            37 => screenshot(world, "34_display_needle"),
            40 => {
                let f0 = Vec2::new(ctx.vars["f0x"], ctx.vars["f0z"]).normalize();
                let turned = f0.angle_to(Vec2::new(fwd.x, fwd.z).normalize()).abs().to_degrees();
                ctx.check("A on the panel steers the car", turned > 15.0, format!("{turned:.0}°"));
                screenshot(world, "33_car_turning");
                keys(world, vec![]);
            }
            52 => {
                let v = world.query_filtered::<&avian3d::prelude::LinearVelocity, With<crate::contraption::Contraption>>().iter(world).next().map(|v| v.0.length()).unwrap_or(9.9);
                ctx.check("released: the car stops", v < 1.0, format!("{v:.2} m/s"));
                world.resource_mut::<crate::signal::PanelKeys>().scripted = false;
                input(world, |m| m.crouch = true);
                world.resource_mut::<CameraView>().0 = View::First;
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("car: back to blocks", |world, ctx| {
        match ctx.frames {
            2 => input(world, |m| m.crouch = false),
            4 => {
                // Stop the motor so the car stands still, then switch off.
                if let Some((body, pos, rot, _)) = contraptions(world).first().copied() {
                    let toggle = pos + rot * Vec3::new(1.5, 2.5, -25.5);
                    teleport(world, toggle + rot * Vec3::new(0.0, 0.0, 2.5) - Vec3::Y * 0.5);
                    let _ = body;
                }
            }
            8 => {
                if let Some((_, pos, rot, _)) = contraptions(world).first().copied() {
                    look_at(world, pos + rot * Vec3::new(1.5, 2.95, -25.5));
                }
            }
            12 => click(world, MouseButton::Right, true),
            13 => click(world, MouseButton::Right, false),
            // The lever: drag the knob back to «Блоки».
            16 => {
                let mut l = world.resource_mut::<crate::toggle_ui::Lever>();
                l.value = 0.0;
                l.dragging = false;
            }
            22 => {
                let n = contraptions(world).len();
                ctx.check("toggle again: the car returns to the block grid", n == 0, format!("{n} bodies left"));
                ctx.check("all blocks are back", (world_cells(world) as f32 - ctx.vars["cells0"]).abs() < 0.5, format!("{} vs {}", world_cells(world), ctx.vars["cells0"]));
                // Clear the car where it stopped (later tests build there).
                let g = world_grid(world);
                let boxes = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                for (lo, hi) in boxes {
                    clear_area(world, lo.with_y(lo.y.max(0)), hi);
                }
                world.entity_mut(g).insert(crate::tools::glue::Glue(Vec::new()));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// World centre of mass of the first contraption.
fn body_center(world: &mut World) -> Option<(Entity, Vec3)> {
    let mut q = world.query_filtered::<(Entity, &GlobalTransform, &avian3d::prelude::ComputedCenterOfMass), With<crate::contraption::Contraption>>();
    q.iter(world).next().map(|(e, t, c)| (e, t.transform_point(c.0)))
}

fn guns(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("glue tool + toggle: a cube becomes physical", |world, ctx| {
        match ctx.frames {
            1 => {
                for x in 10..12 {
                    for y in 0..2 {
                        for z in -10..-8 {
                            set_block(world, IVec3::new(x, y, z), "stone", 2);
                        }
                    }
                }
                set_block(world, IVec3::new(11, 1, -9), "physics_toggle", 2);
                set_block(world, IVec3::new(13, 0, -10), "stone", 2);
                select(world, "glue");
                // Look from above so the top faces are visible.
                console(world, "fly");
                teleport(world, Vec3::new(10.5, 2.0, -5.5));
            }
            // Corners: front-bottom-left, then the top of the back-right.
            4 => look_at(world, Vec3::new(10.5, 0.5, -8.0)),
            8 => click(world, MouseButton::Left, true),
            9 => click(world, MouseButton::Left, false),
            12 => look_at(world, Vec3::new(11.5, 2.0, -9.6)),
            16 => click(world, MouseButton::Left, true),
            17 => click(world, MouseButton::Left, false),
            22 => {
                let g = world_grid(world);
                let boxes = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                ctx.check("two glue clicks make a box", boxes.iter().any(|(lo, hi)| *lo == IVec3::new(10, 0, -10) && *hi == IVec3::new(11, 1, -9)), format!("{boxes:?}"));
                select(world, "white_block");
                look_at(world, Vec3::new(11.5, 2.0, -8.5));
                console(world, "fly");
            }
            // Aim again once landed (falling time depends on frame length).
            40 => look_at(world, Vec3::new(11.5, 2.0, -8.5)),
            42 => click(world, MouseButton::Right, true),
            43 => click(world, MouseButton::Right, false),
            46 => {
                let open = *world.resource::<State<Ui>>().get() == Ui::Assemble;
                ctx.check("RMB on the toggle opens its lever", open, "");
                screenshot(world, "9x_physics_lever");
            }
            // Drag the knob to «Физика» and let go.
            48 => {
                let mut l = world.resource_mut::<crate::toggle_ui::Lever>();
                l.value = 1.0;
                l.dragging = false;
            }
            56 => {
                let c = contraptions(world);
                ctx.check("glued cube (8 blocks) is physical, the unglued stone stays", c.len() == 1 && c[0].3 == 8 && !get_block(world, IVec3::new(13, 0, -10)).is_air(), format!("{:?}", c.iter().map(|c| c.3).collect::<Vec<_>>()));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("physgun: grab, lift, freeze, unfreeze", |world, ctx| {
        let Some((body, pos)) = body_center(world) else {
            ctx.check("cube exists", false, "");
            return Step::Next;
        };
        let gi = |world: &mut World, f: &dyn Fn(&mut crate::tools::guns::GunInput)| {
            let mut g = world.resource_mut::<crate::tools::guns::GunInput>();
            g.scripted = true;
            f(&mut g);
        };
        match ctx.frames {
            1 => {
                select(world, "physgun");
                ctx.vars.insert("cy".into(), pos.y);
            }
            2 => look_at(world, pos),
            6 => gi(world, &|g| {
                g.left = true;
                g.left_pressed = true;
            }),
            8 => {
                let held = world.resource::<crate::tools::guns::Held>().body;
                ctx.check("physgun grabs the cube", held == Some(body), format!("{held:?}"));
                // Aim higher: the cube follows.
                let (p, _) = player(world);
                look_at(world, p + Vec3::new(0.0, 4.0, -4.0));
            }
            20 => {
                ctx.check("held cube follows the aim up", pos.y > ctx.vars["cy"] + 1.5, format!("{:.2} → {:.2}", ctx.vars["cy"], pos.y));
                screenshot(world, "40_physgun");
                gi(world, &|g| g.right_pressed = true);
            }
            22 => gi(world, &|g| g.left = false),
            24 => ctx.vars.insert("fy".into(), pos.y).map(|_| ()).unwrap_or(()),
            34 => {
                let frozen = world.get::<crate::tools::guns::Frozen>(body).is_some();
                ctx.check("RMB freezes it in the air", frozen && (pos.y - ctx.vars["fy"]).abs() < 0.05 && pos.y > 2.5, format!("y {:.2}", pos.y));
                look_at(world, pos);
            }
            35..=37 => look_at(world, pos),
            38 => gi(world, &|g| g.unfreeze = true),
            50 => {
                let frozen = world.get::<crate::tools::guns::Frozen>(body).is_some();
                ctx.check("R unfreezes: it falls", !frozen && pos.y < ctx.vars["fy"] - 1.0, format!("y {:.2}", pos.y));
                world.resource_mut::<crate::tools::guns::GunInput>().scripted = false;
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("gravity gun: punt and pull", |world, ctx| {
        let Some((body, pos)) = body_center(world) else {
            ctx.check("cube exists", false, "");
            return Step::Next;
        };
        let gi = |world: &mut World, f: &dyn Fn(&mut crate::tools::guns::GunInput)| {
            let mut g = world.resource_mut::<crate::tools::guns::GunInput>();
            g.scripted = true;
            f(&mut g);
        };
        match ctx.frames {
            1 => {
                select(world, "gravity_gun");
                // Walk up to the cube.
                teleport(world, Vec3::new(pos.x, 0.0, pos.z + 4.0));
            }
            3 => {
                let (p, _) = player(world);
                ctx.vars.insert("d0".into(), p.distance(pos));
                info!("gravgun start: player {p:.2}, cube {pos:.2}");
            }
            2..=5 => look_at(world, pos),
            6 => gi(world, &|g| g.left_pressed = true),
            20 => {
                let (p, _) = player(world);
                info!("gravgun after punt: player {p:.2}, cube {pos:.2}");
                let d = p.distance(pos);
                ctx.check("LMB punts the cube away", d > ctx.vars["d0"] + 1.5, format!("{:.1} → {d:.1} m", ctx.vars["d0"]));
                look_at(world, pos);
            }
            24 => gi(world, &|g| g.right_pressed = true),
            40 => {
                let held = world.resource::<crate::tools::guns::Held>().body;
                let (p, _) = player(world);
                ctx.check("RMB pulls it in and holds it", held == Some(body) && p.distance(pos) < 4.5, format!("{held:?}, {:.1} m", p.distance(pos)));
                screenshot(world, "41_gravity_gun");
                gi(world, &|g| g.right_pressed = true);
            }
            44 => {
                world.resource_mut::<crate::tools::guns::GunInput>().scripted = false;
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn build2(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("slopes: walk up a 14° block ramp", |world, ctx| {
        match ctx.frames {
            1 => {
                // Low side towards +Z (facing 4): climbing towards −Z.
                for (i, k) in ["steel_slope_q1", "steel_slope_q2", "steel_slope_q3", "steel_slope_q4"].iter().enumerate() {
                    for x in 20..23 {
                        set_block(world, IVec3::new(x, 0, -42 - i as i32), k, 4);
                    }
                }
                for x in 20..23 {
                    for z in -50..-46 {
                        set_block(world, IVec3::new(x, 0, z), "steel_slab", 3);
                    }
                    set_block(world, IVec3::new(x, 0, -51), "steel_slope", 4);
                }
                teleport(world, Vec3::new(21.5, 0.0, -39.5));
            }
            6 => {
                look_at(world, Vec3::new(21.5, 1.0, -48.0));
                input(world, |m| m.axis = Vec2::Y);
            }
            _ => {}
        }
        {
            let (p, _) = player(world);
            let top = ctx.vars.get("ramp_y").copied().unwrap_or(0.0).max(p.y);
            ctx.vars.insert("ramp_y".into(), top);
        }
        if ctx.frames == 14 {
            screenshot(world, "50_slopes");
        }
        if ctx.frames > 22 && ctx.t > 4.5 {
            let (p, _) = player(world);
            // Feet ≥ 0.9 m: up the ¼ steps, over the slabs, onto the 45° top.
            ctx.check("the player walks up the smooth slope ramp", ctx.vars["ramp_y"] > 1.75, format!("highest centre y {:.2}, now {:.2}", ctx.vars["ramp_y"], p.y));
            input(world, |m| *m = MoveInput { scripted: true, ..default() });
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("headlight: each key press toggles it", |world, ctx| {
        let lamp = IVec3::new(26, 1, -40);
        let on = |world: &mut World| block_entity(world, lamp).and_then(|e| world.get::<crate::lights::Headlight>(e)).is_some_and(|h| h.on);
        let press = |world: &mut World, down: bool| {
            let mut pk = world.resource_mut::<crate::signal::PanelKeys>();
            pk.scripted = true;
            pk.held = if down { vec![KeyCode::KeyF] } else { vec![] };
        };
        match ctx.frames {
            1 => {
                set_block(world, IVec3::new(26, 1, -41), "steel_block", 2);
                set_block(world, IVec3::new(26, 0, -41), "steel_block", 2);
                set_block(world, lamp, "headlight", 4);
                set_block(world, lamp + IVec3::Y, "sensor", 3);
                set_block(world, IVec3::new(28, 0, -38), "seat", 4);
                set_block(world, IVec3::new(28, 0, -39), "control_panel", 4);
                teleport(world, Vec3::new(27.5, 0.0, -35.5));
            }
            4 => {
                let g = world_grid(world);
                set_param(world, lamp + IVec3::Y, "key_pos", 8.0); // F
                let e = block_entity(world, lamp + IVec3::Y).unwrap();
                world.get_mut::<crate::signal::Linked>(e).unwrap().0 = Some(crate::signal::PanelRef { grid: g, cell: IVec3::new(28, 0, -39) });
                // Sit on the seat (scripted).
                let seat = block_entity(world, IVec3::new(28, 0, -38)).unwrap();
                let p = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                world.entity_mut(p).insert((crate::player::Seated { seat }, avian3d::prelude::RigidBody::Kinematic, avian3d::prelude::ColliderDisabled));
                console(world, "time set night");
            }
            8 => {
                ctx.check("headlight starts off", !on(world), "");
                press(world, true);
            }
            12 => press(world, false),
            16 => {
                ctx.check("first press: on (stays on after release)", on(world), "");
                world.resource_mut::<CameraView>().0 = View::Behind;
            }
            24 => screenshot(world, "51_headlight_night"),
            26 => press(world, true),
            30 => press(world, false),
            34 => {
                ctx.check("second press: off", !on(world), "");
                world.resource_mut::<crate::signal::PanelKeys>().scripted = false;
                world.resource_mut::<CameraView>().0 = View::First;
                console(world, "time set day");
                input(world, |m| m.crouch = true);
            }
            38 => {
                input(world, |m| m.crouch = false);
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("blueprint: copy, hologram, rotate, paste, save/load", |world, ctx| {
        let base = IVec3::new(32, 0, -44);
        let clip = |world: &World| world.resource::<crate::tools::blueprint::Clipboard>().schematic.clone();
        match ctx.frames {
            1 => {
                for x in 0..3 {
                    set_block(world, base + IVec3::new(x, 0, 0), "steel_block", 2);
                }
                set_block(world, base + IVec3::new(0, 1, 0), "creative_motor", 0);
                set_block(world, base + IVec3::new(1, 1, 0), "shaft", 0);
                set_block(world, base + IVec3::new(2, 1, 0), "mechanical_display", 4);
                set_block(world, base + IVec3::new(0, 2, 0), "telemetry_sensor", 3);
                let g = world_grid(world);
                world.entity_mut(g).insert(crate::tools::glue::Glue(vec![(base, base + IVec3::new(2, 2, 0))]));
                select(world, "blueprint");
                teleport(world, base.as_vec3() + Vec3::new(1.5, 0.0, 4.5));
            }
            4 => {
                set_param(world, base + IVec3::new(0, 1, 0), "speed", 64.0);
                let g = world_grid(world);
                let e = block_entity(world, base + IVec3::new(0, 2, 0)).unwrap();
                world.get_mut::<crate::signal::Linked>(e).unwrap().0 = Some(crate::signal::PanelRef { grid: g, cell: base + IVec3::new(2, 1, 0) });
                look_at(world, base.as_vec3() + Vec3::new(1.5, 0.5, 1.0));
            }
            8 => click(world, MouseButton::Left, true),
            9 => click(world, MouseButton::Left, false),
            12 => {
                let s = clip(world);
                let n = s.as_ref().map(|s| s.blocks.len()).unwrap_or(0);
                ctx.check("LMB copies the glued build (7 blocks)", n == 7, format!("{n}"));
                let motor = s.as_ref().and_then(|s| s.blocks.iter().find(|b| b.key == "creative_motor").cloned());
                ctx.check("settings are copied", motor.is_some_and(|m| m.params.iter().any(|(k, v)| k == "speed" && *v == 64.0)), "");
                ctx.check("links are copied", s.as_ref().is_some_and(|s| s.links.len() == 1), "");
                ctx.check("copy written to schematics/saved_contraption.ron", std::path::Path::new("schematics/saved_contraption.ron").exists(), "");
                // Aim at the floor further away for the hologram.
                look_at(world, base.as_vec3() + Vec3::new(1.5, -0.5, 8.5));
                key(world, KeyCode::KeyR, true);
            }
            13 => key(world, KeyCode::KeyR, false),
            20 => screenshot(world, "52_blueprint_hologram"),
            22 => click(world, MouseButton::Right, true),
            23 => click(world, MouseButton::Right, false),
            32 => {
                // Rotated 90°: the 3-block base now runs along Z.
                let a = world.resource::<Target>().hit;
                let placed: Vec<IVec3> = world.get::<VoxelGrid>(world_grid(world)).unwrap().cells().into_iter().filter(|(c, cell)| cell.id == BlockId::k("creative_motor") && c.z > base.z + 2 && c.z < base.z + 12 && (c.x - base.x).abs() < 6).map(|(c, _)| c).collect();
                ctx.check("RMB pastes the (turned) copy", placed.len() == 1, format!("{placed:?} aim {:?}", a.map(|a| a.cell)));
                if let Some(m) = placed.first() {
                    let speed = block_entity(world, *m).and_then(|e| world.get::<crate::mech::Params>(e)).map(|p| p.0[0]).unwrap_or(0.0);
                    ctx.check("pasted motor keeps its settings", speed == 64.0, format!("{speed}"));
                    let f = get_block(world, *m).facing;
                    ctx.check("pasted motor is turned", crate::grid::facing_dir(f) == IVec3::NEG_Z, format!("{f}"));
                    let sensor = block_entity(world, *m + IVec3::Y);
                    let link = sensor.and_then(|e| world.get::<crate::signal::Linked>(e)).and_then(|l| l.0);
                    ctx.check("pasted telemetry link points at the pasted display", link.is_some_and(|l| get_block(world, l.cell).id == BlockId::k("mechanical_display") && l.cell.z > base.z + 2), format!("{link:?}"));
                }
                console(world, "save autotest_bp");
            }
            36 => {
                world.resource_mut::<crate::tools::blueprint::Clipboard>().schematic = None;
                console(world, "load autotest_bp");
            }
            40 => {
                let n = clip(world).map(|s| s.blocks.len()).unwrap_or(0);
                ctx.check("save / load a named blueprint", n == 7, format!("{n}"));
                let _ = std::fs::remove_file("schematics/autotest_bp.ron");
                select(world, "white_block");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// Builds a small test car with its front at `front` (driving towards −Z),
/// glued, with a seat, a panel (W = throttle) and a physics toggle; turns
/// physics on. Returns the toggle cell.
fn test_car(world: &mut World, front: IVec3) -> IVec3 {
    let o = front + IVec3::new(-1, 0, 31); // the layout below is relative to the old car
    let at = |v: IVec3| o + v;
    for x in 0..3 {
        for z in -30..=-26 {
            set_block(world, at(IVec3::new(x, 1, z)), "steel_block", 2);
        }
    }
    // The drivetrain sits on top of the rear axle (nothing hangs low
    // behind it: no tail strike on steep ramps).
    for z in [-30, -26] {
        set_block(world, at(IVec3::new(0, 0, z)), "suspension", 1);
        set_block(world, at(IVec3::new(2, 0, z)), "suspension", 0);
        set_block(world, at(IVec3::new(-1, 0, z)), "wheel", 1);
        set_block(world, at(IVec3::new(3, 0, z)), "wheel", 0);
    }
    set_block(world, at(IVec3::new(1, 0, -26)), "cogwheel", 0);
    set_block(world, at(IVec3::new(1, 1, -26)), "cogwheel", 0);
    set_block(world, at(IVec3::new(2, 1, -26)), "speed_controller", 1);
    set_block(world, at(IVec3::new(3, 1, -26)), "creative_motor", 1);
    set_block(world, at(IVec3::new(2, 2, -26)), "sensor", 3);
    set_block(world, at(IVec3::new(1, 2, -27)), "seat", 5);
    set_block(world, at(IVec3::new(1, 2, -28)), "control_panel", 4);
    set_block(world, at(IVec3::new(1, 2, -26)), "physics_toggle", 2);
    set_block(world, at(IVec3::new(0, 2, -30)), "speedometer", 4);
    let g = world_grid(world);
    let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
    glue.push((at(IVec3::new(-1, 0, -31)), at(IVec3::new(3, 2, -26))));
    world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
    at(IVec3::new(1, 2, -26))
}

/// Removes every physical body left by earlier stages.
fn clear_bodies(world: &mut World) {
    let bodies: Vec<Entity> = world.query_filtered::<Entity, With<crate::contraption::Contraption>>().iter(world).collect();
    for e in bodies {
        if let Ok(e) = world.get_entity_mut(e) {
            e.despawn();
        }
    }
}

fn ground(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("proving ground: overview", |world, ctx| {
        match ctx.frames {
            1 => {
                console(world, "fly");
                teleport(world, Vec3::new(-24.0, 9.0, 26.0));
            }
            4 => look_at(world, Vec3::new(-46.0, 0.0, -2.0)),
            16 => screenshot(world, "65_ground_overview"),
            20 => {
                console(world, "fly");
                teleport(world, Vec3::new(0.5, 0.0, 0.5));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    // lane, label, needed height
    for (lane, label) in [(0, "smooth 15° ramp"), (1, "smooth 30° ramp"), (2, "block 27° ramp"), (3, "block 14° ramp"), (4, "washboard")] {
        steps.push(step(&format!("proving ground: {label}"), move |world, ctx| {
            let x = crate::world::lane_x(lane) + 2;
            let front = IVec3::new(x, 0, crate::world::LANE_START_Z + 9);
            match ctx.frames {
                1 => {
                    clear_bodies(world);
                    world.resource_mut::<CameraView>().0 = View::First;
                    let toggle = test_car(world, front);
                    ctx.vars.insert("tx".into(), toggle.x as f32);
                    ctx.vars.insert("ty".into(), toggle.y as f32);
                    ctx.vars.insert("tz".into(), toggle.z as f32);
                    ctx.vars.insert("maxy".into(), -9.0);
                    ctx.vars.insert("minup".into(), 1.0);
                    teleport(world, Vec3::new(x as f32 + 6.5, 0.0, front.z as f32 + 3.0));
                }
                4 => {
                    let o = front + IVec3::new(-1, 0, 31);
                    set_param(world, o + IVec3::new(3, 1, -26), "speed", 128.0);
                    set_param(world, o + IVec3::new(2, 1, -26), "speed", 0.0);
                    // This layout drives towards −Z with a negative speed.
                    set_param(world, o + IVec3::new(2, 1, -26), "signal_speed", -64.0);
                    let g = world_grid(world);
                    let e = block_entity(world, o + IVec3::new(2, 2, -26)).unwrap();
                    world.get_mut::<crate::signal::Linked>(e).unwrap().0 = Some(crate::signal::PanelRef { grid: g, cell: o + IVec3::new(1, 2, -28) });
                    let cell = IVec3::new(ctx.vars["tx"] as i32, ctx.vars["ty"] as i32, ctx.vars["tz"] as i32);
                    world.write_message(crate::interact::UseBlock { grid: g, cell, mech: crate::content::Mech::PhysicsToggle });
                }
                10 => {
                    // Drive: a scripted seat on the car's seat + W.
                    let seat = car_block(world, front + IVec3::new(-1, 0, 31) + IVec3::new(1, 2, -27));
                    let p = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                    if let Some(seat) = seat {
                        world.entity_mut(p).insert((crate::player::Seated { seat }, avian3d::prelude::RigidBody::Kinematic, avian3d::prelude::ColliderDisabled));
                    }
                    let mut pk = world.resource_mut::<crate::signal::PanelKeys>();
                    pk.scripted = true;
                    pk.held = vec![KeyCode::KeyW];
                    world.resource_mut::<CameraView>().0 = View::Behind;
                }
                12 => {
                    // Look where the car goes (the camera is behind the player).
                    let (p, _) = player(world);
                    look_at(world, p + Vec3::new(0.0, -0.5, -10.0));
                }
                13 => {
                    // Forward is −Z: the right wheel (facing +X) must turn
                    // about −X; flip the throttle direction if needed.
                    let o = front + IVec3::new(-1, 0, 31);
                    let rpm = car_block(world, o + IVec3::new(3, 0, -26)).and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(0.0);
                    if rpm > 0.0
                        && let Some(c) = car_block(world, o + IVec3::new(2, 1, -26))
                    {
                        let mut p = world.get_mut::<crate::mech::Params>(c).unwrap();
                        p.0[1] = -p.0[1];
                    }
                }
                _ => {}
            }
            if ctx.frames > 10
                && let Some((_, _, rot, _)) = contraptions(world).first().copied()
                && let Some((_, com)) = body_center(world)
            {
                let up = (rot * Vec3::Y).y;
                if !ctx.vars.contains_key("y0") {
                    ctx.vars.insert("y0".into(), com.y);
                    ctx.vars.insert("z0".into(), com.z);
                }
                ctx.vars.insert("maxy".into(), ctx.vars["maxy"].max(com.y - ctx.vars["y0"]));
                ctx.vars.insert("minup".into(), ctx.vars["minup"].min(up));
                if ctx.frames == 28 {
                    screenshot(world, &format!("6{lane}_ground_{lane}"));
                }

            }
            if ctx.frames > 10 && ctx.t > 9.0 {
                let c = contraptions(world).first().copied();
                let (maxy, minup) = (ctx.vars["maxy"], ctx.vars["minup"]);
                // −Z travel of the centre of mass since the drive started.
                let moved = body_center(world).map(|(_, p)| ctx.vars["z0"] - p.z).unwrap_or(0.0);
                if lane == 4 {
                    ctx.check(&format!("{label}: the car drives over it upright"), moved > 10.0 && minup > 0.8, format!("moved {moved:.1} m, min up {minup:.2}"));
                } else {
                    ctx.check(&format!("{label}: the car climbs to the plateau"), maxy > 2.6 && minup > 0.6, format!("max y {maxy:.2}, min up {minup:.2}, moved {moved:.1} m"));
                }
                // Clean up: stand up, remove the car.
                let mut pk = world.resource_mut::<crate::signal::PanelKeys>();
                pk.held.clear();
                pk.scripted = false;
                let p = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                world.entity_mut(p).remove::<(crate::player::Seated, avian3d::prelude::ColliderDisabled)>().insert(avian3d::prelude::RigidBody::Dynamic);
                world.resource_mut::<CameraView>().0 = View::First;
                if let Some((e, ..)) = c {
                    world.entity_mut(e).despawn();
                }
                ctx.vars.remove("y0");
                ctx.vars.remove("z0");
                teleport(world, Vec3::new(0.5, 0.0, 0.5));
                return Step::Next;
            }
            Step::Wait
        }));
    }
}

pub fn key(world: &mut World, k: KeyCode, down: bool) {
    let mut m = world.resource_mut::<ButtonInput<KeyCode>>();
    if down { m.press(k) } else { m.release(k) }
}

pub struct AutotestPlugin {
    on: bool,
    shots: Option<String>,
}

impl AutotestPlugin {
    pub fn from_args(args: &[String]) -> Self {
        let on = args.iter().any(|a| a == "--autotest");
        let shots = args.iter().position(|a| a == "--shots").and_then(|i| args.get(i + 1).cloned());
        Self { on, shots }
    }
}

impl Plugin for AutotestPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Shots(self.shots.clone()));
        if self.on {
            // Straight into the game (no main menu).
            app.insert_resource(build())
                .add_systems(Startup, |mut next: ResMut<NextState<Ui>>| next.set(Ui::Game))
                .add_systems(PreUpdate, run.after(bevy::input::InputSystems))
                .add_systems(Last, |mut t: MessageReader<crate::core::Toast>| {
                    for m in t.read() {
                        info!("[TOAST] {}", m.0);
                    }
                });
        }
    }
}

/// Moving parts (bearing / piston): (grid, joint body1, angle about the
/// mount axis or offset along it).
fn moving_parts(world: &mut World) -> Vec<(Entity, Entity, Quat, Vec3)> {
    let mut q = world.query::<(Entity, &crate::mounts::MountedOn, &GlobalTransform)>();
    let list: Vec<(Entity, crate::mounts::MountedOn, Transform)> = q.iter(world).map(|(e, m, t)| (e, *m, t.compute_transform())).collect();
    list.into_iter()
        .map(|(e, m, t)| {
            let b1 = world.get::<avian3d::prelude::RevoluteJoint>(m.joint).map(|j| j.body1).or_else(|| world.get::<avian3d::prelude::PrismaticJoint>(m.joint).map(|j| j.body1)).unwrap_or(Entity::PLACEHOLDER);
            (e, b1, t.rotation, t.translation)
        })
        .collect()
}

fn mounts(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("bearing: the motor turns a glued platform", |world, ctx| {
        match ctx.frames {
            1 => {
                clear_bodies(world);
                set_block(world, IVec3::new(24, 0, -24), "creative_motor", 2);
                set_block(world, IVec3::new(24, 1, -24), "mechanical_bearing", 2);
                for x in 22..=26 {
                    set_block(world, IVec3::new(x, 2, -24), "steel_block", 2);
                }
                let g = world_grid(world);
                let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                glue.push((IVec3::new(22, 2, -24), IVec3::new(26, 2, -24)));
                world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
                teleport(world, Vec3::new(24.5, 0.0, -17.5));
            }
            4 => {
                set_param(world, IVec3::new(24, 0, -24), "speed", 16.0);
                look_at(world, Vec3::new(24.5, 2.0, -24.0));
                ctx.vars.insert("t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames > 4 && !ctx.vars.contains_key("done") && ctx.t > ctx.vars["t0"] + 3.0 {
            let parts = moving_parts(world);
            let wg = world_grid(world);
            let turned = parts.first().map(|p| p.2.angle_between(Quat::IDENTITY).to_degrees()).unwrap_or(0.0);
            ctx.check("the platform is a separate body hinged to the world", parts.len() == 1 && parts[0].1 == wg, format!("{} parts", parts.len()));
            ctx.check("it turns about the bearing axis", turned > 20.0 && parts.first().is_some_and(|p| (p.2 * Vec3::Y).y > 0.99), format!("{turned:.0}°"));
            ctx.check("its blocks left the world grid", get_block(world, IVec3::new(22, 2, -24)).is_air() || turned < 1.0, "");
            screenshot(world, "70_bearing");
            ctx.vars.insert("done".into(), ctx.t);
        }
        if ctx.vars.get("done").is_some_and(|t| ctx.t > t + 0.5) {
            // Clean up: the platform stopped at an odd angle stays a body.
            for (e, ..) in moving_parts(world) {
                world.entity_mut(e).despawn();
            }
            clear_area(world, IVec3::new(22, 0, -24), IVec3::new(26, 2, -24));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("piston: pushes out 2 blocks and pulls back", |world, ctx| {
        match ctx.frames {
            1 => {
                // Motor (+X) → piston (+X) → a glued 2-block column.
                set_block(world, IVec3::new(28, 0, -30), "creative_motor", 0);
                set_block(world, IVec3::new(29, 0, -30), "mechanical_piston", 0);
                set_block(world, IVec3::new(30, 0, -30), "steel_block", 2);
                set_block(world, IVec3::new(30, 1, -30), "steel_block", 2);
                let g = world_grid(world);
                let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                glue.push((IVec3::new(30, 0, -30), IVec3::new(30, 1, -30)));
                world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
                teleport(world, Vec3::new(31.5, 0.0, -24.5));
            }
            4 => {
                look_at(world, Vec3::new(32.0, 1.0, -30.0));
                set_param(world, IVec3::new(29, 0, -30), "length", 2.0);
                set_param(world, IVec3::new(28, 0, -30), "speed", 60.0);
                // Counter-clockwise from the output face = turning +X.
                set_param(world, IVec3::new(28, 0, -30), "direction", 1.0);
                ctx.vars.insert("t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames == 12 {
            screenshot(world, "71_piston_moving");
        }
        if ctx.frames > 4 && !ctx.vars.contains_key("out") && ctx.t > ctx.vars["t0"] + 4.5 {
            let at = |world: &World, x: i32, y: i32| get_block(world, IVec3::new(x, y, -30)).id;
            let steel = BlockId::k("steel_block");
            let pole = BlockId::k("piston_pole");
            let ok = at(world, 32, 0) == steel && at(world, 32, 1) == steel && at(world, 30, 0) == pole && at(world, 31, 0) == pole;
            ctx.check("extended by its stroke: the column is back in the grid 2 blocks out, poles behind it", ok && moving_parts(world).is_empty(), format!("{:?} {:?} parts {}", at(world, 32, 0), at(world, 30, 0), moving_parts(world).len()));
            screenshot(world, "72_piston_out");
            set_param(world, IVec3::new(28, 0, -30), "direction", 0.0);
            ctx.vars.insert("out".into(), ctx.t);
        }
        if ctx.vars.get("out").is_some_and(|t| ctx.t > t + 4.5) {
            let at = |world: &World, x: i32, y: i32| get_block(world, IVec3::new(x, y, -30)).id;
            let ok = at(world, 30, 0) == BlockId::k("steel_block") && at(world, 31, 0).is_air() && at(world, 32, 0).is_air();
            ctx.check("the other way: pulled back, poles gone", ok, format!("{:?} {:?}", at(world, 30, 0), at(world, 31, 0)));
            set_param(world, IVec3::new(28, 0, -30), "speed", 0.0);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("bearing on a physical body: a crane arm", |world, ctx| {
        let base = IVec3::new(40, 0, -40);
        match ctx.frames {
            1 => {
                for x in 0..3 {
                    for z in 0..3 {
                        set_block(world, base + IVec3::new(x, 0, z), "steel_block", 2);
                    }
                }
                set_block(world, base + IVec3::new(1, 1, 1), "creative_motor", 2);
                for x in 1..=4 {
                    set_block(world, base + IVec3::new(x, 3, 1), "steel_block", 2);
                }
                set_block(world, base + IVec3::new(0, 1, 0), "physics_toggle", 2);
                let g = world_grid(world);
                let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                glue.push((base, base + IVec3::new(4, 3, 2)));
                world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
                ctx.vars.insert("cells0".into(), world_cells(world) as f32);
                teleport(world, Vec3::new(42.0, 0.0, -32.0));
            }
            3 => {
                // Stopped while it becomes physical, then turned on.
                set_param(world, base + IVec3::new(1, 1, 1), "speed", 0.0);
                set_block(world, base + IVec3::new(1, 2, 1), "mechanical_bearing", 2);
                ctx.vars.insert("cells0".into(), world_cells(world) as f32);
                look_at(world, Vec3::new(42.0, 2.0, -39.0));
            }
            6 => {
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: base + IVec3::new(0, 1, 0), mech: crate::content::Mech::PhysicsToggle });
            }
            12 => {
                let c = contraptions(world);
                ctx.check("the base with the crane becomes one body", c.len() == 1 && c[0].3 >= 15, format!("{:?}", c.iter().map(|c| c.3).collect::<Vec<_>>()));
                if let Some(m) = car_block(world, base + IVec3::new(1, 1, 1))
                    && let Some(mut p) = world.get_mut::<crate::mech::Params>(m)
                {
                    p.0[0] = 16.0;
                }
                ctx.vars.insert("crane_t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames > 12 && !ctx.vars.contains_key("crane_checked") && ctx.t > ctx.vars["crane_t0"] + 3.0 {
            let parts = moving_parts(world);
            let c = contraptions(world);
            let body = c.iter().find(|c| parts.iter().all(|p| p.0 != c.0)).map(|c| c.0);
            ctx.check("the arm is hinged to the physical base", parts.len() == 1 && Some(parts[0].1) == body, format!("{} parts", parts.len()));
            screenshot(world, "73_crane");
            if let Some(b) = body
                && let Some(m) = world.get::<VoxelGrid>(b).and_then(|g| g.entities.get(&(base + IVec3::new(1, 1, 1))).copied())
                && let Some(mut p) = world.get_mut::<crate::mech::Params>(m)
            {
                p.0[0] = 0.0;
            }
            ctx.vars.insert("crane_checked".into(), 1.0);
            ctx.vars.insert("crane_toggle".into(), body.map(|b| b.to_bits() as f32).unwrap_or(0.0));
            if let Some(b) = body {
                let toggle_cell = base + IVec3::new(0, 1, 0);
                world.write_message(crate::interact::UseBlock { grid: b, cell: toggle_cell, mech: crate::content::Mech::PhysicsToggle });
            }
            ctx.vars.insert("crane_t1".into(), ctx.t);
        }
        if ctx.vars.get("crane_t1").is_some_and(|t| ctx.t > t + 1.0) {
            let n = contraptions(world).len();
            ctx.check("physics off: the arm and the base go back into the grid", n == 0 && (world_cells(world) as f32 - ctx.vars["cells0"]).abs() < 0.5, format!("{n} bodies, {} vs {}", world_cells(world), ctx.vars["cells0"]));
            return Step::Next;
        }
        Step::Wait
    }));
}

fn store_at(world: &mut World, grid: Entity, c: IVec3) -> Option<crate::fluids::FluidStore> {
    let e = world.get::<VoxelGrid>(grid)?.entities.get(&c).copied()?;
    world.get::<crate::fluids::FluidStore>(e).copied()
}

fn glue_box(world: &mut World, lo: IVec3, hi: IVec3) {
    let g = world_grid(world);
    let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
    glue.push((lo, hi));
    world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
}

fn fluids(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("diesel engine: fuel through a port and a pipe starts it", |world, ctx| {
        let o = IVec3::new(-20, 0, 26);
        let eng = o + IVec3::new(4, 0, 0);
        match ctx.frames {
            1 => {
                set_block(world, o, "fluid_port", 2);
                set_block(world, o + IVec3::X, "fluid_pipe", 0);
                set_block(world, o + IVec3::new(2, 0, 0), "fluid_tank", 2);
                set_block(world, o + IVec3::new(3, 0, 0), "fluid_pipe", 0);
                // Output to +X, fuel in from the pipe at its back.
                set_block(world, eng, "diesel_engine", 0);
                set_block(world, eng + IVec3::X, "shaft", 0);
                select(world, "bucket_diesel");
                teleport(world, Vec3::new(-18.0, 0.0, 30.5));
            }
            4 => {
                let ok = block_entity(world, eng).and_then(|e| world.get::<crate::engine::Engine>(e).copied()).is_some_and(|e| !e.running);
                ctx.check("without diesel the engine stands", ok && rpm(world, eng + IVec3::X) == 0.0, "");
                look_at(world, Vec3::new(-19.5, 1.0, 26.5));
            }
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the filling port", a.is_some_and(|a| a.cell == o), format!("{:?}", a.map(|a| a.cell)));
                click(world, MouseButton::Right, true);
            }
            9 => click(world, MouseButton::Right, false),
            // The port holds one bucket: wait until it has poured into the tank.
            30 => click(world, MouseButton::Right, true),
            31 => click(world, MouseButton::Right, false),
            34 => {
                let g = world_grid(world);
                let port = store_at(world, g, o).map(|s| s.amount).unwrap_or(0.0);
                let tank = store_at(world, g, o + IVec3::new(2, 0, 0)).map(|s| s.amount).unwrap_or(0.0);
                let engine = store_at(world, g, eng).map(|s| s.amount).unwrap_or(0.0);
                ctx.check("two buckets: 2000 mB of diesel in the network", (port + tank + engine - 2000.0).abs() < 60.0, format!("port {port:.0}, tank {tank:.0}, engine {engine:.0}"));
                ctx.vars.insert("t0".into(), ctx.t);
                look_at(world, Vec3::new(-15.5, 0.8, 26.5));
            }
            _ => {}
        }
        if ctx.frames > 34 && !ctx.vars.contains_key("ran") && ctx.t > ctx.vars["t0"] + 3.0 {
            let g = world_grid(world);
            let running = block_entity(world, eng).and_then(|e| world.get::<crate::engine::Engine>(e).copied()).is_some_and(|e| e.running);
            let r = rpm(world, eng + IVec3::X).abs();
            ctx.check("the engine runs and turns its shaft", running && r == 64.0, format!("running {running}, {r} RPM"));
            let total = [o, o + IVec3::new(2, 0, 0), eng].iter().map(|c| store_at(world, g, *c).map(|s| s.amount).unwrap_or(0.0)).sum::<f32>();
            ctx.check("it burns diesel", total < 1990.0 && total > 1500.0, format!("{total:.0} mB left"));
            let smoke = world.query::<&Mesh3d>().iter(world).count();
            ctx.vars.insert("smoke".into(), smoke as f32);
            screenshot(world, "80_diesel_engine");
            // Block the exhaust: no air.
            set_block(world, eng + IVec3::Y, "stone", 2);
            ctx.vars.insert("ran".into(), ctx.t);
        }
        if ctx.vars.get("ran").is_some_and(|t| ctx.t > t + 1.5) {
            let stall = block_entity(world, eng).and_then(|e| world.get::<crate::engine::Engine>(e).copied()).and_then(|e| e.stall);
            ctx.check("exhaust blocked: it stalls for lack of air", stall == Some(crate::engine::Stall::NoAir) && rpm(world, eng + IVec3::X) == 0.0, format!("{stall:?}"));
            set_block(world, eng + IVec3::Y, "air", 2);
            select(world, "white_block");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("pump: moves fluid from one tank to another", |world, ctx| {
        let o = IVec3::new(-20, 0, 32);
        let (a, b) = (o, o + IVec3::new(4, 0, 0));
        match ctx.frames {
            1 => {
                set_block(world, a, "fluid_tank", 2);
                set_block(world, o + IVec3::X, "fluid_pipe", 0);
                set_block(world, o + IVec3::new(2, 0, 0), "mechanical_pump", 0);
                set_block(world, o + IVec3::new(3, 0, 0), "fluid_pipe", 0);
                set_block(world, b, "fluid_tank", 2);
                // Motor → cogwheel above the pump: it meshes with the pump.
                set_block(world, o + IVec3::new(2, 1, 0), "cogwheel", 0);
                set_block(world, o + IVec3::new(1, 1, 0), "creative_motor", 0);
                teleport(world, Vec3::new(-17.5, 0.0, 38.5));
            }
            4 => {
                let g = world_grid(world);
                if let Some(e) = world.get::<VoxelGrid>(g).and_then(|gr| gr.entities.get(&a).copied()) {
                    world.get_mut::<crate::fluids::FluidStore>(e).unwrap().add(crate::fluids::Fluid::Water, 8000.0);
                }
                set_param(world, o + IVec3::new(1, 1, 0), "speed", 64.0);
                look_at(world, Vec3::new(-17.5, 0.5, 32.5));
                ctx.vars.insert("t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames > 4 && !ctx.vars.contains_key("fwd") && ctx.t > ctx.vars["t0"] + 3.0 {
            let g = world_grid(world);
            let (sa, sb) = (store_at(world, g, a).map(|s| s.amount).unwrap_or(0.0), store_at(world, g, b).map(|s| s.amount).unwrap_or(0.0));
            let pr = rpm(world, o + IVec3::new(2, 0, 0)).abs();
            ctx.check("the cogwheel turns the pump", pr == 64.0, format!("{pr} RPM"));
            ctx.check("it pumps water forward into the second tank", sb > 1000.0 && (sa + sb - 8000.0).abs() < 1.0, format!("{sa:.0} → {sb:.0} mB"));
            screenshot(world, "81_pump");
            set_param(world, o + IVec3::new(2, 0, 0), "direction", 1.0);
            ctx.vars.insert("sb".into(), sb);
            ctx.vars.insert("fwd".into(), ctx.t);
        }
        if ctx.vars.get("fwd").is_some_and(|t| ctx.t > t + 2.0) {
            let g = world_grid(world);
            let sb = store_at(world, g, b).map(|s| s.amount).unwrap_or(0.0);
            ctx.check("direction «Назад»: it pumps back", sb < ctx.vars["sb"], format!("{:.0} → {sb:.0} mB", ctx.vars["sb"]));
            set_param(world, o + IVec3::new(1, 1, 0), "speed", 0.0);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("water: a raft floats, ballast sinks a submarine", |world, ctx| {
        // Raft of planks and a small submarine, both under water at first.
        let raft = IVec3::new(44, -4, 24);
        let sub = IVec3::new(52, -6, 30);
        match ctx.frames {
            1 => {
                for x in 0..3 {
                    for z in 0..3 {
                        set_block(world, raft + IVec3::new(x, 0, z), "planks", 2);
                    }
                }
                set_block(world, raft + IVec3::new(1, 1, 1), "physics_toggle", 2);
                glue_box(world, raft, raft + IVec3::new(2, 1, 2));
                // Submarine: pump (+Y, its back open to the water) under a
                // 2-block ballast column; motor → cogwheel beside the pump.
                set_block(world, sub, "mechanical_pump", 2);
                set_block(world, sub + IVec3::Y, "fluid_tank", 2);
                set_block(world, sub + IVec3::Y * 2, "fluid_tank", 2);
                set_block(world, sub + IVec3::X, "cogwheel", 2);
                set_block(world, sub + IVec3::new(1, -1, 0), "creative_motor", 2);
                for y in 1..=2 {
                    set_block(world, sub + IVec3::new(1, y, 0), "steel_block", 2);
                    set_block(world, sub + IVec3::new(-1, y, 0), "steel_block", 2);
                }
                set_block(world, sub + IVec3::new(0, 3, 0), "physics_toggle", 2);
                glue_box(world, sub + IVec3::new(-1, -1, 0), sub + IVec3::new(1, 3, 0));
                console(world, "fly");
                teleport(world, Vec3::new(48.0, 2.0, 18.0));
            }
            3 => {
                set_param(world, sub + IVec3::new(1, -1, 0), "speed", 0.0);
                for c in [sub + IVec3::Y, sub + IVec3::Y * 2] {
                    set_param(world, c, "mode", 1.0);
                }
            }
            5 => {
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: raft + IVec3::new(1, 1, 1), mech: crate::content::Mech::PhysicsToggle });
                world.write_message(crate::interact::UseBlock { grid: g, cell: sub + IVec3::new(0, 3, 0), mech: crate::content::Mech::PhysicsToggle });
                look_at(world, Vec3::new(48.0, -1.0, 28.0));
                ctx.vars.insert("t0".into(), ctx.t);
            }
            _ => {}
        }
        let bodies = |world: &mut World| -> Vec<(Entity, Vec3, usize)> {
            let mut q = world.query_filtered::<(Entity, &VoxelGrid, &avian3d::prelude::ComputedCenterOfMass, &GlobalTransform), With<crate::contraption::Contraption>>();
            q.iter(world).map(|(e, g, c, t)| (e, t.transform_point(c.0), g.cells().len())).collect()
        };
        if ctx.frames > 5 && !ctx.vars.contains_key("float") && ctx.t > ctx.vars["t0"] + 5.0 {
            let b = bodies(world);
            let raft_y = b.iter().find(|x| x.1.x < 49.0).map(|x| x.1.y).unwrap_or(-99.0);
            let sub_y = b.iter().find(|x| x.1.x >= 49.0).map(|x| x.1.y).unwrap_or(-99.0);
            ctx.check("the planks raft floats up to the surface", raft_y > -2.0, format!("y {raft_y:.2}"));
            ctx.check("the submarine with empty ballast floats too", sub_y > -4.0, format!("y {sub_y:.2}"));
            screenshot(world, "82_floating");
            // Fill the ballast from the water outside.
            if let Some(m) = b.iter().find(|x| x.1.x >= 49.0).and_then(|x| world.get::<VoxelGrid>(x.0).and_then(|g| g.entities.get(&(sub + IVec3::new(1, -1, 0))).copied()))
                && let Some(mut p) = world.get_mut::<crate::mech::Params>(m)
            {
                p.0[0] = 256.0;
            }
            ctx.vars.insert("sub_y".into(), sub_y);
            ctx.vars.insert("float".into(), ctx.t);
        }
        if ctx.vars.get("float").is_some_and(|t| ctx.t > t + 14.0) {
            let b = bodies(world);
            let sub = b.iter().find(|x| x.1.x >= 49.0).copied();
            let water = sub.map(|s| {
                let g = world.get::<VoxelGrid>(s.0).unwrap().clone();
                g.entities.values().filter_map(|e| world.get::<crate::fluids::FluidStore>(*e)).map(|f| f.amount).sum::<f32>()
            });
            let y = sub.map(|s| s.1.y).unwrap_or(0.0);
            ctx.check("the pump fills the ballast tanks with water from outside", water.unwrap_or(0.0) > 15000.0, format!("{:.0} mB", water.unwrap_or(0.0)));
            ctx.check("heavier than the water it displaces: it sinks", y < ctx.vars["sub_y"] - 1.0, format!("y {:.2} → {y:.2}", ctx.vars["sub_y"]));
            teleport(world, Vec3::new(48.0, 2.0, 18.0));
            look_at(world, Vec3::new(52.0, -6.0, 30.0));
            screenshot(world, "83_submarine_sinks");
            for (e, ..) in b {
                world.entity_mut(e).despawn();
            }
            console(world, "fly");
            teleport(world, Vec3::new(0.5, 0.0, 0.5));
            return Step::Next;
        }
        Step::Wait
    }));
}

fn logic_state(world: &mut World, c: IVec3) -> crate::logic::LogicState {
    block_entity(world, c).and_then(|e| world.get::<crate::logic::LogicState>(e).copied()).unwrap_or_default()
}

fn logic(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("logic: lever → red wire → flasher; the blue line stays off", |world, ctx| {
        let l = IVec3::new(0, 0, 44);
        match ctx.frames {
            1 => {
                set_block(world, l, "lever", 3);
                for x in 1..=4 {
                    set_block(world, l + IVec3::new(x, 0, 0), "wire", 3);
                    set_block(world, l + IVec3::new(x, 0, 1), "wire", 3);
                }
                set_block(world, l + IVec3::new(5, 0, 0), "flasher_light", 2);
                set_block(world, l + IVec3::new(5, 0, 1), "flasher_light", 2);
                // Timer → flasher.
                set_block(world, l + IVec3::new(8, 0, 0), "timer_block", 4);
                set_block(world, l + IVec3::new(9, 0, 0), "flasher_light", 2);
                select(world, "white_block");
                teleport(world, Vec3::new(0.5, 0.0, 47.5));
            }
            3 => {
                // The second row carries blue only.
                for x in 1..=4 {
                    if let Some(e) = block_entity(world, l + IVec3::new(x, 0, 1)) {
                        world.get_mut::<crate::logic::Bus>(e).unwrap().0 = 1 << 1;
                    }
                }
                look_at(world, Vec3::new(0.5, 0.15, 44.5));
            }
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the lever", a.is_some_and(|a| a.cell == l), format!("{:?}", a.map(|a| a.cell)));
                let off = !logic_state(world, l + IVec3::new(5, 0, 0)).out && !logic_state(world, l + IVec3::new(2, 0, 0)).input;
                ctx.check("lever off: wire and flasher are off", off, "");
                click(world, MouseButton::Right, true);
            }
            9 => click(world, MouseButton::Right, false),
            14 => {
                let red = (1..=4).all(|x| logic_state(world, l + IVec3::new(x, 0, 0)).input);
                let blue = (1..=4).any(|x| logic_state(world, l + IVec3::new(x, 0, 1)).input);
                let f1 = logic_state(world, l + IVec3::new(5, 0, 0));
                let f2 = logic_state(world, l + IVec3::new(5, 0, 1));
                ctx.check("RMB on the lever: the red line is on", red && logic_state(world, l).memory, "");
                ctx.check("the flasher at its end gets the signal", f1.input, "");
                ctx.check("the blue line beside it does not join: off", !blue && !f2.input, "");
                look_at(world, Vec3::new(4.0, 0.0, 44.5));
                ctx.vars.insert("t0".into(), ctx.t);
            }
            18 => screenshot(world, "90_logic_wires"),
            _ => {}
        }
        if ctx.frames > 14 {
            let f = logic_state(world, l + IVec3::new(9, 0, 0)).out;
            let k = if f { "timer_on" } else { "timer_off" };
            ctx.vars.insert(k.into(), 1.0);
            let held = logic_state(world, l + IVec3::new(5, 0, 0)).out;
            ctx.vars.insert(if held { "blink_on" } else { "blink_off" }.into(), 1.0);
        }
        if ctx.frames > 14 && ctx.t > ctx.vars["t0"] + 4.0 {
            ctx.check("the timer flashes its lamp on and off", ctx.vars.contains_key("timer_on") && ctx.vars.contains_key("timer_off"), "");
            ctx.check("held on, the flasher blinks", ctx.vars.contains_key("blink_on") && ctx.vars.contains_key("blink_off"), "");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("logic: a NOT gate drives a clutch; a bus takes a second colour, the wrench takes it out", |world, ctx| {
        let m = IVec3::new(0, 0, 50);
        let gate = m + IVec3::new(1, 0, 1);
        let lever = m + IVec3::new(1, 0, 2);
        match ctx.frames {
            1 => {
                set_block(world, m, "creative_motor", 0);
                set_block(world, m + IVec3::X, "clutch", 0);
                set_block(world, m + IVec3::new(2, 0, 0), "shaft", 0);
                // Facing −Z: its output (front) is the clutch; the lever at its back.
                set_block(world, gate, "logic_gate", 5);
                set_block(world, lever, "lever", 3);
            }
            3 => set_param(world, gate, "mode", 2.0),
            10 => {
                ctx.check("NOT with no input is on: the clutch is open", rpm(world, m + IVec3::new(2, 0, 0)) == 0.0 && logic_state(world, gate).out, format!("{} RPM", rpm(world, m + IVec3::new(2, 0, 0))));
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: lever, mech: crate::content::Mech::Lever });
            }
            18 => {
                let r = rpm(world, m + IVec3::new(2, 0, 0)).abs();
                ctx.check("lever at its back: NOT is off, the clutch closes, the shaft turns", r == 32.0 && !logic_state(world, gate).out, format!("{r} RPM"));
                select(world, "wrench");
                teleport(world, Vec3::new(2.5, 0.0, 41.0));
            }
            20 => {
                // RMB with a blue wire on the red bus at x=2: it carries both.
                world.resource_mut::<crate::logic::WireColor>().0 = 1;
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: IVec3::new(2, 0, 44), mech: crate::content::Mech::Wire });
            }
            22 => look_at(world, Vec3::new(2.5, 0.5, 44.5)),
            26 => {
                let mask = block_entity(world, IVec3::new(2, 0, 44)).and_then(|e| world.get::<crate::logic::Bus>(e).map(|b| b.0)).unwrap_or(0);
                ctx.check("one bus carries red and blue", mask == 0b11, format!("{mask:b}"));
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the bus", a.is_some_and(|a| a.cell == IVec3::new(2, 0, 44)), format!("{:?}", a.map(|a| a.cell)));
                click(world, MouseButton::Left, true);
            }
            27 => click(world, MouseButton::Left, false),
            32 => {
                let mask = block_entity(world, IVec3::new(2, 0, 44)).and_then(|e| world.get::<crate::logic::Bus>(e).map(|b| b.0)).unwrap_or(0);
                ctx.check("LMB with the wrench takes the blue wire out", mask == 0b01, format!("{mask:b}"));
                world.resource_mut::<crate::logic::WireColor>().0 = 0;
                select(world, "white_block");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn menu(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("main menu, settings and pause", |world, ctx| {
        match ctx.frames {
            1 => world.resource_mut::<NextState<Ui>>().set(Ui::Menu),
            12 => {
                ctx.check("the main menu is open", *world.resource::<State<Ui>>().get() == Ui::Menu, "");
                screenshot(world, "95_main_menu");
            }
            16 => {
                world.resource_mut::<crate::menu::MenuState>().back = Some(Ui::Menu);
                world.resource_mut::<NextState<Ui>>().set(Ui::Options);
            }
            20 => {
                screenshot(world, "96_settings");
                let mut s = world.resource_mut::<crate::menu::GameSettings>();
                crate::menu::step(&mut s, crate::menu::Setting::Fov, 1);
            }
            24 => {
                let fov = world.query_filtered::<&Projection, With<crate::player::camera::MainCamera>>().iter(world).next().and_then(|p| if let Projection::Perspective(p) = p { Some(p.fov.to_degrees()) } else { None }).unwrap_or(0.0);
                ctx.check("a setting applies at once (field of view 75°)", (fov - 75.0).abs() < 0.1, format!("{fov:.1}°"));
                let mut s = world.resource_mut::<crate::menu::GameSettings>();
                crate::menu::step(&mut s, crate::menu::Setting::Fov, -1);
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
            }
            28 => world.resource_mut::<NextState<Ui>>().set(Ui::Pause),
            34 => {
                let paused = world.resource::<Time<Virtual>>().is_paused();
                ctx.check("Esc menu: the game is paused", paused && *world.resource::<State<Ui>>().get() == Ui::Pause, "");
                screenshot(world, "97_pause");
            }
            38 => world.resource_mut::<NextState<Ui>>().set(Ui::Game),
            44 => {
                ctx.check("«Продолжить»: the world runs again", !world.resource::<Time<Virtual>>().is_paused(), "");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// The proving-ground car with a diesel engine on top of the rear axle:
/// direct drive (cog → cog, 1:1) or a reduction (small → large cog, 2:1).
fn diesel_car(world: &mut World, front: IVec3, low: bool) -> IVec3 {
    let o = front + IVec3::new(-1, 0, 31);
    let at = |v: IVec3| o + v;
    for x in 0..3 {
        for z in -30..=-26 {
            set_block(world, at(IVec3::new(x, 1, z)), "steel_block", 2);
        }
    }
    for z in [-30, -26] {
        set_block(world, at(IVec3::new(0, 0, z)), "suspension", 1);
        set_block(world, at(IVec3::new(2, 0, z)), "suspension", 0);
        set_block(world, at(IVec3::new(-1, 0, z)), "wheel", 1);
        set_block(world, at(IVec3::new(3, 0, z)), "wheel", 0);
    }
    if low {
        // Engine → shaft → small cog, meshing diagonally with a large cog on
        // the axle: half the wheel speed, half the stress.
        set_block(world, at(IVec3::new(1, 0, -26)), "large_cogwheel", 0);
        set_block(world, at(IVec3::new(1, 1, -25)), "cogwheel", 0);
        set_block(world, at(IVec3::new(2, 1, -25)), "shaft", 0);
        set_block(world, at(IVec3::new(3, 1, -25)), "diesel_engine", 1);
    } else {
        set_block(world, at(IVec3::new(1, 0, -26)), "cogwheel", 0);
        set_block(world, at(IVec3::new(1, 1, -26)), "cogwheel", 0);
        set_block(world, at(IVec3::new(2, 1, -26)), "shaft", 0);
        set_block(world, at(IVec3::new(3, 1, -26)), "diesel_engine", 1);
    }
    set_block(world, at(IVec3::new(1, 2, -26)), "physics_toggle", 2);
    set_block(world, at(IVec3::new(0, 2, -30)), "speedometer", 4);
    glue_box(world, at(IVec3::new(-1, 0, -31)), at(IVec3::new(3, 2, -25)));
    at(IVec3::new(if low { 3 } else { 3 }, 1, if low { -25 } else { -26 }))
}

fn torque(steps: &mut Vec<(String, StepFn)>) {
    for low in [false, true] {
        let label = if low { "a reduction gear (small → large cog)" } else { "direct drive" };
        steps.push(step(&format!("torque: diesel car on the 30° ramp, {label}"), move |world, ctx| {
            let x = crate::world::lane_x(1) + 2;
            // At the foot of the ramp: no run-up, the climb is on the engine.
            let front = IVec3::new(x, 0, crate::world::LANE_START_Z + 1);
            let o = front + IVec3::new(-1, 0, 31);
            let engine_at = |world: &mut World, cell: IVec3| -> Option<crate::engine::Engine> {
                let mut q = world.query_filtered::<&VoxelGrid, With<crate::contraption::Contraption>>();
                let e = q.iter(world).next().and_then(|g| g.entities.get(&cell).copied())?;
                world.get::<crate::engine::Engine>(e).copied()
            };
            let eng_cell = o + IVec3::new(3, 1, if low { -25 } else { -26 });
            match ctx.frames {
                1 => {
                    clear_bodies(world);
                    world.resource_mut::<CameraView>().0 = View::First;
                    diesel_car(world, front, low);
                    teleport(world, Vec3::new(x as f32 + 6.5, 0.0, front.z as f32 + 3.0));
                }
                3 => {
                    if let Some(e) = block_entity(world, eng_cell) {
                        world.get_mut::<crate::fluids::FluidStore>(e).unwrap().add(crate::fluids::Fluid::Diesel, 1000.0);
                    }
                    set_param(world, eng_cell, "speed", 128.0);
                    set_param(world, eng_cell, "speed", 0.0);
                }
                5 => {
                    let g = world_grid(world);
                    world.write_message(crate::interact::UseBlock { grid: g, cell: o + IVec3::new(1, 2, -26), mech: crate::content::Mech::PhysicsToggle });
                }
                9 => {
                    let mut q = world.query_filtered::<&VoxelGrid, With<crate::contraption::Contraption>>();
                    if let Some(e) = q.iter(world).next().and_then(|g| g.entities.get(&eng_cell).copied())
                        && let Some(mut p) = world.get_mut::<crate::mech::Params>(e)
                    {
                        p.0[0] = 128.0;
                    }
                    ctx.vars.insert("tq_y0".into(), -1.0);
                }
                12 => {
                    // Forward is −Z: the right wheel (facing +X) turns about −X.
                    let rpm = car_block(world, o + IVec3::new(3, 0, -26)).and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(0.0);
                    if rpm > 0.0
                        && let Some(e) = car_block(world, eng_cell)
                    {
                        let mut p = world.get_mut::<crate::mech::Params>(e).unwrap();
                        p.0[1] = 1.0 - p.0[1];
                    }
                    ctx.vars.insert("tq_t0".into(), ctx.t);
                }
                _ => {}
            }
            if ctx.frames > 12 {
                if let Some((_, com)) = body_center(world) {
                    if ctx.vars["tq_y0"] < 0.0 {
                        ctx.vars.insert("tq_y0".into(), com.y);
                    }
                    let up = com.y - ctx.vars["tq_y0"];
                    ctx.vars.insert("tq_maxy".into(), ctx.vars.get("tq_maxy").copied().unwrap_or(0.0).max(up));
                }
                if engine_at(world, eng_cell).is_some_and(|e| e.stall == Some(crate::engine::Stall::Overload)) {
                    ctx.vars.insert("tq_stalled".into(), 1.0);
                }
                if ctx.frames == 30 {
                    screenshot(world, &format!("98_torque_{}", if low { "low" } else { "high" }));
                }
            }
            if ctx.frames > 12 && ctx.t > ctx.vars["tq_t0"] + if low { 50.0 } else { 12.0 } {
                let maxy = ctx.vars.get("tq_maxy").copied().unwrap_or(0.0);
                let stalled = ctx.vars.contains_key("tq_stalled");
                if low {
                    ctx.check("with the reduction gear it climbs the 30° ramp without stalling", maxy > 2.6 && !stalled, format!("rose {maxy:.2} m, stalled {stalled}"));
                } else {
                    ctx.check("direct drive: overloaded on the 30° ramp, the diesel chokes", stalled && maxy < 2.6, format!("rose {maxy:.2} m, stalled {stalled}"));
                }
                for k in ["tq_maxy", "tq_stalled", "tq_y0", "tq_t0"] {
                    ctx.vars.remove(k);
                }
                clear_bodies(world);
                teleport(world, Vec3::new(0.5, 0.0, 0.5));
                return Step::Next;
            }
            Step::Wait
        }));
    }
}

fn shapes(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("shapes: every material as slab, stairs, slope, corners", |world, ctx| {
        let z = -60;
        match ctx.frames {
            1 => {
                // A showcase: rows of materials, columns of shapes.
                for (row, key) in ["stone", "planks", "concrete", "steel_block", "white_block", "glass"].iter().enumerate() {
                    for i in 0..crate::content::SHAPES.len() {
                        let b = crate::content::registry().shaped(BlockId::k(key), i).unwrap();
                        let g = world_grid(world);
                        world.get_mut::<VoxelGrid>(g).unwrap().set(IVec3::new(-8 + i as i32 * 2, 0, z - row as i32 * 2), crate::grid::Cell::new(b, 4));
                    }
                }
                select(world, "stone");
                teleport(world, Vec3::new(-3.5, 0.0, z as f32 + 6.0));
            }
            4 => {
                // Shift + wheel picks the shape (here: stairs = 2).
                world.resource_mut::<crate::interact::ShapeMode>().0 = 2;
                look_at(world, Vec3::new(-0.5, 0.0, z as f32 + 3.5));
            }
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the floor", a.is_some_and(|a| a.normal == IVec3::Y), format!("{:?}", a.map(|a| (a.cell, a.normal))));
                ctx.vars.insert("sh_x".into(), a.map(|a| a.cell.x as f32).unwrap_or(0.0));
                ctx.vars.insert("sh_z".into(), a.map(|a| a.cell.z as f32).unwrap_or(0.0));
                click(world, MouseButton::Right, true);
            }
            9 => click(world, MouseButton::Right, false),
            14 => {
                let c = IVec3::new(ctx.vars["sh_x"] as i32, 0, ctx.vars["sh_z"] as i32);
                let placed = get_block(world, c).id;
                ctx.check("RMB with stone in «ступенька» mode places stone stairs", placed == BlockId::k("stone_stairs"), placed.def().key.clone());
                world.resource_mut::<crate::interact::ShapeMode>().0 = 0;
                look_at(world, Vec3::new(-3.0, 0.0, z as f32 - 4.0));
            }
            24 => {
                screenshot(world, "99_shapes");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn doors(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("sliding door: placed with its top, RMB and a lever open it", |world, ctx| {
        let d = IVec3::new(10, 0, -62);
        let open = |world: &World, c: IVec3| crate::doors::part(get_block(world, c).id).map(|p| p.1);
        let door_use = |world: &mut World| {
            let g = world_grid(world);
            world.write_message(crate::interact::UseBlock { grid: g, cell: d, mech: crate::content::Mech::SlidingDoor });
        };
        match ctx.frames {
            1 => {
                select(world, "sliding_door");
                teleport(world, Vec3::new(10.5, 0.0, -57.5));
            }
            4 => look_at(world, Vec3::new(10.5, 0.0, -61.5)),
            8 => {
                let a = world.resource::<Target>().hit;
                ctx.check("aiming at the floor for the door", a.is_some_and(|a| a.cell + a.normal == d), format!("{:?}", a.map(|a| a.cell + a.normal)));
                click(world, MouseButton::Right, true);
            }
            9 => click(world, MouseButton::Right, false),
            14 => {
                ctx.check("the door stands two blocks tall, closed", open(world, d) == Some(false) && open(world, d + IVec3::Y) == Some(false), format!("{:?} {:?}", open(world, d), open(world, d + IVec3::Y)));
                select(world, "white_block");
                door_use(world);
            }
            20 => {
                ctx.check("RMB: both halves open, nothing blocks the way", open(world, d) == Some(true) && open(world, d + IVec3::Y) == Some(true) && crate::grid::cell_boxes(d, get_block(world, d)).is_none(), "");
                screenshot(world, "9a_door_open");
            }
            23 => door_use(world),
            29 => {
                ctx.check("RMB again: closed", open(world, d) == Some(false), "");
                set_block(world, d + IVec3::X, "lever", 3);
            }
            32 => {
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: d + IVec3::X, mech: crate::content::Mech::Lever });
            }
            38 => {
                ctx.check("a lever beside it: open while on", open(world, d) == Some(true), "");
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: d + IVec3::X, mech: crate::content::Mech::Lever });
            }
            46 => {
                ctx.check("lever off: closed again", open(world, d) == Some(false), "");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn goggles(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("engineer's goggles in the head slot show a block's stats", |world, ctx| {
        let m = IVec3::new(16, 0, -62);
        let panel = |world: &mut World| world.query_filtered::<&Text, With<crate::goggles::PanelText>>().iter(world).next().map(|t| t.0.clone()).unwrap_or_default();
        match ctx.frames {
            1 => {
                set_block(world, m, "creative_motor", 0);
                set_block(world, m + IVec3::X, "shaft", 0);
                set_block(world, m + IVec3::new(2, 0, 0), "shaft", 0);
                teleport(world, Vec3::new(17.5, 0.0, -58.5));
            }
            4 => look_at(world, Vec3::new(17.5, 0.5, -61.5)),
            10 => {
                ctx.check("without goggles: no panel", panel(world).is_empty() || world.query_filtered::<&Node, With<Text>>().iter(world).count() > 0, "");
                let p = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                world.get_mut::<Inventory>(p).unwrap().slots[crate::inventory::HEAD] = Some(crate::inventory::Stack::full(crate::content::ItemId::k("engineer_goggles")));
            }
            16 => {
                let t = panel(world);
                ctx.check("goggles on: the shaft's speed and stress next to the crosshair", t.contains("Обороты: 32") && t.contains("SU"), t.replace('\n', " | "));
                screenshot(world, "9b_goggles");
            }
            19 => world.resource_mut::<NextState<Ui>>().set(Ui::Inventory),
            24 => screenshot(world, "9c_equipment"),
            28 => {
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                let p = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                world.get_mut::<Inventory>(p).unwrap().slots[crate::inventory::HEAD] = None;
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn links(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("chains: a chain over three cogwheels, same side and crossed", |world, ctx| {
        let (a, b, c) = (IVec3::new(-20, 1, -44), IVec3::new(-15, 1, -44), IVec3::new(-10, 4, -44));
        let up = Vec3::new(0.0, 0.3, 0.5);
        let down = Vec3::new(0.0, -0.3, 0.5);
        let centre = |v: IVec3| v.as_vec3() + Vec3::splat(0.5);
        match ctx.frames {
            1 => {
                set_block(world, a - IVec3::Z, "creative_motor", 4);
                for v in [a, b, c] {
                    set_block(world, v, "cogwheel", 4);
                }
                select(world, "chain");
                teleport(world, Vec3::new(-15.0, 0.0, -39.0));
            }
            4 => look_at(world, centre(a) + up),
            8 => click(world, MouseButton::Left, true),
            9 => click(world, MouseButton::Left, false),
            12 => look_at(world, centre(b) + up),
            16 => click(world, MouseButton::Left, true),
            17 => click(world, MouseButton::Left, false),
            // From B (clicked on top) to C, clicked underneath: crossed.
            20 => look_at(world, centre(c) + down),
            24 => click(world, MouseButton::Left, true),
            25 => click(world, MouseButton::Left, false),
            34 => {
                let g = world_grid(world);
                let n = world.get::<crate::chains::Chains>(g).map(|c| c.0.len()).unwrap_or(0);
                let (ra, rb, rc) = (rpm(world, a), rpm(world, b), rpm(world, c));
                ctx.check("two chain runs hung with the chain tool", n == 2, format!("{n}"));
                ctx.check("A and B on the same side turn the same way", ra != 0.0 && rb == ra, format!("{ra} {rb}"));
                ctx.check("B → C crossed: C turns the other way", rc == -rb, format!("{rb} {rc}"));
                screenshot(world, "9d_chains");
                select(world, "white_block");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("belts: a belt between two shafts turns the second and carries the player", |world, ctx| {
        let (a, b) = (IVec3::new(-20, 1, -51), IVec3::new(-14, 1, -51));
        let (d1, d2) = (IVec3::new(-20, 1, -57), IVec3::new(-16, 5, -57));
        match ctx.frames {
            1 => {
                set_block(world, a - IVec3::Z, "creative_motor", 4);
                for v in [a, b, d1, d2] {
                    set_block(world, v, "shaft", 4);
                }
                select(world, "mechanical_belt");
                teleport(world, Vec3::new(-17.0, 0.0, -45.0));
            }
            4 => look_at(world, a.as_vec3() + Vec3::new(0.5, 0.5, 0.9)),
            8 => {
                click(world, MouseButton::Left, true);
            }
            9 => click(world, MouseButton::Left, false),
            12 => look_at(world, b.as_vec3() + Vec3::new(0.5, 0.5, 0.9)),
            16 => click(world, MouseButton::Left, true),
            17 => click(world, MouseButton::Left, false),
            22 => {
                let mid = get_block(world, IVec3::new(-17, 1, -51)).id;
                ctx.check("the belt fills the gap, the shafts become pulleys", mid == BlockId::k("belt") && get_block(world, a).id == BlockId::k("belt_pulley"), mid.def().key.clone());
                let (ra, rb) = (rpm(world, a), rpm(world, b));
                ctx.check("both pulleys turn alike", ra != 0.0 && ra == rb, format!("{ra} {rb}"));
                // A 45° belt too (laid directly).
                let g = world_grid(world);
                let r = crate::belts::lay(&mut world.get_mut::<VoxelGrid>(g).unwrap(), d1, d2);
                ctx.check("a 45° belt is a ramp of slope cells", r.is_ok() && get_block(world, d1 + IVec3::new(1, 1, 0)).id == BlockId::k("belt_slope"), format!("{r:?}"));
                // Stand on the flat belt.
                select(world, "white_block");
                teleport(world, Vec3::new(-16.5, 1.9, -50.5));
                look_at(world, Vec3::new(-17.0, 1.0, -54.0));
                ctx.vars.insert("belt_x0".into(), player(world).0.x);
                world.resource_mut::<CameraView>().0 = View::Behind;
                ctx.vars.insert("belt_t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames == 27 {
            screenshot(world, "9e_belts");
        }
        if ctx.frames > 28 && ctx.t > ctx.vars["belt_t0"] + 1.5 {
            world.resource_mut::<CameraView>().0 = View::First;
            let x = player(world).0.x;
            let moved = (x - ctx.vars["belt_x0"]).abs();
            ctx.check("standing on the belt, the player rides along", moved > 1.0, format!("{moved:.2} m"));
            teleport(world, Vec3::new(0.5, 0.0, 0.5));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("tracks: a tracked vehicle drives on two track rows", |world, ctx| {
        let o = IVec3::new(50, 0, -30);
        match ctx.frames {
            1 => {
                clear_bodies(world);
                for x in 0..3 {
                    for z in 0..5 {
                        set_block(world, o + IVec3::new(x, 1, z), "steel_block", 2);
                    }
                }
                for z in 0..5 {
                    let (k, f) = if z == 0 { ("track_sprocket", 1) } else { ("track_roller", 1) };
                    set_block(world, o + IVec3::new(-1, 0, z), k, f);
                    let (k, f) = if z == 0 { ("track_sprocket", 0) } else { ("track_roller", 0) };
                    set_block(world, o + IVec3::new(3, 0, z), k, f);
                }
                set_block(world, o, "creative_motor", 1);
                set_block(world, o + IVec3::new(2, 0, 0), "creative_motor", 0);
                set_block(world, o + IVec3::new(1, 2, 2), "physics_toggle", 2);
                glue_box(world, o + IVec3::new(-1, 0, 0), o + IVec3::new(3, 2, 4));
                teleport(world, Vec3::new(57.0, 0.0, -24.0));
            }
            3 => {
                // Both tracks the same way about +X, stopped while assembling.
                set_param(world, o, "speed", 0.0);
                set_param(world, o + IVec3::new(2, 0, 0), "speed", 0.0);
                set_param(world, o + IVec3::new(2, 0, 0), "direction", 1.0);
            }
            5 => {
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: o + IVec3::new(1, 2, 2), mech: crate::content::Mech::PhysicsToggle });
                look_at(world, Vec3::new(51.5, 0.5, -28.0));
            }
            9 => screenshot(world, "9f_tracks"),
            12 => {
                for c in [o, o + IVec3::new(2, 0, 0)] {
                    if let Some(e) = car_block(world, c) {
                        world.get_mut::<crate::mech::Params>(e).unwrap().0[0] = 64.0;
                    }
                }
                let rolled = car_block(world, o + IVec3::new(-1, 0, 4)).and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(0.0);
                let _ = rolled;
                if let Some((_, p)) = body_center(world) {
                    ctx.vars.insert("tr_z0".into(), p.z);
                }
                ctx.vars.insert("tr_t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames == 16 {
            let r = car_block(world, o + IVec3::new(-1, 0, 4)).and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(0.0);
            ctx.check("the last road wheel turns with the sprocket", r.abs() == 64.0, format!("{r}"));
        }
        if ctx.frames > 16 && ctx.t > ctx.vars["tr_t0"] + 4.0 {
            let (p, up) = body_center(world).map(|(e, p)| (p, (world.get::<GlobalTransform>(e).unwrap().compute_transform().rotation * Vec3::Y).y)).unwrap_or((Vec3::ZERO, 0.0));
            let moved = (p.z - ctx.vars["tr_z0"]).abs();
            ctx.check("it drives, upright, on its tracks", moved > 3.0 && up > 0.9, format!("{moved:.1} m, up {up:.2}"));
            screenshot(world, "9g_tracks_moving");
            clear_bodies(world);
            return Step::Next;
        }
        Step::Wait
    }));
}

fn showcase(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("mechanisms showcase: wheel on its suspension, cogs, gearboxes", |world, ctx| {
        let o = IVec3::new(-12, 0, -30);
        match ctx.frames {
            1 => {
                set_block(world, o, "steel_frame", 2);
                set_block(world, o + IVec3::X, "suspension", 0);
                set_block(world, o + IVec3::new(2, 0, 0), "wheel", 0);
                // Motor → small, small; large diagonal to the second small.
                set_block(world, o + IVec3::new(5, 0, 0), "creative_motor", 2);
                set_block(world, o + IVec3::new(5, 1, 0), "cogwheel", 2);
                set_block(world, o + IVec3::new(6, 1, 0), "cogwheel", 2);
                set_block(world, o + IVec3::new(7, 1, 1), "large_cogwheel", 2);
                // Motor → gearbox → shafts on its ports.
                set_block(world, o + IVec3::new(10, 0, 0), "creative_motor", 0);
                set_block(world, o + IVec3::new(11, 0, 0), "gearbox", 2);
                set_block(world, o + IVec3::new(11, 0, 1), "shaft", 4);
                set_block(world, o + IVec3::new(12, 0, 0), "shaft", 0);
                set_block(world, o + IVec3::new(14, 0, 0), "vertical_gearbox", 4);
                set_block(world, o + IVec3::new(13, 0, 0), "creative_motor", 0);
                set_block(world, o + IVec3::new(14, 1, 0), "shaft", 2);
                teleport(world, Vec3::new(-4.0, 0.0, -24.0));
            }
            4 => look_at(world, Vec3::new(-4.0, 0.5, -30.0)),
            10 => {
                let up = rpm(world, o + IVec3::new(14, 1, 0));
                ctx.check("the vertical gearbox turns the shaft on its top port", up != 0.0, format!("{up}"));
                let side = rpm(world, o + IVec3::new(11, 0, 1));
                ctx.check("the gearbox turns the shaft on its side port", side != 0.0, format!("{side}"));
                let big = rpm(world, o + IVec3::new(7, 1, 1)).abs();
                ctx.check("the large cog meshes diagonally at half speed", big == 16.0, format!("{big}"));
            }
            16 => {
                screenshot(world, "9h_showcase");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

fn help(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("tooltips and the visual explanation (ponder)", |world, ctx| {
        let gearbox = crate::content::ItemId::k("gearbox");
        match ctx.frames {
            1 => world.resource_mut::<NextState<Ui>>().set(Ui::Inventory),
            6 => {
                let (title, short, hint) = crate::ui::tooltip_text(gearbox, false, 0);
                let (_, long, _) = crate::ui::tooltip_text(gearbox, true, 0);
                ctx.check("hover: a short description", title == "Коробка передач" && !short.is_empty() && long.len() > short.len(), short.clone());
                ctx.check("the hint offers Shift and W", hint.contains("Shift") && hint.contains("W"), hint);
                world.resource_mut::<crate::ui::Catalog>().hovered = Some(gearbox);
            }
            10 => screenshot(world, "9i_tooltip"),
            14 => {
                // What holding W does: open the explanation.
                let mut p = world.resource_mut::<crate::ponder::Ponder>();
                p.item = Some(gearbox);
                world.resource_mut::<crate::ui::Catalog>().hovered = None;
                world.resource_mut::<NextState<Ui>>().set(Ui::Ponder);
                ctx.vars.insert("pd_t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames > 14 && !ctx.vars.contains_key("pd_done") && ctx.t > ctx.vars["pd_t0"] + 14.0 {
            let mut q = world.query_filtered::<&VoxelGrid, With<crate::ponder::PonderGrid>>();
            let g = q.iter(world).next().cloned();
            let has = g.as_ref().is_some_and(|g| g.get(IVec3::ZERO).id == BlockId::k("gearbox"));
            let turning = g.as_ref().and_then(|g| g.entities.get(&IVec3::new(0, 0, 1)).copied()).and_then(|e| world.get::<crate::mech::Spin>(e)).map(|s| s.rpm).unwrap_or(0.0);
            ctx.check("the scene builds itself step by step: the gearbox turns its side shaft", has && turning != 0.0, format!("{turning}"));
            screenshot(world, "9j_ponder");
            ctx.vars.insert("pd_done".into(), ctx.t);
        }
        if ctx.vars.get("pd_done").is_some_and(|t| ctx.t > t + 1.0) {
            world.resource_mut::<NextState<Ui>>().set(Ui::Game);
            ctx.vars.remove("pd_done");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("texture editor: paint, see it on the blocks, undo", |world, ctx| {
        let atlas_px = |world: &mut World| -> Option<[u8; 4]> {
            let atlas = world.resource::<crate::grid::render::BlockAtlas>().image.clone();
            let i = crate::content::registry().textures.iter().position(|t| t == "concrete")?;
            let img = world.resource::<Assets<Image>>().get(&atlas)?;
            let w = img.width() as usize;
            let n = crate::grid::render::TILE as usize;
            let (c, r) = (i % (w / n), i / (w / n));
            let d = img.data.as_ref()?;
            let o = ((r * n + 3) * w + c * n + 3) * 4;
            Some([d[o], d[o + 1], d[o + 2], d[o + 3]])
        };
        match ctx.frames {
            1 => world.resource_mut::<NextState<Ui>>().set(Ui::Pause),
            4 => world.resource_mut::<NextState<Ui>>().set(Ui::Paint),
            8 => {
                let before = atlas_px(world);
                ctx.vars.insert("tx_r".into(), before.map(|c| c[0] as f32).unwrap_or(-1.0));
                let mut p = world.resource_mut::<crate::paint::Paint>();
                p.select("concrete");
                p.snapshot();
                p.color = [0xd4, 0xa8, 0x4f, 255];
                for i in 0..32 {
                    p.set(i, i, [0xd4, 0xa8, 0x4f, 255]);
                    p.set(31 - i, i, [0xd4, 0xa8, 0x4f, 255]);
                }
                p.tool = crate::paint::Tool::Fill;
                p.apply(3, 3);
            }
            12 => {
                let now = atlas_px(world);
                ctx.check("the stroke is in the block atlas at once", now == Some([0xd4, 0xa8, 0x4f, 255]), format!("{now:?}"));
                screenshot(world, "9l_paint");
                let path = world.resource::<Shots>().0.clone().map(|d| format!("{d}/paint_concrete.png"));
                if let Some(path) = path {
                    let r = world.resource::<crate::paint::Paint>().save_to(std::path::Path::new(&path));
                    ctx.check("saves a 32×32 png", r.is_ok() && image::open(&path).is_ok_and(|i| i.width() == 32), format!("{r:?}"));
                }
            }
            16 => world.resource_mut::<NextState<Ui>>().set(Ui::Game),
            19 => screenshot(world, "9m_painted_world"),
            20 => {
                world.resource_mut::<crate::paint::Paint>().undo();
            }
            21 => world.resource_mut::<NextState<Ui>>().set(Ui::Paint),
            25 => {
                let now = atlas_px(world).map(|c| c[0] as f32).unwrap_or(-2.0);
                ctx.check("undo brings the old pixels back", now == ctx.vars["tx_r"], format!("{now}"));
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("render scale: the 3D view at 60 %", |world, ctx| {
        match ctx.frames {
            1 => world.resource_mut::<crate::menu::GameSettings>().render_scale = 0.6,
            8 => {
                let t = world.query_filtered::<&bevy::camera::RenderTarget, With<crate::player::camera::MainCamera>>().iter(world).next().cloned();
                ctx.check("the main camera draws into a smaller image", matches!(t, Some(bevy::camera::RenderTarget::Image(_))), "");
                screenshot(world, "9k_render_scale");
            }
            14 => world.resource_mut::<crate::menu::GameSettings>().render_scale = 1.0,
            20 => {
                let t = world.query_filtered::<&bevy::camera::RenderTarget, With<crate::player::camera::MainCamera>>().iter(world).next().cloned();
                ctx.check("100 %: straight to the window again", matches!(t, Some(bevy::camera::RenderTarget::Window(_))), "");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// Big machine hall: frame time and entity counts before and after.
fn perf(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("perf: 16 lines × 30 mechanisms", |world, ctx| {
        let counts = |world: &mut World| {
            let all = world.query::<Entity>().iter(world).count();
            let meshes = world.query::<&Mesh3d>().iter(world).count();
            let blocks = world.query::<&crate::mech::BlockEntity>().iter(world).count();
            format!("entities {all}, meshes {meshes}, block entities {blocks}")
        };
        let report = |world: &World| crate::profile::report(world.resource::<crate::profile::Profile>()).replace('\n', " | ");
        match ctx.frames {
            1 => clear_area(world, IVec3::new(8, 0, -44), IVec3::new(44, 3, -8)),
            90 => {
                let c = counts(world);
                info!("[PERF] empty: {c} :: {}", report(world));
                for r in 0..16 {
                    let z = -42 + r * 2;
                    set_block(world, IVec3::new(10, 0, z), "creative_motor", 0);
                    for x in 11..41 {
                        let k = if x % 10 == 0 { "gearbox" } else if x % 2 == 0 { "cogwheel" } else { "shaft" };
                        set_block(world, IVec3::new(x, 0, z), k, 0);
                    }
                }
            }
            240 => {
                let c = counts(world);
                info!("[PERF] machines: {c} :: {}", report(world));
                ctx.check("perf numbers logged", true, c);
                clear_area(world, IVec3::new(8, 0, -44), IVec3::new(44, 3, -8));
            }
            250 => return Step::Next,
            _ => {}
        }
        Step::Wait
    }));
}

/// World saves: build, save, load from disk, compare.
fn saves(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("world save → load: blocks, settings, states, a machine, a bearing", |world, ctx| {
        let name = "autotest_world";
        let motor = IVec3::new(4, 0, -10);
        let lever = IVec3::new(6, 0, -10);
        let tank = IVec3::new(8, 0, -10);
        let toggle = IVec3::new(12, 0, -12);
        let bmotor = IVec3::new(18, 0, -10);
        match ctx.frames {
            1 => {
                let _ = crate::save::delete(name);
                console(world, &format!("world new {name}"));
            }
            6 => {
                let ok = world.resource::<crate::save::CurrentWorld>().name.as_deref() == Some(name);
                ctx.check("a new world is created and saved", ok && crate::save::path_of(name).exists(), "");
                set_block(world, motor, "creative_motor", 0);
                set_block(world, motor + IVec3::X, "shaft", 0);
                set_block(world, lever, "lever", 2);
                set_block(world, tank, "fluid_tank", 2);
                // A small machine: three blocks glued to a physics toggle.
                for x in 12..=14 {
                    set_block(world, IVec3::new(x, 0, -12), "steel_block", 2);
                }
                set_block(world, toggle + IVec3::Y, "physics_toggle", 2);
                let g = world_grid(world);
                world.entity_mut(g).insert(crate::tools::glue::Glue(vec![(IVec3::new(12, 0, -12), IVec3::new(14, 1, -12)), (IVec3::new(17, 2, -10), IVec3::new(19, 2, -10))]));
                // A bearing turning a glued bar.
                set_block(world, bmotor, "creative_motor", 2);
                set_block(world, bmotor + IVec3::Y, "mechanical_bearing", 2);
                for x in 17..=19 {
                    set_block(world, IVec3::new(x, 2, -10), "steel_block", 2);
                }
                teleport(world, Vec3::new(10.5, 0.0, -4.5));
            }
            10 => {
                set_param(world, motor, "speed", 96.0);
                set_param(world, bmotor, "speed", 8.0);
                if let Some(e) = block_entity(world, lever) {
                    world.get_mut::<crate::logic::LogicState>(e).unwrap().memory = true;
                }
                if let Some(e) = block_entity(world, tank) {
                    world.get_mut::<crate::fluids::FluidStore>(e).unwrap().add(crate::fluids::Fluid::Water, 3000.0);
                }
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: toggle + IVec3::Y, mech: crate::content::Mech::PhysicsToggle });
                world.resource_mut::<crate::sky::Clock>().hours = 15.25;
                ctx.vars.insert("sv_t0".into(), ctx.t);
            }
            _ => {}
        }
        // Let the machine settle and the bearing turn, then save.
        if ctx.frames > 10 && !ctx.vars.contains_key("sv_saved") && ctx.t > ctx.vars["sv_t0"] + 2.0 {
            let bodies = world.query_filtered::<(&crate::contraption::Contraption, &GlobalTransform), Without<crate::mounts::MountedOn>>().iter(world).map(|(c, t)| (c.mass, t.translation())).collect::<Vec<_>>();
            ctx.check("before saving: one machine and one moving part", bodies.len() == 1 && moving_parts(world).len() == 1, format!("{} bodies, {} parts", bodies.len(), moving_parts(world).len()));
            if let Some((m, p)) = bodies.first() {
                ctx.vars.insert("sv_mass".into(), *m);
                ctx.vars.insert("sv_bx".into(), p.x);
                ctx.vars.insert("sv_by".into(), p.y);
            }
            console(world, "world save");
            ctx.vars.insert("sv_saved".into(), ctx.t);
        }
        if ctx.vars.get("sv_saved").is_some_and(|t| ctx.t > t + 0.5) && !ctx.vars.contains_key("sv_loaded") {
            // Mess the world up, then load it back from the disk.
            set_param(world, motor, "speed", 8.0);
            set_block(world, tank, "air", 2);
            teleport(world, Vec3::new(-20.5, 0.0, 20.5));
            world.resource_mut::<crate::sky::Clock>().hours = 3.0;
            console(world, &format!("world load {name}"));
            ctx.vars.insert("sv_loaded".into(), ctx.t);
        }
        if ctx.vars.get("sv_loaded").is_some_and(|t| ctx.t > t + 1.5) {
            let speed = block_entity(world, motor).and_then(|e| world.get::<crate::mech::Params>(e).map(|p| p.0[0]));
            ctx.check("settings come back (motor 96 RPM)", speed == Some(96.0), format!("{speed:?}"));
            let lever_on = block_entity(world, lever).and_then(|e| world.get::<crate::logic::LogicState>(e).copied()).is_some_and(|s| s.memory);
            ctx.check("the lever is still on", lever_on, "");
            let water = store_at(world, world_grid(world), tank).map(|s| s.amount).unwrap_or(0.0);
            ctx.check("the tank keeps its 3000 mB of water", (water - 3000.0).abs() < 1.0, format!("{water}"));
            let bodies = world.query_filtered::<(&crate::contraption::Contraption, &GlobalTransform), Without<crate::mounts::MountedOn>>().iter(world).map(|(c, t)| (c.mass, t.translation())).collect::<Vec<_>>();
            let same = bodies.first().is_some_and(|(m, p)| *m == ctx.vars["sv_mass"] && (p.x - ctx.vars["sv_bx"]).abs() < 0.3 && (p.y - ctx.vars["sv_by"]).abs() < 0.5);
            ctx.check("the machine is a physical body again, where it was", bodies.len() == 1 && same, format!("{bodies:?}"));
            let parts = moving_parts(world);
            ctx.check("the bearing's part is hinged to the world again", parts.len() == 1 && parts[0].1 == world_grid(world), format!("{}", parts.len()));
            let (feet, _) = player(world);
            ctx.check("the player stands where they saved", (feet.x - 10.5).abs() < 1.0 && (feet.z + 4.5).abs() < 1.0, format!("{feet:?}"));
            let h = world.resource::<crate::sky::Clock>().hours;
            ctx.check("time of day comes back", (h - 15.25).abs() < 0.2, format!("{h}"));
            let listed = crate::save::list().iter().any(|(n, _)| n == name);
            ctx.check("the world is in the list", listed, "");
            screenshot(world, "9o_loaded_world");
            world.resource_mut::<NextState<Ui>>().set(Ui::Worlds);
            ctx.vars.insert("sv_menu".into(), ctx.t);
            ctx.vars.remove("sv_loaded");
        }
        if ctx.vars.get("sv_menu").is_some_and(|t| ctx.t > t + 0.6) && !ctx.vars.contains_key("sv_shot") {
            screenshot(world, "9p_worlds_screen");
            ctx.vars.insert("sv_shot".into(), ctx.t);
        }
        if ctx.vars.get("sv_shot").is_some_and(|t| ctx.t > t + 0.3) {
            world.resource_mut::<NextState<Ui>>().set(Ui::Game);
            let _ = crate::save::delete(name);
            return Step::Next;
        }
        Step::Wait
    }));
}

/// Physics on → off, three times: nothing may change, move or fly away.
fn roundtrip(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("physics round trips keep every block, setting and state", |world, ctx| {
        let o = IVec3::new(30, 0, -50);
        let toggle = o + IVec3::new(1, 1, 0);
        let layout: Vec<(IVec3, &str, u8)> = vec![
            (o, "steel_block", 2),
            (o + IVec3::X, "steel_block", 2),
            (o + IVec3::X * 2, "steel_block", 2),
            (o + IVec3::X * 3, "steel_block", 2),
            (toggle, "physics_toggle", 2),
            (o + IVec3::new(0, 1, 0), "creative_motor", 0),
            (o + IVec3::new(2, 1, 0), "fluid_tank", 2),
            (o + IVec3::new(3, 1, 0), "lever", 2),
            (o + IVec3::new(0, 1, 1), "steel_block", 2),
            (o + IVec3::new(1, 1, 1), "shaft", 0),
            (o + IVec3::new(2, 1, 1), "cogwheel", 0),
        ];
        let lay = layout.clone();
        let check_world = move |world: &mut World| -> (bool, String) {
            let bad: Vec<String> = lay.iter().filter(|(c, k, f)| { let cell = get_block(world, *c); cell.id != BlockId::k(k) || cell.facing != *f }).map(|(c, k, _)| format!("{k}@{c}")).collect();
            (bad.is_empty(), bad.join(", "))
        };
        let bodies = |world: &mut World| world.query_filtered::<Entity, With<crate::contraption::Contraption>>().iter(world).count();
        let round = ctx.vars.get("rt_round").copied().unwrap_or(0.0) as i32;
        match ctx.frames {
            1 => {
                clear_bodies(world);
                clear_area(world, o - IVec3::new(1, 0, 2), o + IVec3::new(5, 3, 3));
                for (c, k, f) in &layout {
                    set_block(world, *c, k, *f);
                }
                let g = world_grid(world);
                let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                glue.push((o, o + IVec3::new(3, 1, 1)));
                world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
                teleport(world, Vec3::new(32.5, 0.0, -44.5));
                look_at(world, o.as_vec3() + Vec3::new(2.0, 1.0, 0.5));
            }
            5 => {
                set_param(world, o + IVec3::new(0, 1, 0), "speed", 48.0);
                if let Some(e) = block_entity(world, o + IVec3::new(2, 1, 0)) {
                    world.get_mut::<crate::fluids::FluidStore>(e).unwrap().add(crate::fluids::Fluid::Diesel, 2000.0);
                }
                if let Some(e) = block_entity(world, o + IVec3::new(3, 1, 0)) {
                    world.get_mut::<crate::logic::LogicState>(e).unwrap().memory = true;
                }
                ctx.vars.insert("rt_phase_t".into(), ctx.t);
                ctx.vars.insert("rt_phase".into(), 0.0);
            }
            _ => {}
        }
        if ctx.frames <= 5 {
            return Step::Wait;
        }
        let phase = ctx.vars["rt_phase"] as i32;
        let since = ctx.t - ctx.vars["rt_phase_t"];
        let next = |ctx: &mut Ctx, p: i32| {
            ctx.vars.insert("rt_phase".into(), p as f32);
            ctx.vars.insert("rt_phase_t".into(), ctx.t);
        };
        match phase {
            // On.
            0 if since > 0.3 => {
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: toggle, mech: crate::content::Mech::PhysicsToggle });
                next(ctx, 1);
            }
            // Physical for a second, then off.
            1 if since > 1.2 => {
                let n = bodies(world);
                ctx.check(&format!("round {}: the machine is one body", round + 1), n == 1, format!("{n}"));
                let body = world.query_filtered::<Entity, With<crate::contraption::Contraption>>().iter(world).next();
                if let Some(b) = body {
                    let cell = world.get::<VoxelGrid>(b).map(|g| g.cells().into_iter().find(|(_, c)| c.id == BlockId::k("physics_toggle")).map(|(c, _)| c)).flatten();
                    if let Some(c) = cell {
                        world.write_message(crate::interact::UseBlock { grid: b, cell: c, mech: crate::content::Mech::PhysicsToggle });
                    }
                }
                next(ctx, 2);
            }
            2 if since > 0.6 => {
                let (ok, bad) = check_world(world);
                ctx.check(&format!("round {}: every block is back in its cell", round + 1), ok && bodies(world) == 0, bad);
                let speed = block_entity(world, o + IVec3::new(0, 1, 0)).and_then(|e| world.get::<crate::mech::Params>(e).map(|p| p.0[0]));
                let fuel = store_at(world, world_grid(world), o + IVec3::new(2, 1, 0)).map(|s| s.amount).unwrap_or(0.0);
                let lever = block_entity(world, o + IVec3::new(3, 1, 0)).and_then(|e| world.get::<crate::logic::LogicState>(e).copied()).is_some_and(|s| s.memory);
                ctx.check(&format!("round {}: settings, fuel and the lever stay", round + 1), speed == Some(48.0) && (fuel - 2000.0).abs() < 1.0 && lever, format!("{speed:?} {fuel} {lever}"));
                let g = world_grid(world);
                let glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.iter().filter(|b| b.0 == o).count()).unwrap_or(0);
                ctx.check(&format!("round {}: the glue comes back once", round + 1), glue == 1, format!("{glue} boxes"));
                let fast = world.query::<&avian3d::prelude::LinearVelocity>().iter(world).filter(|v| v.0.length() > 2.0).count();
                ctx.check(&format!("round {}: nothing flies away", round + 1), fast == 0, format!("{fast} fast bodies"));
                ctx.vars.insert("rt_round".into(), (round + 1) as f32);
                next(ctx, if round + 1 < 3 { 0 } else { 3 });
            }
            // The player in the way is detected.
            3 => {
                let target = IVec3::new(32, 0, -44);
                teleport(world, target.as_vec3() + Vec3::new(0.5, 0.0, 0.5));
                next(ctx, 4);
            }
            4 if since > 0.3 => {
                let g = world_grid(world);
                let (feet, _) = player(world);
                let cell = (feet + Vec3::Y * 0.5).floor().as_ivec3();
                let who = crate::assembly::bodies_in(world, Entity::PLACEHOLDER, g, &[cell]);
                let p = world.query_filtered::<Entity, With<Player>>().iter(world).next();
                ctx.check("the player standing where blocks would land is found", p.is_some_and(|p| who.contains(&p)), format!("{who:?}"));
                teleport(world, Vec3::new(32.5, 0.0, -44.5));
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: toggle, mech: crate::content::Mech::PhysicsToggle });
                next(ctx, 5);
            }
            // Sunk 0.4 into the floor: it still comes back (a block higher).
            5 if since > 1.0 => {
                if let Some(b) = world.query_filtered::<Entity, With<crate::contraption::Contraption>>().iter(world).next() {
                    let mut t = world.get::<Transform>(b).copied().unwrap_or_default();
                    t.translation.y -= 0.4;
                    world.entity_mut(b).insert((t, GlobalTransform::from(t)));
                    let cell = world.get::<VoxelGrid>(b).and_then(|g| g.cells().into_iter().find(|(_, c)| c.id == BlockId::k("physics_toggle")).map(|(c, _)| c));
                    if let Some(c) = cell {
                        world.write_message(crate::interact::UseBlock { grid: b, cell: c, mech: crate::content::Mech::PhysicsToggle });
                    }
                }
                next(ctx, 6);
            }
            6 if since > 0.6 => {
                let n = bodies(world);
                let mut steel = Vec::new();
                for x in o.x - 1..=o.x + 5 {
                    for y in -1..4 {
                        for z in o.z - 2..=o.z + 3 {
                            if get_block(world, IVec3::new(x, y, z)).id == BlockId::k("steel_block") {
                                steel.push(IVec3::new(x, y, z));
                            }
                        }
                    }
                }
                ctx.check("a machine sunk into the floor still goes back into the grid", n == 0 && steel.len() == 5, format!("{n} bodies, steel at {steel:?}"));
                screenshot(world, "9q_roundtrip");
                clear_area(world, o - IVec3::new(1, 0, 2), o + IVec3::new(5, 3, 3));
                clear_bodies(world);
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// Storages, item tubes and the drive.
fn items(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("item tubes: a drive moves stone from a chest to a barrel", |world, ctx| {
        let o = IVec3::new(40, 0, -40);
        let chest = o;
        let drive = o + IVec3::new(2, 0, 0);
        let barrel = o + IVec3::new(4, 1, 0);
        let count = |world: &mut World, c: IVec3| -> u32 {
            block_entity(world, c).and_then(|e| world.get::<crate::storage::ItemStore>(e).map(|s| s.slots.iter().flatten().map(|x| x.count).sum())).unwrap_or(0)
        };
        match ctx.frames {
            1 => {
                clear_area(world, o - IVec3::new(1, 0, 2), o + IVec3::new(8, 3, 4));
                set_block(world, chest, "chest", 4);
                set_block(world, o + IVec3::X, "item_tube", 0);
                set_block(world, drive, "item_drive", 0);
                set_block(world, o + IVec3::new(3, 0, 0), "item_tube", 0);
                set_block(world, o + IVec3::new(3, 1, 0), "item_tube", 2);
                set_block(world, barrel, "barrel", 2);
                // Motor → cog beside the drive (same axis): the ring meshes.
                set_block(world, drive + IVec3::new(-1, 0, 1), "creative_motor", 0);
                set_block(world, drive + IVec3::Z, "cogwheel", 0);
                // Three joined containers.
                for x in 6..=8 {
                    set_block(world, o + IVec3::new(x, 0, 3), "item_container", 2);
                }
                teleport(world, o.as_vec3() + Vec3::new(2.5, 0.0, 3.6));
                look_at(world, o.as_vec3() + Vec3::new(2.5, 0.6, 0.5));
            }
            4 => {
                if let Some(e) = block_entity(world, chest) {
                    let mut st = world.get_mut::<crate::storage::ItemStore>(e).unwrap();
                    st.insert(crate::inventory::Stack::new(crate::content::ItemId::k("stone"), 64));
                    st.insert(crate::inventory::Stack::new(crate::content::ItemId::k("planks"), 32));
                }
                set_param(world, drive + IVec3::new(-1, 0, 1), "speed", 64.0);
                ctx.vars.insert("it_t0".into(), ctx.t);
            }
            _ => {}
        }
        if ctx.frames > 4 && !ctx.vars.contains_key("it_mid") && ctx.t > ctx.vars["it_t0"] + 1.2 {
            let packets = world.query::<&crate::tubes::Packet>().iter(world).count();
            let rpm = block_entity(world, drive).and_then(|e| world.get::<crate::mech::Spin>(e).map(|s| s.rpm)).unwrap_or(0.0);
            ctx.check("the drive turns (its ring meshes with the cog)", rpm.abs() == 64.0, format!("{rpm}"));
            ctx.check("packets travel in the tubes", packets > 0, format!("{packets}"));
            screenshot(world, "9r_item_tubes");
            ctx.vars.insert("it_mid".into(), ctx.t);
        }
        if ctx.vars.get("it_mid").is_some_and(|t| ctx.t > t + 6.0) && !ctx.vars.contains_key("it_done") {
            let (a, b) = (count(world, chest), count(world, barrel));
            let flying: u32 = world.query::<&crate::tubes::Packet>().iter(world).map(|p| p.stack.count).sum();
            ctx.check("items arrive in the barrel, none lost", b > 0 && a + b + flying == 96, format!("chest {a}, barrel {b}, on the way {flying}"));
            // Open the chest: its slots above the inventory.
            let g = world_grid(world);
            world.write_message(crate::interact::UseBlock { grid: g, cell: chest, mech: crate::content::Mech::Chest });
            ctx.vars.insert("it_done".into(), ctx.t);
        }
        if ctx.vars.get("it_done").is_some_and(|t| ctx.t > t + 0.8) && !ctx.vars.contains_key("it_ui") {
            let open = world.resource::<crate::storage::OpenStore>().len();
            let ui = *world.resource::<State<Ui>>().get();
            ctx.check("RMB opens the chest above the inventory", ui == Ui::Inventory && open == 27, format!("{ui:?} {open}"));
            screenshot(world, "9s_chest_open");
            ctx.vars.insert("it_ui".into(), ctx.t);
        }
        if ctx.vars.get("it_ui").is_some_and(|t| ctx.t > t + 0.5) && !ctx.vars.contains_key("it_cont") {
            world.resource_mut::<NextState<Ui>>().set(Ui::Game);
            let g = world_grid(world);
            world.write_message(crate::interact::UseBlock { grid: g, cell: o + IVec3::new(7, 0, 3), mech: crate::content::Mech::Container });
            ctx.vars.insert("it_cont".into(), ctx.t);
        }
        if ctx.vars.get("it_cont").is_some_and(|t| ctx.t > t + 0.6) {
            let open = world.resource::<crate::storage::OpenStore>().len();
            ctx.check("three containers open as one storage (108 slots)", open == 108, format!("{open}"));
            world.resource_mut::<NextState<Ui>>().set(Ui::Game);
            clear_area(world, o - IVec3::new(1, 0, 2), o + IVec3::new(8, 3, 4));
            return Step::Next;
        }
        Step::Wait
    }));
}

/// Hand crank, winch + rope, valve, snorkel, a three-module diesel.
fn workshop(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("hand crank turns a shaft while held", |world, ctx| {
        let shaft = IVec3::new(40, 0, -30);
        match ctx.frames {
            1 => {
                clear_area(world, IVec3::new(38, 0, -32), IVec3::new(44, 2, -28));
                set_block(world, shaft, "shaft", 0);
                set_block(world, shaft + IVec3::X, "hand_crank", 0);
            }
            4..=20 => {
                if let Some(e) = block_entity(world, shaft + IVec3::X)
                    && let Some(mut c) = world.get_mut::<crate::ropes::Crank>(e)
                {
                    c.held = true;
                }
                if ctx.frames == 20 {
                    let r = rpm(world, shaft).abs();
                    ctx.check("held: the shaft turns at 32 RPM", r == 32.0, format!("{r}"));
                }
            }
            24 => {
                let r = rpm(world, shaft).abs();
                ctx.check("let go: it stops", r == 0.0, format!("{r}"));
                clear_area(world, IVec3::new(38, 0, -32), IVec3::new(44, 2, -28));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("winch and rope lift a machine", |world, ctx| {
        let base = IVec3::new(50, 0, -20);
        let winch = base + IVec3::new(0, 6, 0);
        let crate_c = base;
        let anchor = crate_c + IVec3::new(0, 2, 0);
        match ctx.frames {
            1 => {
                clear_bodies(world);
                clear_area(world, base - IVec3::new(3, 0, 3), base + IVec3::new(3, 8, 3));
                // A gantry: a column with an arm, the winch under the arm.
                for y in 0..8 {
                    set_block(world, base + IVec3::new(2, y, 0), "steel_block", 2);
                }
                set_block(world, base + IVec3::new(1, 7, 0), "steel_block", 2);
                set_block(world, base + IVec3::new(0, 7, 0), "steel_block", 2);
                set_block(world, winch, "winch", 3);
                // A glued load: two steel blocks + the physics toggle, anchor on top.
                set_block(world, crate_c, "steel_block", 2);
                set_block(world, crate_c + IVec3::Y, "physics_toggle", 2);
                set_block(world, anchor, "rope_anchor", 3);
                let g = world_grid(world);
                let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                glue.push((crate_c, anchor));
                world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
                teleport(world, base.as_vec3() + Vec3::new(0.5, 0.0, 8.5));
                look_at(world, base.as_vec3() + Vec3::new(0.5, 3.0, 0.5));
            }
            4 => {
                // Drive the drum from its side with a motor.
                let b = block_entity(world, winch).and_then(|e| world.get::<crate::mech::BlockEntity>(e).copied());
                if let Some(b) = b {
                    let side = crate::mech::side_axis(&b);
                    set_block(world, winch + side, "creative_motor", crate::grid::facing_of(-side));
                    ctx.vars.insert("ws_sx".into(), side.x as f32);
                    ctx.vars.insert("ws_sz".into(), side.z as f32);
                }
                let (w, a) = (block_entity(world, winch), block_entity(world, anchor));
                if let (Some(w), Some(a)) = (w, a) {
                    world.entity_mut(w).insert(crate::ropes::RopeEnd::new(a, 4.0));
                }
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: crate_c + IVec3::Y, mech: crate::content::Mech::PhysicsToggle });
            }
            10 => {
                let side = IVec3::new(ctx.vars["ws_sx"] as i32, 0, ctx.vars["ws_sz"] as i32);
                set_param(world, winch + side, "speed", 32.0);
                ctx.vars.insert("ws_t0".into(), ctx.t);
                ctx.vars.insert("ws_y0".into(), body_y(world));
            }
            _ => {}
        }
        if ctx.frames > 10 && !ctx.vars.contains_key("ws_dir") && ctx.t > ctx.vars["ws_t0"] + 1.0 {
            // Reeling out instead of in? turn the motor around.
            let len = block_entity(world, winch).and_then(|e| world.get::<crate::ropes::RopeEnd>(e).map(|r| r.length)).unwrap_or(0.0);
            if len > 4.0 {
                let side = IVec3::new(ctx.vars["ws_sx"] as i32, 0, ctx.vars["ws_sz"] as i32);
                set_param(world, winch + side, "direction", 1.0);
            }
            ctx.vars.insert("ws_dir".into(), ctx.t);
        }
        if ctx.vars.get("ws_dir").is_some_and(|t| ctx.t > t + 6.0) && !ctx.vars.contains_key("ws_shot") {
            let y = body_y(world);
            let joint = world.query::<&avian3d::prelude::DistanceJoint>().iter(world).count();
            ctx.check("the rope is a joint between the world and the load", joint == 1, format!("{joint}"));
            let len = block_entity(world, winch).and_then(|e| world.get::<crate::ropes::RopeEnd>(e).map(|r| r.length)).unwrap_or(-1.0);
            let r = rpm(world, winch);
            ctx.check("reeling in lifts the load", y > ctx.vars["ws_y0"] + 0.8, format!("{:.2} → {y:.2}, rope {len:.2} m, drum {r} RPM", ctx.vars["ws_y0"]));
            screenshot(world, "9t_winch");
            ctx.vars.insert("ws_shot".into(), ctx.t);
        }
        if ctx.vars.get("ws_shot").is_some_and(|t| ctx.t > t + 0.5) {
            clear_bodies(world);
            clear_area(world, base - IVec3::new(3, 0, 3), base + IVec3::new(3, 8, 3));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("valve: open passes water, shut (or a signal) stops it", |world, ctx| {
        let o = IVec3::new(40, 0, -24);
        let tank = o + IVec3::new(4, 0, 0);
        let valve = o + IVec3::new(2, 0, 0);
        let amount = |world: &mut World| store_at(world, world_grid(world), tank).map(|s| s.amount).unwrap_or(0.0);
        match ctx.frames {
            1 => {
                clear_area(world, o - IVec3::new(1, 0, 2), o + IVec3::new(6, 2, 2));
                set_block(world, o, "creative_tank", 2);
                set_block(world, o + IVec3::X, "fluid_pipe", 0);
                set_block(world, valve, "fluid_valve", 0);
                set_block(world, o + IVec3::new(3, 0, 0), "fluid_pipe", 0);
                set_block(world, tank, "fluid_tank", 2);
            }
            4 => set_param(world, o, "fluid", 0.0),
            30 => {
                let a = amount(world);
                ctx.check("open: water flows into the tank", a > 100.0, format!("{a}"));
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: valve, mech: crate::content::Mech::Valve });
            }
            36 => {
                let shut = get_block(world, valve).id == BlockId::k("fluid_valve_closed");
                ctx.check("RMB turns the handwheel: shut", shut, "");
                ctx.vars.insert("ws_a1".into(), amount(world));
            }
            56 => {
                let a = amount(world);
                ctx.check("shut: nothing flows", (a - ctx.vars["ws_a1"]).abs() < 1.0, format!("{} → {a}", ctx.vars["ws_a1"]));
                // Open again, then shut it with a lever's signal.
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: valve, mech: crate::content::Mech::Valve });
            }
            60 => {
                set_block(world, valve + IVec3::Y, "lever", 3);
            }
            64 => {
                if let Some(e) = block_entity(world, valve + IVec3::Y) {
                    world.get_mut::<crate::logic::LogicState>(e).unwrap().memory = true;
                }
            }
            72 => {
                let shut = get_block(world, valve).id == BlockId::k("fluid_valve_closed");
                ctx.check("a lever's signal shuts it", shut, format!("{:?}", get_block(world, valve).id.def().key));
                if let Some(e) = block_entity(world, valve + IVec3::Y) {
                    world.get_mut::<crate::logic::LogicState>(e).unwrap().memory = false;
                }
            }
            80 => {
                let open = get_block(world, valve).id == BlockId::k("fluid_valve");
                ctx.check("signal gone: open again", open, "");
                screenshot(world, "9u_valve");
            }
            86 => {
                clear_area(world, o - IVec3::new(1, 0, 2), o + IVec3::new(6, 2, 2));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
    steps.push(step("snorkel and a three-module diesel", |world, ctx| {
        let o = IVec3::new(40, 0, -14);
        // Three modules facing +Z, the output shaft in front.
        let mods = [o, o + IVec3::Z, o + IVec3::Z * 2];
        let shaft = o + IVec3::Z * 3;
        match ctx.frames {
            1 => {
                clear_area(world, o - IVec3::new(2, 0, 2), o + IVec3::new(3, 5, 5));
                for m in mods {
                    set_block(world, m, "diesel_engine", 4);
                }
                set_block(world, shaft, "shaft", 4);
                teleport(world, o.as_vec3() + Vec3::new(3.2, 0.0, 3.8));
                look_at(world, o.as_vec3() + Vec3::new(0.5, 0.6, 1.5));
            }
            4 => {
                for m in mods {
                    if let Some(e) = block_entity(world, m) {
                        world.get_mut::<crate::fluids::FluidStore>(e).unwrap().add(crate::fluids::Fluid::Diesel, 200.0);
                    }
                }
            }
            12 => {
                let r = rpm(world, shaft).abs();
                let cap = block_entity(world, shaft).and_then(|e| world.get::<crate::mech::NetState>(e).map(|n| n.capacity)).unwrap_or(0.0);
                ctx.check("three modules are one engine: one output", r == 64.0, format!("{r}"));
                ctx.check("with three times the power", cap == crate::engine::POWER * 3.0, format!("{cap}"));
                let hidden = block_entity(world, mods[1]).map(|e| {
                    let kids: Vec<Entity> = world.get::<Children>(e).map(|c| c.iter().collect()).unwrap_or_default();
                    kids.iter().filter(|k| world.get::<crate::style::LayerNode>(**k).is_some_and(|l| matches!(l.0, crate::style::Layer::CapFront | crate::style::Layer::CapBack)) && world.get::<Visibility>(**k) == Some(&Visibility::Hidden)).count()
                });
                ctx.check("the middle module shows no end caps", hidden == Some(2), format!("{hidden:?}"));
                screenshot(world, "9v_diesel_row");
            }
            // Snorkel: block the middle exhaust, give it a pipe to the air.
            15 => set_block(world, mods[1] + IVec3::Y, "stone", 2),
            23 => {
                let stall = block_entity(world, mods[1]).and_then(|e| world.get::<crate::engine::Engine>(e).and_then(|g| g.stall));
                ctx.check("exhaust blocked: that module has no air", stall == Some(crate::engine::Stall::NoAir), format!("{stall:?}"));
                let cap = block_entity(world, shaft).and_then(|e| world.get::<crate::mech::NetState>(e).map(|n| n.capacity)).unwrap_or(0.0);
                ctx.check("the row runs on with two modules' power", cap == crate::engine::POWER * 2.0, format!("{cap}"));
                for y in 0..4 {
                    set_block(world, mods[1] + IVec3::new(1, y, 0), "fluid_pipe", 2);
                }
            }
            31 => {
                let stall = block_entity(world, mods[1]).and_then(|e| world.get::<crate::engine::Engine>(e).and_then(|g| g.stall));
                ctx.check("a snorkel pipe up into the air: it breathes again", stall.is_none(), format!("{stall:?}"));
                screenshot(world, "9w_snorkel");
            }
            36 => {
                clear_area(world, o - IVec3::new(2, 0, 2), o + IVec3::new(3, 5, 5));
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// Height of the first physical body (or −100).
fn body_y(world: &mut World) -> f32 {
    world.query_filtered::<&GlobalTransform, (With<crate::contraption::Contraption>, Without<crate::mounts::MountedOn>)>().iter(world).next().map(|t| t.translation().y).unwrap_or(-100.0)
}

/// Crowbar, flashlight, wheel sizes on the new suspension.
fn hand_tools(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("crowbar knocks a body, flashlight shines, three wheel sizes", |world, ctx| {
        let o = IVec3::new(50, 0, -40);
        let gi = |world: &mut World, f: &dyn Fn(&mut crate::tools::guns::GunInput)| {
            let mut g = world.resource_mut::<crate::tools::guns::GunInput>();
            g.scripted = true;
            f(&mut g);
        };
        match ctx.frames {
            1 => {
                clear_bodies(world);
                clear_area(world, o - IVec3::new(3, 0, 3), o + IVec3::new(8, 3, 6));
                set_block(world, o, "steel_frame", 2);
                set_block(world, o + IVec3::Y, "physics_toggle", 2);
                let g = world_grid(world);
                let mut glue = world.get::<crate::tools::glue::Glue>(g).map(|g| g.0.clone()).unwrap_or_default();
                glue.push((o, o + IVec3::Y));
                world.entity_mut(g).insert(crate::tools::glue::Glue(glue));
                // Wheel sizes: suspension (axle +Z) and a wheel in front.
                for (i, k) in ["wheel_small", "wheel", "wheel_large"].iter().enumerate() {
                    let c = o + IVec3::new(3 + i as i32 * 2, 1, 3);
                    set_block(world, c, "suspension", 4);
                    set_block(world, c + IVec3::Z, k, 4);
                }
                teleport(world, o.as_vec3() + Vec3::new(0.5, 0.0, 2.6));
                look_at(world, o.as_vec3() + Vec3::new(0.5, 0.6, 0.5));
                select(world, "crowbar");
            }
            4 => {
                let g = world_grid(world);
                world.write_message(crate::interact::UseBlock { grid: g, cell: o + IVec3::Y, mech: crate::content::Mech::PhysicsToggle });
            }
            20 => gi(world, &|g| g.left_pressed = true),
            21 => gi(world, &|g| g.left_pressed = false),
            26 => {
                let v = world.query_filtered::<&avian3d::prelude::LinearVelocity, With<crate::contraption::Contraption>>().iter(world).map(|v| v.0.length()).fold(0.0, f32::max);
                let moved = world.query_filtered::<&GlobalTransform, With<crate::contraption::Contraption>>().iter(world).next().map(|t| t.translation().z).unwrap_or(0.0);
                ctx.check("a crowbar hit knocks the body away", v > 0.5 || moved < -0.3, format!("{v:.2} m/s, z {moved:.2}"));
                select(world, "flashlight");
                world.resource_mut::<crate::sky::Clock>().hours = 23.0;
            }
            28 => gi(world, &|g| g.right_pressed = true),
            29 => gi(world, &|g| g.right_pressed = false),
            34 => {
                let n = world.query::<&SpotLight>().iter(world).count();
                ctx.check("RMB with the flashlight: a beam", world.resource::<crate::tools::hand::Flashlight>().0 && n > 0, format!("{n}"));
                look_at(world, o.as_vec3() + Vec3::new(5.5, 1.0, 3.5));
            }
            40 => screenshot(world, "9y_flashlight_wheels"),
            42 => {
                world.resource_mut::<crate::sky::Clock>().hours = 12.0;
                teleport(world, o.as_vec3() + Vec3::new(5.5, 0.0, 8.5));
                look_at(world, o.as_vec3() + Vec3::new(5.5, 1.2, 4.0));
            }
            48 => screenshot(world, "9z_wheel_sizes"),
            50 => {
                gi(world, &|g| g.right_pressed = true);
            }
            51 => {
                gi(world, &|g| g.right_pressed = false);
                world.resource_mut::<crate::tools::guns::GunInput>().scripted = false;
                clear_bodies(world);
                clear_area(world, o - IVec3::new(3, 0, 3), o + IVec3::new(8, 3, 6));
                select(world, "white_block");
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
}

/// The new menu, the F3 report, the hand, Ponder scenes of new blocks.
fn visuals(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("visuals: menu diorama, F3 report, held item, new Ponder scenes", |world, ctx| {
        let o = IVec3::new(-4, 0, -12);
        let scenes = ["chest", "winch", "diesel_engine"];
        let shots = ["9d_ponder_storage", "9e_ponder_winch", "9f_ponder_diesel"];
        match ctx.frames {
            1 => world.resource_mut::<NextState<Ui>>().set(Ui::Menu),
            20 => {
                let scene = world.query_filtered::<&VoxelGrid, With<crate::menu_scene::MenuScene>>().iter(world).next().map(|g| g.entities.len()).unwrap_or(0);
                ctx.check("the menu shows the workshop diorama", scene > 20, format!("{scene} block entities"));
                screenshot(world, "9a_menu_diorama");
            }
            24 => {
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                set_block(world, o, "creative_motor", 0);
                set_block(world, o + IVec3::X, "gearbox", 0);
                teleport(world, o.as_vec3() + Vec3::new(1.5, 0.0, 3.0));
                look_at(world, o.as_vec3() + Vec3::new(1.5, 0.5, 0.5));
                select(world, "wrench");
            }
            30 => {
                let gone = world.query_filtered::<(), With<crate::menu_scene::MenuScene>>().iter(world).count() == 0;
                ctx.check("leaving the menu removes the diorama", gone, "");
                screenshot(world, "9b_held_wrench");
            }
            32 => world.resource_mut::<NextState<Ui>>().set(Ui::Debug),
            36 => {
                let text = world.query_filtered::<&Text, With<crate::debug::DebugInfoText>>().iter(world).next().map(|t| t.0.clone()).unwrap_or_default();
                ctx.check("F3 shows memory, the adapter and the block under the crosshair", text.contains("Память") && text.contains("Видеокарта") && text.contains("Коробка передач"), text.lines().take(3).collect::<Vec<_>>().join(" | "));
                screenshot(world, "9c_debug_report");
            }
            40 => {
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                clear_area(world, o, o + IVec3::X);
            }
            _ => {}
        }
        if ctx.frames < 44 {
            return Step::Wait;
        }
        // Scene k: inventory → (3 frames) → Ponder → 20 s → shot → (3 frames) → next.
        let k = ctx.vars.get("vis_k").copied().unwrap_or(0.0) as usize;
        let phase = ctx.vars.get("vis_p").copied().unwrap_or(0.0) as i32;
        let since = ctx.t - ctx.vars.get("vis_t").copied().unwrap_or(0.0);
        let next = |ctx: &mut Ctx, p: i32| {
            ctx.vars.insert("vis_p".into(), p as f32);
            ctx.vars.insert("vis_t".into(), ctx.t);
        };
        if k >= scenes.len() {
            if phase == 0 {
                world.resource_mut::<NextState<Ui>>().set(Ui::Game);
                next(ctx, 1);
            } else if since > 0.5 {
                return Step::Next;
            }
            return Step::Wait;
        }
        match phase {
            0 => {
                world.resource_mut::<NextState<Ui>>().set(Ui::Inventory);
                next(ctx, 1);
            }
            1 if since > 0.4 => {
                world.resource_mut::<crate::ponder::Ponder>().item = Some(crate::content::ItemId::k(scenes[k]));
                world.resource_mut::<NextState<Ui>>().set(Ui::Ponder);
                next(ctx, 2);
            }
            2 if since > 20.0 => {
                screenshot(world, shots[k]);
                next(ctx, 3);
            }
            3 if since > 0.5 => {
                ctx.vars.insert("vis_k".into(), (k + 1) as f32);
                next(ctx, 0);
            }
            _ => {}
        }
        Step::Wait
    }));
}
