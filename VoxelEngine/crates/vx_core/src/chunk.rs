use glam::IVec3;

use crate::{AIR, BlockId};

/// Размер чанка по каждой оси.
pub const CHUNK: i32 = 16;
pub const CHUNK_VOLUME: usize = (CHUNK * CHUNK * CHUNK) as usize;

/// Разбить координату блока на (координата чанка, координата внутри чанка).
pub fn split(pos: IVec3) -> (IVec3, IVec3) {
    (pos.div_euclid(IVec3::splat(CHUNK)), pos.rem_euclid(IVec3::splat(CHUNK)))
}

/// Куб 16³ блоков. Хранение плоским массивом; позже можно заменить
/// на палитру — интерфейс (`get`/`set`/`iter`) от этого не изменится.
#[derive(Clone)]
pub struct Chunk {
    blocks: Box<[BlockId; CHUNK_VOLUME]>,
    count: u16,
}

impl Default for Chunk {
    fn default() -> Self {
        Chunk { blocks: Box::new([AIR; CHUNK_VOLUME]), count: 0 }
    }
}

#[inline]
fn index(p: IVec3) -> usize {
    debug_assert!(p.cmpge(IVec3::ZERO).all() && p.cmplt(IVec3::splat(CHUNK)).all());
    (p.x + CHUNK * (p.z + CHUNK * p.y)) as usize
}

impl Chunk {
    /// `local` — координата внутри чанка (0..16).
    #[inline]
    pub fn get(&self, local: IVec3) -> BlockId {
        self.blocks[index(local)]
    }

    pub fn set(&mut self, local: IVec3, id: BlockId) -> BlockId {
        let slot = &mut self.blocks[index(local)];
        let old = std::mem::replace(slot, id);
        match (old == AIR, id == AIR) {
            (true, false) => self.count += 1,
            (false, true) => self.count -= 1,
            _ => {}
        }
        old
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Количество непустых блоков.
    pub fn len(&self) -> usize {
        self.count as usize
    }

    /// Непустые блоки: (локальная координата, id).
    pub fn iter(&self) -> impl Iterator<Item = (IVec3, BlockId)> + '_ {
        self.blocks.iter().enumerate().filter(|(_, b)| **b != AIR).map(|(i, b)| {
            let i = i as i32;
            (IVec3::new(i % CHUNK, i / (CHUNK * CHUNK), (i / CHUNK) % CHUNK), *b)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_negative() {
        assert_eq!(split(IVec3::new(-1, 17, 0)), (IVec3::new(-1, 1, 0), IVec3::new(15, 1, 0)));
    }

    #[test]
    fn set_get_iter() {
        let mut c = Chunk::default();
        c.set(IVec3::new(3, 5, 7), 2);
        assert_eq!(c.get(IVec3::new(3, 5, 7)), 2);
        assert_eq!(c.iter().collect::<Vec<_>>(), vec![(IVec3::new(3, 5, 7), 2)]);
        c.set(IVec3::new(3, 5, 7), AIR);
        assert!(c.is_empty());
    }
}
