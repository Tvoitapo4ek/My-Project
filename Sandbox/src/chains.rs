//! Chains between cogwheels.
//!
//! * The chain tool: LMB on a cogwheel, then on another one with the same
//!   axis in the same plane (up to `MAX_LEN` blocks away, at any angle) —
//!   a chain joins them; LMB on a third one continues the chain from the
//!   second, and so on. RMB on a cogwheel removes its chains, RMB in the
//!   air cancels.
//! * Where you click a cogwheel (which side of the line to the next one)
//!   is the side the chain runs around it: cogwheels on the same side of
//!   the chain turn the same way, on opposite sides the other way. The
//!   speed scales with the size: small → large halves it.
//! * Chains belong to the grid (like glue), so they move with contraptions.

use bevy::prelude::*;

use crate::{
    content::{Mech, Tool},
    core::toast,
    grid::VoxelGrid,
    interact::{Clicks, Eye, Target},
    inventory::Inventory,
    mech::BlockEntity,
    style::{BoxCache, Materials},
};

pub const MAX_LEN: i32 = 16;

/// A chain run between two cogwheels and the side it passes each one on
/// (+1 / −1, relative to the line from `a` to `b` turned about the axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainLink {
    pub a: IVec3,
    pub b: IVec3,
    pub sa: i8,
    pub sb: i8,
}

/// Chains of a grid.
#[derive(Component, Debug, Clone, Default)]
pub struct Chains(pub Vec<ChainLink>);

/// Radius of a cog role for chains (blocks).
pub fn radius(m: Mech) -> Option<f32> {
    match m {
        Mech::Cog => Some(0.5),
        Mech::LargeCog => Some(1.0),
        _ => None,
    }
}

/// Speed ratio along a chain: `speed_b = r · speed_a`.
pub fn ratio(ra: f32, rb: f32, sa: i8, sb: i8) -> f32 {
    ra / rb * (sa * sb) as f32
}

/// Side of the chain line a click falls on: the line `a → b` in the plane
/// of `axis`, `off` = the clicked point from the cog's centre.
pub fn side(axis: IVec3, a: IVec3, b: IVec3, off: Vec3) -> i8 {
    let n = axis.as_vec3().cross((b - a).as_vec3());
    if n.dot(off) >= 0.0 { 1 } else { -1 }
}

/// Two cogwheels can be chained: same axis, same plane, not neighbours.
pub fn can_chain(axis_a: IVec3, axis_b: IVec3, a: IVec3, b: IVec3) -> bool {
    let d = b - a;
    axis_a.abs() == axis_b.abs() && d.dot(axis_a.abs()) == 0 && d != IVec3::ZERO && d.abs().max_element() <= MAX_LEN && d.abs().element_sum() > 1
}

/// First cogwheel picked: (grid, cell, clicked offset from its centre).
#[derive(Resource, Default)]
pub struct ChainStart(pub Option<(Entity, IVec3, Vec3)>);

#[allow(clippy::too_many_arguments)]
fn use_chain(
    mut commands: Commands,
    clicks: Res<Clicks>,
    target: Res<Target>,
    eye: Res<Eye>,
    players: Query<&Inventory, With<crate::player::Player>>,
    grids: Query<(&VoxelGrid, &GlobalTransform, Option<&Chains>)>,
    blocks: Query<&BlockEntity>,
    mut start: ResMut<ChainStart>,
    mut toasts: MessageWriter<crate::core::Toast>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
) {
    let Ok(inv) = players.single() else { return };
    if crate::tools::held_tool(inv) != Some(Tool::Chain) || !(clicks.left || clicks.right) {
        return;
    }
    let cog = target.hit.and_then(|a| {
        let (g, gt, _) = grids.get(a.grid).ok()?;
        let b = blocks.get(*g.entities.get(&a.cell)?).ok()?;
        radius(b.mech()?)?;
        let hit = gt.affine().inverse().transform_point3(eye.pos + eye.dir * a.distance);
        Some((a.grid, a.cell, b.dir().abs(), hit - (a.cell.as_vec3() + Vec3::splat(0.5))))
    });
    if clicks.right {
        start.0 = None;
        if let Some((g, c, ..)) = cog
            && let Ok((_, _, Some(ch))) = grids.get(g)
        {
            let left: Vec<ChainLink> = ch.0.iter().copied().filter(|l| l.a != c && l.b != c).collect();
            if left.len() != ch.0.len() {
                commands.entity(g).insert(Chains(left));
                toasts.write(toast("Цепь снята"));
            }
        }
        return;
    }
    let Some((g, c, axis, off)) = cog else {
        toasts.write(toast("Цепь вешается на шестерни"));
        return;
    };
    match start.0 {
        Some((sg, sc, soff)) if sg == g && sc != c => {
            let sa_axis = grids.get(g).ok().and_then(|(gr, ..)| gr.entities.get(&sc)).and_then(|e| blocks.get(*e).ok()).map(|b| b.dir().abs()).unwrap_or(IVec3::ZERO);
            if !can_chain(sa_axis, axis, sc, c) {
                toasts.write(toast(format!("Шестерни должны быть в одной плоскости, не дальше {MAX_LEN} блоков")));
                return;
            }
            let link = ChainLink { a: sc, b: c, sa: side(axis, sc, c, soff), sb: side(axis, sc, c, off) };
            let mut all = grids.get(g).ok().and_then(|(_, _, ch)| ch.cloned()).unwrap_or_default();
            all.0.retain(|l| !((l.a == link.a && l.b == link.b) || (l.a == link.b && l.b == link.a)));
            all.0.push(link);
            commands.entity(g).insert(all);
            // Continue from this cogwheel.
            start.0 = Some((g, c, off));
            toasts.write(toast("Цепь натянута — ЛКМ по следующей шестерне продолжит её, ПКМ — готово"));
            sounds.write(crate::sound::sfx("link"));
        }
        _ => {
            start.0 = Some((g, c, off));
            toasts.write(toast("Шестерня выбрана: ЛКМ по второй шестерне"));
            sounds.write(crate::sound::sfx("ui_click"));
        }
    }
}

