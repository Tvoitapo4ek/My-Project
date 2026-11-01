//! Физика сеток-тел на rapier3d — по мотивам Sable / Create Aeronautics.
//!
//! Каждая [`Grid`] получает своё твёрдое тело: мир — неподвижное,
//! корабли — динамические. Коллайдер — воксельная форма rapier на каждый
//! чанк, поэтому правка блока перестраивает только один чанк.
//! Масса считается из `density` блоков, подъёмная сила — из `lift`.

mod character;

use std::collections::HashMap;

use glam::{IVec3, Vec3};
use rapier3d::prelude::*;
use serde::Deserialize;
use vx_core::{CHUNK, Grid, GridId, GridKind, Level, Pose as GridPose, Registry};

pub use character::Character;
pub use rapier3d;

/// Настройки из раздела `[physics]` файла `world.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsConfig {
    pub gravity: f32,
    /// Высота, на которой воздух «кончается» и подъёмная сила падает до нуля.
    /// Благодаря этому шары сами находят высоту равновесия.
    pub air_ceiling: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        PhysicsConfig { gravity: 20.0, air_ceiling: 96.0, linear_damping: 0.3, angular_damping: 0.8 }
    }
}

/// Результат луча: какой блок какой сетки задет.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub grid: GridId,
    /// Задетый блок (локальные координаты сетки).
    pub block: IVec3,
    /// Нормаль грани в координатах сетки: `block + normal` — куда ставить блок.
    pub normal: IVec3,
    /// Точка попадания в мире.
    pub point: Vec3,
    pub distance: f32,
}

struct ChunkBody {
    collider: ColliderHandle,
    /// Сумма подъёмной силы и её момент (сила × локальная точка).
    lift: f32,
    lift_moment: Vec3,
}

struct GridBody {
    body: RigidBodyHandle,
    chunks: HashMap<IVec3, ChunkBody>,
}

pub struct Physics {
    pub world: PhysicsWorld,
    pub config: PhysicsConfig,
    grids: HashMap<GridId, GridBody>,
}

