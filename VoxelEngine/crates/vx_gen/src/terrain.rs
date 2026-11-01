use anyhow::{Context, Result};
use glam::IVec3;
use serde::Deserialize;
use vx_core::{BlockId, CHUNK, Chunk, Registry};

/// Генератор рельефа: заполняет один чанк по его координате.
///
/// Работа по чанкам делает генерацию независимой и параллелизуемой —
/// это задел для бесконечного мира с подгрузкой.
pub trait Terrain: Send + Sync {
    fn generate(&self, cpos: IVec3, chunk: &mut Chunk);
}

/// Плоский мир из горизонтальных слоёв (снизу вверх).
pub struct Flat {
    /// Блок для каждой высоты, начиная с y = 0.
    column: Vec<BlockId>,
}

#[derive(Deserialize)]
struct Layer {
    block: String,
    #[serde(default = "one")]
    depth: u32,
}

fn one() -> u32 {
    1
}

impl Flat {
    pub fn new(layers: &[(BlockId, u32)]) -> Self {
        Flat { column: layers.iter().flat_map(|&(b, n)| std::iter::repeat_n(b, n as usize)).collect() }
    }

    pub fn from_config(cfg: &toml::Table, reg: &Registry) -> Result<Self> {
        let layers: Vec<Layer> = cfg.get("layers").context("flat: нужен список `layers`")?.clone().try_into()?;
        let layers = layers.iter().map(|l| Ok((reg.expect(&l.block)?, l.depth))).collect::<Result<Vec<_>>>()?;
        Ok(Flat::new(&layers))
    }
}

impl Terrain for Flat {
    fn generate(&self, cpos: IVec3, chunk: &mut Chunk) {
        for ly in 0..CHUNK {
            let y = cpos.y * CHUNK + ly;
            let Some(&block) = usize::try_from(y).ok().and_then(|y| self.column.get(y)) else { continue };
            for lz in 0..CHUNK {
                for lx in 0..CHUNK {
                    chunk.set(IVec3::new(lx, ly, lz), block);
                }
            }
        }
    }
}
