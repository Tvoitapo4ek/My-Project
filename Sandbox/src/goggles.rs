//! Engineer's goggles (worn in the head slot): a small panel next to the
//! crosshair tells what the block under it is doing — like Create's
//! goggles. Speed, stress and capacity of its network, fluid, engine
//! state, signals and logic, a flywheel's charge, a door, a belt.

use bevy::prelude::*;

use crate::{
    content::Mech,
    core::Ui,
    engine::{Engine, Stall},
    fluids::FluidStore,
    grid::VoxelGrid,
    interact::Target,
    inventory::Inventory,
    logic::LogicState,
    mech::{BlockEntity, Flywheel, NetState, Signal, Spin},
    player::Player,
    ui::{FRAME, font, rivets},
};

#[derive(Component)]
struct Panel;
#[derive(Component)]
pub struct PanelText;

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            Node { position_type: PositionType::Absolute, left: percent(53), top: percent(44), flex_direction: FlexDirection::Column, padding: UiRect::axes(px(10), px(8)), border: UiRect::all(px(2)), display: Display::None, ..default() },
            BackgroundColor(Color::srgba(0.06, 0.07, 0.09, 0.82)),
            BorderColor::all(FRAME),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            rivets(p);
            p.spawn((PanelText, Text::new(""), font(14.0), TextColor(Color::srgb(0.92, 0.92, 0.9)), Pickable::IGNORE));
        });
}

/// Lines of the goggles panel for a block.
#[allow(clippy::too_many_arguments)]
pub fn describe(name: &str, mech: Option<Mech>, spin: Option<&Spin>, net: Option<&NetState>, fluid: Option<&FluidStore>, engine: Option<&Engine>, signal: Option<&Signal>, logic: Option<&LogicState>, fly: Option<&Flywheel>) -> Vec<String> {
    let mut v = vec![name.to_string()];
    let kinetic = net.is_some_and(|n| n.capacity > 0.0 || n.stress > 0.0) || spin.is_some_and(|s| s.rpm != 0.0);
    if kinetic {
        if let Some(s) = spin {
            v.push(format!("Обороты: {:.0} об/мин", s.rpm.abs()));
        }
        if let Some(n) = net {
            v.push(format!("Нагрузка: {:.0} / {:.0} SU", n.stress, n.capacity));
            if n.conflict {
                v.push("⚠ Конфликт скоростей".into());
            } else if n.overstressed {
                v.push("⚠ Перегрузка: стоит".into());
            } else if n.k < 1.0 {
                v.push(format!("⚠ Перегрузка: {:.0}% скорости", n.k * 100.0));
            }
        }
    }
    if let Some(f) = fluid {
        let what = f.fluid.map(|x| x.name()).unwrap_or("пусто");
        v.push(format!("Жидкость: {:.0} / {:.0} mB ({what})", f.amount, f.cap));
    }
    if let Some(e) = engine {
        v.push(
            match e.stall {
                None => "Двигатель: работает",
                Some(Stall::Off) => "Двигатель: выключен",
                Some(Stall::NoFuel) => "Двигатель: нет топлива",
                Some(Stall::NoAir) => "Двигатель: нет воздуха",
                Some(Stall::Overload) => "Двигатель: заглох (перегрузка)",
            }
            .into(),
        );
    }
    if let Some(f) = fly {
        v.push(format!("Запас энергии: {:.0}%", f.energy / crate::mech::FLY_MAX * 100.0));
    }
    if let Some(l) = logic {
        if mech == Some(Mech::Wire) {
            v.push(if l.input { "Провод: включён".into() } else { "Провод: выключен".into() });
        } else {
            v.push(format!("Выход: {}", if l.out { "вкл" } else { "выкл" }));
        }
    } else if signal.is_some_and(|s| s.0 != 0.0) {
        v.push(format!("Сигнал: {:+.0}%", signal.map(|s| s.0).unwrap_or(0.0) * 100.0));
    }
    v
}

#[allow(clippy::type_complexity)]
fn update(
    ui: Res<State<Ui>>,
    target: Res<Target>,
    players: Query<&Inventory, With<Player>>,
    grids: Query<&VoxelGrid>,
    blocks: Query<(&BlockEntity, Option<&Spin>, Option<&NetState>, Option<&FluidStore>, Option<&Engine>, Option<&Signal>, Option<&LogicState>, Option<&Flywheel>)>,
    mut panel: Query<&mut Node, With<Panel>>,
    mut text: Query<&mut Text, With<PanelText>>,
) {
    let (Ok(mut node), Ok(mut t)) = (panel.single_mut(), text.single_mut()) else { return };
    let worn = players.single().is_ok_and(|i| i.slots[crate::inventory::HEAD].is_some_and(|s| s.item.tool() == Some(crate::content::Tool::Goggles)));
    let lines = (|| {
        if !worn || *ui.get() != Ui::Game {
            return None;
        }
        let a = target.hit?;
        let grid = grids.get(a.grid).ok()?;
        let cell = grid.get(a.cell);
        let def = cell.id.def();
        let name = &def.name;
        match grid.entities.get(&a.cell).and_then(|e| blocks.get(*e).ok()) {
            Some((b, spin, net, fluid, eng, sig, logic, fly)) => Some(describe(name, b.mech(), spin, net, fluid, eng, sig, logic, fly)),
            None => Some(vec![name.clone(), format!("Масса: {:.0} кг", def.mass)]),
        }
    })();
    match lines {
        Some(l) => {
            node.display = Display::Flex;
            let s = l.join("\n");
            if t.0 != s {
                t.0 = s;
            }
        }
        None => node.display = Display::None,
    }
}

pub struct GogglesPlugin;

impl Plugin for GogglesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_panel).add_systems(Update, update.after(crate::mech::MechSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goggles_tell_speed_stress_and_trouble() {
        let spin = Spin { rpm: -64.0, angle: 0.0 };
        let net = NetState { stress: 3000.0, capacity: 2048.0, k: 0.68, ..default() };
        let l = describe("Вал", Some(Mech::Shaft), Some(&spin), Some(&net), None, None, None, None, None);
        assert_eq!(l[1], "Обороты: 64 об/мин");
        assert_eq!(l[2], "Нагрузка: 3000 / 2048 SU");
        assert!(l[3].contains("68%"));
        let tank = FluidStore { fluid: Some(crate::fluids::Fluid::Water), amount: 4000.0, cap: 16000.0 };
        let l = describe("Цистерна", Some(Mech::FluidTank), Some(&Spin::default()), Some(&NetState::default()), Some(&tank), None, None, None, None);
        assert_eq!(l, vec!["Цистерна".to_string(), "Жидкость: 4000 / 16000 mB (вода)".to_string()]);
    }
}
