//! Updates without reinstalling.
//!
//! Every push of the game builds `Sandbox-macOS.zip` on GitHub Actions and
//! publishes it as a release `sandbox-b<build>` (`.github/workflows/sandbox.yml`).
//! The main menu asks GitHub for the newest build; if it is newer than this
//! one, «Обновить» downloads it, swaps the app in place and restarts.
//! Your files (settings, blueprints, textures, models) live outside the app
//! and stay. Downloads made by the game itself carry no quarantine flag,
//! so macOS does not ask again — only the very first install needs
//! right-click → «Открыть».
//!
//! Uses the system `curl` and `ditto` (both ship with macOS): no network
//! code in the game.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use bevy::prelude::*;

use crate::{
    core::Ui,
    ui::{FRAME, font},
};

pub const REPO: &str = "Tvoitapo4ek/My-Project";
pub const TAG: &str = "sandbox-b";
pub const ASSET: &str = "Sandbox-macOS.zip";

/// This build's number (set by CI; 0 = a local build).
pub fn build() -> u32 {
    option_env!("SANDBOX_BUILD").and_then(|b| b.parse().ok()).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available { build: u32, url: String, notes: String },
    Downloading(u32),
    Ready,
    Failed(String),
    /// Not running from an installed app (development build).
    Unsupported,
}

#[derive(Resource, Clone)]
pub struct Updater(pub Arc<Mutex<State>>);

impl Default for Updater {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State::Idle)))
    }
}

impl Updater {
    pub fn get(&self) -> State {
        self.0.lock().map(|s| s.clone()).unwrap_or(State::Idle)
    }
    fn set(&self, s: State) {
        if let Ok(mut g) = self.0.lock() {
            *g = s;
        }
    }
}

/// The newest `sandbox-b<N>` release with a macOS zip: (N, url, notes).
pub fn newest(releases_json: &str) -> Option<(u32, String, String)> {
    let v: serde_json::Value = serde_json::from_str(releases_json).ok()?;
    v.as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let n: u32 = r["tag_name"].as_str()?.strip_prefix(TAG)?.parse().ok()?;
            let url = r["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(ASSET))?["browser_download_url"].as_str()?.to_string();
            let notes = r["body"].as_str().unwrap_or("").lines().next().unwrap_or("").to_string();
            Some((n, url, notes))
        })
        .max_by_key(|r| r.0)
}

fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("curl").args(["-fsSL", "--max-time", "600", "-H", "User-Agent: sandbox-updater"]).args(args).output().map_err(|e| format!("curl: {e}"))?;
    if out.status.success() { Ok(out.stdout) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn check(up: Updater) {
    up.set(State::Checking);
    let r = curl(&[&format!("https://api.github.com/repos/{REPO}/releases?per_page=30")]);
    up.set(match r {
        Err(e) => State::Failed(format!("нет связи с GitHub ({e})")),
        Ok(body) => match newest(&String::from_utf8_lossy(&body)) {
            Some((n, url, notes)) if n > build() => State::Available { build: n, url, notes },
            _ => State::UpToDate,
        },
    });
}

/// Download, unpack next to the app, swap, ready to restart.
fn install(up: Updater, n: u32, url: String) {
    up.set(State::Downloading(n));
    let r = (|| -> Result<(), String> {
        let app = crate::paths::bundle().ok_or("не приложение")?;
        let parent = app.parent().ok_or("нет папки приложения")?;
        let stage = crate::paths::user_dir().join("update");
        let _ = std::fs::remove_dir_all(&stage);
        std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
        let zip = stage.join(ASSET);
        curl(&["-o", &zip.to_string_lossy(), &url])?;
        let ok = Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&stage).status().map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("архив не распаковался".into());
        }
        let new = find_app(&stage).ok_or("в архиве нет .app")?;
        // Swap: the running program keeps working from the old files.
        let old = parent.join("Sandbox.old.app");
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&app, &old).map_err(|e| format!("нет прав на {}: {e}", parent.display()))?;
        if let Err(e) = std::fs::rename(&new, &app) {
            let _ = std::fs::rename(&old, &app);
            return Err(e.to_string());
        }
        let _ = std::fs::remove_dir_all(&old);
        let _ = std::fs::remove_dir_all(&stage);
        Ok(())
    })();
    up.set(match r {
        Ok(()) => State::Ready,
        Err(e) => State::Failed(e),
    });
}

fn find_app(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "app"))
}

