//! Ядро движка: данные без графики и физики.
//!
//! * [`Registry`] — описания блоков (загружаются из `assets/blocks.toml`);
//! * [`Chunk`] — куб 16×16×16 блоков;
//! * [`Grid`] — разреженная сетка чанков со своей позой в мире (статичный мир или корабль);
//! * [`Level`] — набор сеток. Сетка [`WORLD`] — основной мир.
//! * [`assembly`] — сборка/разборка конструкций (мир ⇄ отдельная сетка-тело).

pub mod assembly;
mod block;
mod chunk;
mod grid;

pub use block::{AIR, BlockDef, BlockId, Face, Registry};
pub use chunk::{CHUNK, CHUNK_VOLUME, Chunk, split};
pub use glam;
pub use grid::{Bounds, Grid, GridId, GridKind, Level, Pose, WORLD};
