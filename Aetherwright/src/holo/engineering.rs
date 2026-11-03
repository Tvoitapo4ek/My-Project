//! Engineering panels: a holographic plate that appears above the machine
//! part you look at and shows its real state — energy, power, load,
//! rotation speed, connections, mass, diagnostics. It lives in the world
//! (normal render layer, hidden by walls) and does not take input, so it
//! never blocks gameplay. Machine controls stay in the «Механика» mode.

use avian3d::prelude::{AngularVelocity, ComputedMass};
use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    light::NotShadowCaster,
    prelude::*,
    render::render_resource::TextureFormat,
};

use super::style::{self, AMBER, CYAN, DIM, WHITE};
use crate::{
    core::{
        resources::MainCamera,
        states::InputFocus,
    },
    crafting::stations::{CrateStorage, Machine},
    machinery::{
        components::{AetherDynamo, Engine, Part, PartKind, PowerCell},
        power::PowerStats,
    },
    magic::sources::AetherStore,
    physics::joints::{Connection, JointKind, construction_of},
    tools::EntityTarget,
};

const PX: UVec2 = UVec2::new(520, 400);
const SIZE: Vec2 = Vec2::new(0.78, 0.6);
const RANGE: f32 = 7.0;

#[derive(Component)]
struct EngPanel {
    camera: Entity,
}

#[derive(Component)]
struct EngTitle;

#[derive(Component)]
struct EngBody;

#[derive(Component)]
struct EngDiag;

/// Which part the panel shows and how visible it is.
#[derive(Resource, Debug, Default)]
pub struct EngineeringTarget {
    pub entity: Option<Entity>,
    pub fade: f32,
    /// Seconds the panel stays after looking away.
    linger: f32,
    refresh: f32,
}

impl EngineeringTarget {
    /// Shows the panel for `e` for `secs` seconds even if not aimed at
    /// (tutorials, tests, a future «pin panel» action).
    pub fn pin(&mut self, e: Entity, secs: f32) {
        self.entity = Some(e);
        self.fade = 1.0;
        self.linger = secs;
        self.refresh = 0.0;
    }
}

