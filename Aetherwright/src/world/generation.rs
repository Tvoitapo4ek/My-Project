//! Deterministic terrain generation (pure functions, run on worker threads).
//!
//! Layers: bedrock → stone (ores, caves) → dirt → grass/sand/clay surface,
//! water below sea level, trees and plants on grass, plus two hand-placed
//! "points of interest" derived from the seed: a calm aether-crystal spire
//! surrounded by an ancient ruin ring, and an agitated crystal anomaly.

use bevy::math::{IVec3, Vec2};

use super::{
    chunks::ChunkData,
    voxel::{BlockId, blocks},
};

#[derive(Clone, Copy, Debug)]
pub struct GenParams {
    pub seed: u32,
    pub chunk_size: i32,
    /// World size in blocks.
    pub size: IVec3,
    pub sea_level: i32,
}

// ---------------------------------------------------------------- noise ----

#[inline]
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

#[inline]
pub fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    hash(
        seed ^ hash(
            (x as u32).wrapping_mul(0x9E37_79B1) ^ hash(z as u32).wrapping_add(0x632B_E5AB),
        ),
    )
}

#[inline]
pub fn hash3(seed: u32, x: i32, y: i32, z: i32) -> u32 {
    hash(hash2(seed, x, z) ^ (y as u32).wrapping_mul(0x85EB_CA77))
}

#[inline]
fn unit(h: u32) -> f32 {
    (h & 0x00FF_FFFF) as f32 / 16_777_215.0
}

#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in [-1, 1].
pub fn noise2(seed: u32, x: f32, z: f32) -> f32 {
    let (xi, zi) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (smooth(x - xi as f32), smooth(z - zi as f32));
    let v = |dx, dz| unit(hash2(seed, xi + dx, zi + dz));
    let a = v(0, 0) + (v(1, 0) - v(0, 0)) * fx;
    let b = v(0, 1) + (v(1, 1) - v(0, 1)) * fx;
    (a + (b - a) * fz) * 2.0 - 1.0
}

pub fn noise3(seed: u32, x: f32, y: f32, z: f32) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (fx, fy, fz) = (
        smooth(x - xi as f32),
        smooth(y - yi as f32),
        smooth(z - zi as f32),
    );
    let v = |dx, dy, dz| unit(hash3(seed, xi + dx, yi + dy, zi + dz));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(v(0, 0, 0), v(1, 0, 0), fx);
    let x10 = lerp(v(0, 1, 0), v(1, 1, 0), fx);
    let x01 = lerp(v(0, 0, 1), v(1, 0, 1), fx);
    let x11 = lerp(v(0, 1, 1), v(1, 1, 1), fx);
    lerp(lerp(x00, x10, fy), lerp(x01, x11, fy), fz) * 2.0 - 1.0
}

