//! Blueprint library on disk: named blueprints with numbered versions.
//! Blocks are stored by key (like saves), so mods may change block order.
//! File: `saves/blueprints.json` (written atomically, previous kept as .bak).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::blueprints::{Blueprint, BlueprintJoint, BlueprintPart};
use crate::world::voxel::BlockId;

pub const LIBRARY_PATH: &str = "saves/blueprints.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredBlueprint {
    pub size: IVec3,
    pub blocks: Vec<(IVec3, String)>,
    pub parts: Vec<BlueprintPart>,
    pub joints: Vec<BlueprintJoint>,
}

impl StoredBlueprint {
    pub fn from_blueprint(b: &Blueprint) -> Self {
        Self {
            size: b.size,
            blocks: b.blocks.iter().map(|(p, id)| (*p, id.def().key.clone())).collect(),
            parts: b.parts.clone(),
            joints: b.joints.clone(),
        }
    }
    /// Unknown block keys (removed mods) are dropped.
    pub fn to_blueprint(&self) -> Blueprint {
        Blueprint {
            size: self.size,
            blocks: self.blocks.iter().filter_map(|(p, k)| BlockId::from_key(k).map(|b| (*p, b))).collect(),
            parts: self.parts.clone(),
            joints: self.joints.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintVersion {
    pub version: u32,
    pub data: StoredBlueprint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintEntry {
    pub name: String,
    pub versions: Vec<BlueprintVersion>,
}

impl BlueprintEntry {
    pub fn latest(&self) -> Option<&BlueprintVersion> {
        self.versions.last()
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlueprintLibrary {
    pub entries: Vec<BlueprintEntry>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl BlueprintLibrary {
    pub fn load(path: &Path) -> Self {
        let mut lib: BlueprintLibrary = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).map_err(|e| warn!("blueprints: {e}")).ok())
            .unwrap_or_default();
        lib.path = Some(path.to_path_buf());
        lib
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() {
            let _ = std::fs::copy(path, path.with_extension("json.bak"));
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, path)
    }

    fn unique_name(&self, base: &str) -> String {
        if !self.entries.iter().any(|e| e.name == base) {
            return base.to_string();
        }
        (2..).map(|n| format!("{base} ({n})")).find(|n| !self.entries.iter().any(|e| &e.name == n)).unwrap()
    }

    /// New entry with version 1. Returns its index.
    pub fn add(&mut self, base: &str, bp: &Blueprint) -> usize {
        let name = self.unique_name(base);
        self.entries.push(BlueprintEntry {
            name,
            versions: vec![BlueprintVersion { version: 1, data: StoredBlueprint::from_blueprint(bp) }],
        });
        self.entries.len() - 1
    }

    /// Appends a version to an entry. Returns the new version number.
    pub fn add_version(&mut self, entry: usize, bp: &Blueprint) -> Option<u32> {
        let e = self.entries.get_mut(entry)?;
        let v = e.versions.iter().map(|v| v.version).max().unwrap_or(0) + 1;
        e.versions.push(BlueprintVersion { version: v, data: StoredBlueprint::from_blueprint(bp) });
        Some(v)
    }

    /// Copies an entry (all versions) under a new name.
    pub fn duplicate(&mut self, entry: usize) -> Option<usize> {
        let e = self.entries.get(entry)?.clone();
        let name = self.unique_name(&format!("{} — копия", e.name));
        self.entries.push(BlueprintEntry { name, ..e });
        Some(self.entries.len() - 1)
    }

    /// Removes one version; removes the entry when it was the last one.
    pub fn remove_version(&mut self, entry: usize, version: u32) {
        if let Some(e) = self.entries.get_mut(entry) {
            e.versions.retain(|v| v.version != version);
            if e.versions.is_empty() {
                self.entries.remove(entry);
            }
        }
    }

    pub fn rename(&mut self, entry: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let unique = self.unique_name(name);
        if let Some(e) = self.entries.get_mut(entry) {
            e.name = unique;
        }
    }
}

fn load_library(mut commands: Commands) {
    commands.insert_resource(BlueprintLibrary::load(Path::new(LIBRARY_PATH)));
}

pub struct LibraryPlugin;

impl Plugin for LibraryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlueprintLibrary>().add_systems(Startup, load_library);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::voxel::blocks;

    #[test]
    fn versions_copy_rename_roundtrip() {
        let bp = Blueprint {
            size: IVec3::new(2, 1, 1),
            blocks: vec![(IVec3::ZERO, blocks::STONE), (IVec3::X, blocks::PLANKS)],
            parts: vec![],
            joints: vec![],
        };
        let mut lib = BlueprintLibrary::default();
        let a = lib.add("Мост", &bp);
        assert_eq!(lib.add("Мост", &bp), 1);
        assert_eq!(lib.entries[1].name, "Мост (2)");
        assert_eq!(lib.add_version(a, &bp), Some(2));
        let c = lib.duplicate(a).unwrap();
        assert_eq!(lib.entries[c].versions.len(), 2);
        lib.rename(c, "Мост");
        assert_eq!(lib.entries[c].name, "Мост (3)");
        lib.remove_version(a, 1);
        assert_eq!(lib.entries[a].versions[0].version, 2);
        assert_eq!(lib.entries[a].latest().unwrap().data.to_blueprint(), bp);
        let json = serde_json::to_string(&lib).unwrap();
        let back: BlueprintLibrary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries, lib.entries);
    }
}
