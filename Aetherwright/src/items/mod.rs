//! Item definitions, stacks and inventories (pure data, no UI).
//!
//! Items are a static table; saves refer to items by their stable string
//! `key`, never by index, so the table can be reordered safely.

pub mod drops;
pub mod inventory;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::world::voxel::BlockId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl Category {
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
        }
    }

    /// Picked up automatically when auto-pickup is on ("ordinary resources").
    pub fn auto_pickup(self) -> bool {
        matches!(self, Category::Resource | Category::Block | Category::Food | Category::Magic)
    }
}

/// What the item does when held/used. Interpreted by tools/building/etc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Use {
    None,
    /// Places a voxel block (block key).
    Block(&'static str),
    /// Places a functional entity (part kind key, see `machinery`).
    Part(&'static str),
    Manipulator,
    PhysGun,
    Melee,
    Ranged,
    /// Aether focus: casts spells.
    Focus,
    /// Edible: (food, water).
    Eat(f32, f32),
    /// Installs a manipulator module (module key).
    Module(&'static str),
}

pub struct ItemDef {
    pub key: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub max_stack: u32,
    pub use_: Use,
    /// Mass in kg of one unit (used for dropped items / weight).
    pub mass: f32,
    /// Icon colour for UI and dropped-item visuals.
    pub color: [f32; 3],
    pub info: &'static str,
}

macro_rules! item {
    ($key:literal, $name:literal, $cat:ident, $stack:expr, $use_:expr, $mass:expr, $col:expr, $info:literal) => {
        ItemDef { key: $key, name: $name, category: Category::$cat, max_stack: $stack, use_: $use_, mass: $mass, color: $col, info: $info }
    };
}

pub static ITEMS: &[ItemDef] = &[
    // Resources
    item!("wood", "Древесина", Resource, 64, Use::Block("log"), 4.0, [0.55, 0.40, 0.26], "Брёвна. Ставятся как блок, режутся на доски."),
    item!("fiber", "Волокно", Resource, 99, Use::None, 0.1, [0.55, 0.70, 0.35], "Растительное волокно для верёвок и фильтров."),
    item!("stone", "Камень", Block, 64, Use::Block("stone"), 6.0, [0.50, 0.50, 0.52], "Обычный камень."),
    item!("dirt", "Земля", Block, 64, Use::Block("dirt"), 5.0, [0.47, 0.34, 0.24], "Грунт."),
    item!("sand", "Песок", Block, 64, Use::Block("sand"), 5.0, [0.84, 0.77, 0.58], "Песок. Переплавляется в стекло."),
    item!("gravel", "Гравий", Block, 64, Use::Block("gravel"), 5.0, [0.48, 0.46, 0.44], "Гравий."),
    item!("clay", "Глина", Block, 64, Use::Block("clay"), 5.0, [0.59, 0.61, 0.67], "Глина. Обжигается в кирпич."),
    item!("coal", "Уголь", Resource, 64, Use::None, 1.0, [0.15, 0.15, 0.16], "Топливо для плавильни."),
    item!("iron_ore", "Железная руда", Resource, 64, Use::None, 3.0, [0.70, 0.56, 0.46], "Плавится в металл."),
    item!("copper_ore", "Медная руда", Resource, 64, Use::None, 3.0, [0.80, 0.47, 0.28], "Плавится в медь."),
    item!("metal", "Металл", Resource, 64, Use::None, 2.0, [0.72, 0.74, 0.78], "Слиток железа — основа механизмов."),
    item!("copper", "Медь", Resource, 64, Use::None, 2.0, [0.86, 0.52, 0.30], "Медный слиток — проводники и катушки."),
    item!("aether_shard", "Осколок эфира", Magic, 64, Use::None, 0.3, [0.35, 0.90, 1.00], "Кристаллизованная спокойная магия."),
    item!("agitated_shard", "Раздражённый осколок", Magic, 64, Use::None, 0.3, [1.00, 0.35, 0.70], "Нестабильная магия. Опасна, но мощна."),
    item!("ancient_stone", "Древний камень", Block, 64, Use::Block("ancient_stone"), 8.0, [0.30, 0.42, 0.44], "Пропитан древней магией."),
    // Building blocks
    item!("planks", "Доски", Block, 64, Use::Block("planks"), 2.0, [0.69, 0.52, 0.32], "Строительный материал."),
    item!("bricks", "Каменный кирпич", Block, 64, Use::Block("bricks"), 6.0, [0.48, 0.47, 0.50], "Строительный материал."),
    item!("glass", "Стекло", Block, 64, Use::Block("glass"), 2.0, [0.75, 0.88, 0.92], "Прозрачный блок."),
    item!("metal_plate", "Металлическая плита", Block, 64, Use::Block("metal_plate"), 8.0, [0.60, 0.62, 0.66], "Прочная обшивка."),
    // Food
    item!("berries", "Ягоды", Food, 32, Use::Eat(12.0, 4.0), 0.1, [0.75, 0.15, 0.25], "Утоляют голод (и немного жажду)."),
    item!("water_flask", "Фляга воды", Food, 8, Use::Eat(0.0, 35.0), 1.0, [0.30, 0.55, 0.85], "Утоляет жажду."),
    item!("ration", "Походный паёк", Food, 16, Use::Eat(35.0, 5.0), 0.4, [0.80, 0.65, 0.40], "Сытная еда из ягод и волокна."),
    // Tools & weapons
    item!("manipulator", "Манипулятор", Tool, 1, Use::Manipulator, 3.0, [0.35, 0.90, 1.00], "Модульный прибор: добыча, размещение, сканирование, резка, механизмы."),
    item!("physgun", "Гравитационный захват", Tool, 1, Use::PhysGun, 4.0, [1.00, 0.70, 0.25], "Захват, перемещение, вращение, фиксация и соединение физических объектов."),
    item!("blade", "Резонансный клинок", Weapon, 1, Use::Melee, 2.0, [0.80, 0.85, 0.90], "Ближний бой."),
    item!("arc_caster", "Дуговой метатель", Weapon, 1, Use::Ranged, 3.0, [0.95, 0.80, 0.40], "Стреляет металлическими болтами."),
    item!("bolt", "Болт", Resource, 64, Use::None, 0.1, [0.70, 0.70, 0.72], "Боеприпас дугового метателя."),
    item!("aether_focus", "Эфирный фокус", Magic, 1, Use::Focus, 0.5, [0.75, 0.50, 1.00], "Позволяет творить заклинания запасом эфира."),
    // Manipulator modules
    item!("drill_module_2", "Модуль бурения II", Module, 1, Use::Module("drill2"), 1.0, [0.45, 0.85, 0.80], "Позволяет добывать древний камень; ускоряет добычу."),
    item!("cutter_module", "Модуль резки", Module, 1, Use::Module("cutter"), 1.0, [1.00, 0.55, 0.30], "Вырезает блоки из мира как физические тела."),
    // Functional parts (placed as entities; see machinery)
    item!("frame", "Рама", Part, 32, Use::Part("frame"), 10.0, [0.55, 0.55, 0.58], "Лёгкий конструкционный блок для сборки машин."),
    item!("crate", "Ящик", Part, 16, Use::Part("crate"), 15.0, [0.62, 0.45, 0.27], "Деревянный ящик — физический груз."),
    item!("engine", "Двигатель", Part, 8, Use::Part("engine"), 40.0, [0.80, 0.35, 0.22], "Крутит вал через шарнир. Потребляет энергию."),
    item!("power_cell", "Энергоячейка", Part, 8, Use::Part("power_cell"), 15.0, [0.30, 0.85, 0.55], "Хранит энергию. Питает соединённые механизмы."),
    item!("shaft", "Вал", Part, 16, Use::Part("shaft"), 8.0, [0.70, 0.70, 0.74], "Передаёт вращение."),
    item!("gear", "Шестерня", Part, 16, Use::Part("gear"), 6.0, [0.80, 0.70, 0.40], "Передаёт вращение соседней шестерне."),
    item!("wheel", "Колесо", Part, 16, Use::Part("wheel"), 12.0, [0.25, 0.25, 0.27], "Колесо с хорошим сцеплением."),
    item!("connector", "Соединитель", Part, 32, Use::Part("connector"), 2.0, [0.90, 0.85, 0.40], "Узел крепления: удобная точка для шарниров и сварки."),
    item!("aether_jar", "Эфирный накопитель", Part, 8, Use::Part("aether_jar"), 10.0, [0.45, 0.80, 1.00], "Собирает и хранит эфир; передаёт его соседним накопителям."),
    item!("aether_dynamo", "Эфирная динамо-машина", Part, 4, Use::Part("aether_dynamo"), 30.0, [0.55, 0.75, 1.00], "Превращает эфир в энергию для механизмов."),
    // Stations
    item!("workbench", "Верстак", Station, 4, Use::Part("workbench"), 30.0, [0.60, 0.45, 0.30], "Станция для сложных рецептов."),
    item!("smelter", "Плавильня", Station, 4, Use::Part("smelter"), 60.0, [0.45, 0.40, 0.38], "Плавит руду и песок. Нужен уголь."),
    item!("grinder", "Дробилка", Station, 4, Use::Part("grinder"), 50.0, [0.50, 0.55, 0.60], "Перерабатывает материалы, пока её вал вращается."),
];

impl ItemId {
    pub fn def(self) -> &'static ItemDef {
        &ITEMS[self.0 as usize]
    }
    pub fn from_key(key: &str) -> Option<ItemId> {
        ITEMS.iter().position(|i| i.key == key).map(|i| ItemId(i as u16))
    }
    /// Panics on unknown key — for keys written in code (checked by tests).
    pub fn k(key: &str) -> ItemId {
        Self::from_key(key).unwrap_or_else(|| panic!("unknown item key {key}"))
    }
    pub fn block(self) -> Option<BlockId> {
        match self.def().use_ {
            Use::Block(k) => BlockId::from_key(k),
            _ => None,
        }
    }
    pub fn color(self) -> Color {
        let c = self.def().color;
        Color::srgb(c[0], c[1], c[2])
    }
}

/// Item dropped by a block (resolves the block table's `drop` key).
pub fn block_drop(b: BlockId) -> Option<ItemId> {
    let k = b.def().drop;
    if k.is_empty() { None } else { ItemId::from_key(k) }
}

/// Item that places a given block (for creative pick-block / refunds).
pub fn item_for_block(b: BlockId) -> Option<ItemId> {
    ITEMS
        .iter()
        .position(|i| i.use_ == Use::Block(b.def().key))
        .map(|i| ItemId(i as u16))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u32,
}

/// Serialized form (stable keys).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStack {
    pub item: String,
    pub count: u32,
}

impl ItemStack {
    pub fn new(item: ItemId, count: u32) -> Self {
        Self { item, count }
    }
    pub fn to_saved(self) -> SavedStack {
        SavedStack { item: self.item.def().key.to_string(), count: self.count }
    }
    pub fn from_saved(s: &SavedStack) -> Option<Self> {
        let item = ItemId::from_key(&s.item)?;
        (s.count > 0).then(|| Self { item, count: s.count.min(item.def().max_stack) })
    }
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((inventory::InventoryPlugin, drops::DropsPlugin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::voxel::BLOCKS;

    #[test]
    fn keys_unique_and_references_valid() {
        let mut keys: Vec<_> = ITEMS.iter().map(|i| i.key).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len(), "duplicate item keys");
        for i in ITEMS {
            if let Use::Block(b) = i.use_ {
                assert!(BlockId::from_key(b).is_some(), "item {} places unknown block {b}", i.key);
            }
            assert!(i.max_stack >= 1);
        }
        for b in BLOCKS.iter() {
            if !b.drop.is_empty() {
                assert!(ItemId::from_key(b.drop).is_some(), "block {} drops unknown item {}", b.key, b.drop);
            }
        }
    }
}
