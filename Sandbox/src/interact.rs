//! Looking at blocks and editing them (creative): LMB breaks, RMB places
//! (or uses a seat / physics toggle; crouch to place instead), MMB picks
//! the block into the hotbar. Works on the world and on contraptions.

use bevy::prelude::*;

use crate::{
    content::{Mech, Placement, registry},
    core::{Ui, toast, Toast},
    grid::{Cell, VoxelGrid, facing_of},
    inventory::Inventory,
    player::{Body, HEIGHT, Player, RADIUS},
};

pub const REACH: f32 = 8.0;

/// Shape new building blocks are placed in (index into `content::SHAPES`),
/// chosen with Shift + wheel while a building block is in hand.
#[derive(Resource, Debug, Default)]
pub struct ShapeMode(pub usize);

/// A cube item that can be placed in shapes.
pub fn shapeable(b: crate::content::BlockId) -> bool {
    registry().shaped(b, 1).is_some()
}

fn pick_shape(keys: Res<ButtonInput<KeyCode>>, scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>, ui: Res<State<Ui>>, players: Query<&Inventory, With<Player>>, mut mode: ResMut<ShapeMode>, mut toasts: MessageWriter<Toast>) {
    if *ui.get() != Ui::Game || !keys.pressed(KeyCode::ShiftLeft) || scroll.delta.y == 0.0 {
        return;
    }
    let Ok(inv) = players.single() else { return };
    if !inv.held().and_then(|s| s.item.block()).is_some_and(shapeable) {
        return;
    }
    let n = crate::content::SHAPES.len() as i32;
    mode.0 = (mode.0 as i32 - scroll.delta.y.signum() as i32).rem_euclid(n) as usize;
    toasts.write(toast(format!("Форма: {}", crate::content::SHAPES[mode.0].2)));
}

/// The block under the crosshair.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct Target {
    pub hit: Option<Aim>,
}

#[derive(Debug, Clone, Copy)]
pub struct Aim {
    pub grid: Entity,
    pub cell: IVec3,
    /// Face normal in grid space.
    pub normal: IVec3,
    pub distance: f32,
}

/// Eye position and look direction (world).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct Eye {
    pub pos: Vec3,
    pub dir: Vec3,
}

/// A block was placed or removed by the player — for other modules
/// (sounds…).
#[derive(Message, Debug, Clone, Copy)]
pub struct Edited {
    pub grid: Entity,
    pub cell: IVec3,
    pub placed: bool,
    /// The block placed or removed.
    pub block: crate::content::BlockId,
}

/// RMB on a block with its own action.
#[derive(Message, Debug, Clone, Copy)]
pub struct UseBlock {
    pub grid: Entity,
    pub cell: IVec3,
    pub mech: Mech,
}

/// Raycast every grid; nearest hit wins.
pub fn aim(grids: &Query<(Entity, &VoxelGrid, &GlobalTransform)>, origin: Vec3, dir: Vec3, reach: f32) -> Option<Aim> {
    let mut best: Option<Aim> = None;
    for (e, g, gt) in grids.iter() {
        let inv = gt.affine().inverse();
        let o = inv.transform_point3(origin);
        let d = inv.transform_vector3(dir);
        if let Some(h) = g.raycast(o, d, reach)
            && best.is_none_or(|b| h.distance < b.distance)
        {
            best = Some(Aim { grid: e, cell: h.cell, normal: h.normal, distance: h.distance });
        }
    }
    best
}

fn update_target(players: Query<(&GlobalTransform, &Body), With<Player>>, grids: Query<(Entity, &VoxelGrid, &GlobalTransform)>, mut target: ResMut<Target>, mut eye: ResMut<Eye>) {
    let Ok((pt, body)) = players.single() else { return };
    let pos = pt.translation() - Vec3::Y * body.height() * 0.5 + Vec3::Y * body.eye();
    *eye = Eye { pos, dir: body.look() };
    target.hit = aim(&grids, eye.pos, eye.dir, REACH);
}

fn draw_outline(target: Res<Target>, grids: Query<&GlobalTransform, With<VoxelGrid>>, mut gizmos: Gizmos, ui: Res<State<Ui>>) {
    if *ui.get() != Ui::Game {
        return;
    }
    let Some(a) = target.hit else { return };
    let Ok(gt) = grids.get(a.grid) else { return };
    let t = gt.compute_transform();
    let center = t.transform_point(a.cell.as_vec3() + Vec3::splat(0.5));
    gizmos.cube(Transform::from_translation(center).with_rotation(t.rotation).with_scale(Vec3::splat(1.004)), Color::srgba(0.05, 0.05, 0.08, 0.9));
}