fn spawn_panel(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut image = Image::new_target_texture(PX.x, PX.y, TextureFormat::Bgra8UnormSrgb, None);
    image.sampler = bevy::image::ImageSampler::linear();
    let handle = images.add(image);
    let camera = commands
        .spawn((
            Name::new("Engineering UI Camera"),
            Camera2d,
            Camera { order: -40, is_active: false, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(ImageRenderTarget { handle: handle.clone(), scale_factor: 1.0 }),
        ))
        .id();
    commands
        .spawn((
            style::frame(Node {
                width: px(PX.x as f32),
                height: px(PX.y as f32),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(14)),
                row_gap: px(6),
                ..default()
            }),
            UiTargetCamera(camera),
        ))
        .with_children(|p| {
            p.spawn((style::label("", 22.0, CYAN), EngTitle));
            p.spawn((
                Node { height: px(1), width: percent(100), ..default() },
                BackgroundColor(style::LINE_DIM),
            ));
            p.spawn((style::label("", 15.0, WHITE), EngBody));
            p.spawn((style::label("", 14.0, AMBER), EngDiag));
            p.spawn(style::label("Управление — режим «Механика» манипулятора", 12.0, DIM));
        });
    commands.spawn((
        Name::new("Engineering Panel"),
        EngPanel { camera },
        Mesh3d(meshes.add(Rectangle::new(SIZE.x, SIZE.y))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(handle),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
    ));
}

/// Picks the part under the crosshair (gameplay only).
fn pick_target(
    time: Res<Time>,
    target: Res<EntityTarget>,
    parts: Query<(), Or<(With<Part>, With<crate::kinetics::KineticBlock>)>>,
    focus: Res<State<InputFocus>>,
    mut eng: ResMut<EngineeringTarget>,
) {
    let dt = time.delta_secs();
    let aimed = target
        .entity
        .filter(|e| parts.contains(*e) && target.distance < RANGE && *focus.get() == InputFocus::Gameplay);
    match aimed {
        Some(e) => {
            if eng.entity != Some(e) {
                eng.refresh = 0.0;
            }
            eng.entity = Some(e);
            eng.linger = 0.8;
            eng.fade = (eng.fade + dt * 5.0).min(1.0);
        }
        None => {
            eng.linger -= dt;
            if eng.linger <= 0.0 {
                eng.fade = (eng.fade - dt * 4.0).max(0.0);
                if eng.fade == 0.0 {
                    eng.entity = None;
                }
            }
        }
    }
    if eng.entity.is_some_and(|e| !parts.contains(e)) {
        eng.entity = None;
        eng.fade = 0.0;
    }
}

fn kind_name(k: PartKind) -> String {
    crate::items::ItemId::from_key(k.key()).map(|i| i.def().name.clone()).unwrap_or_else(|| k.key().into())
}

/// Text of the panel from the part's real components.
pub fn describe(world: &mut World, e: Entity) -> Option<(String, String, String)> {
    if let Some((name, mut lines, mut diag)) = crate::kinetics::describe_block(world, e) {
        if let Some((l, d)) = crate::electric::describe(world, e) {
            lines.extend(l);
            diag.extend(d);
        }
        if let Some((l, d)) = crate::arcana::describe(world, e) {
            lines.extend(l);
            diag.extend(d);
        }
        let diag = if diag.is_empty() { "✓ Неисправностей не найдено".to_string() } else { diag.iter().map(|d| format!("! {d}")).collect::<Vec<_>>().join("\n") };
        return Some((name, lines.join("\n"), diag));
    }
    let er = world.get_entity(e).ok()?;
    let part = er.get::<Part>()?.kind;
    let mut lines: Vec<String> = Vec::new();
    let mut diag: Vec<String> = Vec::new();
    let net = world.resource::<PowerStats>().by_entity.get(&e).copied();
    let er = world.get_entity(e).ok()?;
    if let Some(m) = er.get::<ComputedMass>() {
        lines.push(format!("Масса: {:.0} кг", m.value()));
    }
    let omega = er.get::<AngularVelocity>().map(|w| w.0.length()).unwrap_or(0.0);
    if let Some(en) = er.get::<Engine>() {
        let power = if en.on { en.draw * en.throttle } else { 0.0 };
        lines.push(format!(
            "Двигатель: {} · газ {:.0}%{}",
            if en.on { "включён" } else { "выключен" },
            en.throttle * 100.0,
            if en.reverse { " · реверс" } else { "" }
        ));
        lines.push(format!("Потребление: {power:.1} ед/с · питание: {}", if en.powered { "есть" } else { "нет" }));
        lines.push(format!("Скорость: {:.0} об/мин (макс {:.0})", omega * 60.0 / std::f32::consts::TAU, en.max_speed * 60.0 / std::f32::consts::TAU));
        if en.on && en.powered {
            let load = (1.0 - (omega / en.max_speed.max(0.01)).min(1.0)) * 100.0;
            lines.push(format!("Нагрузка (оценка по скорости): {load:.0}%"));
            if load > 70.0 {
                diag.push("Перегрузка: вал почти не вращается".into());
            }
        }
        if en.on && !en.powered {
            diag.push("Нет питания: нужна заряженная энергоячейка в конструкции".into());
        }
        if !en.on {
            diag.push("Двигатель выключен".into());
        }
    } else if omega > 0.05 {
        lines.push(format!("Вращение: {:.0} об/мин", omega * 60.0 / std::f32::consts::TAU));
    }
    if let Some(c) = er.get::<PowerCell>() {
        lines.push(format!("Заряд ячейки: {:.0} / {:.0} ({:.0}%)", c.energy, c.capacity, c.energy * 100.0 / c.capacity.max(0.01)));
    }
    if let Some(d) = er.get::<AetherDynamo>() {
        lines.push(format!("Выработка: {:.1} ед/с", d.output));
        if d.output <= 0.0 {
            diag.push("Динамо простаивает: нет эфира в соединённом накопителе".into());
        }
    }
    if let Some(a) = er.get::<AetherStore>() {
        lines.push(format!("Эфир: {:.0} / {:.0} ({})", a.amount, a.capacity, a.kind.label()));
    }
    if let Some(d) = er.get::<crate::kinetics::KineticDrive>() {
        lines.push(format!("Вращение от сети: {:.0} об/мин", d.0));
    }
    if let Some(m) = er.get::<Machine>() {
        let st = |s: Option<crate::items::ItemStack>| s.map(|s| format!("{} ×{}", s.item.def().name, s.count)).unwrap_or("—".into());
        lines.push(format!("Вход: {} · выход: {}", st(m.input), st(m.output)));
        lines.push(format!("Прогресс: {:.0}% · топливо: {} · вал {:.1} рад/с", m.progress * 100.0, m.fuel_ops, m.drive_speed));
        if m.input.is_none() {
            diag.push("Нет сырья".into());
        }
    }
    if let Some(c) = er.get::<CrateStorage>() {
        let used = c.0.slots.iter().filter(|s| s.is_some()).count();
        lines.push(format!("Ящик: занято {used} из {}", c.0.slots.len()));
    }
    if let Some((energy, cap)) = net {
        if cap > 0.0 {
            lines.push(format!("Сеть: {energy:.0} / {cap:.0} ед ({:.0}%)", energy * 100.0 / cap));
            if energy / cap < 0.1 {
                diag.push("Сеть почти разряжена".into());
            }
        } else if matches!(part, PartKind::Engine) {
            diag.push("В конструкции нет энергоячейки".into());
        }
    }
    let mut q = world.query::<&Connection>();
    let links: Vec<(Entity, Entity)> = q.iter(world).map(|c| (c.a, c.b)).collect();
    let own: Vec<JointKind> = q.iter(world).filter(|c| c.a == e || c.b == e).map(|c| c.kind).collect();
    let welds = own.iter().filter(|k| **k == JointKind::Weld).count();
    let hinges = own.len() - welds;
    let size = construction_of(e, &links).len();
    lines.push(format!("Соединения: {welds} жёстких, {hinges} шарниров · деталей в конструкции: {size}"));
    if own.is_empty() && !part.is_station() && !matches!(part, PartKind::Crate) {
        diag.push("Деталь ни с чем не соединена".into());
    }
    if let Some((l, d)) = crate::electric::describe(world, e) {
        lines.extend(l);
        diag.extend(d);
    }
    if let Some((l, d)) = crate::arcana::describe(world, e) {
        lines.extend(l);
        diag.extend(d);
    }
    lines.push("Целостность: нет данных (повреждения — этап V5)".into());
    let diag = if diag.is_empty() { "✓ Неисправностей не найдено".to_string() } else { diag.iter().map(|d| format!("! {d}")).collect::<Vec<_>>().join("\n") };
    Some((kind_name(part), lines.join("\n"), diag))
}

fn update_panel(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let (entity, fade, refresh) = {
        let mut eng = world.resource_mut::<EngineeringTarget>();
        eng.refresh -= dt;
        let r = eng.refresh <= 0.0;
        if r {
            eng.refresh = 0.25;
        }
        (eng.entity, eng.fade, r)
    };
    let cam = {
        let mut q = world.query_filtered::<&Transform, With<MainCamera>>();
        q.iter(world).next().copied()
    };
    let text = match (entity, refresh) {
        (Some(e), true) => describe(world, e),
        _ => None,
    };
    let part_t = entity.and_then(|e| world.get::<Transform>(e).copied());
    let half = entity.and_then(|e| world.get::<Part>(e)).map(|p| p.kind.half_height()).unwrap_or(0.45);
    let mut q = world.query::<(&EngPanel, &mut Transform, &mut GlobalTransform, &mut Visibility)>();
    let mut cam_entity = None;
    for (panel, mut t, mut gt, mut vis) in q.iter_mut(world) {
        cam_entity = Some(panel.camera);
        let shown = fade > 0.0 && part_t.is_some();
        let want = if shown { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let (Some(pt), Some(c)) = (part_t, cam) {
            let pos = pt.translation + Vec3::Y * (half + 0.55);
            let mut dir = c.translation - pos;
            dir.y = 0.0;
            let rot = Transform::IDENTITY.looking_to(-dir.normalize_or(Vec3::Z), Vec3::Y).rotation;
            let s = 0.6 + 0.4 * fade;
            *t = Transform { translation: pos, rotation: rot, scale: Vec3::new(s, s * fade.max(0.05), 1.0) };
            // Propagation already ran this frame; keep the drawn pose current.
            *gt = GlobalTransform::from(*t);
        }
    }
    if let Some(c) = cam_entity
        && let Some(mut camera) = world.get_mut::<Camera>(c)
    {
        let active = fade > 0.0;
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
    if let Some((title, body, diag)) = text {
        let set = |w: &mut World, s: String, f: fn(&mut World) -> Option<Entity>| {
            if let Some(e) = f(w)
                && let Some(mut t) = w.get_mut::<Text>(e)
                && t.0 != s
            {
                t.0 = s;
            }
        };
        set(world, title, |w| w.query_filtered::<Entity, With<EngTitle>>().iter(w).next());
        set(world, body, |w| w.query_filtered::<Entity, With<EngBody>>().iter(w).next());
        set(world, diag, |w| w.query_filtered::<Entity, With<EngDiag>>().iter(w).next());
    }
}

pub struct EngineeringPlugin;

impl Plugin for EngineeringPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EngineeringTarget>()
            .add_systems(Startup, spawn_panel)
            .add_systems(Update, pick_target.run_if(crate::core::states::in_game))
            // Exclusive: runs in `Last` so it never adds a sync point inside
            // gameplay logic (that changed the order of joint commands).
            .add_systems(Last, update_panel.run_if(crate::core::states::in_game));
    }
}
