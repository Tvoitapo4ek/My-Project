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
        ctx.check("white platform under the spawn", get_block(world, IVec3::new(0, -1, 0)).id == BlockId::k("white_block"), "");
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
                ctx.check("MMB picks the block", held.as_deref() == Some("white_block"), format!("{held:?}"));
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
                ctx.check("telemetry: motor RPM on the digital display", a == 96.0, format!("{a}"));
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
            30 => {
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
        if ctx.frames > 30 && ctx.t > ctx.vars["t0"] + 4.0 {
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
            22 => {
                let n = contraptions(world).len();
                ctx.check("toggle again: the car returns to the block grid", n == 0, format!("{n} bodies left"));
                ctx.check("all blocks are back", (world_cells(world) as f32 - ctx.vars["cells0"]).abs() < 0.5, format!("{} vs {}", world_cells(world), ctx.vars["cells0"]));
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
            26 => click(world, MouseButton::Right, true),
            27 => click(world, MouseButton::Right, false),
            34 => {
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
            app.insert_resource(build()).add_systems(PreUpdate, run.after(bevy::input::InputSystems));
        }
    }
}
