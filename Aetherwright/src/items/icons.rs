//! Item icons in the game's pixel style (docs/STYLE_GUIDE.md §7).
//!
//! * Block items: a small isometric cube built from the block's own
//!   textures (top + sides, sides shaded).
//! * Other items: 16×16 pixel art drawn from a shape mask (ingot, ore,
//!   crystal, cog, casing, rune …) in the item's colour, with automatic
//!   dark outline and top-left light — so every new item, including items
//!   from mods, gets an icon in style without drawing anything.
//! * Override: `textures/items/<icon or key>.png` (any square size; mods
//!   win over `assets/`) replaces the generated icon.
//!
//! Icons are built once at startup; `icon(item)` returns the handle.

use std::sync::RwLock;

use bevy::prelude::*;

use super::{Category, ItemId, Use, all_items};
use crate::world::materials::{BlockMaterials, image_from_rgba};

static ICONS: RwLock<Vec<Handle<Image>>> = RwLock::new(Vec::new());

/// The icon of an item (None before startup finished or in headless tests).
pub fn icon(item: ItemId) -> Option<Handle<Image>> {
    ICONS.read().ok()?.get(item.0 as usize).cloned()
}

/// Output size (pixels). Masks are 16×16 and doubled.
const SIZE: usize = 32;

type Px = [u8; 4];

// ---------------------------------------------------------------- masks
//
// '.' empty, 'a' item colour, 'e' item colour glowing (no shading),
// 'b' wood, 'c' brass/gold, 'd' andesite, 'f' dark iron/rubber,
// 'g' leaf green, 'h' glass, 'r' rune stone.

const INGOT: &str = "
....aaaaaaaa....
...aaaaaaaaaa...
..aaaaaaaaaaaa..
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.";

const ORE: &str = "
.....dddd.......
...ddddddddd....
..dddadddddddd..
..ddaaddddaddd..
.dddddddddaaddd.
.ddddddaddddddd.
.dddddaaadddddd.
..ddaddddddddd..
..dddddddddddd..
....dddddddd....";

const LUMP: &str = "
.....aaaa.......
...aaaaaaaaa....
..aaaaaaaaaaaa..
..aaaaaaaaaaaa..
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
..aaaaaaaaaaaa..
..aaaaaaaaaaaa..
....aaaaaaaa....";

const SHARD: &str = "
.......e........
......aea.......
......aeaa......
.....aaeaa......
.....aaeaaa.....
....aaaeaaa.....
....aaaeaaaa....
....aaaaaaaa....
.....aaaaaa.....
......aaaa......
.......aa.......";

const FIBER: &str = "
..a.......a.....
...a.....a......
...a....a...a...
....a..a...a....
....a.aa..a.....
.....aaa.a......
.....aaaa.......
....ccaacc......
.....aaaa.......
....a.aa.a......
...a..a...a.....
..a...a....a....";

const BERRIES: &str = "
.......gg.......
......gggg......
.......bb.......
......b..b......
....aa....aa....
...aaaa..aaaa...
...aaaa..aaaa...
....aa.aa.aa....
......aaaa......
......aaaa......
.......aa.......";

const FLASK: &str = "
......bbbb......
......dddd......
.......hh.......
.......hh.......
.....hhhhhh.....
....hhaaaahh....
...hhaaaaaahh...
...haaaaaaaah...
...haaaaaaaah...
...hhaaaaaahh...
....hhhhhhhh....";

const JAR: &str = "
.....cccccc.....
.....dddddd.....
....hhhhhhhh....
....hheeeehh....
....heeeeeeh....
....heeeeeeh....
....hheeeehh....
....hhhhhhhh....
.....cccccc.....";

const RATION: &str = "
..aaaaaaaaaaaa..
..aaaaaaaaaaaa..
..aaaaccaaaaaa..
..cccccccccccc..
..aaaaccaaaaaa..
..aaaaccaaaaaa..
..aaaaaaaaaaaa..";

const GEAR: &str = "
......aaaa......
...aa.aaaa.aa...
...aaaaaaaaaa...
....aaaaaaaa....
.aaaaaaddaaaaaa.
.aaaaaddddaaaaa.
.aaaaddffddaaaa.
.aaaaddffddaaaa.
.aaaaaddddaaaaa.
.aaaaaaddaaaaaa.
....aaaaaaaa....
...aaaaaaaaaa...
...aa.aaaa.aa...
......aaaa......";

const SHAFT: &str = "
............dd..
...........dddd.
..........dddd..
.........dddd...
........dddd....
.......dddd.....
......dddd......
.....dddd.......
....dddd........
...dddd.........
..dddd..........
.dddd...........
..dd............";

