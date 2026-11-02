//! Save file format (versioned JSON) and its validation.
//!
//! Only data the player changed is stored: modified chunks (RLE), free
//! objects, joints, drops, player and global settings. Terrain is
//! regenerated from the seed on load.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    core::states::GameMode,
    crafting::stations::SavedStorage,
    items::SavedStack,
    magic::energy::AetherKind,
    physics::{
        Prop,
        joints::{JointKind, LocalFrame},
    },
    tools::manipulator::Manipulator,
};

pub const FORMAT: &str = "aetherwright-save";
pub const SAVE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldMeta {
    pub seed: u32,
    pub chunk_size: i32,
    pub size_chunks: [i32; 3],
    pub sea_level: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSave {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub body_temp: f32,
    pub aether: f32,
    pub inventory: Vec<Option<SavedStack>>,
    pub selected: usize,
    pub manipulator: Manipulator,
    pub flying: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSave {
    pub coord: [i32; 3],
    pub rle: Vec<(u16, u8)>,
}

/// Behaviour state of a part (only fields that change at runtime).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<(bool, f32, bool)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aether: Option<(f32, AetherKind)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<SavedStorage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropSave {
    pub id: u32,
    pub prop: Prop,
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub frozen: bool,
    #[serde(default)]
    pub state: PartState,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BodyRef {
    Prop(u32),
    /// Anchored to terrain (chunk coordinate).
    Chunk([i32; 3]),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointSave {
    pub a: BodyRef,
    pub b: BodyRef,
    pub kind: JointKind,
    pub frame_a: LocalFrame,
    pub frame_b: LocalFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropSave {
    pub item: SavedStack,
    pub position: Vec3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFile {
    pub format: String,
    pub version: u32,
    pub world: WorldMeta,
    pub game_mode: GameMode,
    pub time_of_day: f32,
    pub gravity: f32,
    pub player: PlayerSave,
    pub chunks: Vec<ChunkSave>,
    pub props: Vec<PropSave>,
    pub joints: Vec<JointSave>,
    pub drops: Vec<DropSave>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    Io(String),
    Corrupt(String),
    WrongFormat,
    TooNew(u32),
    Invalid(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "не удалось прочитать файл: {e}"),
            LoadError::Corrupt(e) => write!(f, "файл повреждён: {e}"),
            LoadError::WrongFormat => write!(f, "это не сохранение Aetherwright"),
            LoadError::TooNew(v) => write!(
                f,
                "сохранение версии {v} новее игры (поддерживается {SAVE_VERSION})"
            ),
            LoadError::Invalid(e) => write!(f, "некорректные данные: {e}"),
        }
    }
}

/// Parses and validates a save. Older versions would be migrated here.
pub fn parse(text: &str) -> Result<SaveFile, LoadError> {
    // Peek at the header first so a newer version gives a clear error even
    // if the body changed shape.
    #[derive(Deserialize)]
    struct Header {
        format: String,
        version: u32,
    }
    let header: Header =
        serde_json::from_str(text).map_err(|e| LoadError::Corrupt(e.to_string()))?;
    if header.format != FORMAT {
        return Err(LoadError::WrongFormat);
    }
    if header.version > SAVE_VERSION {
        return Err(LoadError::TooNew(header.version));
    }
    let save: SaveFile =
        serde_json::from_str(text).map_err(|e| LoadError::Corrupt(e.to_string()))?;
    validate(&save)?;
    Ok(save)
}

fn validate(s: &SaveFile) -> Result<(), LoadError> {
    let w = &s.world;
    if !(8..=32).contains(&w.chunk_size) || w.size_chunks.iter().any(|c| !(1..=64).contains(c)) {
        return Err(LoadError::Invalid("размеры мира".into()));
    }
    if !s.player.position.is_finite() {
        return Err(LoadError::Invalid("позиция игрока".into()));
    }
    let ids: std::collections::HashSet<u32> = s.props.iter().map(|p| p.id).collect();
    if ids.len() != s.props.len() {
        return Err(LoadError::Invalid("повторяющиеся id объектов".into()));
    }
    for j in &s.joints {
        for r in [j.a, j.b] {
            if let BodyRef::Prop(id) = r
                && !ids.contains(&id)
            {
                return Err(LoadError::Invalid(format!(
                    "соединение ссылается на несуществующий объект {id}"
                )));
            }
        }
    }
    Ok(())
}

pub fn to_json(s: &SaveFile) -> String {
    serde_json::to_string(s).expect("save data is always serializable")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn sample() -> SaveFile {
        SaveFile {
            format: FORMAT.into(),
            version: SAVE_VERSION,
            world: WorldMeta {
                seed: 7,
                chunk_size: 16,
                size_chunks: [4, 3, 4],
                sea_level: 10,
            },
            game_mode: GameMode::Creative,
            time_of_day: 0.4,
            gravity: 19.0,
            player: PlayerSave {
                position: Vec3::new(1.0, 2.0, 3.0),
                yaw: 0.5,
                pitch: -0.1,
                health: 80.0,
                hunger: 50.0,
                thirst: 40.0,
                body_temp: 36.8,
                aether: 30.0,
                inventory: vec![
                    Some(SavedStack {
                        item: "stone".into(),
                        count: 5,
                    }),
                    None,
                ],
                selected: 1,
                manipulator: Manipulator::default(),
                flying: false,
            },
            chunks: vec![ChunkSave {
                coord: [0, 0, 0],
                rle: vec![(4096, 3)],
            }],
            props: vec![PropSave {
                id: 0,
                prop: Prop::Part("engine".into()),
                position: Vec3::ONE,
                rotation: Quat::IDENTITY,
                linear_velocity: Vec3::ZERO,
                angular_velocity: Vec3::ZERO,
                frozen: true,
                state: PartState {
                    engine: Some((true, 0.5, false)),
                    ..default()
                },
            }],
            joints: vec![JointSave {
                a: BodyRef::Chunk([0, 0, 0]),
                b: BodyRef::Prop(0),
                kind: JointKind::Hinge,
                frame_a: LocalFrame {
                    anchor: Vec3::ZERO,
                    basis: Quat::IDENTITY,
                },
                frame_b: LocalFrame {
                    anchor: Vec3::X,
                    basis: Quat::IDENTITY,
                },
            }],
            drops: vec![],
        }
    }

    #[test]
    fn roundtrip() {
        let s = sample();
        let back = parse(&to_json(&s)).unwrap();
        assert_eq!(back.world, s.world);
        assert_eq!(back.player.inventory, s.player.inventory);
        assert_eq!(back.props[0].state, s.props[0].state);
        assert_eq!(back.joints[0].b, BodyRef::Prop(0));
    }

    #[test]
    fn corrupt_and_incompatible_files_are_rejected() {
        assert!(matches!(parse("{ not json"), Err(LoadError::Corrupt(_))));
        assert!(matches!(
            parse(r#"{"format":"other","version":1}"#),
            Err(LoadError::WrongFormat)
        ));
        let mut s = sample();
        s.version = SAVE_VERSION + 1;
        assert!(matches!(parse(&to_json(&s)), Err(LoadError::TooNew(_))));
        let mut s = sample();
        s.joints[0].b = BodyRef::Prop(99);
        assert!(matches!(parse(&to_json(&s)), Err(LoadError::Invalid(_))));
        let text = to_json(&sample());
        assert!(parse(&text[..text.len() / 2]).is_err(), "truncated file");
    }
}
