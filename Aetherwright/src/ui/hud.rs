//! Gameplay HUD built from independent panels. Each panel is a root node
//! tagged with `HudPanel`; visibility comes from `HudConfig` (data) and the
//! global F1 toggle, so panels can be added/removed without touching others.

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use super::{slots::{spawn_slot, SlotSource}, theme};
use crate::{
    core::{
        config::GameConfig,
        events::{Notify, NotifyKind},
        states::AppState,
    },
    items::inventory::{Inventory, HOTBAR},
    player::Player,
    tools::{
        manipulator::{Manipulator, MiningProgress},
        scanning::{ScanResult, Scannable},
        ActiveTool, EntityTarget,
    },
    world::interaction::BlockTarget,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudPanel {
    Crosshair,
    Hotbar,
    ToolLine,
    TargetInfo,
    ScanPanel,
    Notifications,
    Vitals,
    Energy,
}

#[derive(Resource)]
pub struct HudVisible(pub bool);

#[derive(Component)]
struct NotificationLine {
    ttl: f32,
}

#[derive(Component)]
struct ToolLineText;

#[derive(Component)]
struct TargetText;

#[derive(Component)]
struct ProgressFill;

#[derive(Component)]
struct ScanText;

#[derive(Component, Clone, Copy)]
enum VitalBar {
    Health,
    Hunger,
    Thirst,
    Temperature,
    Aether,
}

#[derive(Component)]
struct VitalLabel(VitalBar);

fn root(panel: HudPanel, node: Node) -> impl Bundle {
    (panel, Node { position_type: PositionType::Absolute, ..node }, Pickable::IGNORE)
}

fn spawn_hud(mut commands: Commands) {
    // Crosshair.
    commands
        .spawn(root(HudPanel::Crosshair, Node {
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        }))
        .with_children(|p| {
            p.spawn((
                Node { width: px(6), height: px(6), border: UiRect::all(px(1)), ..default() },
                BorderColor::all(theme::ACCENT),
                BackgroundColor(Color::srgba(0.3, 0.9, 1.0, 0.25)),
            ));
        });

    // Hotbar + tool line (bottom centre).
    commands
        .spawn(root(HudPanel::Hotbar, Node {
            width: percent(100),
            bottom: px(14),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            ..default()
        }))
        .with_children(|p| {
            p.spawn((HudPanel::ToolLine, ToolLineText, Text::new(""), theme::font(15.0), TextColor(theme::TEXT)));
            p.spawn(Node { column_gap: px(4), ..default() }).with_children(|row| {
                for i in 0..HOTBAR {
                    spawn_slot(row, SlotSource::Player(i), true);
                }
            });
        });

    // Target info (top centre).
    commands
        .spawn(root(HudPanel::TargetInfo, Node {
            width: percent(100),
            top: px(14),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4),
            ..default()
        }))
        .with_children(|p| {
            p.spawn((TargetText, Text::new(""), theme::font(16.0), TextColor(theme::TEXT)));
            p.spawn((
                Node { width: px(160), height: px(5), border: UiRect::all(px(1)), ..default() },
                BorderColor::all(theme::PANEL_BORDER),
                BackgroundColor(theme::PANEL_BG),
            ))
            .with_children(|b| {
                b.spawn((ProgressFill, Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(theme::ACCENT)));
            });
        });

    // Scanner results (right).
    commands
        .spawn(root(HudPanel::ScanPanel, Node { right: px(16), top: percent(30), max_width: px(360), ..default() }))
        .insert((BackgroundColor(theme::PANEL_BG), BorderColor::all(theme::PANEL_BORDER), Visibility::Hidden))
        .with_children(|p| {
            p.spawn((ScanText, Text::new(""), theme::font(14.0), TextColor(theme::TEXT), Node { margin: UiRect::all(px(8)), ..default() }));
        });

    // Vitals (bottom left): bars for health and needs + aether.
    commands
        .spawn(root(HudPanel::Vitals, Node { left: px(16), bottom: px(16), flex_direction: FlexDirection::Column, row_gap: px(5), ..default() }))
        .with_children(|p| {
            for (bar, label, color) in [
                (VitalBar::Health, "Здоровье", Color::srgb(0.95, 0.35, 0.35)),
                (VitalBar::Hunger, "Сытость", Color::srgb(0.95, 0.7, 0.3)),
                (VitalBar::Thirst, "Вода", Color::srgb(0.35, 0.65, 1.0)),
                (VitalBar::Temperature, "Темп.", Color::srgb(0.5, 0.95, 0.6)),
                (VitalBar::Aether, "Эфир", theme::MAGIC),
            ] {
                p.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                    row.spawn((VitalLabel(bar), Text::new(label), theme::font(12.0), TextColor(theme::TEXT), Node { width: px(150), ..default() }));
                    row.spawn((
                        Node { width: px(140), height: px(8), border: UiRect::all(px(1)), ..default() },
                        BorderColor::all(theme::PANEL_BORDER),
                        BackgroundColor(theme::PANEL_BG),
                    ))
                    .with_children(|b| {
                        b.spawn((bar, Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(color)));
                    });
                });
            }
        });

    commands.spawn(root(HudPanel::Notifications, Node {
        right: px(16),
        top: px(16),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexEnd,
        row_gap: px(4),
        ..default()
    }));
}

