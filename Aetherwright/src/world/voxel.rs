//! Block types. Block data is a static table indexed by `BlockId`; gameplay
//! code queries properties instead of matching on ids.

use serde::{Deserialize, Serialize};

/// Compact block identifier stored in chunks (1 byte per voxel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockId(pub u8);

/// How a block is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderKind {
    Invisible,
    /// Opaque cube; culls neighbour faces.
    Cube,
    /// See-through cube (glass); faces between same blocks are culled.
    Transparent,
    /// Liquid surface; drawn translucent, slightly lowered top.
    Liquid,
    /// Two crossed quads (plants).
    Cross,
}

/// Texture slots for procedural block textures (see `materials`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Tex {
    Grass,
    GrassSide,
    Dirt,
    Stone,
    Sand,
    Water,
    LogSide,
    LogTop,
    Leaves,
    IronOre,
    CopperOre,
    CoalOre,
    Aether,
    Ancient,
    Planks,
    Bricks,
    Glass,
    Metal,
    Gravel,
    Bedrock,
    TallGrass,
    Berry,
    Clay,
    Moss,
    AgitatedAether,
}

impl Tex {
    pub const COUNT: usize = Tex::AgitatedAether as usize + 1;
    pub fn all() -> impl Iterator<Item = Tex> {
        (0..Self::COUNT as u8).map(|i| tex_from_index(i))
    }
}

fn tex_from_index(i: u8) -> Tex {
    // `i` is always < COUNT (callers iterate 0..COUNT).
    use Tex::*;
    const ALL: [Tex; Tex::COUNT] = [
        Grass, GrassSide, Dirt, Stone, Sand, Water, LogSide, LogTop, Leaves, IronOre, CopperOre, CoalOre,
        Aether, Ancient, Planks, Bricks, Glass, Metal, Gravel, Bedrock, TallGrass, Berry, Clay, Moss,
        AgitatedAether,
    ];
    ALL[i as usize]
}

pub struct BlockDef {
    pub key: &'static str,
    pub name: &'static str,
    pub render: RenderKind,
    /// Participates in collision.
    pub solid: bool,
    /// Seconds to mine with the base manipulator (tier 1). `f32::INFINITY` = unbreakable.
    pub hardness: f32,
    /// Minimum manipulator mining tier.
    pub tier: u8,
    /// Textures for +Y, sides, −Y faces.
    pub tex: [Tex; 3],
    /// Item key dropped when mined ("" = nothing). Resolved by the item registry.
    pub drop: &'static str,
    /// Light emission strength for magical blocks (0 = none).
    pub glow: f32,
    /// Ambient magic emitted (aether units/s) — read by the magic system.
    pub aether: f32,
    /// Short scanner description.
    pub info: &'static str,
}

macro_rules! b {
    ($key:literal, $name:literal, $render:ident, $solid:expr, $hard:expr, $tier:expr, [$t:ident, $s:ident, $bt:ident], $drop:literal, $glow:expr, $aether:expr, $info:literal) => {
        BlockDef {
            key: $key,
            name: $name,
            render: RenderKind::$render,
            solid: $solid,
            hardness: $hard,
            tier: $tier,
            tex: [Tex::$t, Tex::$s, Tex::$bt],
            drop: $drop,
            glow: $glow,
            aether: $aether,
            info: $info,
        }
    };
}

pub mod blocks {
    use super::BlockId;
    pub const AIR: BlockId = BlockId(0);
    pub const GRASS: BlockId = BlockId(1);
    pub const DIRT: BlockId = BlockId(2);
    pub const STONE: BlockId = BlockId(3);
    pub const SAND: BlockId = BlockId(4);
    pub const WATER: BlockId = BlockId(5);
    pub const LOG: BlockId = BlockId(6);
    pub const LEAVES: BlockId = BlockId(7);
    pub const IRON_ORE: BlockId = BlockId(8);
    pub const COPPER_ORE: BlockId = BlockId(9);
    pub const COAL_ORE: BlockId = BlockId(10);
    pub const AETHER_CRYSTAL: BlockId = BlockId(11);
    pub const ANCIENT_STONE: BlockId = BlockId(12);
    pub const PLANKS: BlockId = BlockId(13);
    pub const BRICKS: BlockId = BlockId(14);
    pub const GLASS: BlockId = BlockId(15);
    pub const METAL_PLATE: BlockId = BlockId(16);
    pub const GRAVEL: BlockId = BlockId(17);
    pub const BEDROCK: BlockId = BlockId(18);
    pub const TALL_GRASS: BlockId = BlockId(19);
    pub const BERRY_BUSH: BlockId = BlockId(20);
    pub const CLAY: BlockId = BlockId(21);
    pub const MOSS_STONE: BlockId = BlockId(22);
    pub const AGITATED_CRYSTAL: BlockId = BlockId(23);
}

