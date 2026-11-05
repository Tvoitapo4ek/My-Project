//! Debug tools.
//! * F3 — debug menu: FPS, frame sections (profiler), memory and CPU of the
//!   game, the graphics adapter and driver, entity / body / chunk counts,
//!   position, time; everything about the block under the crosshair
//!   (kind, facing, grid, settings, speed and load, fluid, logic, engine,
//!   items); buttons stop/start time and set the time of day / speed.
//! * T or / — console. Commands: `help`, `time set <0-24|morning|day|
//!   evening|night>`, `time stop|start`, `time speed <×>`, `tp <x> <y> <z>`,
//!   `fly`, `give <key> [count]`, `clear`.

use avian3d::prelude::*;
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};

use crate::{
    content::registry,
    core::Ui,
    inventory::{Inventory, Stack},
    player::{Body, Player},
    sky::Clock,
    ui::font,
};

/// Default speed: a day in 20 real minutes.
const BASE_SPEED: f32 = 24.0 / 1200.0;

/// Runs a console command; returns the reply.
pub fn run_command(world: &mut World, line: &str) -> String {
    let parts: Vec<&str> = line.trim().trim_start_matches('/').split_whitespace().collect();
    let num = |i: usize| parts.get(i).and_then(|s| s.replace(',', ".").parse::<f32>().ok());
    match parts.as_slice() {
        [] => String::new(),
        ["help"] => "time set <ч|morning|day|evening|night>, time stop|start, time speed <×>, tp x y z, fly, give <ключ> [n], clear, save <имя>, load <имя>, blueprints, model export <ключ|player>, world new|load <имя>, world save, worlds".into(),
        ["time", "set", v] => {
            let h = match *v {
                "morning" | "утро" => Some(6.5),
                "day" | "noon" | "день" => Some(12.0),
                "evening" | "вечер" => Some(18.0),
                "night" | "ночь" => Some(0.0),
                _ => num(2),
            };
            match h {
                Some(h) => {
                    world.resource_mut::<Clock>().hours = h.rem_euclid(24.0);
                    format!("Время: {}", world.resource::<Clock>().label())
                }
                None => "time set: нужно число часов или morning/day/evening/night".into(),
            }
        }
        ["time", "stop"] | ["time", "pause"] => {
            world.resource_mut::<Clock>().paused = true;
            "Время остановлено".into()
        }
        ["time", "start"] | ["time", "resume"] => {
            world.resource_mut::<Clock>().paused = false;
            "Время идёт".into()
        }
        ["time", "speed", _] => match num(2) {
            Some(x) if x >= 0.0 => {
                world.resource_mut::<Clock>().speed = BASE_SPEED * x;
                format!("Скорость времени ×{x}")
            }
            _ => "time speed: нужно число ≥ 0".into(),
        },
        ["tp", _, _, _] => match (num(1), num(2), num(3)) {
            (Some(x), Some(y), Some(z)) => {
                let mut q = world.query_filtered::<(&mut Position, &mut LinearVelocity), With<Player>>();
                for (mut p, mut v) in q.iter_mut(world) {
                    p.0 = Vec3::new(x, y + 0.95, z);
                    v.0 = Vec3::ZERO;
                }
                format!("Телепорт: {x} {y} {z}")
            }
            _ => "tp: нужны три числа".into(),
        },
        ["fly"] => {
            let mut q = world.query_filtered::<&mut Body, With<Player>>();
            let mut on = false;
            for mut b in q.iter_mut(world) {
                b.flying = !b.flying;
                on = b.flying;
            }
            if on { "Полёт включён".into() } else { "Полёт выключен".into() }
        }
        ["give", key, ..] => match registry().item(key) {
            Some(item) => {
                let n = num(2).map(|n| n as u32).unwrap_or(item.def().max_stack);
                let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
                for mut inv in q.iter_mut(world) {
                    inv.add(Stack::new(item, n));
                }
                format!("Выдано: {}", item.def().name)
            }
            None => format!("Нет предмета «{key}»"),
        },
        ["save", name] => crate::tools::blueprint::save_named(world, name),
        ["load", name] => crate::tools::blueprint::load_named(world, name),
        ["blueprints"] => crate::tools::blueprint::list(),
        ["model", "export", key] => crate::bbmodel::export_cmd(key),
        ["world", ..] | ["worlds"] => crate::worlds::command(world, &parts),
        ["clear"] => {
            let mut q = world.query_filtered::<&mut Inventory, With<Player>>();
            for mut inv in q.iter_mut(world) {
                *inv = Inventory::default();
            }
            "Инвентарь очищен".into()
        }
        _ => format!("Неизвестная команда: {line}  (help — список)"),
    }
}

