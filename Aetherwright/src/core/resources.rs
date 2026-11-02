//! Shared world-level resources that several subsystems read.

use bevy::prelude::*;

/// In-game clock. `time_of_day` is in [0, 1): 0 = midnight, 0.25 = sunrise,
/// 0.5 = noon, 0.75 = sunset.
#[derive(Resource, Debug, Clone)]
pub struct WorldClock {
    pub time_of_day: f32,
    /// Real seconds per full in-game day.
    pub day_length_secs: f32,
    pub paused: bool,
}

impl Default for WorldClock {
    fn default() -> Self {
        Self {
            time_of_day: 0.32,
            day_length_secs: 1200.0,
            paused: false,
        }
    }
}

impl WorldClock {
    /// 0 at night, 1 at noon; smooth.
    pub fn daylight(&self) -> f32 {
        let sun_height = (self.sun_angle()).sin();
        (sun_height * 3.0 + 0.3).clamp(0.0, 1.0)
    }

    /// Angle of the sun above the horizon (radians, negative = below).
    pub fn sun_angle(&self) -> f32 {
        (self.time_of_day - 0.25) * std::f32::consts::TAU
    }
}

fn advance_clock(time: Res<Time>, mut clock: ResMut<WorldClock>) {
    if clock.paused {
        return;
    }
    let dt = time.delta_secs() / clock.day_length_secs.max(1.0);
    clock.time_of_day = (clock.time_of_day + dt).rem_euclid(1.0);
}

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldClock>()
            .add_systems(Update, advance_clock.run_if(super::states::in_game));
    }
}
