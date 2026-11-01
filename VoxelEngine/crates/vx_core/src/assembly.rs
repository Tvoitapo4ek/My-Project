//! Сборка и разборка конструкций (аналог «assemble» из Create / Sable).
//!
//! * [`assemble`] — вырезать связную группу блоков из сетки и сделать из неё
//!   новую динамическую сетку (корабль). Блоки рельефа (`terrain = true`)
//!   не захватываются — поэтому постройку можно «оторвать» от земли.
//! * [`disassemble`] — «приземлить» корабль: повернуть к ближайшей из 24
//!   осевых ориентаций и вписать блоки обратно в мир.

use std::collections::{HashSet, VecDeque};

use anyhow::{Result, bail};
use glam::{IVec3, Mat3, Quat, Vec3};

use crate::{AIR, Face, Grid, GridId, GridKind, Level, Pose, Registry, WORLD};

/// Найти связную (по граням) группу непустых блоков, не являющихся рельефом.
pub fn collect(grid: &Grid, start: IVec3, reg: &Registry, limit: usize) -> Result<Vec<IVec3>> {
    let movable = |p: IVec3| {
        let id = grid.get(p);
        id != AIR && !reg.get(id).terrain
    };
    if !movable(start) {
        bail!("этот блок нельзя собрать (воздух или рельеф)");
    }
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(p) = queue.pop_front() {
        for f in Face::ALL {
            let n = p + f.normal();
            if movable(n) && seen.insert(n) {
                if seen.len() > limit {
                    bail!("конструкция больше {limit} блоков");
                }
                queue.push_back(n);
            }
        }
    }
    Ok(seen.into_iter().collect())
}

/// Собрать конструкцию, начиная с блока `start` сетки `source`.
pub fn assemble(level: &mut Level, source: GridId, start: IVec3, reg: &Registry, limit: usize) -> Result<GridId> {
    let Some(src) = level.get_mut(source) else { bail!("нет сетки {source:?}") };
    let blocks = collect(src, start, reg, limit)?;
    let mut ship = Grid::new(GridKind::Dynamic);
    // Начало координат корабля — в блоке `start`, так что в момент сборки
    // блоки корабля совпадают с исходными один в один.
    ship.pose = Pose { pos: src.pose.to_world(start.as_vec3()), rot: src.pose.rot };
    for p in blocks {
        ship.set(p - start, src.get(p));
        src.set(p, AIR);
    }
    Ok(level.add(ship))
}

/// Ближайший к `rot` поворот из 24 осевых. `None`, если конструкция
/// наклонена почти на 45° и выбор неоднозначен.
pub fn snap_rotation(rot: Quat) -> Option<Mat3> {
    let snap = |v: Vec3| {
        let a = v.abs();
        let axis = if a.x >= a.y && a.x >= a.z {
            Vec3::X
        } else if a.y >= a.z {
            Vec3::Y
        } else {
            Vec3::Z
        };
        axis * v.dot(axis).signum()
    };
    let (x, y, z) = (snap(rot * Vec3::X), snap(rot * Vec3::Y), snap(rot * Vec3::Z));
    (x.cross(y) == z).then(|| Mat3::from_cols(x, y, z))
}

/// Вписать корабль `ship` обратно в мир.
pub fn disassemble(level: &mut Level, ship: GridId) -> Result<()> {
    if ship == WORLD {
        bail!("мир нельзя разобрать");
    }
    let (Some(s), world) = (level.get(ship), level.world()) else { bail!("нет сетки {ship:?}") };
    // Поза корабля относительно мира.
    let rot = world.pose.rot.inverse() * s.pose.rot;
    let pos = world.pose.to_local(s.pose.pos);
    let Some(m) = snap_rotation(rot) else {
        bail!("слишком сильный наклон — выровняйте конструкцию")
    };

    let blocks: Vec<_> = s.blocks().collect();
    let Some(&(first, _)) = blocks.first() else { bail!("пустая конструкция") };
    // Опорный блок: его центр округляем до ближайшей клетки мира,
    // остальные блоки ставим относительно него с выровненным поворотом.
    let centre = |l: IVec3| l.as_vec3() + 0.5;
    let snapped = (pos + rot * centre(first)).floor() + 0.5;
    let origin = snapped - m * centre(first);
    let target = |l: IVec3| (origin + m * centre(l)).floor().as_ivec3();

    for &(l, _) in &blocks {
        let t = target(l);
        if world.get(t) != AIR || world.bounds.is_some_and(|b| !b.contains(t)) {
            bail!("место занято или вне мира: {t}");
        }
    }
    let world = level.world_mut();
    for &(l, id) in &blocks {
        world.set(target(l), id);
    }
    level.remove(ship);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Registry {
        Registry::from_toml("[[block]]\nname='ground'\nterrain=true\n[[block]]\nname='plank'").unwrap()
    }

    #[test]
    fn assemble_then_disassemble_rotated() {
        let reg = reg();
        let (ground, plank) = (reg.id("ground").unwrap(), reg.id("plank").unwrap());
        let mut world = Grid::new(GridKind::Static);
        for x in -5..5 {
            world.set(IVec3::new(x, 0, 0), ground);
        }
        for x in 0..3 {
            world.set(IVec3::new(x, 1, 0), plank);
        }
        let mut level = Level::new(world);

        let id = assemble(&mut level, WORLD, IVec3::new(1, 1, 0), &reg, 100).unwrap();
        assert_eq!(level.get(id).unwrap().blocks().count(), 3);
        assert_eq!(level.world().get(IVec3::new(1, 1, 0)), AIR);
        assert_eq!(level.world().get(IVec3::new(1, 0, 0)), ground);

        // Корабль «полетал», повернулся почти на 90° вокруг Y и сдвинулся.
        let s = level.get_mut(id).unwrap();
        s.pose.rot = Quat::from_rotation_y(1.5);
        s.pose.pos += Vec3::new(0.3, 4.2, 0.1);
        disassemble(&mut level, id).unwrap();
        let placed: Vec<_> = level.world().blocks().filter(|b| b.1 == plank).map(|b| b.0).collect();
        assert_eq!(placed.len(), 3);
        // После поворота на 90° вокруг Y линия вдоль X стала линией вдоль Z.
        assert!(placed.iter().all(|p| p.x == placed[0].x && p.y == 5));
    }
}
