//! Scripted end-to-end scenario (`--autotest`).
//!
//! Runs the real game (window, renderer, physics) and drives it through a
//! list of steps with exclusive `World` access: it simulates player intent,
//! uses the same public APIs the tools use, and checks observable results
//! (positions, block states, joints, inventories, save files). Each check is
//! logged as PASS/FAIL; the process exits with code 1 if any check failed.
//! Screenshots are written when `--screenshots <dir>` is given.

pub mod scenario;
pub mod scenario3;
pub mod scenario4;
pub mod scenario5;
pub mod scenario6;
pub mod scenario7;

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::core::config::LaunchOptions;

/// Result of one tick of a step.
pub enum Step {
    /// Keep running this step next frame.
    Wait,
    /// Step finished; go to the next one.
    Next,
}

pub type StepFn = Box<dyn FnMut(&mut World, &mut Ctx) -> Step + Send + Sync>;

/// Mutable context shared by the steps.
#[derive(Default)]
pub struct Ctx {
    /// Seconds spent in the current step.
    pub t: f32,
    /// Frames spent in the current step.
    pub frames: u32,
    pub passed: Vec<String>,
    pub failed: Vec<String>,
    /// Scratch storage between steps.
    pub vars: std::collections::HashMap<String, f32>,
    pub entities: std::collections::HashMap<String, Entity>,
}

impl Ctx {
    pub fn check(&mut self, name: &str, ok: bool, detail: impl std::fmt::Display) {
        if ok {
            info!("[AUTOTEST] PASS {name}: {detail}");
            self.passed.push(name.to_string());
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
    pub elapsed_total: f32,
}

fn run_scenario(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let Some(mut scenario) = world.remove_resource::<Scenario>() else {
        return;
    };
    scenario.elapsed_total += dt;
    if scenario.elapsed_total > 600.0 {
        scenario.ctx.failed.push("scenario timeout".into());
        scenario.index = scenario.steps.len();
    }
    if scenario.index < scenario.steps.len() {
        let idx = scenario.index;
        scenario.ctx.t += dt;
        scenario.ctx.frames += 1;
        let (name, step) = &mut scenario.steps[idx];
        if scenario.ctx.frames == 1 {
            info!("[AUTOTEST] step {}: {}", idx + 1, name);
        }
        let result = step(world, &mut scenario.ctx);
        if matches!(result, Step::Next) {
            scenario.index += 1;
            scenario.ctx.t = 0.0;
            scenario.ctx.frames = 0;
        }
    } else if scenario.ctx.t < 1.5 {
        // Let pending screenshots reach the disk before exiting.
        scenario.ctx.t += dt;
    } else {
        let failed = scenario.ctx.failed.len();
        info!(
            "[AUTOTEST] finished: {} passed, {} failed",
            scenario.ctx.passed.len(),
            failed
        );
        for f in &scenario.ctx.failed {
            error!("[AUTOTEST]   failed: {f}");
        }
        std::process::exit(if failed == 0 { 0 } else { 1 });
    }
    world.insert_resource(scenario);
}

/// Requests a screenshot of the primary window if screenshots are enabled.
pub fn screenshot(world: &mut World, name: &str) {
    let dir = world.resource::<LaunchOptions>().screenshot_dir.clone();
    if let Some(dir) = dir {
        let _ = std::fs::create_dir_all(&dir);
        let path = format!("{dir}/{name}.png");
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

pub struct AutotestPlugin;

impl Plugin for AutotestPlugin {
    fn build(&self, app: &mut App) {
        let enabled = app.world().resource::<LaunchOptions>().autotest;
        if !enabled {
            return;
        }
        app.insert_resource(scenario::build())
            .add_systems(PreUpdate, run_scenario.after(bevy::input::InputSystems));
    }
}
