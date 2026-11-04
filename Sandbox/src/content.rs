//! Content registry: categories, blocks and tools come from
//! `assets/content/*.ron` (human-editable, with comments). Code refers to
//! content by string key; `BlockId` / `ItemId` are indices valid for one
//! run. Adding a block = a line in `blocks.ron` (+ a texture PNG, or a model
//! in `mech::model` for mechanisms).

use std::{collections::HashMap, sync::OnceLock};

use serde::Deserialize;

/// Behaviour of a block beyond being a solid cube.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Mech {
    /// Creative motor: always running, speed and direction set by the wrench.
    Motor,
    Shaft,
    Cog,
    LargeCog,
    /// Turns rotation by 90° (four ports around its axis).
    Gearbox,
    /// Shaft that disconnects while it gets a signal.
    Clutch,
    /// Shaft that reverses its output side while it gets a signal.
    Gearshift,
    /// Outputs its own speed (wrench), blended by a signal.
    SpeedController,
    /// Holds a wheel on its axle side; steers by a signal.
    Suspension,
    Wheel,
    Seat,
    ControlPanel,
    /// Receives a control-panel channel and passes it to the block it is
    /// attached to.
    Sensor,
    /// Gives physics to itself and everything glued to it, and takes it back.
    PhysicsToggle,
    /// Measures the speed of the contraption it is on (km/h).
    Speedometer,
    /// Reads a value from the block it is attached to and sends it to a
    /// linked mechanical display.
    TelemetrySensor,
    /// Shows a value (digits or a needle); powered by rotation from the side.
    MechanicalDisplay,
    /// A lamp shining along its facing; a signal press switches it on/off.
    Headlight,
    /// Turns the glued structure on its face (a separate body on a hinge)
    /// with the shaft speed from its back; a signal holds a set angle.
    Bearing,
    /// Pushes / pulls the glued structure in front of it: one block per
    /// turn of the shaft at its back, up to its stroke.
    Piston,
    /// Extension pole left by an extended piston (placed automatically).
    PistonPole,
    /// Multiblock fluid tank (16 000 mB per block).
    FluidTank,
    /// Pipe: joins fluid blocks into a pipe network.
    Pipe,
    /// A pipe in a copper casing.
    CasedPipe,
    /// A shaft in a steel casing.
    CasedShaft,
    /// Moves fluid between pipe networks, powered by rotation.
    Pump,
    /// Filling port: RMB with a bucket pours fluid in.
    FluidPort,
    /// Burns diesel, gives rotation; needs air at its exhaust.
    DieselEngine,
    /// Heavy disc: covers short overloads with its stored energy.
    Flywheel,
    /// Logic (`logic`): a wire on a block face, coloured.
    Wire,
    /// Short pulse when pressed (RMB).
    Button,
    /// On / off, flipped with RMB.
    Lever,
    /// Pulse every «Интервал» seconds.
    Timer,
    /// Short pulse on the rising edge of its input.
    PulseGenerator,
    /// AND / OR / NOT / RS latch.
    LogicGate,
    /// Signal lamp under a glass dome, blinks.
    Flasher,
}

/// How a block's orientation is chosen when placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Placement {
    /// Not oriented.
    #[default]
    None,
    /// Along the normal of the clicked face (shafts, cogs, motors…).
    Face,
    /// Into the clicked block (sensors attach to what they control).
    Attach,
    /// Horizontal, facing the player (seats, panels).
    Horizontal,
}

/// Collision shape in block-local space (Y = block facing).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
pub enum Collision {
    #[default]
    Full,
    None,
    /// Centred box of this size (m).
    Box(f32, f32, f32),
}

/// Geometry of a textured (non-mechanism) block, in block space. The
/// rotation comes from the facing like for models (`models::block_rot`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
pub enum Shape {
    #[default]
    Cube,
    /// Half block lying against the face it was placed on.
    Slab,
    /// Smooth slope: height `.0` at the front (towards the player when
    /// placed) rising to `.1` at the back, in blocks (0..1).
    Wedge(f32, f32),
    /// Stairs: a bottom half and a back top quarter (step towards the player).
    Stairs,
    /// Outer corner of a 45° slope: a pyramid rising to the back-left corner.
    CornerOuter,
    /// Inner corner of a 45° slope: two slopes meeting at the back-left.
    CornerInner,
}

/// Shapes a building cube can be placed in (Shift + wheel): (shape, key
/// suffix, name, volume share). Index 0 = the cube itself.
pub const SHAPES: [(Shape, &str, &str, f32); 6] = [
    (Shape::Cube, "", "куб", 1.0),
    (Shape::Slab, "_slab", "плита", 0.5),
    (Shape::Stairs, "_stairs", "ступенька", 0.75),
    (Shape::Wedge(0.0, 1.0), "_slope", "скос", 0.5),
    (Shape::CornerOuter, "_corner", "внешний угол", 1.0 / 3.0),
    (Shape::CornerInner, "_inner", "внутренний угол", 5.0 / 6.0),
];

