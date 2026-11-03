//! Content definitions (what the `.ron` files contain).

use serde::{Deserialize, Serialize};

/// How a block is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderKind {
    Invisible,
    /// Opaque cube; culls neighbour faces.
    Cube,
    /// See-through cube (glass); faces between equal blocks are culled.
    Transparent,
    /// Liquid surface; translucent, slightly lowered top.
    Liquid,
    /// Two crossed quads (plants).
    Cross,
}

/// Kinds of magic (concept §4). Only the basic differences are simulated
/// yet; the categories exist in the data model so content can use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AetherKind {
    /// Ordinary / natural background force.
    #[default]
    Calm,
    /// Irritated: distorted, unstable, spawns anomalies and entities.
    Agitated,
    /// Condensed: dense, dangerous clusters of agitated magic.
    Condensed,
    /// Ancient: rare, resists distortion.
    Ancient,
    /// The unique magic of the in-between world.
    Unique,
}

impl AetherKind {
    pub fn label(self) -> &'static str {
        match self {
            AetherKind::Calm => "естественная",
            AetherKind::Agitated => "раздражённая",
            AetherKind::Condensed => "конденсированная",
            AetherKind::Ancient => "древняя",
            AetherKind::Unique => "уникальная",
        }
    }
    pub fn rgb(self) -> [f32; 3] {
        match self {
            AetherKind::Calm => [0.35, 0.9, 1.0],
            AetherKind::Agitated => [1.0, 0.35, 0.75],
            AetherKind::Condensed => [0.85, 0.25, 0.45],
            AetherKind::Ancient => [0.4, 1.0, 0.8],
            AetherKind::Unique => [0.95, 0.95, 1.0],
        }
    }
    /// Multiplier when converting to mechanical energy.
    pub fn potency(self) -> f32 {
        match self {
            AetherKind::Calm => 1.0,
            AetherKind::Agitated => 1.6,
            AetherKind::Condensed => 2.5,
            AetherKind::Ancient => 2.2,
            AetherKind::Unique => 3.0,
        }
    }
}

/// Index of a texture in the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct TexId(pub u16);

/// Block as written in `blocks.ron`.
#[derive(Debug, Clone, Deserialize)]
pub struct BlockSpec {
    pub key: String,
    pub name: String,
    pub render: RenderKind,
    pub solid: bool,
    /// Seconds to mine with the base drill; `None` = unbreakable.
    pub hardness: Option<f32>,
    pub tier: u8,
    /// (top, side, bottom) texture names.
    pub textures: (String, String, String),
    #[serde(default)]
    pub drop: String,
    #[serde(default)]
    pub glow: f32,
    #[serde(default)]
    pub aether: f32,
    #[serde(default)]
    pub aether_kind: Option<AetherKind>,
    #[serde(default)]
    pub info: String,
}

/// Resolved block.
#[derive(Debug, Clone)]
pub struct BlockDef {
    pub key: String,
    pub name: String,
    pub render: RenderKind,
    pub solid: bool,
    /// `f32::INFINITY` = unbreakable.
    pub hardness: f32,
    pub tier: u8,
    /// Textures for +Y, sides, −Y.
    pub tex: [TexId; 3],
    pub texture_names: [String; 3],
    pub drop: String,
    pub glow: f32,
    pub aether: f32,
    pub aether_kind: Option<AetherKind>,
    pub info: String,
}

