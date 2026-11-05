//! Visual explanations of blocks (like Create's Ponder): in the inventory
//! or catalogue, hold W over an item — a window opens with a small live
//! scene that builds itself step by step with captions; the mechanisms in
//! it really work (it is an ordinary grid far above the world).
//! ← / → or the wheel: step back / forward, Esc: close.

use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::render_resource::TextureFormat,
};

use crate::{
    chains::{ChainLink, Chains},
    content::{BlockId, ItemId},
    core::Ui,
    fluids::{Fluid, FluidStore},
    grid::{Cell, VoxelGrid},
    mech::{BlockEntity, Params},
    tools::glue::Glue,
    ui::{FRAME, font, rivets},
};

/// Where the scenes are built (far above the world, out of its sight).
pub const ORIGIN: Vec3 = Vec3::new(0.0, 1000.0, 0.0);
const VIEW_W: u32 = 720;
const VIEW_H: u32 = 420;
/// Seconds per step before it moves on by itself.
const STEP_TIME: f32 = 6.0;
const HOLD: f32 = 0.5;

/// What a step does to the scene.
#[derive(Debug, Clone)]
pub enum Op {
    Block(IVec3, &'static str, u8),
    Param(IVec3, &'static str, f32),
    Fill(IVec3, Fluid, f32),
    /// Flip a lever.
    Lever(IVec3),
    GlueBox(IVec3, IVec3),
    Belt(IVec3, IVec3),
    Chain(IVec3, IVec3, i8, i8),
    /// Telemetry sensor → display.
    Link(IVec3, IVec3),
}

#[derive(Debug, Clone)]
pub struct Step {
    pub caption: &'static str,
    pub ops: Vec<Op>,
}

fn st(caption: &'static str, ops: Vec<Op>) -> Step {
    Step { caption, ops }
}

use Op::*;

fn v(x: i32, y: i32, z: i32) -> IVec3 {
    IVec3::new(x, y, z)
}

/// The scene of an item (by key), if it has one.
pub fn scene(key: &str) -> Option<Vec<Step>> {
    let motor_line = || vec![Block(v(-2, 0, 0), "creative_motor", 0), Block(v(-1, 0, 0), "shaft", 0)];
    Some(match key {
        "creative_motor" => vec![
            st("Креативный мотор даёт вращение через грань, куда смотрит.", vec![Block(v(-1, 0, 0), "creative_motor", 0)]),
            st("Валы передают вращение дальше по своей оси.", (0..3).map(|x| Block(v(x, 0, 0), "shaft", 0)).collect()),
            st("Ключом — скорость, направление и мощность (SU). Здесь: 96 об/мин.", vec![Param(v(-1, 0, 0), "speed", 96.0)]),
        ],
        "shaft" | "cased_shaft" => vec![
            st("Вал передаёт вращение вдоль своей оси.", vec![Block(v(-2, 0, 0), "creative_motor", 0), Block(v(-1, 0, 0), "shaft", 0), Block(v(0, 0, 0), "shaft", 0)]),
            st("Сбоку вал не передаёт: этот вал стоит.", vec![Block(v(0, 0, 1), "shaft", 4)]),
            st("Стальным корпусом (ПКМ) на вал надевается кожух — работает так же.", vec![Block(v(1, 0, 0), "cased_shaft", 0)]),
        ],
        "cogwheel" | "large_cogwheel" => vec![
            st("Мотор крутит шестерню снизу (вдоль оси она работает как вал).", vec![Block(v(-1, -0, 0), "creative_motor", 2), Block(v(-1, 1, 0), "cogwheel", 2)]),
            st("Соседние шестерни одной оси крутятся навстречу.", vec![Block(v(0, 1, 0), "cogwheel", 2)]),
            st("Большая шестерня цепляется по диагонали: вдвое медленнее, вдвое меньше нагрузки.", vec![Block(v(1, 1, 1), "large_cogwheel", 2)]),
            st("Две большие — через клетку друг от друга.", vec![Block(v(1, 1, 3), "large_cogwheel", 2)]),
        ],
        "gearbox" => vec![
            st("Коробка передач: четыре выхода вокруг своей оси.", [motor_line(), vec![Block(v(0, 0, 0), "gearbox", 2)]].concat()),
            st("Вбок вращение поворачивает на 90°…", vec![Block(v(0, 0, 1), "shaft", 4), Block(v(0, 0, -1), "shaft", 4)]),
            st("…насквозь — разворачивает в обратную сторону.", vec![Block(v(1, 0, 0), "shaft", 0)]),
            st("Вдоль оси (вверх) не передаёт.", vec![Block(v(0, 1, 0), "shaft", 2)]),
        ],
        "vertical_gearbox" => vec![
            st("Вертикальная коробка: выходы вверх, вниз, влево и вправо.", [motor_line(), vec![Block(v(0, 0, 0), "vertical_gearbox", 4)]].concat()),
            st("Вращение уходит вверх и вниз…", vec![Block(v(0, 1, 0), "shaft", 2), Block(v(0, 2, 0), "shaft", 2)]),
            st("…и насквозь, в обратную сторону.", vec![Block(v(1, 0, 0), "shaft", 0)]),
        ],
        "clutch" | "gearshift" => {
            let k = if key == "clutch" { "clutch" } else { "gearshift" };
            vec![
                st(if k == "clutch" { "Муфта — вал, который размыкается по сигналу." } else { "Реверс — вал, который по сигналу крутит выход в обратную сторону." }, [motor_line(), vec![Block(v(0, 0, 0), k, 0), Block(v(1, 0, 0), "shaft", 0), Block(v(2, 0, 0), "shaft", 0)]].concat()),
                st("Рычаг на ней даёт сигнал…", vec![Block(v(0, 1, 0), "lever", 3)]),
                st(if k == "clutch" { "…и вал за муфтой останавливается." } else { "…и вал за реверсом крутится обратно." }, vec![Lever(v(0, 1, 0))]),
            ]
        }
        "speed_controller" => vec![
            st("Регулятор: вход сзади, выход спереди со своей скоростью.", vec![Block(v(-2, 0, 0), "creative_motor", 0), Block(v(-1, 0, 0), "speed_controller", 0), Block(v(0, 0, 0), "shaft", 0), Block(v(1, 0, 0), "shaft", 0)]),
            st("Ключом — скорость выхода. Здесь 128 об/мин.", vec![Param(v(-1, 0, 0), "speed", 128.0)]),
            st("Сигнал ведёт к «скорости по сигналу»: при скорости 0 это педаль газа.", vec![Param(v(-1, 0, 0), "speed", 0.0), Block(v(-1, 1, 0), "lever", 3), Lever(v(-1, 1, 0))]),
        ],
        "mechanical_bearing" => vec![
            st("Подшипник получает вращение от вала сзади.", vec![Block(v(0, 0, 0), "creative_motor", 2), Block(v(0, 1, 0), "mechanical_bearing", 2)]),
            st("Всё, что приклеено к блоку на его грани, становится платформой…", vec![Block(v(-1, 2, 0), "steel_block", 2), Block(v(0, 2, 0), "steel_block", 2), Block(v(1, 2, 0), "steel_block", 2), GlueBox(v(-1, 2, 0), v(1, 2, 0))]),
            st("…и крутится вместе с ним — в мире и на машине.", vec![Param(v(0, 0, 0), "speed", 16.0)]),
        ],
        "mechanical_piston" => vec![
            st("Поршень получает вращение от вала сзади.", vec![Block(v(-2, 0, 0), "creative_motor", 0), Param(v(-2, 0, 0), "speed", 0.0), Block(v(-1, 0, 0), "mechanical_piston", 0)]),
            st("Перед ним — приклеенная группа.", vec![Block(v(0, 0, 0), "steel_block", 2), Block(v(0, 1, 0), "steel_block", 2), GlueBox(v(0, 0, 0), v(0, 1, 0)), Param(v(-1, 0, 0), "length", 3.0)]),
            st("Один блок за оборот вала, до «Хода»; обратное вращение втягивает.", vec![Param(v(-2, 0, 0), "direction", 1.0), Param(v(-2, 0, 0), "speed", 30.0)]),
        ],
        "fluid_tank" | "fluid_pipe" | "cased_pipe" | "fluid_port" | "bucket_water" | "bucket_diesel" => vec![
            st("Заливной порт, труба и цистерна — одна трубная сеть.", vec![Block(v(-2, 0, 0), "fluid_port", 2), Block(v(-1, 0, 0), "fluid_pipe", 0), Block(v(0, 0, 0), "fluid_pipe", 0), Block(v(1, 0, 0), "fluid_tank", 2), Block(v(1, 1, 0), "fluid_tank", 2)]),
            st("Ведро в порт (ПКМ) — 1000 mB; порт сам сливает в цистерны.", vec![Fill(v(-2, 0, 0), Fluid::Water, 1000.0)]),
            st("Цистерны рядом — одна цистерна (16 000 mB на блок), жидкость оседает вниз.", vec![Fill(v(-2, 0, 0), Fluid::Water, 1000.0)]),
        ],
        "mechanical_pump" | "creative_tank" => vec![
            st("Две цистерны, между ними помпа — две отдельные сети.", vec![Block(v(-2, 0, 0), "fluid_tank", 2), Block(v(-1, 0, 0), "fluid_pipe", 0), Block(v(0, 0, 0), "mechanical_pump", 0), Block(v(1, 0, 0), "fluid_pipe", 0), Block(v(2, 0, 0), "fluid_tank", 2), Fill(v(-2, 0, 0), Fluid::Water, 12000.0)]),
            st("Шестерня с мотором крутит помпу (помпа — как шестерня).", vec![Block(v(-1, 1, 0), "creative_motor", 0), Block(v(0, 1, 0), "cogwheel", 0)]),
            st("Помпа переносит |об/мин| × 8 mB/с вперёд; ключом — направление.", vec![Param(v(-1, 1, 0), "speed", 128.0)]),
        ],
        "diesel_engine" => vec![
            st("Дизелю нужно топливо: бесконечный бак дизеля и труба.", vec![Block(v(-3, 0, 0), "creative_tank", 2), Block(v(-2, 0, 0), "fluid_pipe", 0)]),
            st("Двигатель берёт топливо с любой стороны, кроме выхода спереди.", vec![Block(v(-1, 0, 0), "diesel_engine", 0)]),
            st("Работает, пока над ним пусто (воздух): 10 mB/с, 2048 SU.", vec![Block(v(0, 0, 0), "shaft", 0), Block(v(1, 0, 0), "shaft", 0)]),
        ],
        "flywheel" => vec![
            st("Маховик стоит на валу и крутится с ним.", [motor_line(), vec![Block(v(0, 0, 0), "flywheel", 0), Block(v(1, 0, 0), "shaft", 0)]].concat()),
            st("Из запаса мощности он копит энергию и при перегрузке отдаёт до 1000 SU на 2 с.", vec![]),
        ],
        "chain" => vec![
            st("Шестерни одной оси в одной плоскости, на расстоянии.", vec![Block(v(-3, 0, 0), "creative_motor", 4), Block(v(-3, 0, 1), "cogwheel", 4), Block(v(1, 0, 1), "cogwheel", 4), Block(v(3, 2, 1), "large_cogwheel", 4)]),
            st("Цепь (ЛКМ, ЛКМ) по одной стороне: шестерни крутятся одинаково.", vec![Chain(v(-3, 0, 1), v(1, 0, 1), 1, 1)]),
            st("Цепь крест-накрест — в обратную сторону; на большую — вдвое медленнее.", vec![Chain(v(1, 0, 1), v(3, 2, 1), 1, -1)]),
        ],
        "mechanical_belt" => vec![
            st("Два вала одной оси на одной линии.", vec![Block(v(-3, 0, -1), "creative_motor", 4), Block(v(-3, 0, 0), "shaft", 4), Block(v(2, 0, 0), "shaft", 4)]),
            st("Лента (ЛКМ, ЛКМ): шкивы крутятся одинаково, поверхность везёт игрока и тела.", vec![Belt(v(-3, 0, 0), v(2, 0, 0))]),
            st("Ленту можно вести вертикально или под 45°.", vec![Block(v(2, 1, 0), "shaft", 4), Block(v(-1, 4, 0), "shaft", 4), Belt(v(2, 1, 0), v(-1, 4, 0))]),
        ],
        "track_sprocket" | "track_roller" => vec![
            st("Звёздочка крутится от вала, как колесо.", vec![Block(v(1, 0, -2), "creative_motor", 1), Block(v(0, 0, -2), "track_sprocket", 1)]),
            st("Катки рядом (та же ось) крутятся вместе — лента гусеницы натягивается сама.", (-1..=2).map(|z| Block(v(0, 0, z), "track_roller", 1)).collect()),
        ],
        "suspension" | "wheel" => vec![
            st("Подвеска ставится на кузов, колесо — на подвеску.", vec![Block(v(-1, 0, 0), "steel_block", 2), Block(v(0, 0, 0), "suspension", 0), Block(v(1, 0, 0), "wheel", 0)]),
            st("Сигнал с пульта (датчик) или логика поворачивает колесо; на физической машине пружинит.", vec![Block(v(0, 1, 0), "lever", 3), Lever(v(0, 1, 0))]),
        ],
        "telemetry_sensor" | "mechanical_display" | "speedometer" => vec![
            st("Механический экран работает, пока его крутят сбоку.", vec![Block(v(-2, 0, 0), "creative_motor", 0), Block(v(-1, 0, 0), "shaft", 0), Block(v(0, 0, 0), "mechanical_display", 4)]),
            st("Датчик телеметрии на валу читает обороты…", vec![Block(v(-1, 1, 0), "telemetry_sensor", 3)]),
            st("…ключом его связывают с экраном.", vec![Link(v(-1, 1, 0), v(0, 0, 0))]),
            st("Ключом у экрана — цифры или стрелка.", vec![Param(v(0, 0, 0), "style", 1.0)]),
        ],
        "wire" | "lever" | "lamp" | "button" => vec![
            st("Рычаг питает соседние блоки.", vec![Block(v(-3, 0, 0), "lever", 3)]),
            st("Шина проводов: в одном блоке до 8 цветов, каждый — своя линия.", (-2..=1).map(|x| Block(v(x, 0, 0), "wire", 2)).collect()),
            st("Цвет касается соседних блоков только там, где его линия кончается.", vec![Block(v(2, 0, 0), "lamp", 2)]),
            st("Рычаг у конца линии включает её — лампа на другом конце горит.", vec![Lever(v(-3, 0, 0))]),
        ],
        "logic_gate" => vec![
            st("Логический элемент: входы слева, справа и сзади, выход спереди.", vec![Block(v(0, 0, 0), "logic_gate", 4), Block(v(0, 0, 1), "lamp", 2), Block(v(-1, 0, 0), "lever", 3), Block(v(1, 0, 0), "lever", 3)]),
            st("И: лампа горит, только когда включены оба входа.", vec![Lever(v(-1, 0, 0))]),
            st("Второй вход — и выход включён.", vec![Lever(v(1, 0, 0))]),
            st("Ключом — режим: ИЛИ, НЕ, RS-триггер.", vec![Param(v(0, 0, 0), "mode", 2.0)]),
        ],
        "timer_block" | "flasher_light" | "pulse_generator" => vec![
            st("Таймер даёт короткий импульс каждые «Интервал» секунд.", vec![Block(v(-1, 0, 0), "timer_block", 4)]),
            st("Мигалка рядом вспыхивает на каждый импульс.", vec![Block(v(0, 0, 0), "flasher_light", 2)]),
            st("Интервал — ключом. Здесь 0,5 с.", vec![Param(v(-1, 0, 0), "interval", 0.5)]),
        ],
        "threshold_sensor" => vec![
            st("Пороговый датчик читает блок, на котором стоит: здесь — обороты вала.", vec![Block(v(-2, 0, 0), "creative_motor", 0), Param(v(-2, 0, 0), "speed", 32.0), Block(v(-1, 0, 0), "shaft", 0), Block(v(-1, 1, 0), "threshold_sensor", 3), Param(v(-1, 1, 0), "threshold", 64.0), Block(v(-1, 2, 0), "lamp", 2)]),
            st("Порог 64: на 32 об/мин сигнала нет…", vec![]),
            st("…на 128 — есть, лампа горит. Так механика управляет логикой.", vec![Param(v(-2, 0, 0), "speed", 128.0)]),
        ],
        "radio_transmitter" | "radio_receiver" => vec![
            st("Передатчик и приёмник на одном канале.", vec![Block(v(-3, 0, 0), "lever", 3), Block(v(-2, 0, 0), "radio_transmitter", 2), Block(v(2, 0, 0), "radio_receiver", 2), Block(v(3, 0, 0), "lamp", 2)]),
            st("Сигнал на передатчик — приёмник включён где угодно, без проводов.", vec![Lever(v(-3, 0, 0))]),
        ],
        "sliding_door" => vec![
            st("Раздвижная дверь в два блока.", vec![Block(v(0, 0, 0), "sliding_door", 4), Block(v(0, 1, 0), "sliding_door_top", 4), Block(v(1, 0, 0), "lever", 3)]),
            st("ПКМ или сигнал (рычаг рядом) — дверь уезжает вбок.", vec![Lever(v(1, 0, 0))]),
        ],
        "headlight" => vec![
            st("Фара светит вдоль грани.", vec![Block(v(0, 0, 0), "steel_block", 2), Block(v(0, 0, 1), "headlight", 4), Block(v(0, 1, 0), "button", 3)]),
            st("Каждое нажатие (сигнал) включает или выключает её.", vec![Lever(v(0, 1, 0))]),
        ],
        _ => return None,
    })
}

pub fn has_scene(item: ItemId) -> bool {
    scene(&item.def().key).is_some()
}

// ------------------------------------------------------------ runtime

/// The open explanation: item, step, time on it.
#[derive(Resource, Default)]
pub struct Ponder {
    pub item: Option<ItemId>,
    pub step: usize,
    pub t: f32,
    /// Rebuild the scene up to `step`.
    dirty: bool,
    /// Holding W over an item (inventory).
    pub hold: f32,
}

#[derive(Component)]
pub struct PonderGrid;
#[derive(Component)]
struct PonderCam;
#[derive(Component)]
struct PonderUi;
#[derive(Component)]
struct Caption;
#[derive(Component)]
struct StepText;

/// Hold W over an item in the inventory → open its explanation.
fn hold_w(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, cat: Res<crate::ui::Catalog>, mut ponder: ResMut<Ponder>, mut next: ResMut<NextState<Ui>>) {
    let item = cat.hovered.filter(|i| has_scene(*i) && !cat.typing);
    if item.is_some() && keys.pressed(KeyCode::KeyW) {
        ponder.hold += time.delta_secs();
        if ponder.hold >= HOLD {
            ponder.hold = 0.0;
            ponder.item = item;
            ponder.step = 0;
            ponder.t = 0.0;
            ponder.dirty = true;
            next.set(Ui::Ponder);
        }
    } else {
        ponder.hold = 0.0;
    }
}

fn open(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut ponder: ResMut<Ponder>) {
    let Some(item) = ponder.item else { return };
    // Always from the first step.
    ponder.step = 0;
    ponder.t = 0.0;
    ponder.dirty = true;
    let image = images.add(Image::new_target_texture(VIEW_W, VIEW_H, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    commands.spawn((
        PonderCam,
        Camera3d::default(),
        Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::srgb(0.13, 0.15, 0.19)), ..default() },
        RenderTarget::Image(image.clone().into()),
        Projection::Perspective(PerspectiveProjection { fov: 40f32.to_radians(), near: 0.1, far: 80.0, ..default() }),
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        Transform::from_translation(ORIGIN + Vec3::new(7.0, 6.0, 9.0)).looking_at(ORIGIN, Vec3::Y),
    ));
    commands
        .spawn((PonderUi, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(80)))
        .with_children(|root| {
            root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::all(px(14)), border: UiRect::all(px(2)), align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.08, 0.09, 0.11, 0.97)), BorderColor::all(FRAME)))
                .with_children(|p| {
                    rivets(p);
                    p.spawn((Text::new(format!("Как работает: {}", item.def().name)), font(20.0), TextColor(Color::srgb(0.95, 0.76, 0.33))));
                    p.spawn((ImageNode::new(image), Node { width: px(VIEW_W as f32), height: px(VIEW_H as f32), border: UiRect::all(px(2)), ..default() }, BorderColor::all(FRAME)));
                    p.spawn((Caption, Text::new(""), font(16.0), TextColor(Color::srgb(0.92, 0.92, 0.9)), Node { max_width: px(VIEW_W as f32), ..default() }));
                    p.spawn((StepText, Text::new(""), font(13.0), TextColor(Color::srgb(0.6, 0.6, 0.62))));
                });
        });
}