const WHEEL: &str = "
.....ffffff.....
...ffffffffff...
..ffffbbbbffff..
.fffbb....bbfff.
.fffb......bfff.
.ffb...bb...bff.
.ffb..bccb..bff.
.ffb..bccb..bff.
.ffb...bb...bff.
.fffb......bfff.
.fffbb....bbfff.
..ffffbbbbffff..
...ffffffffff...
.....ffffff.....";

const CASING: &str = "
.aaaaaaaaaaaaaa.
.acaaaaaaaaaaca.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.aabbbbbbbbbbaa.
.acaaaaaaaaaaca.
.aaaaaaaaaaaaaa.";

const SENSOR: &str = "
.dddddddddddddd.
.dcddddddddddcd.
.ddaaaaaaaaaadd.
.ddaaahhhhaaadd.
.ddaahheehhaadd.
.ddaaheeeehaadd.
.ddaaheeeehaadd.
.ddaahheehhaadd.
.ddaaahhhhaaadd.
.ddaaaaaaaaaadd.
.dcddddddddddcd.
.dddddddddddddd.";

const CHIP: &str = "
....c.c.c.c.c...
...fffffffffff..
..cfaaaaaaaaafc.
...faaaaaaaaaf..
..cfaaaeeeaaafc.
...faaaeeeaaaf..
..cfaaaaaaaaafc.
...fffffffffff..
....c.c.c.c.c...";

const RUNE: &str = "
....rrrrrrrr....
...rrrrrrrrrr...
...rrrreerrrr...
...rrrerrerrr...
...rrrreerrrr...
...rrrrreerrr...
...rrrrerrerr...
...rrreerrrr....
...rrrrrrrrrr...
....rrrrrrrr....";

const PEDESTAL: &str = "
......eeee......
.....eeeeee.....
.....eeeeee.....
......eeee......
................
.......cc.......
......rrrr......
......rrrr......
.....cccccc.....
....rrrrrrrr....
....rrrrrrrr....";

const WAND: &str = "
...........eee..
..........eeeee.
..........eeeee.
..........ceee..
.........cc.....
........bb......
.......bb.......
......bb........
.....bb.........
....bb..........
...bb...........
..bb............
.bb.............";

const SWORD: &str = "
.............aa.
............aaa.
...........aaa..
..........aaa...
.........aaa....
........aaa.....
...c...aaa......
...cc.aaa.......
....ccaa........
.....cc.........
....bbcc........
...bb..cc.......
..bb............
.bb.............";

const GUN: &str = "
..cccccccccc....
.caaaaaaaaaaeee.
.caaaaaaaaaaeee.
..cccccccccc....
.....bbcc.......
.....bbc........
....bbb.........
....bbb.........";

const WRENCH: &str = "
..........aa.a..
.........aaa.aa.
.........aaaaaa.
........aaaaaa..
.......aaa......
......aaa.......
.....aaa........
....bbb.........
...bbb..........
..bbb...........
.bbb............";

const CABLE: &str = "
....aaaaaa......
..aa......aa....
.a..aaaaaa..a...
.a.a......a.a...
.a.a..cc..a.a...
.a.a......a.a...
.a..aaaaaa..a...
..aa......aa....
....aaaaaa..aaa.";

const BOLT: &str = "
.....aaaaaa.....
.....aaaaaa.....
.......aa.......
.......aa.......
.......aa.......
.......aa.......
.......aa.......";

const HELMET: &str = "
....aaaaaaaa....
...aaaaaaaaaa...
..aaaaaaaaaaaa..
..aeeeeeeeeeea..
..aeeeeeeeeeea..
..aaaaaaaaaaaa..
..aa........aa..";

const COAT: &str = "
..aaa......aaa..
.aaaaaa..aaaaaa.
.aaaaaaccaaaaaa.
.aaaaaaccaaaaaa.
...aaaaccaaaa...
...aaaaccaaaa...
...aaaaccaaaa...
...aaaaccaaaa...
...aaaaaaaaaa...";

const GLOVES: &str = "
....a.a.a.......
....a.a.a.a.....
....aaaaaaa.....
..a.aaaaaaa.....
..aaaaaaaaa.....
...aaaaaaaa.....
....aaaaaa......
....cccccc......
....aaaaaa......";

const BOOTS: &str = "
...aaaa.........
...aaaa.........
...aaaa.........
...aaaa.........
...aaaaaaaa.....
...aaaaaaaaaa...
...cccccccccc...";

