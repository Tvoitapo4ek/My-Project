//! Игровая логика: игрок, инструменты, синхронизация мира с физикой и рендером.

use std::{
    collections::HashSet,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use glam::{IVec3, Mat4, Vec3};
use serde::Deserialize;
use vx_core::{AIR, BlockId, CHUNK, GridId, GridKind, Level, Registry, WORLD, assembly};
use vx_gen::WorldGen;
use vx_mesh::{Mesh, mesh_chunk};
use vx_physics::{Character, Hit, Physics, PhysicsConfig};
use vx_render::LineVertex;
use winit::{event::MouseButton, keyboard::KeyCode};

use crate::input::Input;

const TICK: f32 = 1.0 / 60.0;
const REACH: f32 = 8.0;
const MAX_ASSEMBLY: usize = 8192;
const EYE: f32 = 0.72; // глаза над центром капсулы

/// Раздел `[player]` в `world.toml`.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct PlayerConfig {
    spawn: Option<[f32; 3]>,
    yaw: f32,
    pitch: f32,
}

/// Изменения, которые надо отправить в рендер.
#[derive(Default)]
pub struct Sync {
    pub meshes: Vec<(GridId, IVec3, Mesh)>,
    pub removed: Vec<GridId>,
}

pub struct Game {
    pub reg: Registry,
    pub level: Level,
    pub phys: Physics,
    player: Character,
    spawn: Vec3,
    yaw: f32,
    pitch: f32,
    fly: bool,
    hotbar: Vec<BlockId>,
    selected: usize,
    target: Option<Hit>,
    accumulator: f32,
    known: HashSet<GridId>,
    message: Option<(String, Instant)>,
}

fn section<T: for<'de> Deserialize<'de> + Default>(extra: &toml::Table, name: &str) -> Result<T> {
    match extra.get(name) {
        Some(v) => v.clone().try_into().with_context(|| format!("раздел [{name}]")),
        None => Ok(T::default()),
    }
}

impl Game {
    pub fn load(assets: &Path) -> Result<Self> {
        let reg = Registry::load(&assets.join("blocks.toml"))?;
        let wg = WorldGen::load(&assets.join("world.toml"), &assets.join("structures"), &reg)?;
        let level = wg.generate()?;
        let physics: PhysicsConfig = section(&wg.config.extra, "physics")?;
        let player: PlayerConfig = section(&wg.config.extra, "player")?;

        let bounds = wg.bounds();
        let spawn = match player.spawn {
            Some(s) => Vec3::from(s),
            None => {
                let c = (bounds.min + bounds.max) / 2;
                let top = level.world().top(c.x, c.z, 0..bounds.max.y).unwrap_or(0);
                Vec3::new(c.x as f32 + 0.5, top as f32 + 2.5, c.z as f32 + 0.5)
            }
        };
        let mut phys = Physics::new(physics);
        let player_body = Character::spawn(&mut phys, spawn, 0.3, 0.6, 1.5);
        let hotbar = reg.iter().filter(|(_, b)| !b.props.contains_key("unbreakable")).map(|(id, _)| id).collect();
        log::info!("мир {}×{}×{} блоков, сеток: {}", bounds.max.x, bounds.max.y, bounds.max.z, level.ids().len());

        Ok(Game {
            reg,
            level,
            phys,
            player: player_body,
            spawn,
            yaw: player.yaw.to_radians(),
            pitch: player.pitch.to_radians(),
            fly: false,
            hotbar,
            selected: 0,
            target: None,
            accumulator: 0.0,
            known: HashSet::new(),
            message: None,
        })
    }

    fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        log::info!("{text}");
        self.message = Some((text, Instant::now()));
    }

    pub fn look_dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, sp, -cy * cp)
    }

    pub fn eye(&self) -> Vec3 {
        self.player.position(&self.phys) + Vec3::Y * EYE
    }

    /// Один кадр игры.
    pub fn update(&mut self, input: &Input, dt: f32) {
        // --- камера ---
        let sensitivity = 0.0025;
        self.yaw -= input.mouse_delta.x * sensitivity;
        self.pitch = (self.pitch - input.mouse_delta.y * sensitivity).clamp(-1.55, 1.55);

        // --- движение и физика (фиксированный шаг) ---
        if input.pressed(KeyCode::KeyV) {
            self.fly = !self.fly;
            self.say(if self.fly { "полёт: вкл" } else { "полёт: выкл" });
        }
        let (sy, cy) = self.yaw.sin_cos();
        let forward = Vec3::new(-sy, 0.0, -cy);
        let right = forward.cross(Vec3::Y);
        let axis = |pos, neg| input.held(pos) as i32 as f32 - input.held(neg) as i32 as f32;
        let mut wish = forward * axis(KeyCode::KeyW, KeyCode::KeyS) + right * axis(KeyCode::KeyD, KeyCode::KeyA);
        wish = wish.normalize_or_zero();
        let speed = match (self.fly, input.held(KeyCode::ControlLeft)) {
            (true, _) => 14.0,
            (false, true) => 7.5,
            (false, false) => 4.6,
        };
        let jump = input.held(KeyCode::Space).then(|| (2.0 * self.phys.config.gravity * 1.25).sqrt());
        let fly = self.fly.then(|| axis(KeyCode::Space, KeyCode::ShiftLeft) * speed);

        self.accumulator = (self.accumulator + dt).min(TICK * 5.0);
        while self.accumulator >= TICK {
            self.accumulator -= TICK;
            self.player.drive(&mut self.phys, wish * speed, jump, fly);
            self.phys.step(&mut self.level, TICK);
        }
        self.cleanup();

        // --- инструменты ---
        let eye = self.eye();
        let dir = self.look_dir();
        self.target = self.phys.raycast(eye, dir, REACH, Some(self.player.body));

        if !self.hotbar.is_empty() {
            for (i, key) in DIGITS.iter().enumerate() {
                if input.pressed(*key) && i < self.hotbar.len() {
                    self.selected = i;
                }
            }
            if input.scroll != 0.0 {
                let n = self.hotbar.len() as i32;
                self.selected = (self.selected as i32 - input.scroll.signum() as i32).rem_euclid(n) as usize;
            }
        }

        let Some(hit) = self.target else { return };
        if input.clicked(MouseButton::Left) {
            self.break_block(hit);
        }
        if input.clicked(MouseButton::Right) {
            self.place_block(hit);
        }
        if input.pressed(KeyCode::KeyG) {
            self.toggle_assembly(hit);
        }
        if input.button_held(MouseButton::Middle) || input.held(KeyCode::KeyR) {
            // «Толкатель»: ускорение 15 м/с² в точке взгляда — видно вращение от рычага.
            let impulse = dir * self.phys.mass(hit.grid) * 15.0 * dt;
            self.phys.apply_impulse(hit.grid, hit.point, impulse);
        }
    }

    fn break_block(&mut self, hit: Hit) {
        let Some(grid) = self.level.get_mut(hit.grid) else { return };
        if self.reg.get(grid.get(hit.block)).props.contains_key("unbreakable") {
            return;
        }
        grid.set(hit.block, AIR);
    }

    fn place_block(&mut self, hit: Hit) {
        let Some(&block) = self.hotbar.get(self.selected) else { return };
        let pos = hit.block + hit.normal;
        let player = self.player.position(&self.phys);
        let Some(grid) = self.level.get_mut(hit.grid) else { return };
        // Не ставить блок внутрь игрока.
        let d = (grid.pose.to_local(player) - (pos.as_vec3() + 0.5)).abs();
        if self.reg.solid(block) && d.x < 0.8 && d.z < 0.8 && d.y < 1.4 {
            return;
        }
        if grid.get(pos) == AIR && !grid.set(pos, block) {
            self.say("за границей мира");
        }
    }

    /// G: собрать постройку в корабль или «приземлить» корабль обратно в мир.
    fn toggle_assembly(&mut self, hit: Hit) {
        let result = if hit.grid == WORLD {
            assembly::assemble(&mut self.level, WORLD, hit.block, &self.reg, MAX_ASSEMBLY).map(|id| {
                let n = self.level.get(id).map_or(0, |g| g.blocks().count());
                format!("собрано: {n} блоков")
            })
        } else {
            assembly::disassemble(&mut self.level, hit.grid).map(|_| "конструкция встала в мир".to_string())
        };
        match result {
            Ok(text) => self.say(text),
            Err(e) => self.say(format!("{e}")),
        }
    }

    /// Убрать пустые и упавшие за мир корабли, вернуть игрока на спавн.
    fn cleanup(&mut self) {
        let dead: Vec<_> = self
            .level
            .iter()
            .filter(|(id, g)| *id != WORLD && (g.is_empty() || g.pose.pos.y < -64.0))
            .map(|(id, _)| id)
            .collect();
        for id in dead {
            self.level.remove(id);
        }
        if self.player.position(&self.phys).y < -32.0 {
            self.player.teleport(&mut self.phys, self.spawn);
        }
    }

    /// Перестроить меши и коллайдеры изменённых чанков.
    pub fn sync(&mut self) -> Sync {
        let mut out = Sync::default();
        let ids: HashSet<_> = self.level.ids().into_iter().collect();
        for &gone in self.known.difference(&ids) {
            self.phys.remove_grid(gone);
            out.removed.push(gone);
        }
        for (id, grid) in self.level.iter_mut() {
            let dirty = grid.take_dirty();
            if dirty.is_empty() {
                continue;
            }
            self.phys.sync_grid(id, grid, &dirty, &self.reg);
            for cpos in dirty {
                out.meshes.push((id, cpos, mesh_chunk(grid, cpos, &self.reg)));
            }
        }
        self.known = ids;
        out
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_to_mat4};
        perspective(70f32.to_radians(), aspect, 0.05, 1000.0) * look_to_mat4(self.eye(), self.look_dir(), Vec3::Y)
    }

    pub fn grid_poses(&self) -> Vec<(GridId, Mat4)> {
        self.level.iter().map(|(id, g)| (id, g.pose.matrix())).collect()
    }

    /// Рамка вокруг блока под прицелом + прицел.
    pub fn lines(&self, aspect: f32) -> Vec<LineVertex> {
        let mut out = vec![];
        let world = |p: Vec3| LineVertex { pos: p.extend(1.0).to_array(), color: [0.05, 0.05, 0.05, 0.9] };
        if let Some(hit) = self.target
            && let Some(grid) = self.level.get(hit.grid)
        {
            let corner = |i: usize| {
                let c = Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32);
                grid.pose.to_world(hit.block.as_vec3() + c * 1.004 - 0.002)
            };
            for (a, b) in BOX_EDGES {
                out.extend([world(corner(a)), world(corner(b))]);
            }
        }
        let screen = |x: f32, y: f32| LineVertex { pos: [x / aspect, y, 0.0, 0.0], color: [1.0, 1.0, 1.0, 0.9] };
        out.extend([screen(-0.03, 0.0), screen(0.03, 0.0), screen(0.0, -0.03), screen(0.0, 0.03)]);
        out
    }

    pub fn status(&self) -> String {
        let block = self.hotbar.get(self.selected).map_or("—", |&b| self.reg.get(b).name.as_str());
        let mut s = format!("блок: {block} [{}/{}]", self.selected + 1, self.hotbar.len());
        if self.fly {
            s += " | полёт";
        }
        if let Some(hit) = self.target
            && hit.grid != WORLD
            && let Some(g) = self.level.get(hit.grid)
        {
            let kind = if g.kind == GridKind::Dynamic { "корабль" } else { "сетка" };
            s += &format!(" | {kind} #{}: масса {:.1}", hit.grid.0, self.phys.mass(hit.grid));
        }
        if let Some((text, at)) = &self.message
            && at.elapsed() < Duration::from_secs(4)
        {
            s += &format!(" | {text}");
        }
        s
    }

    /// Дальность тумана — по размеру мира.
    pub fn view_distance(&self) -> f32 {
        let b = self.level.world().bounds.map_or(IVec3::splat(8 * CHUNK), |b| b.max - b.min);
        b.max_element() as f32 * 1.2
    }
}

const DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

const BOX_EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        let mut g = Game::load(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))).unwrap();
        g.sync();
        g
    }

    fn run(g: &mut Game, secs: f32, mut each: impl FnMut(&mut Game)) {
        for _ in 0..(secs * 60.0) as usize {
            each(g);
            g.update(&Input::default(), TICK);
            g.sync();
        }
    }

    #[test]
    fn raft_lifts_off_after_assembly() {
        let mut g = game();
        (g.yaw, g.pitch) = (0.0, 0.0); // смотрим на стойку плота
        g.player.teleport(&mut g.phys, Vec3::new(64.5, 11.0, 68.5));
        run(&mut g, 0.5, |_| {});
        let mut input = Input::default();
        input.key(KeyCode::KeyG, true);
        g.update(&input, TICK);
        g.sync();
        let ship = *g.level.ids().last().unwrap();
        assert_eq!(g.level.get(ship).unwrap().blocks().count(), 29, "{}", g.status());
        let start = g.level.get(ship).unwrap().pose.pos.y;
        run(&mut g, 5.0, |_| {});
        let y = g.level.get(ship).unwrap().pose.pos.y;
        assert!(y > start + 3.0, "плот должен взлететь: {start} → {y}");
    }

    #[test]
    fn player_rides_moving_ship() {
        let mut g = game();
        let ship = GridId(1);
        let pose = g.level.get(ship).unwrap().pose;
        // На палубу: центр капсулы на 0.9 выше верха палубы.
        let deck = pose.to_world(Vec3::new(2.5, 1.0 + 0.95, 4.5));
        g.player.teleport(&mut g.phys, deck);
        let body = g.phys.body_of(ship).unwrap();
        run(&mut g, 4.0, |g| g.phys.world.bodies[body].set_linvel(Vec3::new(3.0, 0.0, 0.0), true));
        let ship_pose = g.level.get(ship).unwrap().pose;
        let local = ship_pose.to_local(g.player.position(&g.phys));
        assert!(ship_pose.pos.x > pose.pos.x + 10.0, "корабль должен лететь");
        assert!((local.x - 2.5).abs() < 0.5 && (local.z - 4.5).abs() < 0.5, "игрок соскользнул: {local}");
        assert!(g.player.on_ground, "игрок должен стоять на палубе");
    }
}
