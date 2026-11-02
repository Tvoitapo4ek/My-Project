//! Точка входа: окно winit + игровой цикл + интерфейс.
//!
//! `cargo run --release` — играть.
//! `cargo run --release -- --screenshot shot.png [--frames 120] [--screen menu|game|inventory|pause|settings]`
//! — снять кадр и выйти.

// В релизной сборке под Windows не открывать окно консоли.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod game;
mod input;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use game::Game;
use glam::Vec2;
use input::Input;
use vx_render::{Frame, Renderer};
use vx_ui::{Action, FontAtlas, Gui, Screen, UiContext, UiInput, UiKey};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

const SKY: [f32; 3] = [0.62, 0.78, 0.96];

/// Папка `assets`: из `VX_ASSETS`, рядом с программой, в текущей папке или в корне проекта.
fn find_assets() -> PathBuf {
    if let Ok(p) = std::env::var("VX_ASSETS") {
        return p.into();
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("assets")));
    exe_dir
        .into_iter()
        .chain(["assets", "../assets", "../../assets"].map(PathBuf::from))
        .find(|p| p.join("blocks.toml").exists())
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")))
}

/// Клавиши интерфейса. Физические коды — работают при любой раскладке.
fn ui_key(code: KeyCode) -> Option<UiKey> {
    Some(match code {
        KeyCode::Escape => UiKey::Escape,
        KeyCode::KeyE => UiKey::Inventory,
        KeyCode::F3 => UiKey::Debug,
        KeyCode::Enter => UiKey::Enter,
        _ => return None,
    })
}

struct Running {
    window: Arc<Window>,
    renderer: Renderer,
}

struct App {
    game: Game,
    gui: Gui,
    font: FontAtlas,
    assets: PathBuf,
    /// Ввод для игры (только когда открыт HUD).
    input: Input,
    /// Ввод для интерфейса.
    ui_input: UiInput,
    cursor: Vec2,
    ui_scale: f32,
    running: Option<Running>,
    last: Instant,
    started: Instant,
    grabbed: bool,
    want_grab: bool,
    /// Режим «снять кадр»: путь и сколько кадров осталось.
    screenshot: Option<(PathBuf, u32)>,
    fps: (u32, Instant, f32),
    error: Option<anyhow::Error>,
}