pub fn fbm2(seed: u32, x: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += noise2(seed.wrapping_add(o * 101), x * freq, z * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

// ------------------------------------------------------------- terrain ----

/// Points of interest derived from the seed.
pub struct Sites {
    pub spire: Vec2,
    pub anomaly: Vec2,
}

pub fn sites(p: &GenParams) -> Sites {
    let w = p.size.x as f32;
    let d = p.size.z as f32;
    let jitter = |k: u32| unit(hash(p.seed ^ k)) * 0.12 - 0.06;
    Sites {
        spire: Vec2::new(w * (0.70 + jitter(1)), d * (0.32 + jitter(2))),
        anomaly: Vec2::new(w * (0.26 + jitter(3)), d * (0.72 + jitter(4))),
    }
}

/// Terrain surface height at a column.
pub fn height_at(p: &GenParams, x: i32, z: i32) -> i32 {
    let (fx, fz) = (x as f32, z as f32);
    let base = fbm2(p.seed, fx / 52.0, fz / 52.0, 4);
    let hills = fbm2(p.seed ^ 0xA5A5, fx / 23.0, fz / 23.0, 3);
    let mountains = (1.0 - fbm2(p.seed ^ 0x5151, fx / 70.0, fz / 70.0, 3).abs()).powi(3);
    let mask =
        smooth(((fbm2(p.seed ^ 0x77, fx / 90.0, fz / 90.0, 2) + 0.15) * 1.6).clamp(0.0, 1.0));
    // Lake basins: smooth depressions that dip below sea level.
    let lake =
        smooth(((-fbm2(p.seed ^ 0x1A4E, fx / 42.0, fz / 42.0, 2) - 0.1) * 3.0).clamp(0.0, 1.0));
    let mut h =
        p.sea_level as f32 + 5.0 + base * 9.0 + hills * 2.5 + mountains * mask * 20.0 - lake * 11.0;

    // Flatten a clearing around the spire and anomaly so they read clearly.
    let s = sites(p);
    for (site, r) in [(s.spire, 14.0), (s.anomaly, 10.0)] {
        let dist = Vec2::new(fx, fz).distance(site);
        if dist < r {
            let target = p.sea_level as f32 + 6.0;
            let t = smooth(1.0 - dist / r);
            h = h + (target - h) * t;
        }
    }
    // Keep the world edge a low rim so the finite map has a soft border.
    let edge = (x.min(z).min(p.size.x - 1 - x).min(p.size.z - 1 - z)) as f32;
    if edge < 6.0 {
        h = h.max(p.sea_level as f32 + 2.0 + (6.0 - edge) * 1.5);
    }
    (h.round() as i32).clamp(4, p.size.y - 12)
}

fn is_cave(p: &GenParams, x: i32, y: i32, z: i32, surface: i32) -> bool {
    if y < 3 || y > surface - 3 {
        return false;
    }
    let (fx, fy, fz) = (x as f32 / 18.0, y as f32 / 12.0, z as f32 / 18.0);
    let a = noise3(p.seed ^ 0xC0FE, fx, fy, fz);
    let b = noise3(p.seed ^ 0xBEEF, fx, fy, fz);
    a.abs() < 0.075 && b.abs() < 0.12
}

fn ore_at(p: &GenParams, x: i32, y: i32, z: i32) -> Option<BlockId> {
    let cluster = |salt: u32, scale: f32, threshold: f32| {
        noise3(
            p.seed ^ salt,
            x as f32 / scale,
            y as f32 / scale,
            z as f32 / scale,
        ) > threshold
    };
    if y < p.sea_level + 10 && cluster(0x1111, 4.0, 0.62) {
        return Some(blocks::COAL_ORE);
    }
    if y < p.sea_level && cluster(0x2222, 3.5, 0.66) {
        return Some(blocks::IRON_ORE);
    }
    if y < p.sea_level + 4 && cluster(0x3333, 3.5, 0.68) {
        return Some(blocks::COPPER_ORE);
    }
    // Rare ancient stone deep below.
    if y < 10 && cluster(0x4444, 5.0, 0.7) {
        return Some(blocks::ANCIENT_STONE);
    }
    None
}

/// Blocks of the two points of interest (overrides terrain).
fn site_block(p: &GenParams, s: &Sites, x: i32, y: i32, z: i32) -> Option<BlockId> {
    let floor = p.sea_level + 6;
    let v = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);

    // Spire: a tapering crystal needle on a ring of ancient stone pillars.
    let ds = v.distance(s.spire);
    let hy = y - floor;
    if (1..=11).contains(&hy) {
        let radius = 2.2 - hy as f32 * 0.17;
        if ds < radius {
            return Some(blocks::AETHER_CRYSTAL);
        }
    }
    if hy == 0 && ds < 9.0 {
        return Some(if ds < 3.5 {
            blocks::ANCIENT_STONE
        } else {
            blocks::MOSS_STONE
        });
    }
    if (1..=4).contains(&hy) && (8.0..9.2).contains(&ds) {
        // Eight broken pillars.
        let ang = (v.y - s.spire.y).atan2(v.x - s.spire.x);
        let sector = ((ang / std::f32::consts::TAU * 8.0).rem_euclid(1.0) - 0.5).abs();
        let pillar_height = 2
            + (hash2(
                p.seed,
                (ang * 8.0 / std::f32::consts::TAU).round() as i32,
                7,
            ) % 3) as i32;
        if sector > 0.38 && hy <= pillar_height {
            return Some(blocks::ANCIENT_STONE);
        }
    }

    // Anomaly: a cracked crater with jagged agitated crystals.
    let da = v.distance(s.anomaly);
    let hy = y - floor;
    if da < 5.0 && hy == 0 {
        return Some(blocks::GRAVEL);
    }
    if da < 2.6 && (1..=4).contains(&hy) {
        let jag = unit(hash3(p.seed, x, 0, z));
        if (hy as f32) < 1.0 + jag * 4.0 * (1.0 - da / 2.6) + 0.5 {
            return Some(blocks::AGITATED_CRYSTAL);
        }
    }
    None
}

struct Column {
    h: i32,
    surface: BlockId,
    sub: BlockId,
    rocky: bool,
}

fn column(p: &GenParams, x: i32, z: i32) -> Column {
    let h = height_at(p, x, z);
    let rocky =
        fbm2(p.seed ^ 0x9999, x as f32 / 40.0, z as f32 / 40.0, 2) > 0.32 || h > p.sea_level + 22;
    let (surface, sub) = if h <= p.sea_level + 1 {
        if fbm2(p.seed ^ 0x6060, x as f32 / 12.0, z as f32 / 12.0, 2) > 0.2 {
            (blocks::CLAY, blocks::SAND)
        } else {
            (blocks::SAND, blocks::SAND)
        }
    } else if rocky {
        if unit(hash2(p.seed ^ 3, x, z)) < 0.3 {
            (blocks::MOSS_STONE, blocks::STONE)
        } else {
            (blocks::STONE, blocks::STONE)
        }
    } else {
        (blocks::GRASS, blocks::DIRT)
    };
    Column {
        h,
        surface,
        sub,
        rocky,
    }
}

/// Tree root at this column? Returns trunk height.
fn tree_at(p: &GenParams, s: &Sites, x: i32, z: i32) -> Option<i32> {
    if x < 3 || z < 3 || x > p.size.x - 4 || z > p.size.z - 4 {
        return None;
    }
    let v = Vec2::new(x as f32, z as f32);
    if v.distance(s.spire) < 13.0 || v.distance(s.anomaly) < 9.0 {
        return None;
    }
    let forest = fbm2(p.seed ^ 0x4242, x as f32 / 35.0, z as f32 / 35.0, 2);
    let density = if forest > 0.15 { 0.035 } else { 0.006 };
    let r = unit(hash2(p.seed ^ 0x7777, x, z));
    if r >= density {
        return None;
    }
    let c = column(p, x, z);
    if c.surface != blocks::GRASS {
        return None;
    }
    Some(4 + (hash2(p.seed ^ 0x1234, x, z) % 3) as i32)
}

/// Generates one chunk. Pure and thread-safe.
pub fn generate_chunk(p: &GenParams, cc: IVec3) -> ChunkData {
    let n = p.chunk_size;
    let mut chunk = ChunkData::new(n);
    let origin = cc * n;
    let s = sites(p);

    for lz in 0..n {
        for lx in 0..n {
            let (x, z) = (origin.x + lx, origin.z + lz);
            let col = column(p, x, z);
            for ly in 0..n {
                let y = origin.y + ly;
                let b = if let Some(b) = site_block(p, &s, x, y, z) {
                    b
                } else if y == 0 {
                    blocks::BEDROCK
                } else if y > col.h {
                    if y <= p.sea_level {
                        blocks::WATER
                    } else {
                        blocks::AIR
                    }
                } else if is_cave(p, x, y, z, col.h) {
                    if y <= 6 { blocks::WATER } else { blocks::AIR }
                } else if y == col.h {
                    col.surface
                } else if y > col.h - 3 && !col.rocky {
                    col.sub
                } else if let Some(ore) = ore_at(p, x, y, z) {
                    ore
                } else if y > col.h - 3 {
                    col.sub
                } else {
                    blocks::STONE
                };
                if !b.is_air() {
                    chunk.set(IVec3::new(lx, ly, lz), b);
                }
            }
            // Ground plants.
            let ly = col.h + 1 - origin.y;
            if col.surface == blocks::GRASS
                && (0..n).contains(&ly)
                && site_block(p, &s, x, col.h + 1, z).is_none()
            {
                let r = unit(hash2(p.seed ^ 0x5EED, x, z));
                let plant = if r < 0.10 {
                    Some(blocks::TALL_GRASS)
                } else if r < 0.112 {
                    Some(blocks::BERRY_BUSH)
                } else {
                    None
                };
                if let Some(plant) = plant {
                    chunk.set(IVec3::new(lx, ly, lz), plant);
                }
            }
        }
    }

    // Trees may straddle chunk borders: scan a margin and write only the
    // blocks that fall inside this chunk.
    const R: i32 = 3;
    for z in origin.z - R..origin.z + n + R {
        for x in origin.x - R..origin.x + n + R {
            let Some(trunk) = tree_at(p, &s, x, z) else {
                continue;
            };
            let base = height_at(p, x, z) + 1;
            let top = base + trunk;
            let mut put = |wp: IVec3, b: BlockId, force: bool| {
                let lp = wp - origin;
                if chunk.in_bounds(lp) {
                    let cur = chunk.get(lp);
                    if force || cur.is_air() || cur == blocks::TALL_GRASS {
                        chunk.set(lp, b);
                    }
                }
            };
            for dy in -2..=1i32 {
                let radius: i32 = if dy >= 0 { 1 } else { 2 };
                for dz in -radius..=radius {
                    for dx in -radius..=radius {
                        let corner = dx.abs() == radius && dz.abs() == radius;
                        if corner
                            && (dy == 1 || unit(hash3(p.seed, x + dx, top + dy, z + dz)) < 0.5)
                        {
                            continue;
                        }
                        put(IVec3::new(x + dx, top + dy, z + dz), blocks::LEAVES, false);
                    }
                }
            }
            for y in base..top {
                put(IVec3::new(x, y, z), blocks::LOG, true);
            }
        }
    }
    chunk
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> GenParams {
        GenParams {
            seed: 42,
            chunk_size: 16,
            size: IVec3::new(160, 80, 160),
            sea_level: 22,
        }
    }

    #[test]
    fn deterministic() {
        let p = params();
        assert_eq!(
            generate_chunk(&p, IVec3::new(2, 1, 3)),
            generate_chunk(&p, IVec3::new(2, 1, 3))
        );
    }

    #[test]
    fn has_lakes() {
        for seed in [42, 1337, 7] {
            let p = GenParams { seed, ..params() };
            let mut deep = 0;
            for z in 0..p.size.z {
                for x in 0..p.size.x {
                    if height_at(&p, x, z) <= p.sea_level - 2 {
                        deep += 1;
                    }
                }
            }
            assert!(
                deep > 150,
                "seed {seed}: only {deep} columns with ≥ 3 m of water"
            );
        }
    }

    #[test]
    fn noise_in_range() {
        for i in 0..2000 {
            let v = fbm2(7, i as f32 * 0.37, i as f32 * 0.11, 4);
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn world_has_resources_and_features() {
        let p = params();
        let mut counts = std::collections::HashMap::new();
        for cy in 0..5 {
            for cz in 0..10 {
                for cx in 0..10 {
                    let c = generate_chunk(&p, IVec3::new(cx, cy, cz));
                    for b in &c.blocks {
                        *counts.entry(*b).or_insert(0usize) += 1;
                    }
                }
            }
        }
        for b in [
            blocks::GRASS,
            blocks::STONE,
            blocks::WATER,
            blocks::LOG,
            blocks::LEAVES,
            blocks::IRON_ORE,
            blocks::COAL_ORE,
            blocks::COPPER_ORE,
            blocks::AETHER_CRYSTAL,
            blocks::AGITATED_CRYSTAL,
            blocks::TALL_GRASS,
            blocks::BERRY_BUSH,
            blocks::BEDROCK,
            blocks::ANCIENT_STONE,
        ] {
            assert!(
                counts.get(&b).copied().unwrap_or(0) > 0,
                "missing {}",
                b.def().key
            );
        }
    }
}
