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

/// Saves an image asset (e.g. the item icon atlas).
pub fn save_image(world: &mut World, image: Handle<Image>, name: &str) {
    if let Some(dir) = world.resource::<Shots>().0.clone() {
        world.spawn(Screenshot::image(image)).observe(save_to_disk(format!("{dir}/{name}.png")));
    }
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

pub fn key(world: &mut World, k: KeyCode, down: bool) {
    let mut m = world.resource_mut::<ButtonInput<KeyCode>>();
    if down { m.press(k) } else { m.release(k) }
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
    if want("icons") {
        steps.push(step("icon atlas", |world, ctx| {
            if ctx.frames == 60 {
                if let Some(icons) = world.get_resource::<crate::icons::ItemIcons>() {
                    let h = icons.image.clone();
                    world.spawn((ImageNode::new(h), Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: px(512), height: px(256), ..default() }, GlobalZIndex(100)));
                }
            }
            if ctx.frames == 64 {
                screenshot(world, "00_icon_atlas_ui");
            }
            if ctx.frames > 68 { Step::Next } else { Step::Wait }
        }));
    }
    if want("basics") {
        basics(&mut steps);
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
                return Step::Next;
            }
            _ => {}
        }
        Step::Wait
    }));
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
