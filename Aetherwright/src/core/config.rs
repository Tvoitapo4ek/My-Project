//! Data-driven configuration. Defaults live in code; `assets/config/game.json`
//! (optional) overrides any subset of fields.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const CONFIG_PATH: &str = "assets/config/game.json";

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameConfig {
    pub video: VideoConfig,
    pub world: WorldConfig,
    pub physics: PhysicsConfig,
    pub controls: ControlsConfig,
    pub camera: CameraConfig,
    pub player: PlayerConfig,
    pub hud: HudConfig,
}

/// Which HUD panels are shown (modular HUD; F1 hides everything).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HudConfig {
    pub hotbar: bool,
    pub vitals: bool,
    pub target_info: bool,
    pub scan_panel: bool,
    pub notifications: bool,
    pub tool_line: bool,
    pub energy: bool,
    /// UI scale multiplier.
    pub scale: f32,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            hotbar: true,
            vitals: true,
            target_info: true,
            scan_panel: true,
            notifications: true,
            tool_line: true,
            energy: true,
            scale: 1.0,
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            hud: HudConfig::default(),
            video: VideoConfig::default(),
            world: WorldConfig::default(),
            physics: PhysicsConfig::default(),
            controls: ControlsConfig::default(),
            camera: CameraConfig::default(),
            player: PlayerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub shadows: bool,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 900,
            vsync: true,
            shadows: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldConfig {
    /// Edge length of a cubic chunk in blocks.
    pub chunk_size: i32,
    /// World extent in chunks along X and Z (the test region is finite).
    pub size_chunks_xz: i32,
    /// World height in chunks.
    pub height_chunks: i32,
    pub seed: u32,
    pub sea_level: i32,
    /// Max chunks re-meshed per frame after edits.
    pub remesh_budget_per_frame: usize,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            chunk_size: 16,
            size_chunks_xz: 10,
            height_chunks: 5,
            seed: 1337,
            sea_level: 22,
            remesh_budget_per_frame: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PhysicsConfig {
    pub tick_hz: f64,
    pub gravity: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            tick_hz: 60.0,
            gravity: 19.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlsConfig {
    /// Radians per pixel of mouse motion.
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
}

impl Default for ControlsConfig {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.0022,
            invert_y: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfig {
    pub fov_degrees: f32,
    pub third_person_distance: f32,
    pub third_person_min: f32,
    pub third_person_max: f32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            fov_degrees: 75.0,
            third_person_distance: 4.5,
            third_person_min: 1.5,
            third_person_max: 10.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub crouch_speed: f32,
    pub swim_speed: f32,
    pub fly_speed: f32,
    /// Horizontal acceleration on ground (m/s²).
    pub ground_accel: f32,
    /// Horizontal acceleration in the air (m/s²) — low, so impulses persist.
    pub air_accel: f32,
    pub jump_speed: f32,
    pub reach: f32,
    pub auto_pickup: bool,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            walk_speed: 5.0,
            sprint_speed: 8.5,
            crouch_speed: 2.4,
            swim_speed: 3.5,
            fly_speed: 14.0,
            ground_accel: 60.0,
            air_accel: 12.0,
            jump_speed: 7.6,
            reach: 6.0,
            auto_pickup: true,
        }
    }
}

impl GameConfig {
    /// Loads overrides from [`CONFIG_PATH`]; a missing or broken file falls
    /// back to defaults (with a warning) instead of aborting the game.
    pub fn load_or_default() -> Self {
        match std::fs::read_to_string(CONFIG_PATH) {
            Ok(text) => match serde_json::from_str::<GameConfig>(&text) {
                Ok(cfg) => cfg.sanitized(),
                Err(err) => {
                    warn!("Config {CONFIG_PATH} is invalid ({err}); using defaults");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Clamps values into ranges the engine can handle.
    pub fn sanitized(mut self) -> Self {
        self.world.chunk_size = self.world.chunk_size.clamp(8, 32);
        self.world.size_chunks_xz = self.world.size_chunks_xz.clamp(2, 32);
        self.world.height_chunks = self.world.height_chunks.clamp(2, 12);
        let height = self.world.chunk_size * self.world.height_chunks;
        self.world.sea_level = self.world.sea_level.clamp(1, height - 4);
        self.physics.tick_hz = self.physics.tick_hz.clamp(30.0, 240.0);
        self.camera.third_person_distance = self
            .camera
            .third_person_distance
            .clamp(self.camera.third_person_min, self.camera.third_person_max);
        self
    }
}

/// Command line options (not persisted).
#[derive(Resource, Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Run a scripted scenario that exercises the core mechanics, then exit
    /// with a non-zero code on failure. See `autotest` module.
    pub autotest: bool,
    /// Directory where screenshots of the autotest are written.
    pub screenshot_dir: Option<String>,
    /// Start in creative mode.
    pub creative: bool,
}

impl LaunchOptions {
    pub fn from_args(args: impl Iterator<Item = String>) -> Self {
        let mut opts = Self::default();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--autotest" => opts.autotest = true,
                "--creative" => opts.creative = true,
                "--screenshots" => opts.screenshot_dir = args.next(),
                other => eprintln!("unknown argument: {other}"),
            }
        }
        opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_overrides_only_given_fields() {
        let cfg: GameConfig = serde_json::from_str(r#"{"world":{"chunk_size":24}}"#).unwrap();
        assert_eq!(cfg.world.chunk_size, 24);
        assert_eq!(
            cfg.world.size_chunks_xz,
            WorldConfig::default().size_chunks_xz
        );
        assert_eq!(cfg.player.walk_speed, PlayerConfig::default().walk_speed);
    }

    #[test]
    fn sanitize_clamps_out_of_range() {
        let mut cfg = GameConfig::default();
        cfg.world.chunk_size = 1000;
        cfg.world.sea_level = -5;
        let cfg = cfg.sanitized();
        assert_eq!(cfg.world.chunk_size, 32);
        assert_eq!(cfg.world.sea_level, 1);
    }
}