const CHARM: &str = "
.....c.....c....
......c...c.....
.......c.c......
........c.......
.......eee......
......eeeee.....
......eeeee.....
.......eee......";

const TILE: &str = "
..aaaaaaaaaaaa..
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
.aaaaaaaaaaaaaa.
..aaaaaaaaaaaa..";

/// Which mask an item uses: by key words first, then by use, then category.
fn shape(def: &super::ItemDef) -> &'static str {
    let k = def.key.as_str();
    let has = |w: &str| k.contains(w);
    if has("coal") {
        return LUMP;
    }
    if has("ore") {
        return ORE;
    }
    if k == "metal" || k == "copper" || has("ingot") {
        return INGOT;
    }
    if has("shard") {
        return SHARD;
    }
    for (w, m) in [
        ("fiber", FIBER),
        ("berr", BERRIES),
        ("flask", FLASK),
        ("ration", RATION),
        ("jar", JAR),
        ("wrench", WRENCH),
        ("cable", CABLE),
        ("wheel", WHEEL),
        ("visor", HELMET),
        ("helmet", HELMET),
        ("coat", COAT),
        ("glove", GLOVES),
        ("boot", BOOTS),
        ("charm", CHARM),
    ] {
        if has(w) {
            return m;
        }
    }
    if k == "bolt" {
        return BOLT;
    }
    if (has("cog") || k == "gear") && !has("box") {
        return GEAR;
    }
    if k == "shaft" || k == "drive_shaft" {
        return SHAFT;
    }
    if k.starts_with("sensor_") {
        return SENSOR;
    }
    if k.starts_with("gate_") || k.starts_with("logic_") || k.starts_with("signal_") {
        return CHIP;
    }
    match &def.use_ {
        Use::Melee => return SWORD,
        Use::Ranged | Use::Manipulator | Use::PhysGun => return GUN,
        Use::Focus => return WAND,
        Use::Rune(_) => return RUNE,
        Use::Module(_) => return CHIP,
        Use::Repair => return WRENCH,
        _ => {}
    }
    match def.category {
        Category::Magic => PEDESTAL,
        Category::Part | Category::Station => CASING,
        _ => TILE,
    }
}

/// Item colour; very dark colours are lifted so the shape stays readable
/// against its own (darker) outline.
fn rgb(c: (f32, f32, f32)) -> [f32; 3] {
    let max = c.0.max(c.1).max(c.2);
    let k = if max < 0.3 { 0.3 / max.max(0.01) } else { 1.0 };
    [c.0 * k, c.1 * k, c.2 * k]
}

/// Base colour of a mask cell (None = empty), and whether it is shaded.
fn cell_color(ch: char, item: [f32; 3]) -> Option<([f32; 3], bool)> {
    Some(match ch {
        'a' => (item, true),
        'e' => ([(item[0] * 1.25 + 0.1).min(1.0), (item[1] * 1.25 + 0.1).min(1.0), (item[2] * 1.25 + 0.1).min(1.0)], false),
        'b' => ([0.66, 0.46, 0.26], true),
        'c' => ([0.93, 0.74, 0.32], true),
        'd' => ([0.62, 0.64, 0.61], true),
        'f' => ([0.24, 0.24, 0.27], true),
        'g' => ([0.33, 0.62, 0.27], true),
        'h' => ([0.72, 0.88, 0.94], true),
        'r' => ([0.25, 0.23, 0.33], true),
        _ => return None,
    })
}

fn to_px(c: [f32; 3]) -> Px {
    [(c[0].clamp(0.0, 1.0) * 255.0) as u8, (c[1].clamp(0.0, 1.0) * 255.0) as u8, (c[2].clamp(0.0, 1.0) * 255.0) as u8, 255]
}

