# Моддинг: блоки, текстуры, предметы, рецепты

Весь контент игры описан в текстовых файлах формата **RON** (похож на JSON,
но с комментариями `//` и без лишних кавычек). Код игры знает только поведение
(«блок твёрдый», «предмет ставит деталь»), а *что именно* есть в мире — решают
файлы.

## Где лежат файлы

| Что | Базовая игра | Мод |
|---|---|---|
| Блоки | `assets/content/blocks.ron` | `mods/<мод>/content/blocks.ron` |
| Текстуры (материалы) | `assets/content/textures.ron` | `mods/<мод>/content/textures.ron` |
| Предметы | `assets/content/items.ron` | `mods/<мод>/content/items.ron` |
| Рецепты | `assets/content/recipes.ron` | `mods/<мод>/content/recipes.ron` |
| Журнал | `assets/content/journal.ron` | `mods/<мод>/content/journal.ron` |
| Картинки блоков | `assets/textures/blocks/<имя>.png` | `mods/<мод>/textures/blocks/<имя>.png` |
| 3D-модели | `assets/models/<категория>/<ключ>.glb` | `mods/<мод>/models/<категория>/<ключ>.glb` |

Порядок загрузки: встроенный контент → `assets/content` → моды по алфавиту
названия папки. Запись с тем же **ключом** заменяет предыдущую (сохраняя её
номер), новая — добавляется в конец. Мод может содержать только те файлы,
которые ему нужны.

Ошибка в файле мода **не роняет игру**: файл пропускается, сообщение пишется в
консоль (`content warning: …`). Проверить мод быстро: `cargo test` или просто
запустить игру и посмотреть первые строки вывода.

## Пример: новый блок «Мрамор»

`mods/marble/content/textures.ron`:
```ron
[
    (name: "marble", roughness: 0.4),
]
```

`mods/marble/content/blocks.ron`:
```ron
[
    (key: "marble", name: "Мрамор", render: Cube, solid: true, hardness: Some(1.4), tier: 1,
     textures: ("marble", "marble", "marble"), drop: "marble", glow: 0.0, aether: 0.0,
     aether_kind: None, info: "Светлый камень древних мастеров."),
]
```

`mods/marble/content/items.ron`:
```ron
[
    (key: "marble", name: "Мрамор", category: Block, max_stack: 64, use: Block("marble"),
     mass: 6.0, color: (0.92, 0.9, 0.86), info: "Строительный камень."),
]
```

`mods/marble/content/recipes.ron`:
```ron
[
    (station: Grinder, inputs: [("stone", 2)], output: ("marble", 1), time: 3.0),
]
```

`mods/marble/textures/blocks/marble.png` — картинка 16×16 (или 32×32, 64×64).
Если картинки нет, игра нарисует нейтральную «шахматку» — сразу видно, что
текстура не задана.

## Поля блока

| Поле | Значение |
|---|---|
| `key` | постоянный идентификатор (не меняйте после выпуска — на него ссылаются сохранения) |
| `render` | `Cube` (непрозрачный), `Transparent` (стекло), `Liquid`, `Cross` (растение), `Invisible` |
| `solid` | участвует ли в столкновениях |
| `hardness` | секунды добычи базовым буром, `None` — неразрушимый |
| `tier` | нужный уровень бура (2 = модуль бурения II) |
| `textures` | имена текстур (верх, бока, низ) из `textures.ron` |
| `drop` | ключ предмета, который выпадает (`""` — ничего) |
| `glow`, `aether` | свечение и излучение эфира (источник магии) |
| `aether_kind` | `Calm`, `Agitated`, `Condensed`, `Ancient`, `Unique` или `None` |

## Поля текстуры (материала)

`alpha: Opaque | Blend | Mask`, `roughness`, `metallic`, `reflectance`,
`emissive: (r, g, b)` (свечение, можно > 1), `double_sided`, `shadows`,
`procedural: Some("stone")` — какой встроенный узор рисовать, если PNG нет.

## Поля предмета

`category: Resource | Block | Tool | Weapon | Part | Station | Food | Magic | Module`,
`use`: `None`, `Block("ключ")`, `Part("ключ детали")`, `Eat(food: 10.0, water: 0.0)`,
`Module("drill2" | "cutter")`, `Manipulator`, `PhysGun`, `Melee`, `Ranged`, `Focus`,
`Wear(Head | Body | Hands | Legs | Accessory)` — снаряжение (категория `Gear`).
`color` — цвет значка; `icon` — зарезервировано для PNG-иконок.

## Записи журнала

`journal.ron`: `key`, `category` (`Creatures`, `Plants`, `Materials`, `Biomes`,
`Ruins`, `Technologies`, `Magic`, `Anomalies`), `title`, `text`,
`hypothesis` (необязательно), `image` (`Texture("имя")`, `Item("ключ")`,
`Scheme([(x0, y0, x1, y1), ...])`), `unlock` (`Always`, `ScanBlock("ключ")`,
`ScanObject("часть названия")`, `Obtain("ключ предмета")`).

## Текстуры

* **В игре**: `F7` — редактор текстур. Рисуете — мир меняется сразу;
  «Сохранить PNG» (`Ctrl+S`) пишет `assets/textures/blocks/<имя>.png`.
* **Во внешнем редакторе** (Aseprite, LibreSprite, Krita, GIMP, Photoshop):
  откройте PNG из `assets/textures/blocks/`, сохраните — игра перечитает файл
  в течение полсекунды (горячая перезагрузка), перезапуск не нужен.
* Все текущие текстуры выгружаются командой
  `cargo run -- --export-textures` (существующие файлы не перезаписываются).
* Размер — любой квадрат (16, 32, 64…). Одна картинка = один блок; в мире она
  повторяется на каждом блоке. Прозрачность учитывается для `Blend`/`Mask`.

## Сохранения и моды

Сохранение хранит **палитру ключей** блоков, а не номера, поэтому добавление,
удаление и перестановка модов не портят мир. Блоки удалённого мода при загрузке
становятся воздухом (с предупреждением в консоли).

## Что пока нельзя менять данными

Поведение (как работает двигатель, плавильня, заклинание) — код на Rust.
Новые *виды* деталей требуют кода; параметры существующих — данные.
Полноценный скриптовый API модов — в плане (концепт §16).