/// Mouse buttons for the world; repeat while held (creative speed).
#[derive(Resource, Default)]
pub struct Clicks {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    repeat_l: f32,
    repeat_r: f32,
}

fn read_clicks(mouse: Res<ButtonInput<MouseButton>>, time: Res<Time>, ui: Res<State<Ui>>, mut c: ResMut<Clicks>) {
    let dt = time.delta_secs();
    if *ui.get() != Ui::Game {
        *c = Clicks::default();
        return;
    }
    c.repeat_l -= dt;
    c.repeat_r -= dt;
    c.left = mouse.just_pressed(MouseButton::Left) || (mouse.pressed(MouseButton::Left) && c.repeat_l <= 0.0);
    c.right = mouse.just_pressed(MouseButton::Right) || (mouse.pressed(MouseButton::Right) && c.repeat_r <= 0.0);
    c.middle = mouse.just_pressed(MouseButton::Middle);
    if c.left {
        c.repeat_l = if mouse.just_pressed(MouseButton::Left) { 0.3 } else { 0.2 };
    }
    if c.right {
        c.repeat_r = if mouse.just_pressed(MouseButton::Right) { 0.3 } else { 0.2 };
    }
}

/// The cased block made by putting casing `casing` on a block, if any.
pub fn encased(casing: &str, on: Option<Mech>) -> Option<&'static str> {
    match (casing, on?) {
        ("steel_casing" | "copper_casing", Mech::Shaft) => Some("cased_shaft"),
        ("steel_casing" | "copper_casing", Mech::Pipe) => Some("cased_pipe"),
        _ => None,
    }
}