// ------------------------------------------------------------ console

#[derive(Resource, Default)]
pub struct Console {
    pub line: String,
    pub log: Vec<String>,
    /// Commands waiting to run (typed, or queued by scripts).
    pub queue: Vec<String>,
}

#[derive(Component)]
struct ConsoleRoot;
#[derive(Component)]
struct ConsoleText;

fn open_keys(keys: Res<ButtonInput<KeyCode>>, ui: Res<State<Ui>>, mut next: ResMut<NextState<Ui>>, mut con: ResMut<Console>) {
    if *ui.get() == Ui::Game {
        if keys.just_pressed(KeyCode::KeyT) || keys.just_pressed(KeyCode::Slash) {
            con.line = if keys.just_pressed(KeyCode::Slash) { "/".into() } else { String::new() };
            next.set(Ui::Console);
        } else if keys.just_pressed(KeyCode::F3) {
            next.set(Ui::Debug);
        }
    } else if *ui.get() == Ui::Debug && keys.just_pressed(KeyCode::F3) {
        next.set(Ui::Game);
    }
}

fn spawn_console(mut commands: Commands) {
    commands
        .spawn((ConsoleRoot, Node { position_type: PositionType::Absolute, left: px(8), bottom: px(90), width: px(640), padding: UiRect::all(px(8)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))))
        .with_children(|c| {
            c.spawn((ConsoleText, Text::new(""), font(15.0), TextColor(Color::WHITE)));
        });
}