fn close(mut commands: Commands, q: Query<Entity, Or<(With<PonderCam>, With<PonderUi>, With<PonderGrid>)>>, parts: Query<(Entity, &GlobalTransform), With<crate::contraption::Contraption>>, mut ponder: ResMut<Ponder>) {
    for e in &q {
        commands.entity(e).despawn();
    }
    // Moving parts of bearings / pistons in the scene.
    for (e, t) in &parts {
        if t.translation().y > ORIGIN.y - 50.0 {
            commands.entity(e).despawn();
        }
    }
    ponder.item = None;
}

/// Steps, captions, the turning camera; the scene rebuilt when needed.
#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut ponder: ResMut<Ponder>,
    grids: Query<Entity, With<PonderGrid>>,
    parts: Query<(Entity, &GlobalTransform), With<crate::contraption::Contraption>>,
    mut cams: Query<&mut Transform, With<PonderCam>>,
    mut caption: Query<&mut Text, (With<Caption>, Without<StepText>)>,
    mut step_text: Query<&mut Text, (With<StepText>, Without<Caption>)>,
) {
    let Some(item) = ponder.item else { return };
    let Some(steps) = scene(&item.def().key) else { return };
    let n = steps.len();
    let go = |p: &mut Ponder, to: usize| {
        if to != p.step && to < n {
            // Going back rebuilds from scratch.
            if to < p.step {
                p.dirty = true;
            } else {
                p.dirty = p.dirty || to != p.step + 1;
            }
            p.step = to;
            p.t = 0.0;
        }
    };
    ponder.t += time.delta_secs();
    let back = keys.just_pressed(KeyCode::ArrowLeft) || scroll.delta.y > 0.0;
    let fwd = keys.just_pressed(KeyCode::ArrowRight) || scroll.delta.y < 0.0 || (ponder.t > STEP_TIME && ponder.step + 1 < n);
    let mut apply_from = None;
    if back {
        let s = ponder.step.saturating_sub(1);
        go(&mut ponder, s);
    } else if fwd && ponder.step + 1 < n {
        let s = ponder.step + 1;
        let was_dirty = ponder.dirty;
        go(&mut ponder, s);
        if !was_dirty && !ponder.dirty {
            apply_from = Some(s);
        }
    }
    if ponder.dirty {
        ponder.dirty = false;
        for g in &grids {
            commands.entity(g).despawn();
        }
        for (e, t) in &parts {
            if t.translation().y > ORIGIN.y - 50.0 {
                commands.entity(e).despawn();
            }
        }
        let ops: Vec<Op> = steps[..=ponder.step].iter().flat_map(|s| s.ops.clone()).collect();
        commands.queue(move |w: &mut World| build(w, &ops, true));
    } else if let Some(s) = apply_from {
        let ops = steps[s].ops.clone();
        commands.queue(move |w: &mut World| build(w, &ops, false));
    }
    // A slow turn around the scene.
    let a = time.elapsed_secs() * 0.25;
    for mut t in &mut cams {
        *t = Transform::from_translation(ORIGIN + Vec3::new(a.sin() * 9.5, 6.0, a.cos() * 9.5)).looking_at(ORIGIN + Vec3::Y * 0.5, Vec3::Y);
    }
    if let Ok(mut c) = caption.single_mut() {
        let s = steps[ponder.step].caption;
        if c.0 != s {
            c.0 = s.to_string();
        }
    }
    if let Ok(mut c) = step_text.single_mut() {
        let s = format!("Шаг {} из {n} · ← → или колесо — шаги · Esc — закрыть", ponder.step + 1);
        if c.0 != s {
            c.0 = s;
        }
    }
}

