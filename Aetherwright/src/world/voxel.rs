//! Block ids. Block *data* lives in the content registry (`content`,
//! `assets/content/blocks.ron`, mods); this module gives the compact id
//! stored in chunks and convenience queries.

use serde::{Deserialize, Serialize};

pub use crate::content::{BlockDef, RenderKind, TexId};

/// Compact block identifier stored in chunks (2 bytes per voxel; index
/// into the content registry). Saves store a key palette, never raw ids.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct BlockId(pub u16);

/// Built-in blocks referenced by engine code (world generation, physics).
/// Their order is fixed (`content::CORE_BLOCK_KEYS`, checked at load).
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

impl BlockId {
    #[inline]
    pub fn def(self) -> &'static BlockDef {
        let r = crate::content::registry();
        r.blocks.get(self.0 as usize).unwrap_or(&r.blocks[0])
    }
    /// Number of registered block types.
    pub fn count() -> usize {
        crate::content::registry().blocks.len()
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
        !matches!(
            self.def().render,
            RenderKind::Invisible | RenderKind::Liquid
        )
    }
    /// Replaceable by placement (air, water, plants).
    #[inline]
    pub fn is_replaceable(self) -> bool {
        matches!(
            self.def().render,
            RenderKind::Invisible | RenderKind::Liquid | RenderKind::Cross
        )
    }
    pub fn from_key(key: &str) -> Option<BlockId> {
        crate::content::registry().block_id(key).map(BlockId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_ids_match_content() {
        for (i, k) in crate::content::CORE_BLOCK_KEYS.iter().enumerate() {
            assert_eq!(BlockId(i as u16).def().key, *k);
        }
        assert_eq!(blocks::AGITATED_CRYSTAL.def().key, "agitated_crystal");
        assert_eq!(BlockId::from_key("stone"), Some(blocks::STONE));
    }
}
