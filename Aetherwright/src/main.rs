//! Aetherwright — first playable of a voxel sandbox about exploration,
//! engineering and magic.
//!
//! `main.rs` only parses the launch options; everything else lives in
//! feature plugins assembled in [`app`].

mod app;
mod autotest;
mod core;
mod items;
mod player;
mod tools;
mod ui;
mod world;

fn main() {
    let options = core::config::LaunchOptions::from_args(std::env::args().skip(1));
    app::build_app(options).run();
}
