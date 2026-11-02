//! App assembly: engine plugins + every game plugin in dependency order.

use avian3d::prelude::*;
use bevy::{prelude::*, window::PresentMode};

use crate::core::config::LaunchOptions;

pub fn build_app(options: LaunchOptions) -> App {
    let mut app = App::new();

    let config = crate::core::config::GameConfig::load_or_default();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Aetherwright — First Playable".into(),
                    resolution: (config.video.width, config.video.height).into(),
                    present_mode: if config.video.vsync {
                        PresentMode::AutoVsync
                    } else {
                        PresentMode::AutoNoVsync
                    },
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),
    )
    // Physics runs in Bevy's fixed timestep (FixedPostUpdate) and rendered
    // transforms of every rigid body are interpolated between ticks.
    .add_plugins(PhysicsPlugins::default().set(PhysicsInterpolationPlugin::interpolate_all()))
    .add_plugins(PhysicsDebugPlugin)
    .insert_resource(Time::<Fixed>::from_hz(config.physics.tick_hz))
    .insert_resource(Gravity(Vec3::NEG_Y * config.physics.gravity))
    .insert_resource(config)
    .insert_resource(options);

    app.add_plugins((
        crate::core::CorePlugin,
        crate::world::WorldPlugin,
        crate::player::PlayerPlugin,
        crate::items::ItemsPlugin,
        crate::tools::ToolsPlugin,
        crate::physics::PhysicsPlugin,
        crate::machinery::MachineryPlugin,
        crate::building::BuildingPlugin,
        crate::ui::UiPlugin,
        crate::autotest::AutotestPlugin,
    ));

    app
}
