//! Game content as data: blocks, textures, items, recipes.
//!
//! Load order (later entries override earlier ones by key, new keys are
//! appended):
//! 1. built-in defaults compiled into the binary (`assets/content/*.ron`
//!    at build time) — the game always starts;
//! 2. `assets/content/*.ron` on disk (edit to rebalance the base game);
//! 3. every `mods/<name>/content/*.ron`, mods sorted by folder name.
//!
//! The registry is built once, before the Bevy app starts, and stored in a
//! global (`registry()`), so lookups like `BlockId::def()` stay cheap and
//! return `&'static` data. Content cannot change while the game runs;
//! textures can (see `world::materials`).

pub mod defs;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub use defs::*;

/// Fully resolved content.
#[derive(Debug, Default)]
pub struct Registry {
    pub blocks: Vec<BlockDef>,
    pub textures: Vec<TextureDef>,
    pub items: Vec<ItemDef>,
    pub recipes: Vec<RecipeDef>,
    pub journal: Vec<JournalDef>,
    journal_index: HashMap<String, u16>,
    block_index: HashMap<String, u16>,
    texture_index: HashMap<String, u16>,
    item_index: HashMap<String, u16>,
    /// Where content came from (for the debug screen / modding).
    pub sources: Vec<String>,
    /// Problems found while loading (shown in the log; bad entries skipped).
    pub warnings: Vec<String>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// The global content registry. Loads built-in content on first use if
/// `init` was not called (unit tests).
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| Registry::load(&[]))
}

/// Builds the registry from built-ins + `assets/content` + mods. Call once
/// at startup, before anything reads content.
pub fn init(roots: &[PathBuf]) -> &'static Registry {
    REGISTRY.get_or_init(|| Registry::load(roots))
}

/// Mod folders: `mods/*/` sorted by name.
pub fn mod_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("mods")
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

const BUILTIN_BLOCKS: &str = include_str!("../../assets/content/blocks.ron");
const BUILTIN_TEXTURES: &str = include_str!("../../assets/content/textures.ron");
const BUILTIN_ITEMS: &str = include_str!("../../assets/content/items.ron");
const BUILTIN_RECIPES: &str = include_str!("../../assets/content/recipes.ron");
const BUILTIN_JOURNAL: &str = include_str!("../../assets/content/journal.ron");

/// Keys of the built-in blocks in their fixed order (code refers to them by
/// index through `world::voxel::blocks`). Also the v1 save palette.
pub const CORE_BLOCK_KEYS: [&str; 24] = [
    "air",
    "grass",
    "dirt",
    "stone",
    "sand",
    "water",
    "log",
    "leaves",
    "iron_ore",
    "copper_ore",
    "coal_ore",
    "aether_crystal",
    "ancient_stone",
    "planks",
    "bricks",
    "glass",
    "metal_plate",
    "gravel",
    "bedrock",
    "tall_grass",
    "berry_bush",
    "clay",
    "moss_stone",
    "agitated_crystal",
];

fn parse<T: serde::de::DeserializeOwned>(
    text: &str,
    origin: &str,
    warnings: &mut Vec<String>,
) -> Vec<T> {
    match ron::from_str::<Vec<T>>(text) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!("{origin}: {e}"));
            Vec::new()
        }
    }
}

fn merge<T>(
    list: &mut Vec<T>,
    index: &mut HashMap<String, u16>,
    items: Vec<T>,
    key: impl Fn(&T) -> &str,
) {
    for it in items {
        let k = key(&it).to_string();
        if let Some(&i) = index.get(&k) {
            list[i as usize] = it;
        } else if list.len() < u16::MAX as usize {
            index.insert(k, list.len() as u16);
            list.push(it);
        }
    }
}

impl Registry {
    fn load(roots: &[PathBuf]) -> Registry {
        let mut r = Registry::default();
        let mut w = Vec::new();
        let mut layers: Vec<(String, [Option<String>; 5])> = vec![(
            "встроенный контент".into(),
            [
                Some(BUILTIN_BLOCKS.into()),
                Some(BUILTIN_TEXTURES.into()),
                Some(BUILTIN_ITEMS.into()),
                Some(BUILTIN_RECIPES.into()),
                Some(BUILTIN_JOURNAL.into()),
            ],
        )];
        for root in roots {
            let dir = root.join("content");
            let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
            let files = [
                read("blocks.ron"),
                read("textures.ron"),
                read("items.ron"),
                read("recipes.ron"),
                read("journal.ron"),
            ];
            if files.iter().any(Option::is_some) {
                layers.push((root.display().to_string(), files));
            }
        }
        for (origin, [blocks, textures, items, recipes, journal]) in layers {
            if let Some(t) = journal {
                let v: Vec<JournalDef> = parse(&t, &format!("{origin}/journal.ron"), &mut w);
                merge(&mut r.journal, &mut r.journal_index, v, |j| &j.key);
            }
            if let Some(t) = textures {
                let v: Vec<TextureDef> = parse(&t, &format!("{origin}/textures.ron"), &mut w);
                merge(&mut r.textures, &mut r.texture_index, v, |t| &t.name);
            }
            if let Some(t) = blocks {
                let v: Vec<BlockSpec> = parse(&t, &format!("{origin}/blocks.ron"), &mut w);
                let specs: Vec<BlockSpec> = v;
                // Blocks are resolved after all textures are known.
                let defs: Vec<BlockDef> = specs.into_iter().map(BlockDef::from_spec).collect();
                merge(&mut r.blocks, &mut r.block_index, defs, |b| &b.key);
            }
            if let Some(t) = items {
                let v: Vec<ItemDef> = parse(&t, &format!("{origin}/items.ron"), &mut w);
                merge(&mut r.items, &mut r.item_index, v, |i| &i.key);
            }
            if let Some(t) = recipes {
                // Recipes have no key: a later recipe with the same station,
                // output and input items replaces the earlier one.
                let v: Vec<RecipeDef> = parse(&t, &format!("{origin}/recipes.ron"), &mut w);
                for rec in v {
                    let same = |o: &RecipeDef| {
                        o.station == rec.station
                            && o.output.0 == rec.output.0
                            && o.inputs
                                .iter()
                                .map(|i| &i.0)
                                .eq(rec.inputs.iter().map(|i| &i.0))
                    };
                    if let Some(old) = r.recipes.iter_mut().find(|o| same(o)) {
                        *old = rec;
                    } else {
                        r.recipes.push(rec);
                    }
                }
            }
            r.sources.push(origin);
        }
        r.resolve(&mut w);
        for msg in &w {
            eprintln!("content warning: {msg}");
        }
        r.warnings = w;
        r
    }