/// Builds (or extends) the scene grid with `ops`. Deferred ops (settings,
/// fluids, links) wait until the block entities exist.
fn build(world: &mut World, ops: &[Op], fresh: bool) {
    let g = if fresh {
        let mut grid = VoxelGrid::default();
        let floor = BlockId::k("concrete");
        for x in -4..=4 {
            for z in -4..=4 {
                grid.set(IVec3::new(x, -1, z), Cell::new(floor, 2));
            }
        }
        world.spawn((Name::new("Ponder scene"), PonderGrid, grid, Glue::default(), Chains::default(), Transform::from_translation(ORIGIN), Visibility::default(), avian3d::prelude::RigidBody::Static)).id()
    } else {
        match world.query_filtered::<Entity, With<PonderGrid>>().iter(world).next() {
            Some(g) => g,
            None => return,
        }
    };
    let mut later = Vec::new();
    for op in ops {
        match op {
            Block(c, key, f) => {
                world.get_mut::<VoxelGrid>(g).unwrap().set(*c, Cell::new(BlockId::k(key), *f));
            }
            GlueBox(a, b) => world.get_mut::<Glue>(g).unwrap().0.push((*a, *b)),
            Belt(a, b) => {
                let _ = crate::belts::lay(&mut world.get_mut::<VoxelGrid>(g).unwrap(), *a, *b);
            }
            Chain(a, b, sa, sb) => world.get_mut::<Chains>(g).unwrap().0.push(ChainLink { a: *a, b: *b, sa: *sa, sb: *sb }),
            other => later.push(other.clone()),
        }
    }
    if !later.is_empty() {
        world.insert_resource(Pending(g, later));
    }
}