pub static BLOCKS: [BlockDef; 24] = [
    b!("air", "Воздух", Invisible, false, 0.0, 0, [Dirt, Dirt, Dirt], "", 0.0, 0.0, ""),
    b!("grass", "Трава", Cube, true, 0.45, 1, [Grass, GrassSide, Dirt], "dirt", 0.0, 0.0, "Плодородный слой почвы."),
    b!("dirt", "Земля", Cube, true, 0.4, 1, [Dirt, Dirt, Dirt], "dirt", 0.0, 0.0, "Мягкий грунт."),
    b!("stone", "Камень", Cube, true, 1.1, 1, [Stone, Stone, Stone], "stone", 0.0, 0.0, "Плотная горная порода."),
    b!("sand", "Песок", Cube, true, 0.35, 1, [Sand, Sand, Sand], "sand", 0.0, 0.0, "Сыпучий кварцевый песок. Переплавляется в стекло."),
    b!("water", "Вода", Liquid, false, f32::INFINITY, 255, [Water, Water, Water], "", 0.0, 0.0, "Пресная вода. Можно пить."),
    b!("log", "Бревно", Cube, true, 0.9, 1, [LogTop, LogSide, LogTop], "wood", 0.0, 0.0, "Ствол дерева."),
    b!("leaves", "Листва", Cube, true, 0.15, 1, [Leaves, Leaves, Leaves], "fiber", 0.0, 0.0, "Густая листва. Даёт волокно."),
    b!("iron_ore", "Железная руда", Cube, true, 1.8, 1, [IronOre, IronOre, IronOre], "iron_ore", 0.0, 0.0, "Руда железа. Переплавляется в металл."),
    b!("copper_ore", "Медная руда", Cube, true, 1.5, 1, [CopperOre, CopperOre, CopperOre], "copper_ore", 0.0, 0.0, "Руда меди — проводник энергии."),
    b!("coal_ore", "Уголь", Cube, true, 1.2, 1, [CoalOre, CoalOre, CoalOre], "coal", 0.0, 0.0, "Ископаемое топливо."),
    b!("aether_crystal", "Кристалл эфира", Cube, true, 1.6, 1, [Aether, Aether, Aether], "aether_shard", 3.0, 1.5, "Кристаллизованная спокойная магия. Излучает эфир."),
    b!("ancient_stone", "Древний камень", Cube, true, 3.0, 2, [Ancient, Ancient, Ancient], "ancient_stone", 0.6, 0.4, "Камень, пропитанный древней магией. Нужен модуль бурения II."),
    b!("planks", "Доски", Cube, true, 0.6, 1, [Planks, Planks, Planks], "planks", 0.0, 0.0, "Обработанная древесина."),
    b!("bricks", "Каменный кирпич", Cube, true, 1.4, 1, [Bricks, Bricks, Bricks], "bricks", 0.0, 0.0, "Строительный камень."),
    b!("glass", "Стекло", Transparent, true, 0.3, 1, [Glass, Glass, Glass], "glass", 0.0, 0.0, "Прозрачное стекло."),
    b!("metal_plate", "Металлическая плита", Cube, true, 2.0, 1, [Metal, Metal, Metal], "metal_plate", 0.0, 0.0, "Прочная металлическая обшивка."),
    b!("gravel", "Гравий", Cube, true, 0.5, 1, [Gravel, Gravel, Gravel], "gravel", 0.0, 0.0, "Мелкий камень."),
    b!("bedrock", "Коренная порода", Cube, true, f32::INFINITY, 255, [Bedrock, Bedrock, Bedrock], "", 0.0, 0.0, "Неразрушимое основание мира."),
    b!("tall_grass", "Высокая трава", Cross, false, 0.05, 1, [TallGrass, TallGrass, TallGrass], "fiber", 0.0, 0.0, "Трава. Даёт волокно."),
    b!("berry_bush", "Ягодный куст", Cross, false, 0.1, 1, [Berry, Berry, Berry], "berries", 0.0, 0.0, "Съедобные ягоды."),
    b!("clay", "Глина", Cube, true, 0.5, 1, [Clay, Clay, Clay], "clay", 0.0, 0.0, "Пластичная глина у воды."),
    b!("moss_stone", "Мшистый камень", Cube, true, 1.1, 1, [Moss, Moss, Stone], "stone", 0.0, 0.0, "Камень, поросший мхом."),
    b!("agitated_crystal", "Раздражённый кристалл", Cube, true, 2.2, 1, [AgitatedAether, AgitatedAether, AgitatedAether], "agitated_shard", 3.5, 3.0, "Нестабильная раздражённая магия. Толкает предметы вокруг."),
];

impl BlockId {
    #[inline]
    pub fn def(self) -> &'static BlockDef {
        BLOCKS.get(self.0 as usize).unwrap_or(&BLOCKS[0])
    }
    #[inline]
    pub fn is_air(self) -> bool {
        self.0 == 0
    }
    /// Hides faces of neighbouring blocks.
    #[inline]
    pub fn is_opaque(self) -> bool {
        self.def().render == RenderKind::Cube
    }
    #[inline]
    pub fn is_solid(self) -> bool {
        self.def().solid
    }
    #[inline]
    pub fn is_liquid(self) -> bool {
        self.def().render == RenderKind::Liquid
    }
    /// Can be hit by the block-targeting ray.
    #[inline]
    pub fn is_targetable(self) -> bool {
        !matches!(self.def().render, RenderKind::Invisible | RenderKind::Liquid)
    }
    /// Replaceable by placement (air, water, plants).
    #[inline]
    pub fn is_replaceable(self) -> bool {
        matches!(self.def().render, RenderKind::Invisible | RenderKind::Liquid | RenderKind::Cross)
    }
    pub fn from_key(key: &str) -> Option<BlockId> {
        BLOCKS.iter().position(|b| b.key == key).map(|i| BlockId(i as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_table() {
        assert_eq!(blocks::AGITATED_CRYSTAL.def().key, "agitated_crystal");
        assert_eq!(blocks::BEDROCK.def().key, "bedrock");
        assert_eq!(BlockId::from_key("stone"), Some(blocks::STONE));
        assert_eq!(Tex::all().count(), Tex::COUNT);
    }

    #[test]
    fn keys_unique() {
        let mut keys: Vec<_> = BLOCKS.iter().map(|b| b.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), BLOCKS.len());
    }
}