impl BlockDef {
    pub fn from_spec(s: BlockSpec) -> Self {
        Self {
            key: s.key,
            name: s.name,
            render: s.render,
            solid: s.solid,
            hardness: s.hardness.unwrap_or(f32::INFINITY),
            tier: s.tier,
            tex: [TexId(0); 3],
            texture_names: [s.textures.0, s.textures.1, s.textures.2],
            drop: s.drop,
            glow: s.glow,
            aether: s.aether,
            aether_kind: s.aether_kind,
            info: s.info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AlphaKind {
    #[default]
    Opaque,
    Blend,
    Mask,
}

/// Texture + material parameters (`textures.ron`).
#[derive(Debug, Clone, Deserialize)]
pub struct TextureDef {
    pub name: String,
    /// Built-in procedural pattern used when no PNG exists (default: `name`).
    #[serde(default)]
    pub procedural: Option<String>,
    #[serde(default)]
    pub alpha: AlphaKind,
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default)]
    pub metallic: f32,
    #[serde(default = "default_reflectance")]
    pub reflectance: f32,
    /// Linear RGB emission.
    #[serde(default)]
    pub emissive: (f32, f32, f32),
    #[serde(default)]
    pub double_sided: bool,
    #[serde(default = "default_true")]
    pub shadows: bool,
}

fn default_roughness() -> f32 {
    0.92
}
fn default_reflectance() -> f32 {
    0.2
}
fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    Resource,
    Block,
    Tool,
    Weapon,
    Part,
    Station,
    Food,
    Magic,
    Module,
    /// Worn equipment.
    Gear,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Resource,
        Category::Block,
        Category::Tool,
        Category::Weapon,
        Category::Part,
        Category::Station,
        Category::Food,
        Category::Magic,
        Category::Module,
        Category::Gear,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Category::Resource => "Ресурс",
            Category::Block => "Блок",
            Category::Tool => "Инструмент",
            Category::Weapon => "Оружие",
            Category::Part => "Деталь",
            Category::Station => "Станция",
            Category::Food => "Еда",
            Category::Magic => "Магия",
            Category::Module => "Модуль",
            Category::Gear => "Снаряжение",
        }
    }
    /// Picked up automatically when auto-pickup is on ("ordinary resources").
    pub fn auto_pickup(self) -> bool {
        matches!(
            self,
            Category::Resource | Category::Block | Category::Food | Category::Magic
        )
    }
}

/// What the item does when held/used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemUse {
    None,
    Block(String),
    Part(String),
    Manipulator,
    PhysGun,
    Melee,
    Ranged,
    Focus,
    Eat { food: f32, water: f32 },
    Module(String),
    /// Worn in an equipment slot (holographic device → Экипировка).
    Wear(EquipSlot),
    /// Places a kinetic block (`kinetics.ron` key).
    Kinetic(String),
    /// Lays wires between electric devices.
    Wire(WireSpec),
    /// Lays magical links (crystal conductor / rune channel).
    MagicLink(LinkSpec),
    /// Inscribes a rune on a magical device.
    Rune(RuneKind),
    /// Lays signal cables (output → input).
    SignalCable { max_len: f32 },
    /// Repairs damaged machines (hold LMB).
    Repair,
}

/// A logic element block (`content/logic.ron`); its block is the kinetic
/// block with the same key.
#[derive(Clone, Debug, Deserialize)]
pub struct LogicDef {
    pub key: String,
    pub element: crate::logic::circuit::Element,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMode {
    /// Physical crystal conductor: both ways, short.
    Conductor,
    /// Rune channel: one way (first → second), needs line of sight.
    Channel,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkSpec {
    pub mode: LinkMode,
    /// Aether per second.
    pub throughput: f32,
    pub loss_per_m: f32,
    pub max_len: f32,
}

/// Runes configure magical devices (concept: visual language of magic
/// schemes; the full specification is an open question — this is the
/// extensible base).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuneKind {
    /// Accept only one kind of magic.
    Filter,
    /// Limit the outflow.
    Limit,
    /// Only send / only receive.
    Direction,
    /// Resonance frequency of transmitters and receivers.
    Link,
}

impl RuneKind {
    pub fn label(self) -> &'static str {
        match self {
            RuneKind::Filter => "Фильтр",
            RuneKind::Limit => "Ограничение",
            RuneKind::Direction => "Направление",
            RuneKind::Link => "Связь",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MagicRole {
    Collector,
    Filter,
    Stabilizer,
    Accumulator,
    Engine,
    Transmitter,
    Receiver,
}

/// A magical device (`content/arcana.ron`); its block is the kinetic
/// block with the same key.
#[derive(Clone, Debug, Deserialize)]
pub struct MagicDef {
    pub key: String,
    pub role: MagicRole,
    pub capacity: f32,
    /// Collector: share of the local field collected; engine: aether / s.
    #[serde(default)]
    pub rate: f32,
    #[serde(default = "one")]
    pub max_distortion: f32,
    /// Kinds accepted by design (None = all).
    #[serde(default)]
    pub accepts: Option<Vec<AetherKind>>,
    /// Stabilizer: distortion removed per second at full power.
    #[serde(default)]
    pub stabilize: f32,
    /// Electric draw (W) — the stabilizer needs power.
    #[serde(default)]
    pub electric_draw: f32,
    /// Transmitters: range (m) and throughput (aether / s).
    #[serde(default)]
    pub range: f32,
    #[serde(default)]
    pub throughput: f32,
}

fn one() -> f32 {
    1.0
}

/// Electric wire parameters (temporary balance).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireSpec {
    /// Max power through (energy units per second, "W").
    pub capacity: f32,
    /// Fraction of power lost per metre.
    pub loss_per_m: f32,
    /// Longest span (m).
    pub max_len: f32,
}

/// A kinetic block type (`content/kinetics.ron`). Behaviour by `role`,
/// numbers here (temporary balance — open question of the concept).
#[derive(Clone, Debug, Deserialize)]
pub struct KineticDef {
    pub key: String,
    pub role: crate::kinetics::network::Role,
    /// Friction / work in SU per RPM.
    #[serde(default)]
    pub impact: f32,
    /// Max transmitted power (SU); None = unbreakable.
    #[serde(default)]
    pub strength: Option<f32>,
    /// Sources: wanted RPM and power (SU).
    #[serde(default)]
    pub source_rpm: f32,
    #[serde(default)]
    pub source_power: f32,
    /// Motor: energy per second per SU actually delivered.
    #[serde(default)]
    pub energy_per_su: f32,
    /// Item dropped when broken / collected.
    pub item: String,
}

/// Equipment slots that hold worn items. Weapon / tool / manipulator
/// modules are shown by the equipment screen from their own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    Head,
    Body,
    Hands,
    Legs,
    Accessory,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 5] = [
        EquipSlot::Head,
        EquipSlot::Body,
        EquipSlot::Hands,
        EquipSlot::Legs,
        EquipSlot::Accessory,
    ];
    pub fn label(self) -> &'static str {
        match self {
            EquipSlot::Head => "Голова",
            EquipSlot::Body => "Броня",
            EquipSlot::Hands => "Руки",
            EquipSlot::Legs => "Ноги",
            EquipSlot::Accessory => "Аксессуар",
        }
    }
}

