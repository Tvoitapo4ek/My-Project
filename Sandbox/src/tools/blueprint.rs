//! Blueprint tool («Чертёж»): copy a build and paste it elsewhere.
//!
//! * LMB on a block copies its glued group (everything glued to it; just the
//!   block if nothing is) — or the whole physical contraption — with block
//!   settings, sensor/telemetry links and glue. The copy is also written to
//!   `schematics/saved_contraption.ron`.
//! * While the blueprint is in hand, a blue hologram shows where the copy
//!   will go (red if the space is taken); R turns it by 90°, RMB places it as
//!   ordinary blocks.
//! * Console: `save <name>` / `load <name>` keep blueprints in
//!   `schematics/<name>.ron`; `blueprints` lists them.

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{
    content::{Tool, registry},
    contraption::{Contraption, glued_group},
    core::{Toast, Ui, toast},
    grid::{Cell, VoxelGrid, WorldGrid, rotate_facing},
    interact::{Clicks, Target},
    inventory::Inventory,
    mech::{BlockEntity, Params},
    player::{Body, Player},
    signal::{Linked, PanelRef},
    tools::glue::{Glue, inside},
};

pub const DIR: &str = "schematics";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SBlock {
    pub at: IVec3,
    pub key: String,
    pub facing: u8,
    /// Settings by key (only blocks with settings).
    #[serde(default)]
    pub params: Vec<(String, f32)>,
}

/// A saved build: cells relative to its minimum corner.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Schematic {
    pub blocks: Vec<SBlock>,
    /// Links (sensor cell → panel / display cell).
    pub links: Vec<(IVec3, IVec3)>,
    pub glue: Vec<(IVec3, IVec3)>,
}

impl Schematic {
    pub fn size(&self) -> IVec3 {
        self.blocks.iter().fold(IVec3::ZERO, |m, b| m.max(b.at + IVec3::ONE))
    }

    /// Turned `quarters` × 90° about Y and moved back to the origin.
    pub fn rotated(&self, quarters: u8) -> Schematic {
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 * (quarters % 4) as f32);
        // A cell [c, c+1) turns into the cell whose centre is turn(c + ½).
        let cell = |c: IVec3| (q * (c.as_vec3() + Vec3::splat(0.5)) - Vec3::splat(0.5)).round().as_ivec3();
        let mut out = self.clone();
        for b in &mut out.blocks {
            b.at = cell(b.at);
            for _ in 0..quarters % 4 {
                b.facing = rotate_facing(b.facing, IVec3::Y);
            }
        }
        for l in &mut out.links {
            *l = (cell(l.0), cell(l.1));
        }
        for g in &mut out.glue {
            let (a, b) = (cell(g.0), cell(g.1));
            *g = (a.min(b), a.max(b));
        }
        let min = out.blocks.iter().fold(IVec3::splat(i32::MAX), |m, b| m.min(b.at));
        if min.x != i32::MAX {
            out.shift(-min);
        }
        out
    }

    fn shift(&mut self, d: IVec3) {
        for b in &mut self.blocks {
            b.at += d;
        }
        for l in &mut self.links {
            *l = (l.0 + d, l.1 + d);
        }
        for g in &mut self.glue {
            *g = (g.0 + d, g.1 + d);
        }
    }
}

/// The blueprint in hand.
#[derive(Resource, Default)]
pub struct Clipboard {
    pub schematic: Option<Schematic>,
    pub quarters: u8,
    /// Bumped on every change (rebuilds the hologram).
    pub version: u32,
}

/// Settings and links to apply once pasted block entities exist.
#[derive(Resource, Default)]
struct Pending {
    grid: Option<Entity>,
    params: Vec<(IVec3, Vec<(String, f32)>)>,
    links: Vec<(IVec3, IVec3)>,
    tries: u32,
}

#[derive(Component)]
struct Hologram {
    version: u32,
}

