//! Equipment section: a 3D projection of the character with the worn
//! items, slots around it, and an inspection panel. Worn slots are
//! interactive; hand (weapon / tool) and manipulator modules are shown
//! from their own state — combat still uses the item in hand.

use bevy::prelude::*;

use super::{HoloAction, HoloSet, action_button, heading, item_icon, rule};
use crate::{
    content::Category,
    holo::{
        BuildSection, HoloNav, HoloSections, SectionInfo, section_active,
        projection::{DragView, HoloMats, spawn_figure},
        style::{self, CYAN, DIM, LINE, LINE_DIM, SELECTED, WHITE},
    },
    items::{
        Use,
        equipment::{Equipment, SlotRef, wear_slot},
        inventory::Inventory,
    },
    player::Player,
    tools::manipulator::Manipulator,
    world::materials::BlockMaterials,
};

#[derive(Resource, Debug, Default)]
pub struct EquipUi {
    pub inspect: Option<SlotRef>,
}

fn slot_button(p: &mut ChildSpawnerCommands, s: SlotRef, eq: &Equipment, ui: &EquipUi, blocks: Option<&BlockMaterials>) {
    let worn = eq.get(s);
    let sel = ui.inspect == Some(s);
    p.spawn((
        style::button(Node {
            width: px(250),
            height: px(64),
            column_gap: px(10),
            justify_content: JustifyContent::FlexStart,
            ..default()
        }),
        HoloAction::new(move |w| {
            let mut ui = w.resource_mut::<EquipUi>();
            ui.inspect = if ui.inspect == Some(s) { None } else { Some(s) };
        }),
    ))
    .insert(if sel { (BackgroundColor(SELECTED), BorderColor::all(LINE)) } else { (BackgroundColor(style::GLASS_LIGHT), BorderColor::all(LINE_DIM)) })
    .with_children(|b| {
        match worn {
            Some(st) => item_icon(b, st.item, 40.0, blocks),
            None => {
                b.spawn((
                    Node { width: px(40), height: px(40), border: UiRect::all(px(1)), ..default() },
                    BorderColor::all(LINE_DIM),
                    Pickable::IGNORE,
                ));
            }
        }
        b.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
            t.spawn(style::label(s.label(), 13.0, CYAN));
            t.spawn(style::label(worn.map(|st| st.item.def().name.clone()).unwrap_or_else(|| "— пусто —".into()), 14.0, if worn.is_some() { WHITE } else { DIM }));
        });
    });
}

fn info_box(p: &mut ChildSpawnerCommands, title: &str, value: String) {
    p.spawn(style::frame(Node { width: px(250), flex_direction: FlexDirection::Column, padding: UiRect::all(px(8)), ..default() }))
        .with_children(|b| {
            b.spawn(style::label(title, 13.0, CYAN));
            b.spawn(style::label(value, 14.0, WHITE));
        });
}

