//! Интерфейс игры, не привязанный к способу отрисовки.
//!
//! Три слоя, каждый можно заменить отдельно:
//!
//! 1. **Модель** — данные игры (`vx_core::inventory::Inventory`, [`Settings`]).
//! 2. **Экраны** ([`screens`]) — главное меню, HUD, пауза, настройки, инвентарь.
//!    Написаны на маленьком immediate-mode API [`Ui`] и выдают [`DrawList`].
//! 3. **Бэкенд** — кто угодно, кто умеет рисовать [`DrawList`]. Для GPU есть
//!    готовая триангуляция [`tessellate`] → [`UiMesh`] (цветные текстурированные
//!    треугольники), её рисует `vx_render`. Для egui/iced/macroquad и т. п. можно
//!    либо рисовать `DrawList` их средствами, либо переписать слой экранов.
//!
//! Координаты — логические пиксели, начало в левом верхнем углу.

mod draw;
mod font;
mod mesh;
pub mod screens;
mod ui;

pub use draw::{Color, DrawList, Rect, Shape, Texture};
pub use font::FontAtlas;
pub use mesh::{UiMesh, UiVertex, tessellate};
pub use screens::{Action, Gui, HudInfo, Screen, Settings, UiContext};
pub use ui::{Column, Memory, Theme, Ui, UiInput, UiKey};
