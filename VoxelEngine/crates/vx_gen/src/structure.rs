use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result, bail};
use glam::IVec3;
use serde::Deserialize;
use vx_core::{AIR, BlockId, Grid, Registry};

/// Структура — шаблон блоков из файла `assets/structures/<имя>.toml`.
///
/// ```toml
/// anchor = [1, 0, 1]                    # клетка шаблона, которая встанет в точку размещения
/// palette = { L = "log", "#" = "planks" }  # символ → блок; '.' и ' ' — пропуск, '_' — воздух
/// layers = [                            # слои снизу вверх; строки — по Z, символы — по X
///   ["...", ".L.", "..."],
/// ]
/// ```
pub struct Structure {
    blocks: Vec<(IVec3, BlockId)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructureFile {
    #[serde(default)]
    anchor: [i32; 3],
    palette: HashMap<String, String>,
    layers: Vec<Vec<String>>,
}

impl Structure {
    pub fn from_toml(text: &str, reg: &Registry) -> Result<Self> {
        let file: StructureFile = toml::from_str(text)?;
        let mut palette = HashMap::from([('_', AIR)]);
        for (k, v) in &file.palette {
            let mut chars = k.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                bail!("ключ палитры `{k}` — не один символ")
            };
            palette.insert(c, reg.expect(v)?);
        }
        let anchor = IVec3::from_array(file.anchor);
        let mut blocks = vec![];
        for (y, layer) in file.layers.iter().enumerate() {
            for (z, row) in layer.iter().enumerate() {
                for (x, c) in row.chars().enumerate() {
                    if c == '.' || c == ' ' {
                        continue;
                    }
                    let id = *palette.get(&c).with_context(|| format!("символ `{c}` нет в палитре"))?;
                    blocks.push((IVec3::new(x as i32, y as i32, z as i32) - anchor, id));
                }
            }
        }
        Ok(Structure { blocks })
    }

    /// Загрузить все `*.toml` из папки; ключ — имя файла.
    pub fn load_dir(dir: &Path, reg: &Registry) -> Result<HashMap<String, Structure>> {
        let mut out = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else { return Ok(out) };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "toml") {
                let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&path)?;
                let s = Structure::from_toml(&text, reg).with_context(|| format!("{}", path.display()))?;
                log::info!("структура `{name}`: {} блоков", s.blocks.len());
                out.insert(name, s);
            }
        }
        Ok(out)
    }

    /// Поставить структуру в сетку так, чтобы якорь оказался в `at`.
    pub fn place(&self, grid: &mut Grid, at: IVec3) {
        for &(p, id) in &self.blocks {
            grid.set(at + p, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_place() {
        let reg = Registry::from_toml("[[block]]\nname='log'\n[[block]]\nname='leaves'").unwrap();
        let s = Structure::from_toml(
            r#"
            anchor = [1, 0, 1]
            palette = { L = "log", X = "leaves" }
            layers = [["...", ".L.", "..."], ["XXX", "XLX", "XXX"]]
            "#,
            &reg,
        )
        .unwrap();
        let mut g = Grid::new(vx_core::GridKind::Static);
        s.place(&mut g, IVec3::new(10, 4, 10));
        assert_eq!(g.get(IVec3::new(10, 4, 10)), reg.id("log").unwrap());
        assert_eq!(g.get(IVec3::new(9, 5, 9)), reg.id("leaves").unwrap());
        assert_eq!(g.blocks().count(), 10);
    }
}
