//! Settings section: controls, camera and the customizable HUD (each
//! panel's position, size, opacity and visibility). Changes apply at once;
//! «Сохранить» writes `assets/config/game.json`.

use bevy::prelude::*;

use super::{HoloAction, HoloSet, action_button, heading, rule, scroll_column};
use crate::{
    core::{
        config::{GameConfig, HudLayout},
        events::Notify,
        resources::MainCamera,
    },
    holo::{
        BuildSection, HoloSections, SectionInfo,
        style::{self, CYAN, DIM, WHITE},
    },
    player::camera::CameraRig,
    ui::hud::HudPanel,
};

#[derive(Resource, Debug, Default)]
pub struct SettingsUi {
    pub hud_panel: Option<HudPanel>,
}

type Getter = fn(&GameConfig) -> String;
type Setter = fn(&mut GameConfig, f32);

/// A row: label, value, − / + buttons.
fn stepper(p: &mut ChildSpawnerCommands, label: &str, cfg: &GameConfig, get: Getter, set: Setter, step: f32) {
    p.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
        r.spawn((style::label(label, 14.0, WHITE), Node { width: px(260), ..default() }));
        action_button(r, "−", 15.0, Node { width: px(34), ..default() }, false, HoloAction::new(move |w| change(w, set, -step)));
        r.spawn((style::label(get(cfg), 14.0, CYAN), Node { width: px(90), justify_content: JustifyContent::Center, ..default() }));
        action_button(r, "+", 15.0, Node { width: px(34), ..default() }, false, HoloAction::new(move |w| change(w, set, step)));
    });
}

fn change(w: &mut World, set: Setter, d: f32) {
    let mut cfg = w.resource::<GameConfig>().clone();
    set(&mut cfg, d);
    let cfg = cfg.sanitized();
    apply_live(w, &cfg);
    *w.resource_mut::<GameConfig>() = cfg;
}

fn toggle(p: &mut ChildSpawnerCommands, label: &str, on: bool, f: fn(&mut GameConfig)) {
    action_button(p, format!("{} {label}", if on { "■" } else { "□" }), 14.0, Node { width: px(250), justify_content: JustifyContent::FlexStart, ..default() }, on, HoloAction::new(move |w| {
        let mut cfg = w.resource::<GameConfig>().clone();
        f(&mut cfg);
        *w.resource_mut::<GameConfig>() = cfg;
    }));
}

/// Pushes camera settings to the running camera.
fn apply_live(w: &mut World, cfg: &GameConfig) {
    let mut q = w.query_filtered::<&mut Projection, With<MainCamera>>();
    if let Some(mut p) = q.iter_mut(w).next()
        && let Projection::Perspective(pp) = &mut *p
    {
        pp.fov = cfg.camera.fov_degrees.to_radians();
    }
    w.resource_mut::<CameraRig>().distance = cfg.camera.third_person_distance;
}

fn layout_of(cfg: &mut GameConfig, p: HudPanel) -> &mut HudLayout {
    cfg.hud.layout.entry(p.key().to_string()).or_default()
}

fn hud_change(p: HudPanel, f: fn(&mut HudLayout)) -> HoloAction {
    HoloAction::new(move |w| {
        let mut cfg = w.resource::<GameConfig>().clone();
        f(layout_of(&mut cfg, p));
        *w.resource_mut::<GameConfig>() = cfg.sanitized();
    })
}

