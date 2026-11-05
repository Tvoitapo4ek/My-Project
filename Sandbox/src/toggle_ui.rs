//! The physics toggle's lever (like Create Aeronautics): RMB on the toggle
//! opens a small panel with a slider «Блоки ↔ Физика». Drag the knob to the
//! other end with the mouse and let go: the machine assembles or goes back
//! into the grid. Let go earlier — it springs back. Esc closes.

use bevy::prelude::*;

use crate::{
    content::Mech,
    contraption::Contraption,
    core::Ui,
    interact::UseBlock,
    ui::{FRAME, font, rivets},
};

const TRACK: f32 = 300.0;
const KNOB: f32 = 44.0;

/// RMB on a physics toggle (from `interact`): open the lever.
#[derive(Message, Debug, Clone, Copy)]
pub struct ToggleRequest {
    pub grid: Entity,
    pub cell: IVec3,
}

/// The open lever: which toggle, where the knob is (0 = blocks, 1 =
/// physics), whether it is being dragged.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct Lever {
    pub target: Option<(Entity, IVec3)>,
    pub value: f32,
    /// The side it started on (true = physical).
    pub physical: bool,
    pub dragging: bool,
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct Knob;
#[derive(Component)]
struct Fill;
#[derive(Component)]
struct State_;

fn request(mut reqs: MessageReader<ToggleRequest>, mut lever: ResMut<Lever>, bodies: Query<(), With<Contraption>>, mut next: ResMut<NextState<Ui>>) {
    for r in reqs.read() {
        let physical = bodies.contains(r.grid);
        *lever = Lever { target: Some((r.grid, r.cell)), value: if physical { 1.0 } else { 0.0 }, physical, dragging: false };
        next.set(Ui::Assemble);
    }
}

fn open(mut commands: Commands, lever: Res<Lever>) {
    let (title, state) = if lever.physical { ("Переключатель физики", "Сейчас: физическое тело. Сдвиньте влево — вернуть блоки в сетку.") } else { ("Переключатель физики", "Сейчас: блоки в сетке. Сдвиньте вправо — дать физику.") };
    commands
        .spawn((Root, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35))))
        .with_children(|root| {
            root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(12), padding: UiRect::all(px(18)), border: UiRect::all(px(2)), align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.08, 0.09, 0.11, 0.95)), BorderColor::all(FRAME))).with_children(|p| {
                rivets(p);
                p.spawn((Text::new(title), font(20.0), TextColor(Color::srgb(0.95, 0.76, 0.33))));
                p.spawn((State_, Text::new(state), font(13.0), TextColor(Color::srgb(0.7, 0.7, 0.72))));
                p.spawn(Node { column_gap: px(12), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                    r.spawn((Text::new("Блоки"), font(15.0), TextColor(Color::WHITE)));
                    // The track: a dark slot, a brass fill up to the knob.
                    r.spawn((Node { width: px(TRACK), height: px(KNOB), border: UiRect::all(px(2)), ..default() }, BackgroundColor(Color::srgb(0.11, 0.12, 0.14)), BorderColor::all(FRAME))).with_children(|t| {
                        t.spawn((Fill, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), height: percent(100), width: px(0), ..default() }, BackgroundColor(Color::srgb(0.42, 0.33, 0.14)), Pickable::IGNORE));
                        t.spawn((Knob, Node { position_type: PositionType::Absolute, top: px(-4), width: px(KNOB), height: px(KNOB), border: UiRect::all(px(2)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgb(0.83, 0.66, 0.31)), BorderColor::all(Color::srgb(0.45, 0.32, 0.12))))
                            .observe(|ev: On<PointerDrag>, mut lever: ResMut<Lever>| {
                                lever.dragging = true;
                                lever.value = (lever.value + ev.delta.x / (TRACK - KNOB)).clamp(0.0, 1.0);
                            })
                            .observe(|_: On<PointerDragEnd>, mut lever: ResMut<Lever>| {
                                lever.dragging = false;
                            })
                            .with_children(|k| {
                                k.spawn((Node { width: px(8), height: px(26), ..default() }, BackgroundColor(Color::srgb(0.45, 0.32, 0.12)), Pickable::IGNORE));
                            });
                    });
                    r.spawn((Text::new("Физика"), font(15.0), TextColor(Color::WHITE)));
                });
                p.spawn((Text::new("Тяните ползунок мышью · Esc — закрыть"), font(12.0), TextColor(Color::srgb(0.55, 0.55, 0.58))));
            });
        });
}

fn close(mut commands: Commands, q: Query<Entity, With<Root>>, mut lever: ResMut<Lever>) {
    for e in &q {
        commands.entity(e).despawn();
    }
    lever.dragging = false;
}

/// Knob position; let go at the far end → act; else spring back.
fn update(mut lever: ResMut<Lever>, mut knob: Query<&mut Node, (With<Knob>, Without<Fill>)>, mut fill: Query<&mut Node, (With<Fill>, Without<Knob>)>, mut used: MessageWriter<UseBlock>, mut next: ResMut<NextState<Ui>>, time: Res<Time<Real>>) {
    if !lever.dragging {
        let goal = if lever.physical { 0.0 } else { 1.0 };
        let reached = (lever.value - goal).abs() < 0.15;
        if reached {
            if let Some((grid, cell)) = lever.target.take() {
                used.write(UseBlock { grid, cell, mech: Mech::PhysicsToggle });
                next.set(Ui::Game);
            }
            lever.value = goal;
        } else {
            // Spring back to where it started.
            let home = if lever.physical { 1.0 } else { 0.0 };
            lever.value += (home - lever.value) * (time.delta_secs() * 10.0).min(1.0);
        }
    }
    for mut n in &mut knob {
        n.left = px(lever.value * (TRACK - KNOB) - 2.0);
    }
    for mut n in &mut fill {
        n.width = px(lever.value * (TRACK - KNOB) + KNOB * 0.5);
    }
}

pub struct ToggleUiPlugin;

impl Plugin for ToggleUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Lever>()
            .add_message::<ToggleRequest>()
            .add_systems(Update, request.after(crate::interact::InteractSet))
            .add_systems(OnEnter(Ui::Assemble), open)
            .add_systems(OnExit(Ui::Assemble), close)
            .add_systems(Update, update.run_if(in_state(Ui::Assemble)));
    }
}
