//! Экраны игры и переходы между ними.
//!
//! Новый экран: добавьте вариант в [`Screen`], функцию отрисовки в отдельном
//! файле и строку в `match` внутри [`Gui::frame`].

mod hud;
mod inventory;
mod menu;

use vx_core::{BlockId, Registry, inventory::Inventory};

use crate::{DrawList, FontAtlas, Memory, Theme, Ui, UiInput, UiKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    /// Игра идёт: виден только HUD.
    Playing,
    Paused,
    Inventory,
    Settings,
}

/// Действия, которые интерфейс просит выполнить у игры.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
}

/// Пользовательские настройки (меняются на экране настроек).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Множитель чувствительности мыши.
    pub sensitivity: f32,
    /// Вертикальный угол обзора, градусы.
    pub fov: f32,
    /// Множитель масштаба интерфейса (поверх масштаба экрана ОС).
    pub ui_scale: f32,
    pub creative: bool,
    pub show_fps: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { sensitivity: 1.0, fov: 70.0, ui_scale: 1.0, creative: true, show_fps: true }
    }
}

/// Сведения для HUD, которые собирает игра.
#[derive(Clone, Debug, Default)]
pub struct HudInfo {
    pub fps: f32,
    /// Недавние сообщения (самые новые — в конце).
    pub messages: Vec<String>,
    /// Строки отладочного экрана (F3).
    pub debug: Vec<String>,
}

/// Данные игры, доступные интерфейсу на этот кадр.
pub struct UiContext<'a> {
    pub reg: &'a Registry,
    pub inventory: &'a mut Inventory,
    pub settings: &'a mut Settings,
    /// Блоки творческой палитры.
    pub palette: &'a [BlockId],
    pub hud: &'a HudInfo,
}

/// Навигация: текущий экран и куда вернуться из настроек.
pub struct Nav {
    pub screen: Screen,
    back: Screen,
}

impl Nav {
    pub fn go(&mut self, screen: Screen) {
        if screen == Screen::Settings {
            self.back = self.screen;
        }
        self.screen = screen;
    }
}

pub struct Gui {
    pub nav: Nav,
    pub theme: Theme,
    pub show_debug: bool,
    memory: Memory,
}

impl Default for Gui {
    fn default() -> Self {
        Gui::new(Screen::MainMenu)
    }
}

impl Gui {
    pub fn new(screen: Screen) -> Self {
        Gui {
            nav: Nav { screen, back: Screen::MainMenu },
            theme: Theme::default(),
            show_debug: false,
            memory: Memory::default(),
        }
    }

    pub fn screen(&self) -> Screen {
        self.nav.screen
    }

    /// Нужен ли видимый курсор (иначе мышь управляет камерой).
    pub fn wants_cursor(&self) -> bool {
        self.nav.screen != Screen::Playing
    }

    /// Должна ли игра стоять на паузе.
    pub fn pauses_game(&self) -> bool {
        self.in_game() && matches!(self.nav.screen, Screen::Paused | Screen::Settings)
    }

    /// Идёт ли игра (иначе — главное меню или настройки из него).
    pub fn in_game(&self) -> bool {
        let n = &self.nav;
        n.screen != Screen::MainMenu && !(n.screen == Screen::Settings && n.back == Screen::MainMenu)
    }

    /// Игра потеряла фокус окна.
    pub fn focus_lost(&mut self) {
        if self.nav.screen == Screen::Playing {
            self.nav.go(Screen::Paused);
        }
    }