/// Ops that need block entities (they appear a frame later).
#[derive(Resource)]
struct Pending(Entity, Vec<Op>);

fn apply_pending(world: &mut World) {
    let Some(p) = world.get_resource::<Pending>() else { return };
    let g = p.0;
    let ops = p.1.clone();
    let ent = |w: &World, c: IVec3| w.get::<VoxelGrid>(g).and_then(|gr| gr.entities.get(&c).copied());
    // Wait until every target exists.
    let ready = ops.iter().all(|op| match op {
        Param(c, ..) => ent(world, *c).is_some(),
        Fill(c, ..) => ent(world, *c).is_some_and(|e| world.get::<FluidStore>(e).is_some()),
        Lever(c) => ent(world, *c).is_some_and(|e| world.get::<crate::logic::LogicState>(e).is_some()),
        Link(a, b) => ent(world, *a).is_some_and(|e| world.get::<crate::signal::Linked>(e).is_some()) && ent(world, *b).is_some(),
        _ => true,
    });
    if !ready {
        return;
    }
    world.remove_resource::<Pending>();
    for op in ops {
        match op {
            Param(c, key, val) => {
                if let Some(e) = ent(world, c) {
                    let id = world.get::<BlockEntity>(e).unwrap().id;
                    if let Some((i, _)) = id.def().param(key)
                        && let Some(mut p) = world.get_mut::<Params>(e)
                    {
                        p.0[i] = val;
                    }
                }
            }
            Fill(c, f, n) => {
                if let Some(e) = ent(world, c)
                    && let Some(mut s) = world.get_mut::<FluidStore>(e)
                {
                    s.add(f, n);
                }
            }
            Lever(c) => {
                if let Some(e) = ent(world, c) {
                    let button = world.get::<BlockEntity>(e).is_some_and(|b| b.mech() == Some(crate::content::Mech::Button));
                    let Some(mut s) = world.get_mut::<crate::logic::LogicState>(e) else { continue };
                    if button {
                        s.t = crate::logic::BUTTON;
                    } else {
                        s.memory = !s.memory;
                    }
                }
            }
            Link(a, b) => {
                if let Some(e) = ent(world, a)
                    && let Some(mut l) = world.get_mut::<crate::signal::Linked>(e)
                {
                    l.0 = Some(crate::signal::PanelRef { grid: g, cell: b });
                }
            }
            _ => {}
        }
    }
}