/// A block setting changed with the wrench.
#[derive(Debug, Clone, Deserialize)]
pub struct ParamDef {
    pub key: String,
    pub label: String,
    pub kind: ParamKind,
    #[serde(default)]
    pub default: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ParamKind {
    Number { min: f32, max: f32, step: f32, unit: String },
    Choice(Vec<String>),
    /// A keyboard key from `controls::BINDABLE_KEYS` (index; 0 = none).
    Key,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockDef {
    pub key: String,
    pub name: String,
    pub category: String,
    /// Cube faces: (top, sides, bottom) texture names.
    #[serde(default)]
    pub textures: Option<(String, String, String)>,
    /// See-through: neighbours keep their faces (glass, frames).
    #[serde(default)]
    pub transparent: bool,
    #[serde(default)]
    pub collision: Collision,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub mech: Option<Mech>,
    #[serde(default)]
    pub params: Vec<ParamDef>,
    /// Mass when it becomes physical (kg).
    #[serde(default = "default_mass")]
    pub mass: f32,
    /// Not shown in the catalogue (placed by the game itself).
    #[serde(default)]
    pub hidden: bool,
    /// Lift when fully under water (kg); default 40 % of the mass.
    #[serde(default)]
    pub buoyancy: Option<f32>,
}

fn default_mass() -> f32 {
    100.0
}

impl BlockDef {
    /// Drawn by the chunk mesher (a textured cube); otherwise a model.
    pub fn is_cube(&self) -> bool {
        self.mech.is_none() && self.textures.is_some()
    }
    pub fn param(&self, key: &str) -> Option<(usize, &ParamDef)> {
        self.params.iter().enumerate().find(|(_, p)| p.key == key)
    }
}

/// Tools are items that are not blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Tool {
    Wrench,
    Glue,
    GravityGun,
    PhysGun,
    /// Copies builds and pastes them (with a hologram preview).
    Blueprint,
    /// Pours 1000 mB of diesel into a filling port (never runs dry).
    DieselBucket,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolDef {
    pub key: String,
    pub name: String,
    pub category: String,
    pub tool: Tool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryDef {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockId(pub u16);

impl BlockId {
    pub const AIR: BlockId = BlockId(0);
    pub fn def(self) -> &'static BlockDef {
        &registry().blocks[self.0 as usize]
    }
    pub fn is_air(self) -> bool {
        self.0 == 0
    }
    pub fn k(key: &str) -> BlockId {
        registry().block(key).unwrap_or_else(|| panic!("unknown block {key}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Block(BlockId),
    Tool(Tool),
}

#[derive(Debug, Clone)]
pub struct ItemDef {
    pub key: String,
    pub name: String,
    pub category: String,
    pub kind: ItemKind,
    pub max_stack: u32,
    pub hidden: bool,
}

impl ItemId {
    pub fn def(self) -> &'static ItemDef {
        &registry().items[self.0 as usize]
    }
    pub fn k(key: &str) -> ItemId {
        registry().item(key).unwrap_or_else(|| panic!("unknown item {key}"))
    }
    pub fn block(self) -> Option<BlockId> {
        match self.def().kind {
            ItemKind::Block(b) => Some(b),
            _ => None,
        }
    }
    pub fn tool(self) -> Option<Tool> {
        match self.def().kind {
            ItemKind::Tool(t) => Some(t),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    pub categories: Vec<CategoryDef>,
    /// Index 0 is air.
    pub blocks: Vec<BlockDef>,
    pub items: Vec<ItemDef>,
    block_index: HashMap<String, u16>,
    item_index: HashMap<String, u16>,
    /// Every texture name used by cube blocks (atlas order).
    pub textures: Vec<String>,
}

impl Registry {
    pub fn block(&self, key: &str) -> Option<BlockId> {
        self.block_index.get(key).map(|&i| BlockId(i))
    }
    pub fn item(&self, key: &str) -> Option<ItemId> {
        self.item_index.get(key).map(|&i| ItemId(i))
    }
    pub fn item_for_block(&self, b: BlockId) -> Option<ItemId> {
        self.item(&b.def().key)
    }
    /// The block placed for a cube's item in shape `i` of `SHAPES`.
    pub fn shaped(&self, base: BlockId, i: usize) -> Option<BlockId> {
        if i == 0 {
            return Some(base);
        }
        self.block(&format!("{}{}", base.def().key, SHAPES.get(i)?.1))
    }
    /// A shaped block → (its cube, shape index).
    pub fn unshaped(&self, b: BlockId) -> (BlockId, usize) {
        let key = &b.def().key;
        for (i, (_, suffix, ..)) in SHAPES.iter().enumerate().skip(1) {
            if let Some(base) = key.strip_suffix(suffix)
                && let Some(id) = self.block(base)
                && id.def().shape == Shape::Cube
            {
                return (id, i);
            }
        }
        (b, 0)
    }
    pub fn texture_index(&self, name: &str) -> usize {
        self.textures.iter().position(|t| t == name).unwrap_or(0)
    }

    pub fn load() -> Registry {
        let read = |f: &str, builtin: &str| {
            std::fs::read_to_string(format!("assets/content/{f}")).unwrap_or_else(|_| builtin.to_string())
        };
        let categories: Vec<CategoryDef> = parse(&read("categories.ron", BUILTIN_CATEGORIES), "categories.ron");
        let blocks: Vec<BlockDef> = parse(&read("blocks.ron", BUILTIN_BLOCKS), "blocks.ron");
        let tools: Vec<ToolDef> = parse(&read("tools.ron", BUILTIN_TOOLS), "tools.ron");
        Self::from_defs(categories, blocks, tools)
    }

    pub fn from_defs(categories: Vec<CategoryDef>, blocks: Vec<BlockDef>, tools: Vec<ToolDef>) -> Registry {
        let mut r = Registry { categories, ..Default::default() };
        r.blocks.push(BlockDef {
            key: "air".into(),
            name: "Воздух".into(),
            category: String::new(),
            textures: None,
            transparent: true,
            collision: Collision::None,
            shape: Shape::Cube,
            placement: Placement::None,
            mech: None,
            params: vec![],
            mass: 0.0,
            hidden: true,
            buoyancy: None,
        });
        // Every plain building cube also comes in all shapes (hidden
        // blocks, placed through the cube's item with a chosen shape).
        let mut blocks = blocks;
        let mut shaped = Vec::new();
        for b in &blocks {
            if b.category == "building" && b.is_cube() && b.shape == Shape::Cube {
                for (shape, suffix, name, vol) in SHAPES.iter().skip(1) {
                    let placement = if *shape == Shape::Slab { Placement::Attach } else { Placement::Horizontal };
                    shaped.push(BlockDef { key: format!("{}{suffix}", b.key), name: format!("{} — {name}", b.name), shape: *shape, placement, mass: b.mass * vol, hidden: true, buoyancy: b.buoyancy.map(|x| x * vol), ..b.clone() });
                }
            }
        }
        blocks.extend(shaped);
        for b in blocks {
            if r.block_index.contains_key(&b.key) {
                eprintln!("content: duplicate block {}", b.key);
                continue;
            }
            r.block_index.insert(b.key.clone(), r.blocks.len() as u16);
            r.blocks.push(b);
        }
        for (i, b) in r.blocks.iter().enumerate().skip(1) {
            let id = r.items.len() as u16;
            r.item_index.insert(b.key.clone(), id);
            r.items.push(ItemDef { key: b.key.clone(), name: b.name.clone(), category: b.category.clone(), kind: ItemKind::Block(BlockId(i as u16)), max_stack: 64, hidden: b.hidden });
        }
        for t in tools {
            let id = r.items.len() as u16;
            r.item_index.insert(t.key.clone(), id);
            r.items.push(ItemDef { key: t.key, name: t.name, category: t.category, kind: ItemKind::Tool(t.tool), max_stack: 1, hidden: false });
        }
        for b in &r.blocks {
            if let Some((a, s, d)) = &b.textures {
                for t in [a, s, d] {
                    if !r.textures.contains(t) {
                        r.textures.push(t.clone());
                    }
                }
            }
        }
        r
    }
}

fn parse<T: for<'de> Deserialize<'de>>(text: &str, name: &str) -> Vec<T> {
    match ron::from_str(text) {
        Ok(v) => v,
        Err(e) => panic!("content/{name}: {e}"),
    }
}

const BUILTIN_CATEGORIES: &str = include_str!("../assets/content/categories.ron");
const BUILTIN_BLOCKS: &str = include_str!("../assets/content/blocks.ron");
const BUILTIN_TOOLS: &str = include_str!("../assets/content/tools.ron");

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::load)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_loads() {
        let r = registry();
        assert!(r.block("white_block").is_some());
        assert!(r.item("wrench").is_some_and(|i| i.tool() == Some(Tool::Wrench)));
        for b in &r.blocks[1..] {
            assert!(r.categories.iter().any(|c| c.key == b.category), "{} category", b.key);
            assert!(b.textures.is_some() || b.mech.is_some(), "{} has no look", b.key);
            for p in &b.params {
                if let ParamKind::Choice(c) = &p.kind {
                    assert!((p.default as usize) < c.len(), "{}.{}", b.key, p.key);
                }
            }
        }
        for t in &r.textures {
            assert!(std::path::Path::new(&format!("assets/textures/{t}.png")).exists(), "texture {t}");
        }
    }
}