/// Drawn chain links (children of the grid).
#[derive(Component)]
struct ChainView;

#[allow(clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    grids: Query<(Entity, &Chains, &VoxelGrid, Option<&Children>), Changed<Chains>>,
    views: Query<(), With<ChainView>>,
    blocks: Query<&BlockEntity>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    for (ge, chains, grid, children) in &grids {
        for c in children.into_iter().flatten() {
            if views.contains(*c) {
                commands.entity(*c).despawn();
            }
        }
        let rad = |c: IVec3| grid.entities.get(&c).and_then(|e| blocks.get(*e).ok()).and_then(|b| Some((radius(b.mech()?)?, b.dir().abs())));
        for l in &chains.0 {
            let (Some((ra, axis)), Some((rb, _))) = (rad(l.a), rad(l.b)) else { continue };
            let (pa, pb) = (l.a.as_vec3() + Vec3::splat(0.5), l.b.as_vec3() + Vec3::splat(0.5));
            let n = axis.as_vec3().cross(pb - pa).normalize_or_zero();
            // The cog teeth sit at ~0.42 / 0.9: the chain runs just outside.
            let (ea, eb) = (pa + n * l.sa as f32 * (ra * 0.84 + 0.06), pb + n * l.sb as f32 * (rb * 0.9 + 0.04));
            let d = eb - ea;
            let len = d.length();
            let rot = Quat::from_rotation_arc(Vec3::Z, d / len.max(1e-4));
            let steps = (len / 0.18).ceil() as i32;
            for i in 0..steps {
                let at = ea + d * ((i as f32 + 0.5) / steps as f32);
                let mat = if i % 2 == 0 { "steel" } else { "brass" };
                commands.spawn((ChainView, Mesh3d(cache.get(&mut meshes, Vec3::new(0.09, 0.07, 0.16))), MeshMaterial3d(mats.get(mat)), Transform::from_translation(at).with_rotation(rot), ChildOf(ge)));
            }
        }
    }
}

/// Extra kinetic links of a grid's chains: (a, b, ratio).
pub fn links(chains: &Chains, mech_at: impl Fn(IVec3) -> Option<Mech>) -> Vec<(IVec3, IVec3, f32)> {
    chains
        .0
        .iter()
        .filter_map(|l| {
            let (ra, rb) = (radius(mech_at(l.a)?)?, radius(mech_at(l.b)?)?);
            Some((l.a, l.b, ratio(ra, rb, l.sa, l.sb)))
        })
        .collect()
}

pub struct ChainsPlugin;

impl Plugin for ChainsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChainStart>().add_systems(Update, (use_chain.after(crate::interact::InteractSet), draw));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_sides_and_sizes() {
        // Same side: same way; opposite sides: reversed; small → large: ½.
        assert_eq!(ratio(0.5, 0.5, 1, 1), 1.0);
        assert_eq!(ratio(0.5, 0.5, 1, -1), -1.0);
        assert_eq!(ratio(0.5, 1.0, -1, -1), 0.5);
        // Axis Z, line along +X: a click above the cog (+Y) is side +1.
        assert_eq!(side(IVec3::Z, IVec3::ZERO, IVec3::new(5, 0, 0), Vec3::new(0.0, 0.3, 0.0)), 1);
        assert_eq!(side(IVec3::Z, IVec3::ZERO, IVec3::new(5, 0, 0), Vec3::new(0.0, -0.3, 0.0)), -1);
        // Same plane only, at an angle is fine, not neighbours.
        assert!(can_chain(IVec3::Z, IVec3::Z, IVec3::ZERO, IVec3::new(4, 3, 0)));
        assert!(!can_chain(IVec3::Z, IVec3::Z, IVec3::ZERO, IVec3::new(4, 0, 1)), "another plane");
        assert!(!can_chain(IVec3::Z, IVec3::X, IVec3::ZERO, IVec3::new(4, 0, 0)), "another axis");
        assert!(!can_chain(IVec3::Z, IVec3::Z, IVec3::ZERO, IVec3::X), "meshing neighbours");
    }
}
