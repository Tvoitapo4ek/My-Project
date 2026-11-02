//! Home screen: tiles of every section with a live summary line.

use bevy::prelude::*;

use super::{HoloAction, HoloSet, LiveText, heading};
use crate::{
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo,
        projection::{HoloMats, spawn_core},
        style::{self, CYAN, DIM, WHITE},
    },
    items::inventory::Inventory,
    player::Player,
    survival::{damage::Health, needs::Vitals},
};

fn summary(id: &str) -> Option<fn(&mut World) -> String> {
    Some(match id {
        "inventory" => |w: &mut World| {
            let mut q = w.query_filtered::<&Inventory, With<Player>>();
            q.iter(w)
                .next()
                .map(|i| {
                    let used = i.slots.iter().filter(|s| s.is_some()).count();
                    format!("занято {used} из {}", i.slots.len())
                })
                .unwrap_or_default()
        },
        "journal" => |w: &mut World| {
            let open = w.get_resource::<super::journal::JournalState>().map(|j| j.unlocked.len()).unwrap_or(0);
            format!("открыто {open} из {}", crate::content::registry().journal.len())
        },
        "map" => |w: &mut World| {
            let pct = w.get_resource::<super::map::ExploredMap>().map(|m| m.percent()).unwrap_or(0.0);
            format!("исследовано {pct:.0}%")
        },
        "character" => |w: &mut World| {
            let mut q = w.query_filtered::<(&Health, &Vitals), With<Player>>();
            q.iter(w)
                .next()
                .map(|(h, v)| format!("здоровье {:.0} · голод {:.0} · жажда {:.0}", h.hp, v.hunger, v.thirst))
                .unwrap_or_default()
        },
        "blueprints" => |w: &mut World| {
            let n = w.get_resource::<super::blueprints::BlueprintLibrary>().map(|l| l.entries.len()).unwrap_or(0);
            format!("сохранено: {n}")
        },
        _ => return None,
    })
}

fn build(
    mut msgs: MessageReader<BuildSection>,
    mut commands: Commands,
    sections: Res<HoloSections>,
    mats: Option<Res<HoloMats>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for m in msgs.read().filter(|m| m.id == "home") {
        commands.entity(m.content).with_children(|p| {
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() })
                .with_children(|g| {
                    for s in sections.0.iter().filter(|s| s.id != "home") {
                        let id = s.id;
                        g.spawn((
                            style::button(Node {
                                width: px(228),
                                height: px(112),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::FlexStart,
                                justify_content: JustifyContent::SpaceBetween,
                                padding: UiRect::all(px(12)),
                                ..default()
                            }),
                            HoloAction::new(move |w| w.resource_mut::<HoloNav>().go(id)),
                        ))
                        .with_children(|t| {
                            t.spawn(style::label(s.glyph, 26.0, CYAN));
                            t.spawn(style::label(s.title, 18.0, WHITE));
                            let mut line = t.spawn(style::label("", 12.0, DIM));
                            if let Some(f) = summary(id) {
                                line.insert(LiveText(f));
                            }
                        });
                    }
                });
        });
        commands.entity(m.detail).with_children(|p| {
            heading(p, "Состояние");
            p.spawn((style::label("", 14.0, WHITE), LiveText(summary("character").unwrap())));
            p.spawn((style::label("", 13.0, DIM), LiveText(position_line)));
            super::rule(p);
            heading(p, "Подсказка");
            p.spawn(style::label(
                "I — инвентарь, J — журнал, N — карта.\nПанели можно осматривать, удерживая ПКМ; проекция догонит взгляд.",
                13.0,
                DIM,
            ));
        });
        if let Some(mats) = &mats {
            spawn_core(&mut commands, m.projection, &mut meshes, mats);
        }
    }
}

fn position_line(w: &mut World) -> String {
    let mut q = w.query_filtered::<&Transform, With<Player>>();
    q.iter(w)
        .next()
        .map(|t| format!("Координаты: {:.0} {:.0} {:.0}", t.translation.x, t.translation.y, t.translation.z))
        .unwrap_or_default()
}

pub struct HomeSection;

impl Plugin for HomeSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo {
            id: "home",
            title: "Главная",
            glyph: "⌂",
            order: 0,
        });
        app.add_systems(Update, build.in_set(HoloSet::Build));
    }
}
