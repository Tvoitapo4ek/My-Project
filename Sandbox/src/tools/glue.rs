//! Glue (like Create's super glue): LMB on a block sets the first corner,
//! LMB on another sets the second — every block in that box is glued
//! together. RMB on a glued block removes the box it belongs to. Glued
//! areas are drawn while the glue is in hand.

use bevy::prelude::*;

use crate::{
    content::Tool,
    core::{Toast, toast},
    grid::VoxelGrid,
    interact::{Clicks, Target},
    inventory::Inventory,
    player::{Body, Player},
};

/// Glued boxes of a grid (inclusive cell ranges).
#[derive(Component, Debug, Clone, Default)]
pub struct Glue(pub Vec<(IVec3, IVec3)>);

impl Glue {
    pub fn contains(&self, a: IVec3, b: IVec3) -> bool {
        self.0.iter().any(|(lo, hi)| inside(a, *lo, *hi) && inside(b, *lo, *hi))
    }
}

pub fn inside(c: IVec3, lo: IVec3, hi: IVec3) -> bool {
    c.cmpge(lo).all() && c.cmple(hi).all()
}

/// First corner waiting for the second.
#[derive(Resource, Default)]
pub struct GlueCorner(pub Option<(Entity, IVec3)>);

fn use_glue(clicks: Res<Clicks>, target: Res<Target>, mut players: Query<(&Inventory, &mut Body), With<Player>>, mut corner: ResMut<GlueCorner>, mut glue: Query<&mut Glue>, grids: Query<(), With<VoxelGrid>>, mut commands: Commands, mut toasts: MessageWriter<Toast>) {
    let Ok((inv, mut body)) = players.single_mut() else { return };
    if super::held_tool(inv) != Some(Tool::Glue) {
        corner.0 = None;
        return;
    }
    let Some(a) = target.hit else { return };
    if !grids.contains(a.grid) {
        return;
    }
    if clicks.left {
        body.swing = 1.0;
        match corner.0 {
            Some((g, first)) if g == a.grid => {
                let (lo, hi) = (first.min(a.cell), first.max(a.cell));
                match glue.get_mut(a.grid) {
                    Ok(mut gl) => gl.0.push((lo, hi)),
                    Err(_) => {
                        commands.entity(a.grid).insert(Glue(vec![(lo, hi)]));
                    }
                }
                let n = (hi - lo + IVec3::ONE).element_product();
                corner.0 = None;
                toasts.write(toast(format!("Склеено: область {n} клеток")));
            }
            _ => {
                corner.0 = Some((a.grid, a.cell));
                toasts.write(toast("Первый угол выбран — ЛКМ по второму углу"));
            }
        }
    } else if clicks.right {
        body.swing = 1.0;
        if let Ok(mut gl) = glue.get_mut(a.grid) {
            let before = gl.0.len();
            gl.0.retain(|(lo, hi)| !inside(a.cell, *lo, *hi));
            if gl.0.len() < before {
                toasts.write(toast("Клей снят"));
            }
        }
        corner.0 = None;
    }
}

fn draw(players: Query<&Inventory, With<Player>>, glue: Query<(&Glue, &GlobalTransform)>, corner: Res<GlueCorner>, grids: Query<&GlobalTransform>, target: Res<Target>, mut gizmos: Gizmos) {
    let Ok(inv) = players.single() else { return };
    if super::held_tool(inv) != Some(Tool::Glue) {
        return;
    }
    let col = Color::srgb(0.45, 1.0, 0.45);
    for (g, gt) in &glue {
        let t = gt.compute_transform();
        for (lo, hi) in &g.0 {
            let size = (*hi - *lo + IVec3::ONE).as_vec3();
            let c = t.transform_point(lo.as_vec3() + size * 0.5);
            gizmos.cube(Transform::from_translation(c).with_rotation(t.rotation).with_scale(size + Vec3::splat(0.03)), col);
        }
    }
    // Preview of the box being made.
    if let (Some((ge, first)), Some(a)) = (corner.0, target.hit)
        && a.grid == ge
        && let Ok(gt) = grids.get(ge)
    {
        let t = gt.compute_transform();
        let (lo, hi) = (first.min(a.cell), first.max(a.cell));
        let size = (hi - lo + IVec3::ONE).as_vec3();
        gizmos.cube(Transform::from_translation(t.transform_point(lo.as_vec3() + size * 0.5)).with_rotation(t.rotation).with_scale(size + Vec3::splat(0.05)), Color::srgb(1.0, 0.9, 0.3));
    }
}

pub struct GluePlugin;

impl Plugin for GluePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GlueCorner>().add_systems(Update, (use_glue.after(crate::interact::InteractSet), draw));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glue_box_contains_pairs() {
        let g = Glue(vec![(IVec3::ZERO, IVec3::new(2, 0, 1))]);
        assert!(g.contains(IVec3::ZERO, IVec3::new(2, 0, 1)));
        assert!(!g.contains(IVec3::ZERO, IVec3::new(3, 0, 0)));
    }
}
