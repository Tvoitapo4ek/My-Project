//! Where the game's files live.
//!
//! * **Shipped files** (`assets/…`) — read-only, inside the app:
//!   `Sandbox.app/Contents/Resources/assets` on macOS, next to the
//!   executable elsewhere, or the project folder in development.
//! * **Your files** — settings, blueprints, edited textures, own models:
//!   `~/Library/Application Support/Sandbox` on macOS, the project folder in
//!   development. The game runs with this as its current directory, so all
//!   saves land there, and an update (which replaces the app) never touches
//!   them.
//!
//! Reading `assets/…` looks in your folder first: an edited texture or a
//! model of yours overrides the shipped one.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

struct Dirs {
    /// Holds the shipped `assets` folder.
    shipped: PathBuf,
    /// Current directory: your files.
    user: PathBuf,
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.canonicalize().ok()?.parent().map(Path::to_path_buf)
}

/// `…/Sandbox.app/Contents` when running from a macOS app bundle.
pub fn bundle_contents() -> Option<PathBuf> {
    let dir = exe_dir()?;
    (dir.file_name()? == "MacOS" && dir.parent()?.file_name()? == "Contents").then(|| dir.parent().unwrap().to_path_buf())
}

/// The `.app` folder itself.
pub fn bundle() -> Option<PathBuf> {
    bundle_contents()?.parent().map(Path::to_path_buf)
}

fn dirs() -> &'static Dirs {
    static D: OnceLock<Dirs> = OnceLock::new();
    D.get_or_init(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        if let Some(contents) = bundle_contents() {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| cwd.clone());
            return Dirs { shipped: contents.join("Resources"), user: home.join("Library/Application Support/Sandbox") };
        }
        // Development: the project folder (cargo run, tests, autotest).
        if cwd.join("assets").is_dir() {
            return Dirs { shipped: cwd.clone(), user: cwd };
        }
        // A plain folder build (Linux / Windows zip): next to the executable.
        let exe = exe_dir().unwrap_or(cwd);
        Dirs { shipped: exe.clone(), user: exe }
    })
}

/// Call first thing in `main`: makes your folder the current directory.
pub fn init() {
    let d = dirs();
    let _ = std::fs::create_dir_all(&d.user);
    let _ = std::env::set_current_dir(&d.user);
}

/// The shipped `assets` folder (for Bevy's asset server).
pub fn shipped_assets() -> PathBuf {
    dirs().shipped.join("assets")
}

/// A file under `assets/`: yours if you have one, else the shipped one.
pub fn asset(rel: &str) -> PathBuf {
    let own = dirs().user.join("assets").join(rel);
    if own.exists() { own } else { shipped_assets().join(rel) }
}

/// Where to write a file under `assets/` (your folder).
pub fn own_asset(rel: &str) -> PathBuf {
    dirs().user.join("assets").join(rel)
}

/// Your folder (shown in messages).
pub fn user_dir() -> PathBuf {
    dirs().user.clone()
}

#[cfg(test)]
mod tests {
    #[test]
    fn development_uses_the_project_folder() {
        assert!(super::bundle_contents().is_none());
        assert!(super::asset("textures/steel.png").exists());
        assert!(super::shipped_assets().join("content/blocks.ron").exists());
    }
}