impl App {
    fn grab(&mut self, grab: bool) {
        self.want_grab = grab;
        let Some(r) = &self.running else { return };
        // Locked есть не везде (Windows, X11) — тогда Confined; на macOS наоборот.
        let ok = !grab
            || r.window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
            || r.window.set_cursor_grab(CursorGrabMode::Confined).is_ok();
        if !grab {
            let _ = r.window.set_cursor_grab(CursorGrabMode::None);
        }
        r.window.set_cursor_visible(!grab);
        self.grabbed = grab && ok;
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let now = Instant::now();
        // В режиме скриншота время фиксированное — результат воспроизводим.
        let dt = if self.screenshot.is_some() { 1.0 / 60.0 } else { (now - self.last).as_secs_f32() };
        self.last = now;

        // --- игра ---
        let playing = self.gui.screen() == Screen::Playing;
        if !self.gui.pauses_game() {
            let idle = Input::default();
            self.game.update(if playing { &self.input } else { &idle }, dt);
        }
        let take_shot = self.input.pressed(KeyCode::F2);
        self.input.end_frame();

        let Some(r) = &mut self.running else { return Ok(()) };
        let sync = self.game.sync();
        for id in sync.removed {
            r.renderer.remove_grid(id);
        }
        for (id, cpos, mesh) in &sync.meshes {
            r.renderer.set_chunk(*id, *cpos, mesh);
        }

        // --- интерфейс ---
        self.fps.0 += 1;
        if self.fps.1.elapsed() > Duration::from_millis(500) {
            self.fps.2 = self.fps.0 as f32 / self.fps.1.elapsed().as_secs_f32();
            self.fps = (0, Instant::now(), self.fps.2);
        }
        let hud = self.game.hud(self.fps.2);
        // Масштаб меняем только когда кнопка отпущена — иначе ползунок «убегает» из-под мыши.
        if !self.ui_input.down[0] {
            self.ui_scale = r.window.scale_factor() as f32 * self.game.settings.ui_scale;
        }
        let (w, h) = r.renderer.size();
        self.ui_input.screen = Vec2::new(w as f32, h as f32) / self.ui_scale;
        self.ui_input.mouse = self.cursor / self.ui_scale;
        let ctx = UiContext {
            reg: &self.game.reg,
            inventory: &mut self.game.inventory,
            settings: &mut self.game.settings,
            palette: &self.game.palette,
            hud: &hud,
        };
        let (draw, actions) = self.gui.frame(&self.ui_input, &self.font, ctx);
        self.ui_input.keys.clear();
        self.ui_input.pressed = [false; 2];
        self.ui_input.scroll = 0.0;
        if actions.contains(&Action::Quit) {
            event_loop.exit();
        }
        let ui_mesh = vx_ui::tessellate(&draw, &self.font, self.ui_scale);

        // --- кадр ---
        let aspect = r.renderer.aspect();
        let grids = self.game.grid_poses();
        let in_game = self.gui.in_game();
        let lines = if in_game { self.game.lines() } else { vec![] };
        let (view_proj, camera) = if in_game {
            (self.game.view_proj(aspect), self.game.eye())
        } else {
            self.game.menu_view_proj(aspect, self.started.elapsed().as_secs_f32())
        };
        let frame = Frame {
            view_proj,
            camera,
            grids: &grids,
            lines: &lines,
            sky: SKY,
            fog_distance: self.game.view_distance(),
            ui: Some(&ui_mesh),
        };

        if let Some((path, left)) = &mut self.screenshot {
            *left = left.saturating_sub(1);
            if *left == 0 {
                r.renderer.screenshot(&frame)?.save(&*path)?;
                log::info!("скриншот: {}", path.display());
                event_loop.exit();
            }
            return Ok(());
        }
        if take_shot {
            let path = format!("screenshot-{}.png", std::time::SystemTime::UNIX_EPOCH.elapsed()?.as_secs());
            r.renderer.screenshot(&frame)?.save(&path)?;
            log::info!("скриншот: {path}");
        }
        r.renderer.render(&frame);

        // Курсор: захвачен в игре, свободен в меню.
        let want = !self.gui.wants_cursor();
        if want != self.want_grab {
            self.grab(want);
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Voxel").with_inner_size(PhysicalSize::new(1280, 720));
        let result = event_loop.create_window(attrs).map_err(anyhow::Error::from).and_then(|window| {
            let window = Arc::new(window);
            let mut renderer = pollster::block_on(Renderer::new(
                window.clone(),
                event_loop.owned_display_handle(),
                self.game.reg.textures(),
                &self.assets.join("textures"),
            ))?;
            renderer.set_ui_atlas(self.font.width, self.font.height, &self.font.pixels);
            Ok(Running { window, renderer })
        });
        match result {
            Ok(r) => self.running = Some(r),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.running {
                    r.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => {
                self.input.clear();
                if self.screenshot.is_none() {
                    self.gui.focus_lost();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let down = event.state == ElementState::Pressed;
                    self.input.key(code, down);
                    if down
                        && !event.repeat
                        && let Some(k) = ui_key(code)
                    {
                        self.ui_input.keys.push(k);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Vec2::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                let ui_button = match button {
                    MouseButton::Left => Some(0),
                    MouseButton::Right => Some(1),
                    _ => None,
                };
                // Отпускание всегда доходит до интерфейса, иначе кнопка «залипнет»
                // (например, нажали «Играть» в меню, а отпустили уже в игре).
                if let Some(i) = ui_button
                    && (!down || self.gui.wants_cursor())
                {
                    self.ui_input.down[i] = down;
                }
                if self.gui.wants_cursor() {
                    if let Some(i) = ui_button {
                        self.ui_input.pressed[i] |= down;
                    }
                } else if !self.grabbed {
                    // Захват мог не получиться (например, окно было не в фокусе) — пробуем по клику.
                    self.grab(true);
                } else {
                    self.input.button(button, down);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if self.gui.wants_cursor() {
                    self.ui_input.scroll += lines;
                } else {
                    self.input.scroll += lines;
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame(event_loop) {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (DeviceEvent::MouseMotion { delta }, true) = (event, self.grabbed) {
            self.input.mouse_delta += Vec2::new(delta.0 as f32, delta.1 as f32);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(r) = &self.running {
            r.window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"))
        .init();

    let mut args = std::env::args().skip(1);
    let (mut screenshot, mut frames, mut screen) = (None, 90, Screen::MainMenu);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--frames" => frames = args.next().and_then(|n| n.parse().ok()).unwrap_or(frames),
            "--screen" => {
                screen = match args.next().as_deref() {
                    Some("menu") => Screen::MainMenu,
                    Some("game") => Screen::Playing,
                    Some("inventory") => Screen::Inventory,
                    Some("pause") => Screen::Paused,
                    Some("settings") => Screen::Settings,
                    other => anyhow::bail!("неизвестный экран {other:?}"),
                }
            }
            other => anyhow::bail!("неизвестный аргумент `{other}`"),
        }
    }

    let assets = find_assets();
    log::info!("ресурсы: {}", assets.display());
    let font_path = assets.join("fonts/ui.ttf");
    let font_data = std::fs::read(&font_path).with_context(|| format!("{}", font_path.display()))?;
    let font = FontAtlas::new(&font_data, 32.0).map_err(anyhow::Error::msg)?;

    let mut gui = Gui::default();
    gui.nav.go(screen);
    let mut app = App {
        game: Game::load(&assets)?,
        gui,
        font,
        assets,
        input: Input::default(),
        ui_input: UiInput::default(),
        cursor: Vec2::ZERO,
        ui_scale: 1.0,
        running: None,
        last: Instant::now(),
        started: Instant::now(),
        grabbed: false,
        want_grab: false,
        screenshot: screenshot.map(|p| (p, frames)),
        fps: (0, Instant::now(), 0.0),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}
