use std::collections::{BTreeMap, HashMap, HashSet};

use glam::{IVec3, Mat4, Quat, Vec3};

use crate::{AIR, BlockId, Chunk, split};

/// Положение сетки в мире. Блок `p` сетки занимает куб `[p, p+1)` в её
/// локальных координатах; в мир он переводится через `pose`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pos: Vec3,
    pub rot: Quat,
}

impl Pose {
    pub fn at(pos: Vec3) -> Self {
        Pose { pos, rot: Quat::IDENTITY }
    }
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rot, self.pos)
    }
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.pos + self.rot * local
    }
    pub fn to_local(&self, world: Vec3) -> Vec3 {
        self.rot.inverse() * (world - self.pos)
    }
}

/// Статичная сетка (мир) или динамическая (корабль, контрапция).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridKind {
    Static,
    Dynamic,
}

/// Границы в блоках: `min` включительно, `max` исключительно.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min: IVec3,
    pub max: IVec3,
}

impl Bounds {
    pub fn contains(&self, p: IVec3) -> bool {
        p.cmpge(self.min).all() && p.cmplt(self.max).all()
    }
}

/// Сетка блоков: разреженный набор чанков + поза в мире.
///
/// Это главная абстракция движка (как «саб-уровни» в Sable):
/// мир и каждая летающая конструкция — это `Grid`. Мешинг, рендер,
/// столкновения и редактирование работают с любой сеткой одинаково.
pub struct Grid {
    pub kind: GridKind,
    pub pose: Pose,
    /// Ограничение области (для ограниченного мира). `None` — без границ.
    pub bounds: Option<Bounds>,
    chunks: HashMap<IVec3, Chunk>,
    dirty: HashSet<IVec3>,
}

impl Grid {
    pub fn new(kind: GridKind) -> Self {
        Grid { kind, pose: Pose::default(), bounds: None, chunks: HashMap::new(), dirty: HashSet::new() }
    }

    pub fn get(&self, pos: IVec3) -> BlockId {
        let (c, l) = split(pos);
        self.chunks.get(&c).map_or(AIR, |ch| ch.get(l))
    }

    /// Поставить блок. Возвращает `false`, если позиция вне границ.
    /// Помечает изменённый чанк (и соседей на стыке) как «грязные».
    pub fn set(&mut self, pos: IVec3, id: BlockId) -> bool {
        if self.bounds.is_some_and(|b| !b.contains(pos)) {
            return false;
        }
        let (c, l) = split(pos);
        let old = match self.chunks.get_mut(&c) {
            Some(ch) => ch.set(l, id),
            None if id == AIR => return true,
            None => self.chunks.entry(c).or_default().set(l, id),
        };
        if old == id {
            return true;
        }
        if self.chunks[&c].is_empty() {
            self.chunks.remove(&c);
        }
        self.dirty.insert(c);
        // Соседние чанки зависят от этого блока (отсечение граней, AO).
        for axis in 0..3 {
            if l[axis] == 0 || l[axis] == crate::CHUNK - 1 {
                let mut d = IVec3::ZERO;
                d[axis] = if l[axis] == 0 { -1 } else { 1 };
                self.dirty.insert(c + d);
            }
        }
        true
    }

    /// Вставить готовый чанк целиком (быстрый путь для генератора).
    pub fn insert_chunk(&mut self, cpos: IVec3, chunk: Chunk) {
        self.dirty.insert(cpos);
        if chunk.is_empty() {
            self.chunks.remove(&cpos);
        } else {
            self.chunks.insert(cpos, chunk);
        }
    }

    pub fn chunk(&self, cpos: IVec3) -> Option<&Chunk> {
        self.chunks.get(&cpos)
    }

    pub fn chunks(&self) -> impl Iterator<Item = (IVec3, &Chunk)> {
        self.chunks.iter().map(|(p, c)| (*p, c))
    }

    /// Все непустые блоки сетки.
    pub fn blocks(&self) -> impl Iterator<Item = (IVec3, BlockId)> + '_ {
        self.chunks.iter().flat_map(|(c, ch)| ch.iter().map(move |(l, b)| (*c * crate::CHUNK + l, b)))
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Забрать список изменённых чанков (для перестройки мешей и коллайдеров).
    pub fn take_dirty(&mut self) -> Vec<IVec3> {
        self.dirty.drain().collect()
    }

    /// Пометить все чанки как изменённые.
    pub fn mark_all_dirty(&mut self) {
        self.dirty.extend(self.chunks.keys().copied());
    }

    /// Высшая непустая точка столбца (x, z) в пределах `y_range`.
    pub fn top(&self, x: i32, z: i32, y_range: std::ops::Range<i32>) -> Option<i32> {
        y_range.rev().find(|&y| self.get(IVec3::new(x, y, z)) != AIR)
    }
}

/// Идентификатор сетки внутри [`Level`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GridId(pub u32);

/// Основной (статичный) мир всегда имеет id 0.
pub const WORLD: GridId = GridId(0);

/// Уровень — все сетки: мир и конструкции.
pub struct Level {
    grids: BTreeMap<GridId, Grid>,
    next: u32,
}

impl Level {
    /// Новый уровень с основным миром `world` под id [`WORLD`].
    pub fn new(world: Grid) -> Self {
        Level { grids: BTreeMap::from([(WORLD, world)]), next: 1 }
    }

    pub fn add(&mut self, grid: Grid) -> GridId {
        let id = GridId(self.next);
        self.next += 1;
        self.grids.insert(id, grid);
        id
    }

    pub fn remove(&mut self, id: GridId) -> Option<Grid> {
        if id == WORLD { None } else { self.grids.remove(&id) }
    }

    pub fn get(&self, id: GridId) -> Option<&Grid> {
        self.grids.get(&id)
    }

    pub fn get_mut(&mut self, id: GridId) -> Option<&mut Grid> {
        self.grids.get_mut(&id)
    }

    pub fn world(&self) -> &Grid {
        &self.grids[&WORLD]
    }

    pub fn world_mut(&mut self) -> &mut Grid {
        self.grids.get_mut(&WORLD).unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item = (GridId, &Grid)> {
        self.grids.iter().map(|(id, g)| (*id, g))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (GridId, &mut Grid)> {
        self.grids.iter_mut().map(|(id, g)| (*id, g))
    }

    pub fn ids(&self) -> Vec<GridId> {
        self.grids.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_marks_neighbours_dirty() {
        let mut g = Grid::new(GridKind::Static);
        g.set(IVec3::new(0, 5, 5), 1);
        let mut d = g.take_dirty();
        d.sort_by_key(|v| v.to_array());
        assert_eq!(d, vec![IVec3::new(-1, 0, 0), IVec3::ZERO]);
        g.set(IVec3::new(0, 5, 5), AIR);
        assert!(g.is_empty());
    }

    #[test]
    fn bounds() {
        let mut g = Grid::new(GridKind::Static);
        g.bounds = Some(Bounds { min: IVec3::ZERO, max: IVec3::splat(16) });
        assert!(!g.set(IVec3::new(16, 0, 0), 1));
        assert!(g.set(IVec3::new(15, 0, 0), 1));
    }
}