fn name_ok(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

pub fn write_file(s: &Schematic, name: &str) -> Result<String, String> {
    if !name_ok(name) {
        return Err("имя: буквы, цифры, _ и -".into());
    }
    std::fs::create_dir_all(DIR).map_err(|e| e.to_string())?;
    let path = format!("{DIR}/{name}.ron");
    let text = ron::ser::to_string_pretty(s, ron::ser::PrettyConfig::default()).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn read_file(name: &str) -> Result<Schematic, String> {
    if !name_ok(name) {
        return Err("имя: буквы, цифры, _ и -".into());
    }
    let text = std::fs::read_to_string(format!("{DIR}/{name}.ron")).map_err(|_| format!("нет чертежа «{name}»"))?;
    ron::from_str(&text).map_err(|e| e.to_string())
}

/// Console `save <name>`.
pub fn save_named(world: &mut World, name: &str) -> String {
    let Some(s) = world.resource::<Clipboard>().schematic.clone() else {
        return "Чертёж пуст: ЛКМ «Чертежом» по постройке".into();
    };
    match write_file(&s, name) {
        Ok(p) => format!("Чертёж сохранён: {p} ({} блоков)", s.blocks.len()),
        Err(e) => format!("Не удалось сохранить: {e}"),
    }
}

/// Console `load <name>`.
pub fn load_named(world: &mut World, name: &str) -> String {
    match read_file(name) {
        Ok(s) => {
            let n = s.blocks.len();
            let mut c = world.resource_mut::<Clipboard>();
            c.schematic = Some(s);
            c.quarters = 0;
            c.version += 1;
            format!("Чертёж «{name}» загружен ({n} блоков) — возьмите «Чертёж» в руку")
        }
        Err(e) => format!("Не удалось загрузить: {e}"),
    }
}

/// Console `blueprints`.
pub fn list() -> String {
    let mut names: Vec<String> = std::fs::read_dir(DIR)
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".ron")).map(String::from)).collect())
        .unwrap_or_default();
    names.sort();
    if names.is_empty() { "Сохранённых чертежей нет".into() } else { format!("Чертежи: {}", names.join(", ")) }
}

