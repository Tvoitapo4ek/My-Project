//! Character section: live state of the character with its projection.
//! Archetypes (mage / werewolf, concept §6) are a gameplay stage; the
//! section shows that the archetype is not chosen instead of inventing one.

use bevy::prelude::*;

use super::{HoloSet, LiveText, heading, rule};
use crate::{
    holo::{
        BuildSection, HoloSections, SectionInfo,
        projection::{HoloMats, spawn_figure},
        style::{self, CYAN, DIM, WHITE},
    },
    items::equipment::Equipment,
    magic::spells::AetherReserve,
    player::{Player, movement::CharacterState},
    survival::{damage::Health, needs::Vitals},
    tools::manipulator::Manipulator,
};

fn vitals(w: &mut World) -> String {
    let mut q = w.query_filtered::<(&Health, &Vitals, Option<&AetherReserve>), With<Player>>();
    let Some((h, v, a)) = q.iter(w).next() else {
        return String::new();
    };
    let mut s = format!(
        "Здоровье   {:>5.0} / {:.0}\nСытость    {:>5.0} %\nВода       {:>5.0} %\nТемпература тела {:.1} °C",
        h.hp, h.max, v.hunger, v.thirst, v.body_temp
    );
    if let Some(a) = a {
        s.push_str(&format!("\nЭфир       {:>5.0} / {:.0}", a.amount, a.max));
    }
    s
}

fn movement(w: &mut World) -> String {
    let mut q = w.query_filtered::<(&Transform, &CharacterState, &avian3d::prelude::LinearVelocity), With<Player>>();
    let Some((t, s, v)) = q.iter(w).next() else {
        return String::new();
    };
    let state = if s.flying {
        "полёт"
    } else if s.grounded {
        "на земле"
    } else {
        "в воздухе"
    };
    format!(
        "Позиция: {:.1} {:.1} {:.1}\nСкорость: {:.1} м/с · {state}",
        t.translation.x, t.translation.y, t.translation.z,
        v.0.length()
    )
}

fn gear(w: &mut World) -> String {
    let mut q = w.query_filtered::<(Option<&Equipment>, &Manipulator), With<Player>>();
    let Some((eq, m)) = q.iter(w).next() else {
        return String::new();
    };
    let worn = eq.map(|e| e.worn.len()).unwrap_or(0);
    format!(
        "Надето предметов: {worn}\nМанипулятор: режим {}, бур {} ур.{}",
        m.mode.label(),
        m.drill_tier,
        if m.cutter { ", резак" } else { "" }
    )
}

fn build(
    mut msgs: MessageReader<BuildSection>,
    mut commands: Commands,
    player: Query<Option<&Equipment>, With<Player>>,
    mats: Option<Res<HoloMats>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(m) = msgs.read().filter(|m| m.id == "character").last().copied() else {
        return;
    };
    commands.entity(m.content).with_children(|p| {
        p.spawn(Node { flex_grow: 1.0, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), width: px(320), ..default() }).with_children(|c| {
                heading(c, "Исследователь");
                c.spawn(style::label("Архетип: не выбран\n(маг / оборотень — отдельный этап G3)", 13.0, DIM));
                rule(c);
                heading(c, "Показатели");
                c.spawn((style::label("", 15.0, WHITE), LiveText(vitals)));
            });
            row.spawn((Node { flex_grow: 1.0, ..default() }, crate::holo::projection::DragView("character")));
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), width: px(300), ..default() }).with_children(|c| {
                heading(c, "Движение");
                c.spawn((style::label("", 14.0, WHITE), LiveText(movement)));
                rule(c);
                heading(c, "Снаряжение");
                c.spawn((style::label("", 14.0, WHITE), LiveText(gear)));
            });
        });
    });
    commands.entity(m.detail).with_children(|p| {
        heading(p, "Навыки");
        p.spawn(style::label("Системы навыков и «Размышления» ещё нет (этап G9).", 13.0, DIM));
        rule(p);
        heading(p, "Режим");
        p.spawn((style::label("", 13.0, CYAN), LiveText(|w| {
            match w.get_resource::<State<crate::core::states::GameMode>>().map(|s| *s.get()) {
                Some(crate::core::states::GameMode::Creative) => "Креатив (M — переключить)".into(),
                _ => "Выживание (M — переключить)".into(),
            }
        })));
    });
    if let Some(mats) = &mats {
        let eq = player.single().ok().flatten();
        spawn_figure(&mut commands, m.projection, "character", &mut meshes, &mut materials, mats, eq, None);
    }
}

pub struct CharacterSection;

impl Plugin for CharacterSection {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>();
        app.world_mut().resource_mut::<HoloSections>().register(SectionInfo { id: "character", title: "Персонаж", glyph: "●", order: 80 });
        app.add_systems(Update, build.in_set(HoloSet::Build));
    }
}
