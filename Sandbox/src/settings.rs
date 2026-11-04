//! Block settings window (opened with the wrench): one row per parameter
//! with ‹ value › buttons. Values live in the block's `Params`.

use bevy::prelude::*;

use crate::{
    core::Ui,
    mech::{BlockEntity, NetState, Params, Signal, Spin, param_step, param_text},
    ui::font,
};

/// The block being configured.
#[derive(Resource, Default)]
pub struct Editing(pub Option<Entity>);

#[derive(Component)]
struct Window;
#[derive(Component)]
struct ValueText(usize);
#[derive(Component)]
struct StatusText;
#[derive(Component, Clone, Copy)]
struct StepButton(usize, f32);

fn open(mut commands: Commands, editing: Res<Editing>, blocks: Query<&BlockEntity>) {
    let Some(b) = editing.0.and_then(|e| blocks.get(e).ok()) else { return };
    let def = b.id.def();
    commands
        .spawn((Window, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
        .with_children(|root| {
            root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::all(px(16)), border: UiRect::all(px(2)), min_width: px(380), ..default() }, BackgroundColor(Color::srgba(0.08, 0.09, 0.11, 0.94)), BorderColor::all(Color::srgb(0.95, 0.76, 0.33))))
                .with_children(|p| {
                    p.spawn((Text::new(def.name.clone()), font(20.0), TextColor(Color::srgb(0.95, 0.76, 0.33))));
                    for (i, param) in def.params.iter().enumerate() {
                        p.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
                            row.spawn((Text::new(param.label.clone()), font(16.0), TextColor(Color::WHITE), Node { width: px(170), ..default() }));
                            for (label, dir) in [("‹", -1.0), ("", 0.0), ("›", 1.0)] {
                                if dir == 0.0 {
                                    row.spawn((ValueText(i), Text::new(""), font(16.0), TextColor(Color::srgb(0.95, 0.76, 0.33)), Node { width: px(130), justify_content: JustifyContent::Center, ..default() }, TextLayout::justify(Justify::Center)));
                                    continue;
                                }
                                row.spawn((StepButton(i, dir), Node { width: px(32), height: px(28), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgb(0.2, 0.21, 0.24)), BorderColor::all(Color::srgb(0.4, 0.4, 0.44))))
                                    .observe(on_step)
                                    .with_children(|b| {
                                        b.spawn((Text::new(label), font(18.0), TextColor(Color::WHITE), Pickable::IGNORE));
                                    });
                            }
                        });
                    }
                    p.spawn((StatusText, Text::new(""), font(14.0), TextColor(Color::srgb(0.7, 0.7, 0.72))));
                    p.spawn((Text::new("Esc — закрыть"), font(13.0), TextColor(Color::srgb(0.55, 0.55, 0.58))));
                });
        });
}

fn on_step(ev: On<PointerClick>, buttons: Query<&StepButton>, editing: Res<Editing>, mut blocks: Query<(&BlockEntity, &mut Params)>, mut sounds: MessageWriter<crate::sound::Sfx>) {
    sounds.write(crate::sound::sfx("ui_click"));
    let (Ok(sb), Some(e)) = (buttons.get(ev.entity), editing.0) else { return };
    let Ok((b, mut p)) = blocks.get_mut(e) else { return };
    let Some(def) = b.id.def().params.get(sb.0) else { return };
    if let Some(v) = p.0.get_mut(sb.0) {
        *v = param_step(&def.kind, *v, sb.1);
    }
}

fn refresh(editing: Res<Editing>, blocks: Query<(&BlockEntity, &Params, &Spin, &Signal, &NetState)>, mut values: Query<(&ValueText, &mut Text), Without<StatusText>>, mut status: Query<&mut Text, With<StatusText>>, mut next: ResMut<NextState<Ui>>) {
    let Some(Ok((b, p, spin, sig, net))) = editing.0.map(|e| blocks.get(e)) else {
        next.set(Ui::Game);
        return;
    };
    for (v, mut t) in &mut values {
        if let (Some(def), Some(val)) = (b.id.def().params.get(v.0), p.0.get(v.0)) {
            let s = param_text(&def.kind, *val);
            if t.0 != s {
                t.0 = s;
            }
        }
    }
    if let Ok(mut t) = status.single_mut() {
        let mut s = format!("Обороты: {:.0} об/мин", spin.rpm.abs());
        if sig.0 != 0.0 {
            s.push_str(&format!(" · сигнал {:+.0}%", sig.0 * 100.0));
        }
        if net.conflict {
            s.push_str(" · конфликт скоростей");
        }
        if t.0 != s {
            t.0 = s;
        }
    }
}

fn close(mut commands: Commands, q: Query<Entity, With<Window>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Editing>()
            .add_systems(OnEnter(Ui::Settings), open)
            .add_systems(OnExit(Ui::Settings), close)
            .add_systems(Update, refresh.run_if(in_state(Ui::Settings)));
    }
}