fn build(mut msgs: MessageReader<BuildSection>, mut commands: Commands, cfg: Res<GameConfig>, ui: Res<SettingsUi>) {
    let Some(m) = msgs.read().filter(|m| m.id == "settings").last().copied() else {
        return;
    };
    commands.entity(m.content).with_children(|p| {
        p.spawn(scroll_column(Node { flex_grow: 1.0, min_height: px(0), row_gap: px(6), ..default() })).with_children(|c| {
            heading(c, "Управление");
            stepper(c, "Чувствительность мыши", &cfg, |c| format!("{:.2}", c.controls.mouse_sensitivity * 1000.0), |c, d| {
                c.controls.mouse_sensitivity = (c.controls.mouse_sensitivity + d).clamp(0.0004, 0.008)
            }, 0.0002);
            toggle(c, "Инверсия оси Y", cfg.controls.invert_y, |c| c.controls.invert_y = !c.controls.invert_y);
            rule(c);
            heading(c, "Камера");
            stepper(c, "Поле зрения", &cfg, |c| format!("{:.0}°", c.camera.fov_degrees), |c, d| c.camera.fov_degrees = (c.camera.fov_degrees + d).clamp(55.0, 110.0), 5.0);
            stepper(c, "Дистанция 3-го лица", &cfg, |c| format!("{:.1} м", c.camera.third_person_distance), |c, d| c.camera.third_person_distance += d, 0.5);
            rule(c);
            heading(c, "Интерфейс (HUD)");
            stepper(c, "Масштаб HUD", &cfg, |c| format!("{:.0}%", c.hud.scale * 100.0), |c, d| c.hud.scale += d, 0.1);
            stepper(c, "Прозрачность HUD", &cfg, |c| format!("{:.0}%", c.hud.opacity * 100.0), |c, d| c.hud.opacity += d, 0.1);
            c.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|g| {
                toggle(g, "Быстрые слоты", cfg.hud.hotbar, |c| c.hud.hotbar = !c.hud.hotbar);
                toggle(g, "Показатели", cfg.hud.vitals, |c| c.hud.vitals = !c.hud.vitals);
                toggle(g, "Цель", cfg.hud.target_info, |c| c.hud.target_info = !c.hud.target_info);
                toggle(g, "Сканер", cfg.hud.scan_panel, |c| c.hud.scan_panel = !c.hud.scan_panel);
                toggle(g, "Уведомления", cfg.hud.notifications, |c| c.hud.notifications = !c.hud.notifications);
                toggle(g, "Строка инструмента", cfg.hud.tool_line, |c| c.hud.tool_line = !c.hud.tool_line);
                toggle(g, "Энергия", cfg.hud.energy, |c| c.hud.energy = !c.hud.energy);
            });
            c.spawn(style::label("Расположение отдельных панелей — справа.", 12.0, DIM));
        });
        p.spawn(Node { column_gap: px(8), ..default() }).with_children(|r| {
            action_button(r, "Сохранить настройки", 14.0, Node::default(), false, HoloAction::new(|w| {
                let msg = match w.resource::<GameConfig>().save() {
                    Ok(()) => Notify::ok("Настройки сохранены"),
                    Err(e) => Notify::warn(format!("Не удалось сохранить: {e}")),
                };
                w.write_message(msg);
            }));
            action_button(r, "Сбросить HUD", 14.0, Node::default(), false, HoloAction::new(|w| {
                let mut cfg = w.resource::<GameConfig>().clone();
                cfg.hud = Default::default();
                *w.resource_mut::<GameConfig>() = cfg;
            }));
        });
    });

    commands.entity(m.detail).with_children(|p| {
        heading(p, "Панели HUD");
        p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|g| {
            for panel in HudPanel::ALL {
                action_button(g, panel.label(), 12.0, Node::default(), ui.hud_panel == Some(panel), HoloAction::new(move |w| {
                    let mut ui = w.resource_mut::<SettingsUi>();
                    ui.hud_panel = if ui.hud_panel == Some(panel) { None } else { Some(panel) };
                }));
            }
        });
        rule(p);
        let Some(panel) = ui.hud_panel else {
            p.spawn(style::label("Выберите панель: её можно сдвинуть, увеличить и сделать прозрачнее. Изменения видны сразу.", 13.0, DIM));
            return;
        };
        let l = cfg.hud.layout.get(panel.key()).copied().unwrap_or_default();
        p.spawn(style::label(format!("{}: сдвиг {:.0}, {:.0} · размер {:.0}% · непрозрачность {:.0}%", panel.label(), l.dx, l.dy, l.scale * 100.0, l.opacity * 100.0), 13.0, WHITE));
        p.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
            action_button(r, "←", 15.0, Node::default(), false, hud_change(panel, |l| l.dx -= 20.0));
            action_button(r, "↑", 15.0, Node::default(), false, hud_change(panel, |l| l.dy -= 20.0));
            action_button(r, "↓", 15.0, Node::default(), false, hud_change(panel, |l| l.dy += 20.0));
            action_button(r, "→", 15.0, Node::default(), false, hud_change(panel, |l| l.dx += 20.0));
        });
        p.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
            action_button(r, "размер −", 13.0, Node::default(), false, hud_change(panel, |l| l.scale -= 0.1));
            action_button(r, "размер +", 13.0, Node::default(), false, hud_change(panel, |l| l.scale += 0.1));
        });
        p.spawn(Node { column_gap: px(4), ..default() }).with_children(|r| {
            action_button(r, "прозрачнее", 13.0, Node::default(), false, hud_change(panel, |l| l.opacity -= 0.15));
            action_button(r, "плотнее", 13.0, Node::default(), false, hud_change(panel, |l| l.opacity += 0.15));
        });
        action_button(p, "Сбросить панель", 13.0, Node::default(), false, HoloAction::new(move |w| {
            let mut cfg = w.resource::<GameConfig>().clone();
            cfg.hud.layout.remove(panel.key());
            *w.resource_mut::<GameConfig>() = cfg;
        }));
    });
}

pub struct SettingsSection;

impl Plugin for SettingsSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>().init_resource::<SettingsUi>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo { id: "settings", title: "Настройки", glyph: "⊙", order: 90 });
        app.add_systems(Update, build.in_set(HoloSet::Build));
    }
}