fn despawn_console(mut commands: Commands, q: Query<Entity, With<ConsoleRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn type_console(mut keys: MessageReader<KeyboardInput>, mut con: ResMut<Console>, mut next: ResMut<NextState<Ui>>, mut skip_first: Local<bool>, ui: Res<State<Ui>>) {
    if *ui.get() != Ui::Console {
        keys.clear();
        *skip_first = true;
        return;
    }
    for k in keys.read() {
        if !k.state.is_pressed() {
            continue;
        }
        // The key that opened the console must not be typed.
        if *skip_first {
            *skip_first = false;
            if matches!(&k.logical_key, Key::Character(s) if s == "t" || s == "T" || s == "е" || s == "Е" || s == "/" || s == ".") {
                continue;
            }
        }
        match &k.logical_key {
            Key::Character(s) => con.line.push_str(s),
            Key::Space => con.line.push(' '),
            Key::Backspace => {
                con.line.pop();
            }
            Key::Enter => {
                let l = std::mem::take(&mut con.line);
                con.queue.push(l);
                next.set(Ui::Game);
            }
            _ => {}
        }
    }
}

fn show_console(con: Res<Console>, mut q: Query<&mut Text, With<ConsoleText>>) {
    let Ok(mut t) = q.single_mut() else { return };
    let mut s: String = con.log.iter().rev().take(6).rev().map(|l| format!("{l}\n")).collect();
    s.push_str(&format!("> {}▏", con.line));
    if t.0 != s {
        t.0 = s;
    }
}

/// Executes queued commands with full world access.
fn execute(world: &mut World) {
    let queue = std::mem::take(&mut world.resource_mut::<Console>().queue);
    for line in queue {
        if line.trim().is_empty() {
            continue;
        }
        let reply = run_command(world, &line);
        info!("console: {line} → {reply}");
        let mut con = world.resource_mut::<Console>();
        con.log.push(format!("> {line}"));
        con.log.push(reply.clone());
        world.write_message(crate::core::Toast(reply));
    }
}

// ------------------------------------------------------------ F3 menu

#[derive(Component)]
struct DebugRoot;
#[derive(Component)]
pub struct DebugInfo;

/// The text of the F3 menu (for scripts).
pub type DebugInfoText = DebugInfo;

#[derive(Component, Clone)]
struct Cmd(&'static str);

fn spawn_menu(mut commands: Commands) {
    commands
        .spawn((DebugRoot, Node { position_type: PositionType::Absolute, left: px(8), top: px(8), flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(10)), ..default() }, BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.85))))
        .with_children(|p| {
            p.spawn((DebugInfo, Text::new(""), font(15.0), TextColor(Color::WHITE)));
            let rows: [&[(&str, &'static str)]; 3] = [
                &[("Стоп / пуск времени", "time toggle"), ("−1 ч", "time minus"), ("+1 ч", "time plus")],
                &[("Утро", "time set morning"), ("День", "time set day"), ("Вечер", "time set evening"), ("Ночь", "time set night")],
                &[("×1", "time speed 1"), ("×10", "time speed 10"), ("×60", "time speed 60"), ("Полёт", "fly")],
            ];
            for row in rows {
                p.spawn(Node { column_gap: px(6), ..default() }).with_children(|r| {
                    for (label, cmd) in row {
                        r.spawn((Cmd(cmd), Node { padding: UiRect::axes(px(10), px(5)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgb(0.2, 0.21, 0.24)), BorderColor::all(Color::srgb(0.4, 0.4, 0.44))))
                            .observe(|ev: On<PointerClick>, cmds: Query<&Cmd>, mut con: ResMut<Console>, clock: Res<Clock>| {
                                if let Ok(c) = cmds.get(ev.entity) {
                                    let line = match c.0 {
                                        "time toggle" => if clock.paused { "time start".into() } else { "time stop".into() },
                                        "time minus" => format!("time set {}", (clock.hours - 1.0).rem_euclid(24.0)),
                                        "time plus" => format!("time set {}", (clock.hours + 1.0).rem_euclid(24.0)),
                                        other => other.to_string(),
                                    };
                                    con.queue.push(line);
                                }
                            })
                            .with_children(|b| {
                                b.spawn((Text::new(*label), font(14.0), TextColor(Color::WHITE), Pickable::IGNORE));
                            });
                    }
                });
            }
            p.spawn((Text::new("F3 — закрыть, T или / — консоль"), font(13.0), TextColor(Color::srgb(0.65, 0.65, 0.68))));
        });
}

fn despawn_menu(mut commands: Commands, q: Query<Entity, With<DebugRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Memory and CPU of the game process (from `ps`, every 2 s on a thread).
#[derive(Resource, Clone, Default)]
pub struct SysStats(pub std::sync::Arc<std::sync::Mutex<(f32, f32)>>);

fn poll_stats(stats: Res<SysStats>, time: Res<Time<Real>>, mut next: Local<f32>) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 2.0;
    let s = stats.0.clone();
    std::thread::spawn(move || {
        let pid = std::process::id().to_string();
        if let Ok(out) = std::process::Command::new("ps").args(["-o", "rss=,%cpu=", "-p", &pid]).output() {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut it = text.split_whitespace();
            let rss = it.next().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1024.0;
            let cpu = it.next().and_then(|v| v.replace(',', ".").parse::<f32>().ok()).unwrap_or(0.0);
            if let Ok(mut g) = s.lock() {
                *g = (rss, cpu);
            }
        }
    });
}

/// Everything about the block under the crosshair.
pub fn block_report(world: &World, grid: Entity, cell: IVec3) -> String {
    let Some(g) = world.get::<crate::grid::VoxelGrid>(grid) else { return String::new() };
    let c = g.get(cell);
    if c.is_air() {
        return String::new();
    }
    let def = c.id.def();
    let place = if world.get::<crate::grid::WorldGrid>(grid).is_some() { "мир".to_string() } else if world.get::<crate::mounts::MountedOn>(grid).is_some() { "подвижная часть".to_string() } else { format!("тело {grid}") };
    let mut v = vec![format!("Блок: {} ({}) · {:?} · грань {} · {} кг", def.name, def.key, cell.to_array(), crate::grid::facing_dir(c.facing).to_array().map(|x| x.to_string()).join(" "), def.mass), format!("Сетка: {place}")];
    if let Some(body) = world.get::<LinearVelocity>(grid) {
        v.push(format!("Скорость тела: {:.2} м/с", body.0.length()));
    }
    let Some(&e) = g.entities.get(&cell) else { return v.join("\n") };
    v.push(format!("Сущность: {e}"));
    if let Some(p) = world.get::<crate::mech::Params>(e) {
        let s: Vec<String> = def.params.iter().zip(&p.0).map(|(d, x)| format!("{} = {}", d.label, crate::mech::param_text(&d.kind, *x))).collect();
        if !s.is_empty() {
            v.push(format!("Настройки: {}", s.join(", ")));
        }
    }
    if let (Some(s), Some(n)) = (world.get::<crate::mech::Spin>(e), world.get::<crate::mech::NetState>(e)) {
        v.push(format!("Вращение: {:.1} об/мин · нагрузка {:.0}/{:.0} SU ({:.0}%) · k {:.2}{}{}", s.rpm, n.stress, n.capacity, n.load * 100.0, n.k, if n.driven { " · есть источник" } else { "" }, if n.conflict { " · КОНФЛИКТ" } else { "" }));
    }
    if let Some(sig) = world.get::<crate::mech::Signal>(e).filter(|s| s.0 != 0.0) {
        v.push(format!("Сигнал: {:+.2}", sig.0));
    }
    if let Some(f) = world.get::<crate::fluids::FluidStore>(e) {
        v.push(format!("Жидкость: {} {:.0}/{:.0} mB", f.fluid.map(|f| f.name()).unwrap_or("пусто"), f.amount, f.cap));
    }
    if let Some(l) = world.get::<crate::logic::LogicState>(e) {
        v.push(format!("Логика: вход {}, выход {}, память {}, t {:.2}", l.input, l.out, l.memory, l.t));
    }
    if let Some(b) = world.get::<crate::logic::Bus>(e) {
        let names: Vec<&str> = crate::logic::colours(b.0).map(|k| crate::logic::COLORS[k].0).collect();
        let on = world.get::<crate::logic::BusPower>(e).map(|p| p.0).unwrap_or(0);
        v.push(format!("Шина: {} · включены {:08b}", names.join(", "), on));
    }
    if let Some(en) = world.get::<crate::engine::Engine>(e) {
        v.push(format!("Двигатель: работает {}, готов {}, остановка {:?}", en.running, en.ready, en.stall));
    }
    if let Some(st) = world.get::<crate::storage::ItemStore>(e) {
        let n: u32 = st.slots.iter().flatten().map(|s| s.count).sum();
        v.push(format!("Хранилище: {} предметов в {}/{} ячейках", n, st.slots.iter().flatten().count(), st.slots.len()));
    }
    if let Some(r) = world.get::<crate::ropes::RopeEnd>(e) {
        v.push(format!("Верёвка: {:.2} м", r.length));
    }
    v.join("\n")
}

#[derive(Resource, Default)]
struct BlockInfo(String);

fn hovered(world: &mut World) {
    if *world.resource::<State<Ui>>().get() != Ui::Debug {
        return;
    }
    let text = world.resource::<crate::interact::Target>().hit.map(|a| block_report(world, a.grid, a.cell)).unwrap_or_default();
    world.resource_mut::<BlockInfo>().0 = text;
}

#[allow(clippy::too_many_arguments)]
fn menu_info(diag: Res<DiagnosticsStore>, clock: Res<Clock>, prof: Res<crate::profile::Profile>, stats: Res<SysStats>, adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>, info: Res<BlockInfo>, counts: (Query<()>, Query<(), With<Mesh3d>>, Query<(), With<crate::contraption::Contraption>>, Query<&crate::grid::VoxelGrid>), players: Query<(&Position, &Body), With<Player>>, mut q: Query<&mut Text, With<DebugInfo>>) {
    let Ok(mut t) = q.single_mut() else { return };
    let fps = diag.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let (pos, fly) = players.single().map(|(p, b)| (p.0, b.flying)).unwrap_or_default();
    t.0 = format!(
        "FPS {fps:.0}\nПозиция {:.1} {:.1} {:.1}{}\nВремя {} · {} · скорость ×{:.0}",
        pos.x,
        pos.y - 0.9,
        pos.z,
        if fly { " · полёт" } else { "" },
        clock.label(),
        if clock.paused { "остановлено" } else { "идёт" },
        clock.speed / BASE_SPEED
    );
    let (rss, cpu) = stats.0.lock().map(|g| *g).unwrap_or_default();
    let gpu = adapter.map(|a| format!("{} · {} {} · {:?}", a.name, a.driver, a.driver_info, a.backend)).unwrap_or_else(|| "—".into());
    let chunks: usize = counts.3.iter().map(|g| g.chunks.len()).sum();
    t.0 += &format!(
        "\nПамять {rss:.0} МБ · CPU {cpu:.0}% (всех ядер)\nВидеокарта: {gpu}\nСущностей {} · мешей {} · тел {} · чанков {chunks}\n\n{}",
        counts.0.iter().count(),
        counts.1.iter().count(),
        counts.2.iter().count(),
        crate::profile::report(&prof)
    );
    if !info.0.is_empty() {
        t.0 += &format!("\n\nПод прицелом:\n{}", info.0);
    }
}

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .init_resource::<Console>()
            .init_resource::<SysStats>()
            .init_resource::<BlockInfo>()
            .add_systems(Update, (poll_stats.run_if(in_state(Ui::Debug)), hovered.before(menu_info)))
            .add_systems(Update, (open_keys, type_console, show_console.run_if(in_state(Ui::Console)), menu_info.run_if(in_state(Ui::Debug)), execute))
            .add_systems(OnEnter(Ui::Console), spawn_console)
            .add_systems(OnExit(Ui::Console), despawn_console)
            .add_systems(OnEnter(Ui::Debug), spawn_menu)
            .add_systems(OnExit(Ui::Debug), despawn_menu);
    }
}
