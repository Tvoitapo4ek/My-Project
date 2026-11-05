//! A tiny frame profiler: how long parts of a frame take on the CPU
//! (main world), smoothed. Shown in the F3 menu and used by the `perf`
//! autotest stage.
//!
//! A section is timed by a guard inside the system:
//! `let _t = profile::scope("name");`. Fixed-step sections add up over
//! every fixed step of the frame.

use std::{collections::HashMap, time::Instant};

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

#[derive(Resource, Default)]
pub struct Profile {
    /// Smoothed milliseconds per frame, by section.
    pub ms: Vec<(&'static str, f32)>,
    start: HashMap<&'static str, Instant>,
    acc: HashMap<&'static str, f32>,
}

impl Profile {
    #[allow(dead_code)]
    pub fn get(&self, name: &str) -> f32 {
        self.ms.iter().find(|(n, _)| *n == name).map(|(_, v)| *v).unwrap_or(0.0)
    }
    fn begin(&mut self, name: &'static str) {
        self.start.insert(name, Instant::now());
    }
    fn end(&mut self, name: &'static str) {
        if let Some(t) = self.start.remove(name) {
            *self.acc.entry(name).or_default() += t.elapsed().as_secs_f32() * 1000.0;
        }
    }
}

static SCOPES: std::sync::Mutex<Vec<(&'static str, f32)>> = std::sync::Mutex::new(Vec::new());

/// Times the rest of the enclosing block as section `name`:
/// `let _t = profile::scope("механика");` (works from parallel systems).
pub fn scope(name: &'static str) -> Scope {
    Scope(name, Instant::now())
}

pub struct Scope(&'static str, Instant);

impl Drop for Scope {
    fn drop(&mut self) {
        if let Ok(mut v) = SCOPES.lock() {
            v.push((self.0, self.1.elapsed().as_secs_f32() * 1000.0));
        }
    }
}

pub const FRAME: &str = "кадр (CPU)";
pub const FIXED: &str = "физика";
/// Fixed steps per frame (not milliseconds).
pub const STEPS: &str = "шагов физики за кадр";

fn frame_begin(mut p: ResMut<Profile>) {
    p.begin(FRAME);
}

fn frame_end(mut p: ResMut<Profile>) {
    p.end(FRAME);
    let scoped = SCOPES.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default();
    for (n, ms) in scoped {
        *p.acc.entry(n).or_default() += ms;
    }
    let acc = std::mem::take(&mut p.acc);
    for (name, v) in acc {
        match p.ms.iter_mut().find(|(n, _)| *n == name) {
            Some((_, m)) => *m += (v - *m) * 0.1,
            None => p.ms.push((name, v)),
        }
    }
}

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Profile>().add_systems(First, frame_begin).add_systems(Last, frame_end);
        app.add_systems(FixedFirst, |mut p: ResMut<Profile>| p.begin(FIXED)).add_systems(FixedLast, |mut p: ResMut<Profile>| {
            p.end(FIXED);
            *p.acc.entry(STEPS).or_default() += 1.0;
        });
        // Exact per-section numbers: run the frame on one thread.
        if std::env::var("SANDBOX_PROFILE_SINGLE").is_ok() {
            use bevy::ecs::schedule::SingleThreadedExecutor;
            for label in [Update.intern(), PostUpdate.intern(), PreUpdate.intern(), FixedUpdate.intern(), FixedPostUpdate.intern()] {
                app.edit_schedule(label, |s| {
                    s.set_executor(SingleThreadedExecutor::new());
                });
            }
        }
    }
}

/// Text for the debug menu.
pub fn report(p: &Profile) -> String {
    let mut v = p.ms.clone();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v.iter().map(|(n, ms)| if *n == STEPS { format!("{n}: {ms:.1}") } else { format!("{n}: {ms:.2} мс") }).collect::<Vec<_>>().join("\n")
}