/// Journal categories (concept: knowledge is discovered gradually).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalCategory {
    Creatures,
    Plants,
    Materials,
    Biomes,
    Ruins,
    Technologies,
    Magic,
    Anomalies,
}

impl JournalCategory {
    pub const ALL: [JournalCategory; 8] = [
        JournalCategory::Creatures,
        JournalCategory::Plants,
        JournalCategory::Materials,
        JournalCategory::Biomes,
        JournalCategory::Ruins,
        JournalCategory::Technologies,
        JournalCategory::Magic,
        JournalCategory::Anomalies,
    ];
    pub fn label(self) -> &'static str {
        match self {
            JournalCategory::Creatures => "Существа",
            JournalCategory::Plants => "Растения",
            JournalCategory::Materials => "Материалы",
            JournalCategory::Biomes => "Биомы",
            JournalCategory::Ruins => "Руины",
            JournalCategory::Technologies => "Технологии",
            JournalCategory::Magic => "Магия",
            JournalCategory::Anomalies => "Аномалии",
        }
    }
}

/// Picture shown with a journal entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum JournalImage {
    #[default]
    None,
    /// A block texture by name (`textures.ron`).
    Texture(String),
    /// An item icon colour by item key.
    Item(String),
    /// A schematic drawn from lines: list of (x0, y0, x1, y1) in 0..1.
    Scheme(Vec<(f32, f32, f32, f32)>),
}

/// What opens a journal entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JournalUnlock {
    /// Known from the start.
    Always,
    /// Scanning a block with this key.
    ScanBlock(String),
    /// Scanning an object whose title contains this text.
    ScanObject(String),
    /// Picking up / crafting an item with this key.
    Obtain(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct JournalDef {
    pub key: String,
    pub category: JournalCategory,
    pub title: String,
    pub text: String,
    /// Open questions written as the researcher's assumptions.
    #[serde(default)]
    pub hypothesis: String,
    #[serde(default)]
    pub image: JournalImage,
    pub unlock: JournalUnlock,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    pub key: String,
    pub name: String,
    pub category: Category,
    pub max_stack: u32,
    #[serde(rename = "use")]
    pub use_: ItemUse,
    pub mass: f32,
    /// Icon colour (sRGB 0..1).
    pub color: (f32, f32, f32),
    #[serde(default)]
    pub info: String,
    /// Optional icon file name in `textures/items/`.
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Station {
    Hand,
    Workbench,
    Smelter,
    Grinder,
}

impl Station {
    pub fn label(self) -> &'static str {
        match self {
            Station::Hand => "Руками",
            Station::Workbench => "Верстак",
            Station::Smelter => "Плавильня",
            Station::Grinder => "Дробилка",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeDef {
    pub station: Station,
    pub inputs: Vec<(String, u32)>,
    pub output: (String, u32),
    #[serde(default)]
    pub time: f32,
}