fn start_check(up: Res<Updater>) {
    match up.get() {
        State::Idle | State::Failed(_) => {}
        _ => return,
    }
    if crate::paths::bundle().is_none() || std::env::args().any(|a| a == "--autotest") {
        up.set(State::Unsupported);
        return;
    }
    // Started straight from Downloads, macOS runs a read-only copy.
    if crate::paths::bundle().is_some_and(|b| b.to_string_lossy().contains("/AppTranslocation/")) {
        up.set(State::Failed("перенесите Sandbox.app в «Программы» и откройте оттуда".into()));
        return;
    }
    let u = up.clone();
    std::thread::spawn(move || check(u));
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Label;
#[derive(Component)]
struct Button;

fn open(mut commands: Commands) {
    commands
        .spawn((Panel, Node { position_type: PositionType::Absolute, right: px(16), bottom: px(16), column_gap: px(10), align_items: AlignItems::Center, padding: UiRect::axes(px(10), px(6)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgba(0.08, 0.09, 0.11, 0.85)), BorderColor::all(FRAME)))
        .with_children(|p| {
            p.spawn((Label, Text::new(""), font(13.0), TextColor(Color::srgb(0.75, 0.75, 0.77))));
            p.spawn((Button, Node { padding: UiRect::axes(px(10), px(4)), border: UiRect::all(px(1)), display: Display::None, ..default() }, BackgroundColor(Color::srgb(0.42, 0.33, 0.14)), BorderColor::all(FRAME)))
                .observe(on_click)
                .with_children(|b| {
                    b.spawn((Text::new("Обновить"), font(13.0), TextColor(Color::WHITE), Pickable::IGNORE));
                });
        });
}

fn close(mut commands: Commands, q: Query<Entity, With<Panel>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn on_click(_: On<PointerClick>, up: Res<Updater>, mut exit: MessageWriter<AppExit>) {
    match up.get() {
        State::Available { build, url, .. } => {
            let u = up.clone();
            std::thread::spawn(move || install(u, build, url));
        }
        State::Ready => {
            // Start the new app and leave.
            if let Some(app) = crate::paths::bundle() {
                let _ = Command::new("open").arg("-n").arg(&app).spawn();
            }
            exit.write(AppExit::Success);
        }
        State::Failed(_) => up.set(State::Idle),
        _ => {}
    }
}

/// Text and button of the panel for a state.
pub fn label(s: &State) -> (String, Option<&'static str>) {
    let me = if build() == 0 { "локальная сборка".to_string() } else { format!("сборка {}", build()) };
    match s {
        State::Idle | State::Checking => (format!("{me} · проверка обновлений…"), None),
        State::UpToDate => (format!("{me} · последняя версия"), None),
        State::Available { build, notes, .. } => (format!("{me} · есть сборка {build}{}", if notes.is_empty() { String::new() } else { format!(": {notes}") }), Some("Обновить")),
        State::Downloading(n) => (format!("загрузка сборки {n}…"), None),
        State::Ready => ("обновление установлено".into(), Some("Перезапустить")),
        State::Failed(e) => (format!("обновление: {e}"), Some("Повторить")),
        State::Unsupported => (me, None),
    }
}

fn refresh(up: Res<Updater>, mut text: Query<&mut Text, With<Label>>, mut button: Query<(&mut Node, &Children), With<Button>>, mut texts: Query<&mut Text, Without<Label>>, mut last: Local<Option<State>>) {
    let s = up.get();
    if last.as_ref() == Some(&s) && !text.is_empty() {
        return;
    }
    let (l, b) = label(&s);
    for mut t in &mut text {
        t.0 = l.clone();
    }
    for (mut n, children) in &mut button {
        n.display = if b.is_some() { Display::Flex } else { Display::None };
        if let Some(b) = b {
            for c in children.iter() {
                if let Ok(mut t) = texts.get_mut(c) {
                    t.0 = b.to_string();
                }
            }
        }
    }
    *last = Some(s);
}

pub struct UpdatePlugin;

impl Plugin for UpdatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Updater>()
            .add_systems(OnEnter(Ui::Menu), (open, start_check))
            .add_systems(OnExit(Ui::Menu), close)
            .add_systems(Update, refresh.run_if(in_state(Ui::Menu)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_newest_macos_build() {
        let json = r#"[
            {"tag_name": "sandbox-b7", "draft": false, "body": "Seven", "assets": [{"name": "Sandbox-macOS.zip", "browser_download_url": "https://x/7.zip"}]},
            {"tag_name": "sandbox-b12", "draft": false, "body": "Twelve\nmore", "assets": [{"name": "Sandbox-macOS.zip", "browser_download_url": "https://x/12.zip"}]},
            {"tag_name": "sandbox-b13", "draft": true, "assets": [{"name": "Sandbox-macOS.zip", "browser_download_url": "https://x/13.zip"}]},
            {"tag_name": "sandbox-b14", "draft": false, "assets": []},
            {"tag_name": "other-b99", "draft": false, "assets": [{"name": "Sandbox-macOS.zip", "browser_download_url": "https://x/99.zip"}]}
        ]"#;
        assert_eq!(newest(json), Some((12, "https://x/12.zip".into(), "Twelve".into())));
        assert_eq!(newest("[]"), None);
        assert_eq!(newest("not json"), None);
        assert_eq!(build(), 0, "local builds are build 0");
        assert_eq!(label(&State::Available { build: 3, url: String::new(), notes: String::new() }).1, Some("Обновить"));
    }
}