#[allow(clippy::too_many_arguments)]
fn build(
    mut msgs: MessageReader<BuildSection>,
    mut commands: Commands,
    ui: Res<EquipUi>,
    player: Query<(&Equipment, &Inventory, &Manipulator), With<Player>>,
    blocks: Option<Res<BlockMaterials>>,
    mats: Option<Res<HoloMats>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(m) = msgs.read().filter(|m| m.id == "equipment").last().copied() else {
        return;
    };
    let Ok((eq, inv, manip)) = player.single() else {
        return;
    };
    let blocks = blocks.as_deref();
    let in_hand = inv.selected_stack();
    commands.entity(m.content).with_children(|p| {
        p.spawn(Node { flex_grow: 1.0, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() }).with_children(|c| {
                for s in &SlotRef::ALL[..4] {
                    slot_button(c, *s, eq, &ui, blocks);
                }
            });
            // Centre: the figure is projected in front of this area; drag
            // rotates it.
            row.spawn((
                Node { flex_grow: 1.0, align_items: AlignItems::FlexEnd, justify_content: JustifyContent::Center, ..default() },
                DragView("equipment"),
            ))
            .with_children(|c| {
                c.spawn(style::label("перетаскивание — поворот · колесо — масштаб", 12.0, DIM));
            });
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() }).with_children(|c| {
                for s in &SlotRef::ALL[4..] {
                    slot_button(c, *s, eq, &ui, blocks);
                }
                let hand = in_hand
                    .filter(|s| matches!(s.item.def().category, Category::Weapon | Category::Tool | Category::Magic))
                    .map(|s| s.item.def().name.clone())
                    .unwrap_or_else(|| "— пусто —".into());
                info_box(c, "Оружие / инструмент в руке", hand);
                let mut modules = vec![format!("Бур: уровень {}", manip.drill_tier)];
                modules.push(if manip.cutter { "Резак: установлен".into() } else { "Резак: нет".into() });
                info_box(c, "Модули манипулятора", modules.join("\n"));
            });
        });
    });

    commands.entity(m.detail).with_children(|p| {
        let Some(s) = ui.inspect else {
            heading(p, "Осмотр");
            p.spawn(style::label("Выберите слот: проекция подсветит его, здесь появятся описание и подходящие предметы.", 13.0, DIM));
            rule(p);
            p.spawn(style::label(format!("Надето предметов: {}", eq.worn.len()), 14.0, WHITE));
            return;
        };
        heading(p, &s.label());
        match eq.get(s) {
            Some(st) => {
                let d = st.item.def();
                p.spawn(Node { column_gap: px(10), align_items: AlignItems::Center, ..default() }).with_children(|h| {
                    item_icon(h, st.item, 48.0, blocks);
                    h.spawn(style::label(d.name.clone(), 17.0, WHITE));
                });
                p.spawn(style::label(d.info.clone(), 13.0, DIM));
                action_button(p, "Снять", 14.0, Node { width: percent(100), ..default() }, false, HoloAction::new(move |w| {
                    let mut q = w.query_filtered::<(&mut Equipment, &mut Inventory), With<Player>>();
                    if let Some((mut e, mut i)) = q.iter_mut(w).next() {
                        e.unequip(s, &mut i);
                    }
                }));
            }
            None => {
                p.spawn(style::label("Слот пуст.", 14.0, DIM));
            }
        }
        rule(p);
        p.spawn(style::label("Подходит из инвентаря:", 13.0, CYAN));
        let mut any = false;
        for (i, st) in inv.slots.iter().enumerate() {
            let Some(st) = st else { continue };
            if wear_slot(st.item) != Some(s.slot) || !matches!(st.item.def().use_, Use::Wear(_)) {
                continue;
            }
            any = true;
            let name = st.item.def().name.clone();
            action_button(p, format!("Надеть: {name}"), 13.0, Node { width: percent(100), ..default() }, false, HoloAction::new(move |w| {
                let mut q = w.query_filtered::<(&mut Equipment, &mut Inventory), With<Player>>();
                if let Some((mut e, mut inv)) = q.iter_mut(w).next() {
                    e.equip_from(s, &mut inv, i);
                }
            }));
        }
        if !any {
            p.spawn(style::label("нет подходящих предметов", 13.0, DIM));
        }
    });

    if let Some(mats) = &mats {
        spawn_figure(&mut commands, m.projection, "equipment", &mut meshes, &mut materials, mats, Some(eq), ui.inspect);
    }
}

fn watch(q: Query<(), (With<Player>, Or<(Changed<Equipment>, Changed<Inventory>)>)>, mut nav: ResMut<HoloNav>) {
    if !q.is_empty() {
        nav.rebuild = true;
    }
}

pub struct EquipmentSection;

impl Plugin for EquipmentSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>().init_resource::<EquipUi>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo {
            id: "equipment",
            title: "Экипировка",
            glyph: "◎",
            order: 20,
        });
        app.add_systems(Update, build.in_set(HoloSet::Build))
            .add_systems(Update, watch.before(HoloSet::Frame).run_if(section_active("equipment")));
    }
}