impl Physics {
    pub fn new(config: PhysicsConfig) -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vec3::new(0.0, -config.gravity, 0.0);
        Physics { world, config, grids: HashMap::new() }
    }

    /// Создать тело для сетки (если его нет) и перестроить указанные чанки.
    pub fn sync_grid(&mut self, id: GridId, grid: &Grid, chunks: &[IVec3], reg: &Registry) {
        let gb = self.grids.entry(id).or_insert_with(|| {
            let builder = match grid.kind {
                GridKind::Static => RigidBodyBuilder::fixed(),
                GridKind::Dynamic => RigidBodyBuilder::dynamic()
                    .linear_damping(self.config.linear_damping)
                    .angular_damping(self.config.angular_damping)
                    .ccd_enabled(true),
            };
            let pose = Pose::from_parts(grid.pose.pos, grid.pose.rot);
            GridBody { body: self.world.insert_body(builder.pose(pose)), chunks: HashMap::new() }
        });
        for &cpos in chunks {
            if let Some(old) = gb.chunks.remove(&cpos) {
                self.world.remove_collider(old.collider);
            }
            let Some(chunk) = grid.chunk(cpos) else { continue };
            let (mut voxels, mut mass, mut lift, mut lift_moment) = (vec![], 0.0, 0.0, Vec3::ZERO);
            for (local, block) in chunk.iter() {
                let def = reg.get(block);
                if def.lift != 0.0 {
                    lift += def.lift;
                    lift_moment += def.lift * ((cpos * CHUNK + local).as_vec3() + 0.5);
                }
                if def.solid {
                    voxels.push(local);
                    mass += def.density;
                }
            }
            if voxels.is_empty() {
                continue;
            }
            // Плотность усредняется по чанку — для формы корабля этого достаточно.
            let collider = ColliderBuilder::voxels(Vec3::ONE, &voxels)
                .translation((cpos * CHUNK).as_vec3())
                .density(mass / voxels.len() as f32)
                .user_data(id.0 as u128 + 1);
            let collider = self.world.insert_collider(collider, Some(gb.body));
            gb.chunks.insert(cpos, ChunkBody { collider, lift, lift_moment });
        }
        self.world.wake_up(gb.body, true);
    }

    /// Пересоздать тело сетки полностью (например, после сборки корабля).
    pub fn add_grid(&mut self, id: GridId, grid: &Grid, reg: &Registry) {
        self.remove_grid(id);
        let chunks: Vec<_> = grid.chunks().map(|(c, _)| c).collect();
        self.sync_grid(id, grid, &chunks, reg);
    }

    pub fn remove_grid(&mut self, id: GridId) {
        if let Some(gb) = self.grids.remove(&id) {
            self.world.remove_body_with_colliders(gb.body, true);
        }
    }

    /// Шаг симуляции: подъёмная сила → rapier → позы сеток.
    pub fn step(&mut self, level: &mut Level, dt: f32) {
        self.world.integration_parameters.dt = dt;
        for gb in self.grids.values() {
            let body = &mut self.world.bodies[gb.body];
            if !body.is_dynamic() {
                continue;
            }
            // В rapier силы и моменты копятся, пока их не сбросить — сбрасываем оба.
            body.reset_forces(false);
            body.reset_torques(false);
            let lift: f32 = gb.chunks.values().map(|c| c.lift).sum();
            if lift > 0.0 {
                // Все силы вертикальны → их сумма приложена в «центре подъёма».
                let moment: Vec3 = gb.chunks.values().map(|c| c.lift_moment).sum();
                let point = body.position().transform_point(moment / lift);
                let air = (1.0 - point.y / self.config.air_ceiling).clamp(0.0, 1.0);
                body.add_force_at_point(Vec3::Y * lift * air, point, true);
            }
        }
        self.world.step();
        for (id, gb) in &self.grids {
            let body = &self.world.bodies[gb.body];
            if let (true, Some(grid)) = (body.is_dynamic(), level.get_mut(*id)) {
                grid.pose = GridPose { pos: body.translation(), rot: *body.rotation() };
            }
        }
    }

    /// Луч по всем сеткам (мир и корабли). `exclude` — например, тело игрока.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32, exclude: Option<RigidBodyHandle>) -> Option<Hit> {
        let mut filter = QueryFilter::default().predicate(&|_, c: &Collider| c.user_data != 0);
        if let Some(body) = exclude {
            filter = filter.exclude_rigid_body(body);
        }
        let dir = dir.normalize();
        let (handle, hit) = self.world.cast_ray_and_get_normal(&Ray::new(origin, dir), max, true, filter)?;
        let collider = &self.world.colliders[handle];
        let pose = self.world.bodies[collider.parent()?].position();
        let point = origin + dir * hit.time_of_impact;
        let normal = pose.rotation.inverse() * hit.normal;
        if normal.length_squared() < 0.5 {
            return None; // луч начался внутри блока
        }
        let local = pose.inverse_transform_point(point);
        Some(Hit {
            grid: GridId((collider.user_data - 1) as u32),
            block: (local - normal * 0.5).floor().as_ivec3(),
            normal: normal.round().as_ivec3(),
            point,
            distance: hit.time_of_impact,
        })
    }

    /// Толкнуть сетку импульсом в мировой точке.
    pub fn apply_impulse(&mut self, id: GridId, point: Vec3, impulse: Vec3) {
        if let Some(gb) = self.grids.get(&id) {
            self.world.bodies[gb.body].apply_impulse_at_point(impulse, point, true);
        }
    }

    pub fn body_of(&self, id: GridId) -> Option<RigidBodyHandle> {
        self.grids.get(&id).map(|g| g.body)
    }

    /// Масса сетки (0 для статичной).
    pub fn mass(&self, id: GridId) -> f32 {
        self.body_of(id).map_or(0.0, |b| self.world.bodies[b].mass())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Registry, Level, Physics) {
        let reg = Registry::from_toml(
            "[[block]]\nname='stone'\nterrain=true\n[[block]]\nname='plank'\n[[block]]\nname='balloon'\nlift=60\ndensity=0.1",
        )
        .unwrap();
        let mut world = Grid::new(GridKind::Static);
        for x in -8..8 {
            for z in -8..8 {
                world.set(IVec3::new(x, 0, z), 1);
            }
        }
        (reg, Level::new(world), Physics::new(PhysicsConfig::default()))
    }

    fn ship(level: &mut Level, phys: &mut Physics, reg: &Registry, y: f32, balloons: bool) -> GridId {
        let mut g = Grid::new(GridKind::Dynamic);
        g.pose = GridPose::at(Vec3::new(0.0, y, 0.0));
        for x in 0..3 {
            for z in 0..3 {
                g.set(IVec3::new(x, 0, z), 2);
                if balloons {
                    g.set(IVec3::new(x, 2, z), 3);
                }
            }
        }
        let id = level.add(g);
        phys.add_grid(id, level.get(id).unwrap(), reg);
        id
    }

    fn run(level: &mut Level, phys: &mut Physics, secs: f32) {
        let w = level.world();
        phys.add_grid(vx_core::WORLD, w, &Registry::from_toml("[[block]]\nname='stone'").unwrap());
        for _ in 0..(secs * 60.0) as usize {
            phys.step(level, 1.0 / 60.0);
        }
    }

    #[test]
    fn ship_falls_and_lands() {
        let (reg, mut level, mut phys) = setup();
        let id = ship(&mut level, &mut phys, &reg, 10.0, false);
        run(&mut level, &mut phys, 4.0);
        let y = level.get(id).unwrap().pose.pos.y;
        assert!((y - 1.0).abs() < 0.1, "корабль должен лежать на земле, y = {y}");
        let hit = phys.raycast(Vec3::new(1.5, 10.0, 1.5), Vec3::NEG_Y, 20.0, None).unwrap();
        assert_eq!((hit.grid, hit.block, hit.normal), (id, IVec3::new(1, 0, 1), IVec3::Y));
    }

    #[test]
    fn tilted_balloon_ship_rights_itself() {
        let (reg, mut level, mut phys) = setup();
        let id = ship(&mut level, &mut phys, &reg, 20.0, true);
        level.get_mut(id).unwrap().pose.rot = glam::Quat::from_rotation_z(0.3);
        phys.add_grid(id, level.get(id).unwrap(), &reg);
        run(&mut level, &mut phys, 8.0);
        let tilt = level.get(id).unwrap().pose.rot.angle_between(glam::Quat::IDENTITY);
        assert!(tilt < 0.1, "шары над центром масс должны выровнять корабль, наклон {tilt}");
    }

    #[test]
    fn balloons_lift_ship() {
        let (reg, mut level, mut phys) = setup();
        let id = ship(&mut level, &mut phys, &reg, 3.0, true);
        run(&mut level, &mut phys, 6.0);
        let y = level.get(id).unwrap().pose.pos.y;
        assert!(y > 10.0, "шары должны поднять корабль, y = {y}");
        assert!(y < phys.config.air_ceiling, "и остановиться ниже потолка");
    }
}