    /// Resolves texture names to ids and validates cross references.
    fn resolve(&mut self, w: &mut Vec<String>) {
        for (i, k) in CORE_BLOCK_KEYS.iter().enumerate() {
            if self.blocks.get(i).map(|b| b.key.as_str()) != Some(*k) {
                w.push(format!("встроенный блок {k} должен иметь номер {i}"));
            }
        }
        for b in &mut self.blocks {
            for (slot, name) in b.texture_names.iter().enumerate() {
                b.tex[slot] = match self.texture_index.get(name) {
                    Some(&t) => TexId(t),
                    None => {
                        w.push(format!("блок {}: нет текстуры {name}", b.key));
                        TexId(0)
                    }
                };
            }
            if !b.drop.is_empty() && !self.item_index.contains_key(&b.drop) {
                w.push(format!("блок {}: неизвестный дроп {}", b.key, b.drop));
                b.drop.clear();
            }
        }
        for it in &self.items {
            match &it.use_ {
                ItemUse::Block(k) if !self.block_index.contains_key(k) => {
                    w.push(format!("предмет {}: нет блока {k}", it.key))
                }
                _ => {}
            }
        }
        for j in &self.journal {
            match &j.image {
                JournalImage::Texture(t) if !self.texture_index.contains_key(t) => {
                    w.push(format!("журнал {}: нет текстуры {t}", j.key))
                }
                JournalImage::Item(i) if !self.item_index.contains_key(i) => {
                    w.push(format!("журнал {}: нет предмета {i}", j.key))
                }
                _ => {}
            }
        }
        let items = &self.item_index;
        self.recipes.retain(|r| {
            let ok = r.inputs.iter().all(|(k, _)| items.contains_key(k))
                && items.contains_key(&r.output.0);
            if !ok {
                w.push(format!(
                    "рецепт {:?} ссылается на неизвестные предметы — пропущен",
                    r.output.0
                ));
            }
            ok
        });
    }

    pub fn block_id(&self, key: &str) -> Option<u16> {
        self.block_index.get(key).copied()
    }
    pub fn texture_id(&self, name: &str) -> Option<u16> {
        self.texture_index.get(name).copied()
    }
    pub fn item_id(&self, key: &str) -> Option<u16> {
        self.item_index.get(key).copied()
    }
}

/// Finds a content file in mods (last mod wins) then in `assets/`.
pub fn find_asset(relative: &str) -> Option<PathBuf> {
    for root in mod_roots().iter().rev() {
        let p = root.join(relative);
        if p.exists() {
            return Some(p);
        }
    }
    let p = Path::new("assets").join(relative);
    p.exists().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_loads_cleanly() {
        let r = Registry::load(&[]);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(r.blocks.len(), 24);
        assert_eq!(r.textures.len(), 25);
        assert_eq!(r.items.len(), 48);
        assert!(r.journal.len() >= 12);
        assert_eq!(r.recipes.len(), 28);
    }

    #[test]
    fn mod_overrides_and_extends() {
        let dir = std::env::temp_dir().join(format!("aw_mod_test_{}", std::process::id()));
        let content = dir.join("content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(
            content.join("blocks.ron"),
            r#"[
                (key: "stone", name: "Гранит", render: Cube, solid: true, hardness: Some(2.0), tier: 1,
                 textures: ("stone", "stone", "stone"), drop: "stone", glow: 0.0, aether: 0.0, aether_kind: None, info: ""),
                (key: "marble", name: "Мрамор", render: Cube, solid: true, hardness: Some(1.5), tier: 1,
                 textures: ("marble", "marble", "marble"), drop: "stone", glow: 0.0, aether: 0.0, aether_kind: None, info: "Новый блок"),
            ]"#,
        )
        .unwrap();
        std::fs::write(content.join("textures.ron"), r#"[(name: "marble")]"#).unwrap();
        std::fs::write(content.join("items.ron"), "[ broken").unwrap();
        let r = Registry::load(&[dir.clone()]);
        let stone = &r.blocks[r.block_id("stone").unwrap() as usize];
        assert_eq!(stone.name, "Гранит", "override keeps the index");
        assert_eq!(r.block_id("stone"), Some(3));
        assert_eq!(r.block_id("marble"), Some(24), "new block appended");
        assert!(
            r.warnings.iter().any(|w| w.contains("items.ron")),
            "broken file reported, not fatal"
        );
        assert_eq!(
            r.items.len(),
            Registry::load(&[]).items.len(),
            "broken mod file leaves base items intact"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