/// Copies cells of a grid into a schematic.
pub fn capture(grid: &VoxelGrid, glue: Option<&Glue>, cells: &[IVec3], params: impl Fn(IVec3) -> Vec<(String, f32)>, link: impl Fn(IVec3) -> Option<IVec3>) -> Schematic {
    let min = cells.iter().fold(IVec3::splat(i32::MAX), |m, c| m.min(*c));
    let mut s = Schematic::default();
    for &c in cells {
        let cell = grid.get(c);
        if cell.is_air() {
            continue;
        }
        s.blocks.push(SBlock { at: c - min, key: cell.id.def().key.clone(), facing: cell.facing, params: params(c) });
        if let Some(t) = link(c)
            && cells.contains(&t)
        {
            s.links.push((c - min, t - min));
        }
    }
    if let Some(g) = glue {
        for (lo, hi) in &g.0 {
            if cells.iter().any(|c| inside(*c, *lo, *hi)) {
                s.glue.push((*lo - min, *hi - min));
            }
        }
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn use_tool(
    clicks: Res<Clicks>,
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<State<Ui>>,
    target: Res<Target>,
    mut players: Query<(&Inventory, &mut Body), With<Player>>,
    mut grids: Query<(Entity, &mut VoxelGrid, Option<&Glue>, Has<Contraption>, Has<WorldGrid>)>,
    blocks: Query<(&BlockEntity, &Params, Option<&Linked>)>,
    mut clip: ResMut<Clipboard>,
    mut pending: ResMut<Pending>,
    mut commands: Commands,
    mut toasts: MessageWriter<Toast>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
) {
    let Ok((inv, mut body)) = players.single_mut() else { return };
    if super::held_tool(inv) != Some(Tool::Blueprint) || *ui.get() != Ui::Game {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) && clip.schematic.is_some() {
        clip.quarters = (clip.quarters + 1) % 4;
        clip.version += 1;
        sounds.write(crate::sound::sfx("rotate"));
    }
    let Some(a) = target.hit else { return };
    if clicks.left {
        let Ok((_, grid, glue, is_body, _)) = grids.get(a.grid) else { return };
        body.swing = 1.0;
        let cells: Vec<IVec3> = if is_body { grid.cells().into_iter().map(|(c, _)| c).collect() } else { glued_group(&grid, glue, a.cell) };
        let params = |c: IVec3| {
            let Some((b, p, _)) = grid.entities.get(&c).and_then(|e| blocks.get(*e).ok()) else { return vec![] };
            b.id.def().params.iter().enumerate().map(|(i, d)| (d.key.clone(), p.0.get(i).copied().unwrap_or(d.default))).collect()
        };
        let link = |c: IVec3| grid.entities.get(&c).and_then(|e| blocks.get(*e).ok()).and_then(|(_, _, l)| l.and_then(|l| l.0)).filter(|r| r.grid == a.grid).map(|r| r.cell);
        let s = capture(&grid, glue, &cells, params, link);
        let n = s.blocks.len();
        let _ = write_file(&s, "saved_contraption");
        clip.schematic = Some(s);
        clip.quarters = 0;
        clip.version += 1;
        toasts.write(toast(format!("Скопировано в чертёж: {n} блоков. ПКМ — поставить, R — повернуть")));
        sounds.write(crate::sound::sfx("link"));
        return;
    }
    if !clicks.right {
        return;
    }
    let Some(s) = clip.schematic.as_ref().map(|s| s.rotated(clip.quarters)) else {
        toasts.write(toast("Чертёж пуст: ЛКМ по постройке, чтобы скопировать"));
        return;
    };
    let Ok((ge, mut grid, glue, _, is_world)) = grids.get_mut(a.grid) else { return };
    if !is_world {
        toasts.write(toast("Чертёж ставится только в мир"));
        return;
    }
    let origin = paste_origin(&s, a.cell + a.normal);
    if !fits(&grid, &s, origin) {
        toasts.write(toast("Здесь занято"));
        return;
    }
    body.swing = 1.0;
    for b in &s.blocks {
        if let Some(id) = registry().block(&b.key) {
            grid.set(origin + b.at, Cell::new(id, b.facing));
        }
    }
    let mut boxes = glue.map(|g| g.0.clone()).unwrap_or_default();
    boxes.extend(s.glue.iter().map(|(lo, hi)| (*lo + origin, *hi + origin)));
    commands.entity(ge).insert(Glue(boxes));
    *pending = Pending {
        grid: Some(ge),
        params: s.blocks.iter().filter(|b| !b.params.is_empty()).map(|b| (origin + b.at, b.params.clone())).collect(),
        links: s.links.iter().map(|(a, b)| (*a + origin, *b + origin)).collect(),
        tries: 0,
    };
    toasts.write(toast(format!("Поставлено: {} блоков", s.blocks.len())));
    sounds.write(crate::sound::sfx("physics_on"));
}

/// Bottom centre of the schematic at the anchor cell.
pub fn paste_origin(s: &Schematic, anchor: IVec3) -> IVec3 {
    let size = s.size();
    anchor - IVec3::new(size.x / 2, 0, size.z / 2)
}

pub fn fits(grid: &VoxelGrid, s: &Schematic, origin: IVec3) -> bool {
    s.blocks.iter().all(|b| grid.get(origin + b.at).is_air())
}

/// Settings and links of pasted blocks, once their entities exist.
fn apply_pending(mut commands: Commands, mut pending: ResMut<Pending>, grids: Query<&VoxelGrid>, mut blocks: Query<(&BlockEntity, &mut Params, Option<&mut Linked>)>) {
    let Some(ge) = pending.grid else { return };
    let Ok(grid) = grids.get(ge) else {
        pending.grid = None;
        return;
    };
    pending.tries += 1;
    // Wait until the pasted entities exist and sensors got their `Linked`.
    let ready = pending.params.iter().all(|p| grid.entities.contains_key(&p.0))
        && pending.links.iter().all(|l| grid.entities.get(&l.0).is_some_and(|e| blocks.get(*e).is_ok_and(|(_, _, link)| link.is_some())));
    if !ready && pending.tries < 30 {
        return;
    }
    for (c, values) in std::mem::take(&mut pending.params) {
        if let Some((b, mut p, _)) = grid.entities.get(&c).and_then(|e| blocks.get_mut(*e).ok()) {
            for (k, v) in values {
                if let Some((i, _)) = b.id.def().param(&k)
                    && let Some(slot) = p.0.get_mut(i)
                {
                    *slot = v;
                }
            }
        }
    }
    for (from, to) in std::mem::take(&mut pending.links) {
        let Some(&e) = grid.entities.get(&from) else { continue };
        let link = Some(PanelRef { grid: ge, cell: to });
        match blocks.get_mut(e) {
            Ok((_, _, Some(mut l))) => l.0 = link,
            // `Linked` is added to new sensors a frame later: add it now.
            _ => {
                commands.entity(e).insert(Linked(link));
            }
        }
    }
    pending.grid = None;
}

/// Box geometry of every cell (for the hologram).
fn hologram_mesh(s: &Schematic) -> Mesh {
    let (mut pos, mut nor, mut idx) = (Vec::new(), Vec::new(), Vec::<u32>::new());
    let filled: std::collections::HashSet<IVec3> = s.blocks.iter().map(|b| b.at).collect();
    for b in &s.blocks {
        for d in crate::grid::DIRS {
            if filled.contains(&(b.at + d)) {
                continue;
            }
            let n = d.as_vec3();
            let (u, v) = if d.x != 0 { (Vec3::Z, Vec3::Y) } else if d.y != 0 { (Vec3::X, Vec3::Z) } else { (Vec3::X, Vec3::Y) };
            let c = b.at.as_vec3() + Vec3::splat(0.5) + n * 0.49;
            let start = pos.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                pos.push((c + u * su * 0.49 + v * sv * 0.49).to_array());
                nor.push(n.to_array());
            }
            if u.cross(v).dot(n) > 0.0 {
                idx.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
            } else {
                idx.extend_from_slice(&[start, start + 2, start + 1, start, start + 3, start + 2]);
            }
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_indices(Indices::U32(idx));
    m
}

#[derive(Resource)]
struct HoloMats {
    ok: Handle<StandardMaterial>,
    bad: Handle<StandardMaterial>,
}

fn setup(mut commands: Commands, mut mats: ResMut<Assets<StandardMaterial>>) {
    let holo = |c: LinearRgba| StandardMaterial { base_color: Color::LinearRgba(c), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, double_sided: true, ..default() };
    commands.insert_resource(HoloMats { ok: mats.add(holo(LinearRgba::new(0.25, 0.75, 2.2, 0.32))), bad: mats.add(holo(LinearRgba::new(2.0, 0.3, 0.25, 0.32))) });
}

#[allow(clippy::too_many_arguments)]
fn hologram(
    mut commands: Commands,
    clip: Res<Clipboard>,
    target: Res<Target>,
    ui: Res<State<Ui>>,
    holo_mats: Option<Res<HoloMats>>,
    mut meshes: ResMut<Assets<Mesh>>,
    players: Query<&Inventory, With<Player>>,
    grids: Query<&VoxelGrid, With<WorldGrid>>,
    mut holos: Query<(Entity, &mut Hologram, &mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Some(hm) = holo_mats else { return };
    let show = players.single().is_ok_and(|i| super::held_tool(i) == Some(Tool::Blueprint)) && *ui.get() == Ui::Game;
    let Some(s) = clip.schematic.as_ref().map(|s| s.rotated(clip.quarters)) else {
        for (e, ..) in &holos {
            commands.entity(e).despawn();
        }
        return;
    };
    let Ok((e, mut h, mut t, mut vis, mut mat)) = holos.single_mut() else {
        commands.spawn((Hologram { version: clip.version }, Mesh3d(meshes.add(hologram_mesh(&s))), MeshMaterial3d(hm.ok.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster));
        return;
    };
    if h.version != clip.version {
        h.version = clip.version;
        commands.entity(e).insert(Mesh3d(meshes.add(hologram_mesh(&s))));
    }
    let world_hit = target.hit.filter(|a| grids.contains(a.grid) && a.normal != IVec3::ZERO);
    match (show, world_hit) {
        (true, Some(a)) => {
            let origin = paste_origin(&s, a.cell + a.normal);
            t.translation = origin.as_vec3();
            let ok = grids.get(a.grid).is_ok_and(|g| fits(g, &s, origin));
            mat.0 = if ok { hm.ok.clone() } else { hm.bad.clone() };
            vis.set_if_neq(Visibility::Inherited);
        }
        _ => {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
}

pub struct BlueprintPlugin;

impl Plugin for BlueprintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clipboard>()
            .init_resource::<Pending>()
            .add_systems(Startup, setup)
            .add_systems(Update, (use_tool.after(crate::interact::InteractSet), apply_pending, hologram));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    fn sample() -> Schematic {
        Schematic {
            blocks: vec![
                SBlock { at: IVec3::ZERO, key: "creative_motor".into(), facing: 0, params: vec![("speed".into(), 64.0)] },
                SBlock { at: IVec3::X, key: "shaft".into(), facing: 0, params: vec![] },
                SBlock { at: IVec3::new(0, 1, 0), key: "telemetry_sensor".into(), facing: 3, params: vec![] },
            ],
            links: vec![(IVec3::new(0, 1, 0), IVec3::X)],
            glue: vec![(IVec3::ZERO, IVec3::new(1, 1, 0))],
        }
    }

    #[test]
    fn rotation_turns_cells_and_facings() {
        let s = sample();
        let r = s.rotated(1);
        // +X turned 90° about Y → −Z; back in the positive octant.
        let motor = &r.blocks[0];
        let shaft = &r.blocks[1];
        assert_eq!(crate::grid::facing_dir(motor.facing), IVec3::NEG_Z);
        assert_eq!(shaft.at - motor.at, IVec3::NEG_Z);
        assert!(r.blocks.iter().all(|b| b.at.min_element() >= 0));
        assert_eq!(r.size(), IVec3::new(1, 2, 2));
        // Four turns: back to the start.
        assert_eq!(s.rotated(4), s);
        // Links follow.
        assert_eq!(r.links[0].1, shaft.at);
    }

    #[test]
    fn save_and_load_round_trip() {
        let s = sample();
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: Schematic = ron::from_str(&text).unwrap();
        assert_eq!(back, s);
        assert!(name_ok("car_1") && !name_ok("../x") && !name_ok(""));
    }

    #[test]
    fn capture_keeps_internal_links_only() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 0, 5), Cell::new(BlockId::k("stone"), 2));
        g.set(IVec3::new(6, 0, 5), Cell::new(BlockId::k("telemetry_sensor"), 0));
        let s = capture(&g, None, &[IVec3::new(5, 0, 5), IVec3::new(6, 0, 5)], |_| vec![], |c| if c.x == 6 { Some(IVec3::new(9, 9, 9)) } else { None });
        assert_eq!(s.blocks.len(), 2);
        assert_eq!(s.blocks[0].at, IVec3::ZERO);
        assert!(s.links.is_empty());
    }
}