fn panel_enabled(cfg: &GameConfig, panel: HudPanel) -> bool {
    let h = &cfg.hud;
    match panel {
        HudPanel::Crosshair => true,
        HudPanel::Hotbar => h.hotbar,
        HudPanel::ToolLine => h.tool_line,
        HudPanel::TargetInfo => h.target_info,
        HudPanel::ScanPanel => h.scan_panel,
        HudPanel::Notifications => h.notifications,
        HudPanel::Vitals => h.vitals,
        HudPanel::Energy => h.energy,
    }
}

fn apply_panel_visibility(
    keys: Res<ButtonInput<KeyCode>>,
    cfg: Res<GameConfig>,
    mut visible: ResMut<HudVisible>,
    scan: Res<ScanResult>,
    state: Res<State<AppState>>,
    mut panels: Query<(&HudPanel, &mut Visibility)>,
    mut ui_scale: ResMut<UiScale>,
) {
    if keys.just_pressed(KeyCode::F1) {
        visible.0 = !visible.0;
    }
    ui_scale.0 = cfg.hud.scale.clamp(0.5, 3.0);
    let in_game = *state.get() == AppState::InGame;
    for (panel, mut vis) in &mut panels {
        let mut on = visible.0 && in_game && panel_enabled(&cfg, *panel);
        if *panel == HudPanel::ScanPanel {
            on &= scan.ttl > 0.0 && !scan.title.is_empty();
        }
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hud_text(
    tool: Res<ActiveTool>,
    block: Res<BlockTarget>,
    entity: Res<EntityTarget>,
    progress: Res<MiningProgress>,
    scan: Res<ScanResult>,
    gun: Res<crate::physics::physics_gun::PhysGun>,
    spells: Res<crate::magic::spells::SpellState>,
    player: Query<(&Inventory, &Manipulator, Option<&crate::magic::spells::AetherReserve>), With<Player>>,
    scannables: Query<(Option<&Scannable>, Option<&Name>, Option<&LinearVelocity>)>,
    mut texts: ParamSet<(
        Single<&mut Text, With<ToolLineText>>,
        Single<&mut Text, With<TargetText>>,
        Single<&mut Text, With<ScanText>>,
    )>,
    mut fill: Single<&mut Node, With<ProgressFill>>,
) {
    let Ok((inv, m, reserve)) = player.single() else { return };
    let line = match *tool {
        ActiveTool::Manipulator => format!(
            "Манипулятор · {}  [Q]   ·  бурение {}  ·  резка {}",
            m.mode.label(),
            if m.drill_tier >= 2 { "II" } else { "I" },
            if m.cutter { "есть" } else { "—" }
        ),
        ActiveTool::PhysGun => format!(
            "Гравизахват · {}  [Q]  ·  {}",
            gun.mode.label(),
            match gun.mode {
                crate::physics::physics_gun::GunMode::Grab => "ЛКМ держать · колесо дистанция · R+мышь вращать · ПКМ фиксировать",
                crate::physics::physics_gun::GunMode::Weld => "ЛКМ: деталь, затем основание",
                crate::physics::physics_gun::GunMode::Hinge => "ЛКМ: деталь (её грань), затем грань основания",
                crate::physics::physics_gun::GunMode::Unlink => "ЛКМ: снять все соединения объекта",
            }
        ),
        ActiveTool::Focus => format!(
            "Эфирный фокус · {}  [Q]  ·  стоимость {:.0}  ·  эфир {:.0}/{:.0}",
            spells.selected.label(),
            spells.selected.cost(),
            reserve.map(|r| r.amount).unwrap_or(0.0),
            reserve.map(|r| r.max).unwrap_or(0.0)
        ),
        ActiveTool::Hand => "Рука (манипулятор, слабая добыча)".to_string(),
        ActiveTool::PlaceBlock(item, _) => format!("{} — ПКМ поставить, ЛКМ добыть", item.def().name),
        _ => inv.selected_item().map(|i| i.def().name.to_string()).unwrap_or_default(),
    };
    texts.p0().0 = line;

    let target = if let Some(e) = entity.entity {
        match scannables.get(e) {
            Ok((Some(s), _, _)) => s.title.clone(),
            Ok((None, Some(n), _)) => n.as_str().to_string(),
            _ => String::new(),
        }
    } else if let Some(hit) = block.0 {
        hit.block.def().name.to_string()
    } else {
        String::new()
    };
    texts.p1().0 = target;

    let k = if progress.target.is_some() { (progress.progress / progress.required.max(1e-3)).clamp(0.0, 1.0) } else { 0.0 };
    fill.width = percent(k * 100.0);

    if scan.ttl > 0.0 {
        texts.p2().0 = format!("◈ {}\n{}", scan.title, scan.lines.join("\n"));
    }
}

fn show_notifications(
    mut commands: Commands,
    mut reader: MessageReader<Notify>,
    feed: Query<(Entity, &HudPanel)>,
) {
    let Some((feed, _)) = feed.iter().find(|(_, p)| **p == HudPanel::Notifications) else { return };
    for n in reader.read() {
        info!("[notify] {}", n.text);
        let color = match n.kind {
            NotifyKind::Info => theme::TEXT,
            NotifyKind::Success => theme::OK,
            NotifyKind::Warning => theme::WARN,
        };
        let line = commands
            .spawn((NotificationLine { ttl: 4.0 }, theme::panel(Node { padding: UiRect::axes(px(8), px(3)), ..default() })))
            .with_children(|p| {
                p.spawn((Text::new(n.text.clone()), theme::font(15.0), TextColor(color)));
            })
            .id();
        commands.entity(feed).add_child(line);
    }
}

fn expire_notifications(mut commands: Commands, time: Res<Time>, mut lines: Query<(Entity, &mut NotificationLine)>) {
    for (e, mut l) in &mut lines {
        l.ttl -= time.delta_secs();
        if l.ttl <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

fn update_vitals(
    mode: Res<State<crate::core::states::GameMode>>,
    player: Query<(&crate::survival::damage::Health, &crate::survival::needs::Vitals, &crate::magic::spells::AetherReserve), With<Player>>,
    mut bars: Query<(&VitalBar, &mut Node)>,
    mut labels: Query<(&VitalLabel, &mut Text)>,
) {
    let Ok((h, v, a)) = player.single() else { return };
    let creative = *mode.get() == crate::core::states::GameMode::Creative;
    let value = |b: VitalBar| match b {
        VitalBar::Health => (h.hp / h.max, format!("Здоровье {:.0}", h.hp)),
        VitalBar::Hunger => (v.hunger / 100.0, format!("Сытость {:.0}%", v.hunger)),
        VitalBar::Thirst => (v.thirst / 100.0, format!("Вода {:.0}%", v.thirst)),
        VitalBar::Temperature => (((v.body_temp - 33.0) / 6.0).clamp(0.0, 1.0), format!("Тело {:.1}°C", v.body_temp)),
        VitalBar::Aether => (a.amount / a.max, format!("Эфир {:.0}", a.amount)),
    };
    for (b, mut n) in &mut bars {
        n.width = percent(value(*b).0.clamp(0.0, 1.0) * 100.0);
    }
    for (l, mut t) in &mut labels {
        let mut s = value(l.0).1;
        if creative && !matches!(l.0, VitalBar::Aether) {
            s += " (креатив)";
        }
        if t.0 != s {
            t.0 = s;
        }
    }
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HudVisible(true))
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (
                    apply_panel_visibility,
                    update_hud_text.run_if(in_state(AppState::InGame)),
                    update_vitals.run_if(in_state(AppState::InGame)),
                    show_notifications,
                    expire_notifications,
                ),
            );
    }
}
