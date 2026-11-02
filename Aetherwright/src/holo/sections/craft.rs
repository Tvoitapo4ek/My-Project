//! Assembly section: hand and workbench recipes with availability.
//! Smelter / grinder are machines with their own panels.

use bevy::prelude::*;

use super::{HoloAction, HoloSet, action_button, heading, item_icon, rule, scroll_column};
use crate::{
    content::Station,
    core::{events::Notify, rules::ModeRules},
    crafting::{bench_available, craft_recipe, recipes::recipes},
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo, section_active,
        style::{self, CYAN, DIM, WHITE},
    },
    items::{ItemId, inventory::Inventory},
    player::Player,
    world::materials::BlockMaterials,
};

#[derive(Resource, Default, Debug)]
pub struct CraftUi {
    pub selected: Option<usize>,
}

fn build(world: &mut World) {
    let msgs: Vec<BuildSection> = world
        .resource_mut::<Messages<BuildSection>>()
        .iter_current_update_messages()
        .filter(|m| m.id == "craft")
        .copied()
        .collect();
    let Some(m) = msgs.last().copied() else {
        return;
    };
    let bench = bench_available(world);
    let consume = world.resource::<ModeRules>().consume_items;
    let inv = {
        let mut q = world.query_filtered::<&Inventory, With<Player>>();
        q.iter(world).next().cloned().unwrap_or_default()
    };
    let selected = world.resource::<CraftUi>().selected;
    let blocks = world.get_resource::<BlockMaterials>().map(|b| BlockMaterials { by_tex: b.by_tex.clone(), images: b.images.clone() });
    let mut commands = world.commands();
    commands.entity(m.content).with_children(|p| {
        p.spawn(style::label(
            if bench { "Верстак рядом: доступны все рецепты сборки" } else { "Верстака рядом нет: только ручные рецепты" },
            13.0,
            DIM,
        ));
        rule(p);
        p.spawn(scroll_column(Node { flex_grow: 1.0, min_height: px(0), row_gap: px(4), ..default() })).with_children(|list| {
            for (i, r) in recipes().iter().enumerate() {
                if !matches!(r.station, Station::Hand | Station::Workbench) {
                    continue;
                }
                let available = r.station == Station::Hand || bench;
                let afford = !consume || r.can_afford(&inv);
                let mark = if available && afford { "●" } else if available { "○" } else { "·" };
                let inputs: Vec<String> = r.inputs.iter().map(|(k, n)| format!("{} {n}", ItemId::k(k).def().name)).collect();
                list.spawn((
                    style::button(Node { width: percent(100), justify_content: JustifyContent::FlexStart, column_gap: px(10), ..default() }),
                    HoloAction::new(move |w| w.resource_mut::<CraftUi>().selected = Some(i)),
                ))
                .insert(if selected == Some(i) { BackgroundColor(style::SELECTED) } else { BackgroundColor(style::GLASS_LIGHT) })
                .with_children(|row| {
                    row.spawn(style::label(mark, 14.0, if available && afford { CYAN } else { DIM }));
                    item_icon(row, r.output_item(), 26.0, blocks.as_ref());
                    row.spawn(style::label(
                        format!("{} ×{}   ←   {}   [{}]", r.output_item().def().name, r.output.1, inputs.join(", "), r.station.label()),
                        13.0,
                        if available && afford { WHITE } else { DIM },
                    ));
                });
            }
        });
        p.spawn(style::label("● можно собрать · ○ не хватает материалов · · нужен верстак", 12.0, DIM));
    });
    commands.entity(m.detail).with_children(|p| {
        heading(p, "Рецепт");
        let Some(i) = selected else {
            p.spawn(style::label("Выберите рецепт.", 13.0, DIM));
            return;
        };
        let r = &recipes()[i];
        p.spawn(Node { column_gap: px(10), align_items: AlignItems::Center, ..default() }).with_children(|h| {
            item_icon(h, r.output_item(), 48.0, blocks.as_ref());
            h.spawn(style::label(format!("{} ×{}", r.output_item().def().name, r.output.1), 17.0, WHITE));
        });
        p.spawn(style::label(r.output_item().def().info.clone(), 13.0, DIM));
        rule(p);
        p.spawn(style::label("Нужно (есть / нужно):", 12.0, CYAN));
        for (k, n) in &r.inputs {
            let item = ItemId::k(k);
            let have = inv.count(item);
            p.spawn(style::label(format!("{} {have}/{n}", item.def().name), 13.0, if have >= *n || !consume { WHITE } else { style::AMBER }));
        }
        p.spawn(style::label(format!("Где: {}", r.station.label()), 12.0, DIM));
        action_button(p, "Собрать", 15.0, Node { width: percent(100), margin: UiRect::top(px(8)), ..default() }, false, HoloAction::new(move |w| {
            let msg = match craft_recipe(w, i) {
                Ok(s) => Notify::ok(s),
                Err(s) => Notify::warn(s),
            };
            w.write_message(msg);
        }));
    });
}

fn watch(q: Query<(), (With<Player>, Changed<Inventory>)>, mut nav: ResMut<HoloNav>) {
    if !q.is_empty() {
        nav.rebuild = true;
    }
}

pub struct CraftSection;

impl Plugin for CraftSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>().init_resource::<CraftUi>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo { id: "craft", title: "Сборка", glyph: "⊕", order: 15 });
        app.add_systems(Update, build.in_set(HoloSet::Build))
            .add_systems(Update, watch.before(HoloSet::Frame).run_if(section_active("craft")));
    }
}
