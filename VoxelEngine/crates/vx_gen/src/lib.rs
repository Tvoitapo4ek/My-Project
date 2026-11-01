//! Генерация мира.
//!
//! Конвейер: **рельеф** (по чанкам, [`Terrain`]) → **структуры** ([`Structure`]).
//! Всё настраивается в `assets/world.toml`, структуры лежат в
//! `assets/structures/*.toml`.
//!
//! Новый тип рельефа: реализуйте [`Terrain`] и добавьте строку в [`terrain_from_config`].

mod structure;
mod terrain;

use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use glam::IVec3;
use serde::Deserialize;
use vx_core::{Bounds, CHUNK, Chunk, Grid, GridKind, Level, Pose, Registry};

pub use structure::Structure;
pub use terrain::{Flat, Terrain};

/// Простой детерминированный ГПСЧ (splitmix64) — без внешних зависимостей.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Случайное число в `range`.
    pub fn range(&mut self, range: std::ops::Range<i32>) -> i32 {
        range.start + (self.next_u64() % (range.end - range.start).max(1) as u64) as i32
    }
}

/// Содержимое `world.toml`.
#[derive(Deserialize)]
pub struct WorldConfig {
    #[serde(default)]
    pub seed: u64,
    /// Размер мира в чанках (x, y, z).
    pub size: [i32; 3],
    /// Таблица рельефа; поле `type` выбирает генератор.
    pub terrain: toml::Table,
    #[serde(default, rename = "structure")]
    pub structures: Vec<Placement>,
    /// Прочие разделы (например, `[physics]`) читают другие модули.
    #[serde(flatten)]
    pub extra: toml::Table,
}

/// Где и как ставить структуру.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Имя файла в `assets/structures` без `.toml`.
    pub name: String,
    /// Точная позиция якоря структуры. Если нет — случайно на поверхности.
    #[serde(default)]
    pub at: Option<[i32; 3]>,
    /// Сколько раз разбросать по поверхности (если `at` не задан).
    #[serde(default = "one")]
    pub count: u32,
    /// `true` — структура становится отдельной динамической сеткой (кораблём).
    #[serde(default)]
    pub dynamic: bool,
}

fn one() -> u32 {
    1
}

/// Выбрать генератор рельефа по полю `type`. Сюда добавляются новые типы.
pub fn terrain_from_config(cfg: &toml::Table, reg: &Registry) -> Result<Box<dyn Terrain>> {
    let kind = cfg.get("type").and_then(|v| v.as_str()).unwrap_or("flat");
    Ok(match kind {
        "flat" => Box::new(Flat::from_config(cfg, reg)?),
        other => anyhow::bail!("неизвестный тип рельефа `{other}`"),
    })
}

/// Генератор мира: рельеф + структуры.
pub struct WorldGen {
    pub config: WorldConfig,
    pub terrain: Box<dyn Terrain>,
    pub structures: HashMap<String, Structure>,
}

impl WorldGen {
    /// Загрузить `world.toml` и все структуры из `structures_dir`.
    pub fn load(world_toml: &Path, structures_dir: &Path, reg: &Registry) -> Result<Self> {
        let text = std::fs::read_to_string(world_toml).with_context(|| format!("{}", world_toml.display()))?;
        let config: WorldConfig = toml::from_str(&text).context("world.toml")?;
        let terrain = terrain_from_config(&config.terrain, reg)?;
        let structures = Structure::load_dir(structures_dir, reg)?;
        Ok(WorldGen { config, terrain, structures })
    }

    /// Границы мира в блоках.
    pub fn bounds(&self) -> Bounds {
        Bounds { min: IVec3::ZERO, max: IVec3::from_array(self.config.size) * CHUNK }
    }

    /// Сгенерировать уровень: мир + динамические структуры.
    pub fn generate(&self) -> Result<Level> {
        let bounds = self.bounds();
        let mut world = Grid::new(GridKind::Static);
        world.bounds = Some(bounds);

        let size = IVec3::from_array(self.config.size);
        for y in 0..size.y {
            for z in 0..size.z {
                for x in 0..size.x {
                    let cpos = IVec3::new(x, y, z);
                    let mut chunk = Chunk::default();
                    self.terrain.generate(cpos, &mut chunk);
                    world.insert_chunk(cpos, chunk);
                }
            }
        }

        let mut level = Level::new(world);
        let mut rng = Rng::new(self.config.seed);
        for p in &self.config.structures {
            let s = self.structures.get(&p.name).with_context(|| format!("нет структуры `{}`", p.name))?;
            let spots: Vec<IVec3> = match p.at {
                Some(at) => vec![IVec3::from_array(at)],
                None => (0..p.count)
                    .filter_map(|_| {
                        let (x, z) = (rng.range(2..bounds.max.x - 2), rng.range(2..bounds.max.z - 2));
                        let y = level.world().top(x, z, 0..bounds.max.y)? + 1;
                        Some(IVec3::new(x, y, z))
                    })
                    .collect(),
            };
            for at in spots {
                if p.dynamic {
                    let mut ship = Grid::new(GridKind::Dynamic);
                    s.place(&mut ship, IVec3::ZERO);
                    ship.pose = Pose::at(at.as_vec3());
                    level.add(ship);
                } else {
                    s.place(level.world_mut(), at);
                }
            }
        }
        Ok(level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_world() {
        let reg = Registry::from_toml("[[block]]\nname='stone'\n[[block]]\nname='grass'").unwrap();
        let cfg: WorldConfig = toml::from_str(
            r#"
            size = [2, 1, 2]
            [terrain]
            type = "flat"
            layers = [{ block = "stone", depth = 3 }, { block = "grass", depth = 1 }]
            "#,
        )
        .unwrap();
        let terrain = terrain_from_config(&cfg.terrain, &reg).unwrap();
        let wg = WorldGen { config: cfg, terrain, structures: HashMap::new() };
        let level = wg.generate().unwrap();
        let w = level.world();
        assert_eq!(w.get(IVec3::new(5, 3, 20)), reg.id("grass").unwrap());
        assert_eq!(w.get(IVec3::new(5, 0, 20)), reg.id("stone").unwrap());
        assert_eq!(w.get(IVec3::new(5, 4, 20)), vx_core::AIR);
        assert_eq!(w.top(31, 31, 0..16), Some(3));
    }
}
