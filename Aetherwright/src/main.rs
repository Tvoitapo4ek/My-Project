//! Aetherwright — first playable of a voxel sandbox about exploration,
//! engineering and magic.
//!
//! `main.rs` only parses the launch options; everything else lives in
//! feature plugins assembled in [`app`].

mod app;
mod autotest;
mod building;
mod content;
mod core;
mod crafting;
mod creative;
mod holo;
mod items;
mod machinery;
mod magic;
mod models;
mod physics;
mod player;
mod save;
mod survival;
mod tools;
mod ui;
mod world;

/// Finds the game root (the folder containing `assets/`) and makes it the
/// working directory, so content, textures, saves, mods and Bevy's asset
/// server all resolve paths the same way — whether started with `cargo run`,
/// from `target/debug`, or from an installed folder.
fn enter_game_root() -> std::path::PathBuf {
    let has_assets = |p: &std::path::Path| p.join("assets").is_dir();
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut root = if has_assets(&cwd) {
        Some(cwd.clone())
    } else {
        None
    };
    if root.is_none()
        && let Ok(exe) = std::env::current_exe()
    {
        root = exe
            .ancestors()
            .skip(1)
            .find(|p| has_assets(p))
            .map(|p| p.to_path_buf());
    }
    let root = root.unwrap_or(cwd);
    let _ = std::env::set_current_dir(&root);
    root
}

fn main() {
    let root = enter_game_root();
    if std::env::args().any(|a| a == "--export-textures") {
        content::init(&[std::path::PathBuf::from("assets")]);
        let dir = std::path::Path::new("assets/textures/blocks");
        match world::materials::export_textures(dir) {
            Ok(n) => println!(
                "exported {n} textures to {} (existing files kept)",
                dir.display()
            ),
            Err(e) => eprintln!("export failed: {e}"),
        }
        return;
    }
    let options = core::config::LaunchOptions::from_args(std::env::args().skip(1));
    // Content (blocks, items, recipes) must be loaded before anything uses it.
    let mut roots = vec![std::path::PathBuf::from("assets")];
    roots.extend(content::mod_roots());
    let registry = content::init(&roots);
    eprintln!(
        "content: {} blocks, {} textures, {} items, {} recipes from {:?}",
        registry.blocks.len(),
        registry.textures.len(),
        registry.items.len(),
        registry.recipes.len(),
        registry.sources
    );
    app::build_app(options, root).run();
}
