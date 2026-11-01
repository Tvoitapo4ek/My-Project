//! Точка входа: окно winit + игровой цикл.
//!
//! `cargo run --release` — играть.
//! `cargo run --release -- --screenshot shot.png [--frames 120]` — снять кадр и выйти.

mod game;
mod input;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use game::Game;
use input::Input;
use vx_render::{Frame, Renderer};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

const SKY: [f32; 3] = [0.62, 0.78, 0.96];

/// Папка `assets`: из `VX_ASSETS`, рядом с текущей папкой или в корне проекта.
fn find_assets() -> PathBuf {
    if let Ok(p) = std::env::var("VX_ASSETS") {
        return p.into();
    }
    ["assets", "../assets", "../../assets"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.join("blocks.toml").exists())
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")))
}

struct Running {
    window: Arc<Window>,
    renderer: Renderer,
}

struct App {
    game: Game,
    assets: PathBuf,
    input: Input,
    running: Option<Running>,
    last: Instant,
    grabbed: bool,
    /// Режим «снять кадр»: путь и сколько кадров осталось.
    screenshot: Option<(PathBuf, u32)>,
    fps: (u32, Instant),
    error: Option<anyhow::Error>,
}

impl App {
    fn grab(&mut self, grab: bool) {
        let Some(r) = &self.running else { return };
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

        if self.input.pressed(KeyCode::Escape) {
            self.grab(false);
        }
        self.game.update(&self.input, dt);
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

        let aspect = r.renderer.aspect();
        let grids = self.game.grid_poses();
        let lines = self.game.lines(aspect);
        let frame = Frame {
            view_proj: self.game.view_proj(aspect),
            camera: self.game.eye(),
            grids: &grids,
            lines: &lines,
            sky: SKY,
            fog_distance: self.game.view_distance(),
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

        self.fps.0 += 1;
        if self.fps.1.elapsed() > Duration::from_millis(500) {
            let fps = self.fps.0 as f32 / self.fps.1.elapsed().as_secs_f32();
            r.window.set_title(&format!("Voxel | {:.0} FPS | {}", fps, self.game.status()));
            self.fps = (0, Instant::now());
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
            let renderer = pollster::block_on(Renderer::new(
                window.clone(),
                event_loop.owned_display_handle(),
                self.game.reg.textures(),
                &self.assets.join("textures"),
            ))?;
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
                self.grab(false);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.input.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !self.grabbed {
                    // Первый клик только захватывает мышь.
                    if state == ElementState::Pressed {
                        self.grab(true);
                    }
                    return;
                }
                self.input.button(button, state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.input.scroll += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
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
            self.input.mouse_delta += glam::Vec2::new(delta.0 as f32, delta.1 as f32);
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
    let mut screenshot = None;
    let mut frames = 90;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--frames" => frames = args.next().and_then(|n| n.parse().ok()).unwrap_or(frames),
            other => anyhow::bail!("неизвестный аргумент `{other}`"),
        }
    }

    let assets = find_assets();
    log::info!("ресурсы: {}", assets.display());
    let mut app = App {
        game: Game::load(&assets)?,
        assets,
        input: Input::default(),
        running: None,
        last: Instant::now(),
        grabbed: false,
        screenshot: screenshot.map(|p| (p, frames)),
        fps: (0, Instant::now()),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}