/// Render a 16×16 mask to RGBA (SIZE×SIZE): outline + top-left light.
fn render_mask(mask: &str, item: [f32; 3]) -> Vec<u8> {
    let rows: Vec<Vec<char>> = mask.lines().filter(|l| !l.is_empty()).map(|l| l.chars().take(16).collect()).collect();
    let top = (16usize.saturating_sub(rows.len())) / 2;
    let mut grid: [[Option<([f32; 3], bool)>; 16]; 16] = [[None; 16]; 16];
    for (y, r) in rows.iter().enumerate().take(16) {
        for (x, ch) in r.iter().enumerate() {
            grid[(top + y).min(15)][x] = cell_color(*ch, item);
        }
    }
    let filled = |x: i32, y: i32| (0..16).contains(&x) && (0..16).contains(&y) && grid[y as usize][x as usize].is_some();
    let mut out = [[[0u8; 4]; 16]; 16];
    for y in 0..16i32 {
        for x in 0..16i32 {
            match grid[y as usize][x as usize] {
                Some((c, shaded)) => {
                    let k = if !shaded {
                        1.0
                    } else if !filled(x, y - 1) || !filled(x - 1, y) {
                        1.22
                    } else if !filled(x, y + 1) || !filled(x + 1, y) {
                        0.72
                    } else {
                        1.0
                    };
                    out[y as usize][x as usize] = to_px([c[0] * k, c[1] * k, c[2] * k]);
                }
                None => {
                    // Outline: the darkened colour of a neighbour.
                    let n = [(0, -1), (-1, 0), (1, 0), (0, 1)]
                        .iter()
                        .find_map(|(dx, dy)| filled(x + dx, y + dy).then(|| grid[(y + dy) as usize][(x + dx) as usize].unwrap().0));
                    if let Some(c) = n {
                        out[y as usize][x as usize] = to_px([c[0] * 0.28, c[1] * 0.28, c[2] * 0.32]);
                    }
                }
            }
        }
    }
    let mut data = vec![0u8; SIZE * SIZE * 4];
    let s = SIZE / 16;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = out[y / s][x / s];
            data[(y * SIZE + x) * 4..][..4].copy_from_slice(&p);
        }
    }
    data
}

/// Texture pixels (RGBA8) with its side length.
struct Tex<'a> {
    w: usize,
    data: &'a [u8],
}

impl Tex<'_> {
    fn sample(&self, u: f32, v: f32) -> Px {
        let x = ((u.clamp(0.0, 0.999)) * self.w as f32) as usize;
        let y = ((v.clamp(0.0, 0.999)) * self.w as f32) as usize;
        let i = (y * self.w + x) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }
}

/// An isometric cube from the top and side textures.
fn render_cube(top: &Tex, side: &Tex) -> Vec<u8> {
    let mut data = vec![0u8; SIZE * SIZE * 4];
    let n = SIZE as f32;
    // Cube corners on the 32×32 canvas.
    let o_top = Vec2::new(n / 2.0, 1.0);
    let (ex, ey) = (n / 2.0 - 2.0, (n / 2.0 - 2.0) / 2.0);
    let h = n - 2.0 - 2.0 * ey - 1.0;
    let shade = |p: Px, k: f32| -> Px { [(p[0] as f32 * k) as u8, (p[1] as f32 * k) as u8, (p[2] as f32 * k) as u8, p[3]] };
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            // Top: p = o_top + u·(ex, ey) + v·(−ex, ey).
            let d = p - o_top;
            let (u, v) = ((d.x / ex + d.y / ey) / 2.0, (d.y / ey - d.x / ex) / 2.0);
            let px = if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                Some(shade(top.sample(u, v), 1.08))
            } else {
                // Left side: from (cx−ex, cy) along (ex, ey) and down.
                let ol = Vec2::new(n / 2.0 - ex, 1.0 + ey);
                let d = p - ol;
                let u = d.x / ex;
                let v = (d.y - u * ey) / h;
                if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                    Some(shade(side.sample(u, v), 0.82))
                } else {
                    let or = Vec2::new(n / 2.0, 1.0 + 2.0 * ey);
                    let d = p - or;
                    let u = d.x / ex;
                    let v = (d.y + u * ey) / h;
                    ((0.0..1.0).contains(&u) && (0.0..1.0).contains(&v)).then(|| shade(side.sample(u, v), 0.62))
                }
            };
            if let Some(c) = px {
                data[(y * SIZE + x) * 4..][..4].copy_from_slice(&c);
            }
        }
    }
    data
}

fn load_override(def: &super::ItemDef) -> Option<Image> {
    let name = def.icon.clone().unwrap_or_else(|| def.key.clone());
    let path = crate::content::find_asset(&format!("textures/items/{name}.png"))?;
    let (w, h, data) = crate::world::materials::read_png(&path).ok()?;
    Some(image_from_rgba(w, h, data))
}