    /// Один кадр интерфейса.
    pub fn frame(&mut self, input: &UiInput, font: &FontAtlas, ctx: UiContext) -> (DrawList, Vec<Action>) {
        let mut actions = vec![];
        if input.key(UiKey::Debug) {
            self.show_debug = !self.show_debug;
        }
        let (esc, inv) = (input.key(UiKey::Escape), input.key(UiKey::Inventory));
        match self.nav.screen {
            Screen::Playing if esc => self.nav.go(Screen::Paused),
            Screen::Playing if inv => self.nav.go(Screen::Inventory),
            Screen::Paused if esc => self.nav.go(Screen::Playing),
            Screen::Inventory if esc || inv => self.nav.go(Screen::Playing),
            Screen::Settings if esc => self.nav.screen = self.nav.back,
            _ => {}
        }
        if self.nav.screen != Screen::Inventory {
            ctx.inventory.return_cursor();
        }

        let in_game = self.in_game();
        let mut ui = Ui::new(input, font, &self.theme, &mut self.memory);
        if in_game {
            hud::draw(&mut ui, &ctx, self.show_debug, self.nav.screen == Screen::Playing);
        }
        match self.nav.screen {
            Screen::MainMenu => menu::main_menu(&mut ui, &mut self.nav, &mut actions),
            Screen::Playing => {}
            Screen::Paused => menu::pause(&mut ui, &mut self.nav, &mut actions),
            Screen::Settings => menu::settings(&mut ui, &mut self.nav, ctx.settings, in_game),
            Screen::Inventory => inventory::draw(&mut ui, ctx),
        }
        (ui.draw, actions)
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use vx_core::inventory::Stack;

    use super::*;
    use crate::{Shape, tessellate};

    fn font() -> FontAtlas {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/ui.ttf")).unwrap();
        FontAtlas::new(&data, 32.0).unwrap()
    }

    struct World {
        reg: Registry,
        inv: Inventory,
        settings: Settings,
        hud: HudInfo,
        palette: Vec<BlockId>,
    }

    fn world() -> World {
        let reg = Registry::from_toml("[[block]]\nname='stone'\n[[block]]\nname='planks'").unwrap();
        World {
            reg,
            inv: Inventory::new(36),
            settings: Settings::default(),
            hud: HudInfo::default(),
            palette: vec![1, 2],
        }
    }

    fn frame(gui: &mut Gui, w: &mut World, font: &FontAtlas, input: &UiInput) -> (DrawList, Vec<Action>) {
        let ctx = UiContext {
            reg: &w.reg,
            inventory: &mut w.inv,
            settings: &mut w.settings,
            palette: &w.palette,
            hud: &w.hud,
        };
        gui.frame(input, font, ctx)
    }

    fn input(keys: &[UiKey]) -> UiInput {
        UiInput { screen: Vec2::new(1280.0, 720.0), keys: keys.to_vec(), ..Default::default() }
    }

    /// Найти центр текста на экране, чтобы «нажать» на кнопку.
    fn find(list: &DrawList, label: &str, font: &FontAtlas) -> Vec2 {
        list.shapes
            .iter()
            .find_map(|s| match s {
                Shape::Text { pos, size, text, .. } if text == label => {
                    Some(*pos + Vec2::new(font.text_width(text, *size) / 2.0, size / 2.0))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("нет кнопки `{label}`"))
    }

    fn click(at: Vec2, button: usize) -> UiInput {
        let mut i = input(&[]);
        i.mouse = at;
        i.pressed[button] = true;
        i.down[button] = true;
        i
    }

    #[test]
    fn font_has_cyrillic() {
        let f = font();
        assert!(f.text_width("Привет", 16.0) > 40.0);
        assert_eq!(f.text_width("ab", 16.0), 2.0 * f.text_width("a", 16.0));
    }

    #[test]
    fn menu_navigation() {
        let (font, mut w, mut gui) = (font(), world(), Gui::default());
        let (list, _) = frame(&mut gui, &mut w, &font, &input(&[]));
        let play = find(&list, "Играть", &font);
        frame(&mut gui, &mut w, &font, &click(play, 0));
        assert_eq!(gui.screen(), Screen::Playing);
        assert!(!gui.wants_cursor());

        frame(&mut gui, &mut w, &font, &input(&[UiKey::Escape]));
        assert_eq!(gui.screen(), Screen::Paused);
        assert!(gui.pauses_game());
        let (list, _) = frame(&mut gui, &mut w, &font, &input(&[]));
        frame(&mut gui, &mut w, &font, &click(find(&list, "Настройки", &font), 0));
        assert_eq!(gui.screen(), Screen::Settings);
        frame(&mut gui, &mut w, &font, &input(&[UiKey::Escape]));
        assert_eq!(gui.screen(), Screen::Paused, "из настроек — обратно в паузу");

        let (list, _) = frame(&mut gui, &mut w, &font, &input(&[]));
        let (_, actions) = frame(&mut gui, &mut w, &font, &click(find(&list, "Выход из игры", &font), 0));
        assert_eq!(actions, vec![Action::Quit]);
    }

    #[test]
    fn inventory_drag_from_palette() {
        let (font, mut w, mut gui) = (font(), world(), Gui::new(Screen::Playing));
        frame(&mut gui, &mut w, &font, &input(&[UiKey::Inventory]));
        assert_eq!(gui.screen(), Screen::Inventory);
        let (list, _) = frame(&mut gui, &mut w, &font, &input(&[]));
        // Первый слот палитры — сразу под заголовком «Блоки».
        let title = find(&list, "Блоки", &font);
        let s = gui.theme.slot_size;
        let palette0 = Vec2::new(title.x - font.text_width("Блоки", 18.0) / 2.0 + s / 2.0, title.y + 30.0 + s / 2.0);
        frame(&mut gui, &mut w, &font, &click(palette0, 0));
        assert_eq!(w.inv.cursor, Some(Stack::new(1, 64)));

        // Положить в первый слот панели быстрого доступа (нижний ряд окна).
        let (list, _) = frame(&mut gui, &mut w, &font, &input(&[]));
        let inv_title = find(&list, "Инвентарь", &font);
        let hotbar0 = Vec2::new(palette0.x, inv_title.y + 30.0 + 3.0 * s + 8.0 + s / 2.0);
        frame(&mut gui, &mut w, &font, &click(hotbar0, 0));
        assert_eq!((w.inv.cursor, w.inv.slots[0]), (None, Some(Stack::new(1, 64))));
    }

    #[test]
    fn tessellation_produces_quads() {
        let (font, mut w, mut gui) = (font(), world(), Gui::default());
        let (list, _) = frame(&mut gui, &mut w, &font, &input(&[]));
        let mesh = tessellate(&list, &font, 2.0);
        assert_eq!(mesh.indices.len() * 4, mesh.vertices.len() * 6);
        assert!(mesh.vertices.iter().all(|v| v.pos[0] <= 2560.0 && v.pos[1] <= 1440.0));
    }
}
