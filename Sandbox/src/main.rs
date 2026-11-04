//! Sandbox — a voxel physics sandbox in the spirit of Garry's Mod,
//! Minecraft and Hytale: creative building, Create-like mechanisms,
//! contraptions that become physical, physics tools.
//!
//! Every feature is its own plugin; content (blocks, tools, categories)
//! is data in `assets/content/`. See README.md and docs/.

mod assembly;
mod autotest;
mod bbmodel;
mod belts;
mod chains;
mod content;
mod contraption;
mod core;
mod debug;
mod doors;
mod engine;
mod fluids;
mod goggles;
mod grid;
mod icons;
mod interact;
mod inventory;
mod lights;
mod menu;
mod logic;
mod mech;
mod models;
mod paint;
mod paths;
mod ponder;
mod mounts;
mod player;
mod settings;
mod signal;
mod sound;
mod sky;
mod tracks;
mod style;
mod telemetry;
mod tools;
mod ui;
mod update;
mod vehicle;
mod water;
mod world;

use avian3d::prelude::*;
use bevy::{prelude::*, window::WindowResolution};

fn main() {
    paths::init();
    let args: Vec<String> = std::env::args().collect();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window { title: "Sandbox".into(), resolution: WindowResolution::new(1280, 720), ..default() }),
                ..default()
            })
            .set(ImagePlugin::default_nearest())
            .set(AssetPlugin { file_path: paths::shipped_assets().to_string_lossy().into_owned(), ..default() }),
    )
    .add_plugins(PhysicsPlugins::default())
    .add_plugins((
        core::CorePlugin,
        style::StylePlugin,
        grid::GridPlugin,
        world::WorldPlugin,
        sky::SkyPlugin,
        player::PlayerPlugin,
        interact::InteractPlugin,
        icons::IconsPlugin,
        ui::UiPlugin,
        debug::DebugPlugin,
    ))
    .add_plugins((
        mech::MechPlugin,
        settings::SettingsPlugin,
        signal::SignalPlugin,
        vehicle::VehiclePlugin,
        contraption::ContraptionPlugin,
        tools::ToolsPlugin,
        telemetry::TelemetryPlugin,
        lights::LightsPlugin,
        sound::SoundPlugin,
        mounts::MountsPlugin,
        fluids::FluidsPlugin,
        engine::EnginePlugin,
        water::WaterPlugin,
        logic::LogicPlugin,
        menu::MenuPlugin,
    ))
    .add_plugins((doors::DoorsPlugin, goggles::GogglesPlugin, ponder::PonderPlugin, paint::PaintPlugin, update::UpdatePlugin, chains::ChainsPlugin, belts::BeltsPlugin, tracks::TracksPlugin))
    .add_plugins(autotest::AutotestPlugin::from_args(&args));
    app.run();
}