/// Orientation of a new block from its placement rule.
pub fn place_facing(rule: Placement, normal: IVec3, look: Vec3) -> u8 {
    match rule {
        Placement::None => 2,
        Placement::Face => facing_of(if normal == IVec3::ZERO { IVec3::Y } else { normal }),
        Placement::Attach => facing_of(if normal == IVec3::ZERO { IVec3::NEG_Y } else { -normal }),
        Placement::Horizontal => {
            // Face the player.
            let d = -look;
            let d = if d.x.abs() > d.z.abs() { IVec3::new(d.x.signum() as i32, 0, 0) } else { IVec3::new(0, 0, d.z.signum() as i32) };
            facing_of(d)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn edit_blocks(
    clicks: Res<Clicks>,
    target: Res<Target>,
    eye: Res<Eye>,
    mut players: Query<(&GlobalTransform, &mut Body, &mut Inventory), With<Player>>,
    mut grids: Query<(&mut VoxelGrid, &GlobalTransform)>,
    mut edited: MessageWriter<Edited>,
    mut used: MessageWriter<UseBlock>,
    mut toasts: MessageWriter<Toast>,
    mut shape_mode: ResMut<ShapeMode>,
) {
    let Ok((pt, mut body, mut inv)) = players.single_mut() else { return };
    let held = inv.held();
    let Some(a) = target.hit else { return };
    let Ok((mut grid, gt)) = grids.get_mut(a.grid) else { return };
    // Seats and physics toggles are used with RMB (not with the wrench,
    // which turns them, or the glue; crouch to place a block on them).
    if clicks.right
        && let Some(m) = grid.get(a.cell).id.def().mech
        && matches!(m, Mech::Seat | Mech::PhysicsToggle | Mech::Lever | Mech::Button | Mech::SlidingDoor)
        && !matches!(held.and_then(|s| s.item.tool()), Some(crate::content::Tool::Wrench | crate::content::Tool::Glue | crate::content::Tool::Blueprint))
        && (!body.crouching || held.is_none())
    {
        body.swing = 1.0;
        used.write(UseBlock { grid: a.grid, cell: a.cell, mech: m });
        return;
    }
    // Tools handle their own clicks.
    if held.is_some_and(|s| s.item.tool().is_some()) {
        return;
    }
    if clicks.middle {
        let cell = grid.get(a.cell);
        // A shaped block: pick its cube and that shape.
        let (base, shape) = registry().unshaped(cell.id);
        if let Some(item) = registry().item_for_block(base) {
            inv.pick(item);
            if shape > 0 {
                shape_mode.0 = shape;
            }
        }
        return;
    }
    if clicks.left {
        let old = grid.set(a.cell, Cell::AIR);
        if !old.is_air() {
            body.swing = 1.0;
            edited.write(Edited { grid: a.grid, cell: a.cell, placed: false, block: old.id });
        }
        return;
    }
    if !clicks.right {
        return;
    }
    let Some(stack) = held else { return };
    let Some(item_block) = stack.item.block() else { return };
    let block = registry().shaped(item_block, shape_mode.0).unwrap_or(item_block);
    // A casing on a shaft or a pipe encases it (like Create).
    let here = grid.get(a.cell);
    if let Some(cased) = encased(item_block.def().key.as_str(), here.id.def().mech) {
        grid.set(a.cell, Cell::new(crate::content::BlockId::k(cased), here.facing));
        body.swing = 1.0;
        edited.write(Edited { grid: a.grid, cell: a.cell, placed: true, block });
        return;
    }
    let cell = a.cell + a.normal;
    if a.normal == IVec3::ZERO || !grid.get(cell).is_air() {
        return;
    }
    // Do not place inside the player.
    let inv_t = gt.affine().inverse();
    let feet = pt.translation() - Vec3::Y * body.height() * 0.5;
    let blocking = |p: Vec3| {
        let l = inv_t.transform_point3(p);
        l.cmpge(cell.as_vec3() - Vec3::splat(RADIUS - 0.02)).all() && l.cmple(cell.as_vec3() + Vec3::splat(1.0 + RADIUS - 0.02)).all()
    };
    let solid = block.def().collision != crate::content::Collision::None;
    if solid && (0..=3).any(|k| blocking(feet + Vec3::Y * (k as f32 / 3.0) * (HEIGHT - 0.1).min(body.height() - 0.05))) {
        toasts.write(toast("Здесь стоит игрок"));
        return;
    }
    let look_local = inv_t.transform_vector3(eye.dir);
    let facing = place_facing(block.def().placement, a.normal, look_local);
    grid.set(cell, Cell::new(block, facing));
    body.swing = 1.0;
    edited.write(Edited { grid: a.grid, cell, placed: true, block });
}

fn hotbar_keys(keys: Res<ButtonInput<KeyCode>>, scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>, ui: Res<State<Ui>>, mut q: Query<&mut Inventory, With<Player>>, wheel_busy: Res<WheelBusy>) {
    if *ui.get() != Ui::Game {
        return;
    }
    let Ok(mut inv) = q.single_mut() else { return };
    let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];
    for (i, k) in digits.iter().enumerate() {
        if keys.just_pressed(*k) {
            inv.selected = i;
        }
    }
    // Shift + wheel with a wire in hand picks its colour (`logic`).
    let wire_color = keys.pressed(KeyCode::ShiftLeft) && inv.held().is_some_and(|s| s.item.def().key == "wire" || s.item.block().is_some_and(shapeable));
    if !wheel_busy.0 && !wire_color && scroll.delta.y != 0.0 {
        let n = crate::inventory::HOTBAR as i32;
        inv.selected = (inv.selected as i32 - scroll.delta.y.signum() as i32).rem_euclid(n) as usize;
    }
}

/// Targeting and block editing (tools run after it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InteractSet;

/// The mouse wheel is used by a tool this frame (physgun distance).
#[derive(Resource, Default)]
pub struct WheelBusy(pub bool);

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Target>()
            .init_resource::<Eye>()
            .init_resource::<Clicks>()
            .init_resource::<WheelBusy>()
            .init_resource::<ShapeMode>()
            .add_message::<Edited>()
            .add_message::<UseBlock>()
            .add_systems(Update, (update_target, read_clicks, pick_shape, edit_blocks, hotbar_keys, draw_outline).chain().in_set(InteractSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::facing_dir;

    #[test]
    fn placement_rules() {
        assert_eq!(facing_dir(place_facing(Placement::Face, IVec3::X, Vec3::NEG_Z)), IVec3::X);
        assert_eq!(facing_dir(place_facing(Placement::Attach, IVec3::Y, Vec3::NEG_Z)), IVec3::NEG_Y);
        // Looking north (−Z): a seat faces the player (+Z).
        assert_eq!(facing_dir(place_facing(Placement::Horizontal, IVec3::Y, Vec3::new(0.1, -0.5, -1.0))), IVec3::Z);
        // Casings: on a shaft → cased shaft, on a pipe → cased pipe.
        assert_eq!(encased("steel_casing", Some(Mech::Shaft)), Some("cased_shaft"));
        assert_eq!(encased("copper_casing", Some(Mech::Pipe)), Some("cased_pipe"));
        assert_eq!(encased("stone", Some(Mech::Pipe)), None);
        assert_eq!(encased("copper_casing", Some(Mech::Cog)), None);
    }
}
