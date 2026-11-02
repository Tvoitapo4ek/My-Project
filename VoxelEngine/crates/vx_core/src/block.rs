use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Номер блока в реестре. `0` — всегда воздух.
pub type BlockId = u16;
pub const AIR: BlockId = 0;

/// Грани куба. Порядок общий для текстур, мешинга и нормалей.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Face {
    pub const ALL: [Face; 6] = [Face::PosX, Face::NegX, Face::PosY, Face::NegY, Face::PosZ, Face::NegZ];

    pub fn normal(self) -> glam::IVec3 {
        use glam::IVec3 as V;
        match self {
            Face::PosX => V::X,
            Face::NegX => V::NEG_X,
            Face::PosY => V::Y,
            Face::NegY => V::NEG_Y,
            Face::PosZ => V::Z,
            Face::NegZ => V::NEG_Z,
        }
    }
}

/// Описание блока. Все поля, кроме `name`, необязательны в TOML.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDef {
    /// Внутреннее имя (используется в файлах мира и структур).
    pub name: String,
    /// Имя для интерфейса. По умолчанию — `name`.
    #[serde(default)]
    title: Option<String>,
    /// Текстура для всех граней (имя файла в `assets/textures` без `.png`).
    #[serde(default)]
    texture: Option<String>,
    #[serde(default)]
    top: Option<String>,
    #[serde(default)]
    bottom: Option<String>,
    #[serde(default)]
    side: Option<String>,
    /// Участвует в столкновениях.
    #[serde(default = "yes")]
    pub solid: bool,
    /// Полностью закрывает соседние грани (стекло и листва — `false`).
    #[serde(default = "yes")]
    pub opaque: bool,
    /// Масса одного блока.
    #[serde(default = "one")]
    pub density: f32,
    /// Подъёмная сила (воздушный шар, как в Create Aeronautics).
    #[serde(default)]
    pub lift: f32,
    /// Блок рельефа: не захватывается при сборке конструкций.
    #[serde(default)]
    pub terrain: bool,
    /// Произвольные свойства для будущих модулей: `[block.props] thrust = 10`.
    #[serde(default)]
    pub props: toml::Table,

    /// Номер слоя текстуры для каждой грани (заполняется реестром).
    #[serde(skip)]
    pub faces: [u32; 6],
}

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}

impl BlockDef {
    fn air() -> Self {
        BlockDef {
            name: "air".into(),
            title: None,
            texture: None,
            top: None,
            bottom: None,
            side: None,
            solid: false,
            opaque: false,
            density: 0.0,
            lift: 0.0,
            terrain: false,
            props: Default::default(),
            faces: [0; 6],
        }
    }

    /// Имя для показа игроку.
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.name)
    }

    /// Имена текстур граней в порядке [`Face::ALL`].
    fn face_textures(&self) -> [String; 6] {
        let base = self.texture.clone().unwrap_or_else(|| self.name.clone());
        let side = self.side.clone().unwrap_or_else(|| base.clone());
        let top = self.top.clone().unwrap_or_else(|| base.clone());
        let bottom = self.bottom.clone().unwrap_or(base);
        [side.clone(), side.clone(), top, bottom, side.clone(), side]
    }
}

/// Реестр блоков и список текстур (индекс = слой текстурного массива).
pub struct Registry {
    blocks: Vec<BlockDef>,
    by_name: HashMap<String, BlockId>,
    textures: Vec<String>,
}

#[derive(Deserialize)]
struct BlocksFile {
    block: Vec<BlockDef>,
}

impl Registry {
    /// Реестр из TOML-текста (`[[block]] name = "stone" ...`).
    pub fn from_toml(text: &str) -> Result<Self> {
        let file: BlocksFile = toml::from_str(text).context("blocks.toml")?;
        let mut reg = Registry { blocks: vec![], by_name: HashMap::new(), textures: vec![] };
        reg.add(BlockDef::air())?;
        for def in file.block {
            reg.add(def)?;
        }
        Ok(reg)
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
        Self::from_toml(&text)
    }

    /// Добавить блок (можно и из кода — например, из модуля-плагина).
    pub fn add(&mut self, mut def: BlockDef) -> Result<BlockId> {
        if self.by_name.contains_key(&def.name) {
            bail!("блок `{}` объявлен дважды", def.name);
        }
        if def.name != "air" {
            for (i, tex) in def.face_textures().into_iter().enumerate() {
                def.faces[i] = self.texture_index(tex);
            }
        }
        let id = BlockId::try_from(self.blocks.len()).context("слишком много блоков")?;
        self.by_name.insert(def.name.clone(), id);
        self.blocks.push(def);
        Ok(id)
    }

    fn texture_index(&mut self, name: String) -> u32 {
        match self.textures.iter().position(|t| *t == name) {
            Some(i) => i as u32,
            None => {
                self.textures.push(name);
                (self.textures.len() - 1) as u32
            }
        }
    }

    pub fn get(&self, id: BlockId) -> &BlockDef {
        &self.blocks[id as usize]
    }

    pub fn id(&self, name: &str) -> Option<BlockId> {
        self.by_name.get(name).copied()
    }

    /// Как [`Registry::id`], но с понятной ошибкой.
    pub fn expect(&self, name: &str) -> Result<BlockId> {
        self.id(name).with_context(|| format!("неизвестный блок `{name}`"))
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.len() <= 1
    }

    /// Все блоки, кроме воздуха.
    pub fn iter(&self) -> impl Iterator<Item = (BlockId, &BlockDef)> {
        self.blocks.iter().enumerate().skip(1).map(|(i, b)| (i as BlockId, b))
    }

    /// Имена текстур; индекс в этом списке = слой в текстурном массиве.
    pub fn textures(&self) -> &[String] {
        &self.textures
    }

    pub fn solid(&self, id: BlockId) -> bool {
        self.blocks[id as usize].solid
    }

    pub fn opaque(&self, id: BlockId) -> bool {
        self.blocks[id as usize].opaque
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_textures() {
        let reg = Registry::from_toml(
            r#"
            [[block]]
            name = "grass"
            top = "grass_top"
            bottom = "dirt"
            side = "grass_side"
            [[block]]
            name = "dirt"
            [[block]]
            name = "balloon"
            lift = 30
            [block.props]
            color = "red"
            "#,
        )
        .unwrap();
        let grass = reg.get(reg.id("grass").unwrap());
        let dirt = reg.get(reg.id("dirt").unwrap());
        assert_eq!(grass.faces[Face::NegY as usize], dirt.faces[0]);
        assert_eq!(reg.textures().len(), 4);
        assert_eq!(reg.get(reg.id("balloon").unwrap()).lift, 30.0);
        assert!(!reg.solid(AIR));
    }
}