fn reset_hold(mut ponder: ResMut<Ponder>) {
    ponder.hold = 0.0;
}

pub struct PonderPlugin;

impl Plugin for PonderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ponder>()
            .add_systems(Update, hold_w.run_if(in_state(Ui::Inventory)))
            .add_systems(OnEnter(Ui::Ponder), open)
            .add_systems(OnExit(Ui::Ponder), close)
            .add_systems(OnExit(Ui::Inventory), reset_hold)
            .add_systems(Update, (run, apply_pending).chain().run_if(in_state(Ui::Ponder)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_use_real_blocks_and_settings() {
        let reg = crate::content::registry();
        let mut n = 0;
        for it in &reg.items {
            let Some(steps) = scene(&it.key) else { continue };
            n += 1;
            assert!(!steps.is_empty() && steps.iter().all(|s| !s.caption.is_empty()), "{}", it.key);
            for op in steps.iter().flat_map(|s| s.ops.iter()) {
                match op {
                    Block(_, k, _) => assert!(reg.block(k).is_some(), "{}: block {k}", it.key),
                    Param(c, k, _) => {
                        // The block at c is placed by an earlier op and has the setting.
                        let placed = steps.iter().flat_map(|s| s.ops.iter()).find_map(|o| if let Block(bc, bk, _) = o { (bc == c).then_some(*bk) } else { None });
                        let b = placed.unwrap_or_else(|| panic!("{}: param on an empty cell {c}", it.key));
                        assert!(BlockId::k(b).def().param(k).is_some(), "{}: {b} has no {k}", it.key);
                    }
                    _ => {}
                }
            }
        }
        assert!(n >= 40, "{n} items explained");
    }
}