/// Builds every icon (run after block textures are loaded).
pub fn build_icons(mut images: ResMut<Assets<Image>>, blocks: Option<Res<BlockMaterials>>) {
    let mut out = Vec::with_capacity(all_items().len());
    for (i, def) in all_items().iter().enumerate() {
        let img = load_override(def).or_else(|| {
            // Block items: the cube from the block's textures.
            let b = ItemId(i as u16).block()?;
            let blocks = blocks.as_ref()?;
            let get = |t: crate::world::voxel::TexId| {
                let img = images.get(blocks.images.get(t.0 as usize)?)?;
                let data = img.data.clone()?;
                Some((img.width() as usize, data))
            };
            let (tw, td) = get(b.def().tex[0])?;
            let (sw, sd) = get(b.def().tex[1])?;
            (td.len() >= tw * tw * 4 && sd.len() >= sw * sw * 4).then(|| {
                image_from_rgba(SIZE as u32, SIZE as u32, render_cube(&Tex { w: tw, data: &td }, &Tex { w: sw, data: &sd }))
            })
        });
        let img = img.unwrap_or_else(|| image_from_rgba(SIZE as u32, SIZE as u32, render_mask(shape(def), rgb(def.color))));
        out.push(images.add(img));
    }
    if let Ok(mut icons) = ICONS.write() {
        *icons = out;
    }
}

pub struct ItemIconsPlugin;

impl Plugin for ItemIconsPlugin {
    fn build(&self, app: &mut App) {
        // Block materials are set up in PreStartup.
        app.add_systems(Startup, build_icons);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_has_a_mask_that_renders() {
        for def in all_items() {
            let data = render_mask(shape(def), rgb(def.color));
            assert_eq!(data.len(), SIZE * SIZE * 4);
            let opaque = data.chunks_exact(4).filter(|p| p[3] > 0).count();
            assert!(opaque > 40, "{} icon is almost empty", def.key);
        }
    }

    #[test]
    fn masks_are_at_most_16_by_16() {
        for m in [INGOT, ORE, LUMP, SHARD, FIBER, BERRIES, FLASK, JAR, RATION, GEAR, SHAFT, WHEEL, CASING, SENSOR, CHIP, RUNE, PEDESTAL, WAND, SWORD, GUN, WRENCH, CABLE, BOLT, HELMET, COAT, GLOVES, BOOTS, CHARM, TILE] {
            let rows: Vec<&str> = m.lines().filter(|l| !l.is_empty()).collect();
            assert!(rows.len() <= 16);
            assert!(rows.iter().all(|r| r.chars().count() <= 16), "{rows:?}");
        }
    }

    /// `ICON_SHEET=icons.png cargo test icon_sheet -- --ignored` writes
    /// every generated icon (4× scale) to one picture for review.
    #[test]
    #[ignore]
    fn icon_sheet() {
        let Ok(path) = std::env::var("ICON_SHEET") else { return };
        let items = all_items();
        let cols = 12;
        let rows = items.len().div_ceil(cols);
        let (cell, scale) = (SIZE * 4 + 8, 4);
        let (w, h) = (cols * cell, rows * cell);
        let mut sheet = vec![40u8; w * h * 4];
        for (i, def) in items.iter().enumerate() {
            let data = ItemId(i as u16)
                .block()
                .and_then(|b| {
                    let load = |n: &str| crate::world::materials::texture_file(n).and_then(|p| crate::world::materials::read_png(&p).ok());
                    let (tw, _, td) = load(&b.def().texture_names[0])?;
                    let (sw, _, sd) = load(&b.def().texture_names[1])?;
                    Some(render_cube(&Tex { w: tw as usize, data: &td }, &Tex { w: sw as usize, data: &sd }))
                })
                .unwrap_or_else(|| render_mask(shape(def), rgb(def.color)));
            let (ox, oy) = ((i % cols) * cell + 4, (i / cols) * cell + 4);
            for y in 0..SIZE * scale {
                for x in 0..SIZE * scale {
                    let p = &data[((y / scale) * SIZE + x / scale) * 4..][..4];
                    if p[3] > 0 {
                        sheet[((oy + y) * w + ox + x) * 4..][..4].copy_from_slice(p);
                    }
                }
            }
        }
        for px in sheet.chunks_exact_mut(4) {
            px[3] = 255;
        }
        crate::world::materials::write_png(std::path::Path::new(&path), w as u32, h as u32, &sheet).unwrap();
    }

    #[test]
    fn cube_uses_both_textures() {
        let top = vec![255u8; 4 * 4 * 4];
        let side = [10u8, 20, 30, 255].repeat(16);
        let data = render_cube(&Tex { w: 4, data: &top }, &Tex { w: 4, data: &side });
        let bright = data.chunks_exact(4).filter(|p| p[3] > 0 && p[0] > 200).count();
        let dark = data.chunks_exact(4).filter(|p| p[3] > 0 && p[0] < 20).count();
        assert!(bright > 50 && dark > 50, "top {bright}, sides {dark}");
    }
}
