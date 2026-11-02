//! Area selection (manipulator mode "Выделение") and group operations.
//!
//! LMB on blocks sets two corners. Then: C — copy, X — cut, Delete — remove,
//! T — rotate the selection in place by 90°, V — paste mode (the ghost
//! follows the crosshair, R rotates it, RMB builds it, V again cancels).
//! Parts whose centre is inside the box are part of the selection, joints
//! between them are preserved.

use bevy::prelude::*;

use super::{
    blueprints::Blueprint,
    construction::{Area, MAX_AREA_VOLUME, PasteError, capture, clear_area, paste},
    preview::{GhostShape, Ghosts, Validity},
};
use crate::{
    core::{events::Notify, states::gameplay_active},
    player::Player,
    tools::{
        ActiveTool,
        manipulator::{Manipulator, ManipulatorMode},
    },
    world::interaction::BlockTarget,
};

#[derive(Resource, Debug, Default)]
pub struct Selection {
    pub a: Option<IVec3>,
    pub b: Option<IVec3>,
    pub clipboard: Option<Blueprint>,
    pub paste_mode: bool,
}

impl Selection {
    pub fn area(&self) -> Option<Area> {
        Some(Area::new(self.a?, self.b?))
    }
}

/// Group operations queued by input, executed with exclusive world access.
#[derive(Resource, Debug, Default)]
pub struct PendingOps(pub Vec<GroupOp>);

#[derive(Debug, Clone)]
pub enum GroupOp {
    Copy,
    Cut,
    Delete,
    RotateInPlace,
    Paste(IVec3),
    /// Creative: fill the selection with a block.
    Fill(crate::world::voxel::BlockId),
}

pub fn select_mode_active(
    tool: Res<ActiveTool>,
    player: Query<&Manipulator, With<Player>>,
) -> bool {
    *tool == ActiveTool::Manipulator
        && player
            .single()
            .is_ok_and(|m| m.mode == ManipulatorMode::Select)
}

fn select_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    target: Res<BlockTarget>,
    mut sel: ResMut<Selection>,
    mut ops: ResMut<PendingOps>,
    mut notify: MessageWriter<Notify>,
) {
    if keys.just_pressed(KeyCode::KeyV) {
        if sel.clipboard.as_ref().is_some_and(|c| !c.is_empty()) {
            sel.paste_mode = !sel.paste_mode;
        } else {
            notify.write(Notify::warn("Буфер пуст: сначала скопируйте (C)"));
        }
    }
    if sel.paste_mode {
        if keys.just_pressed(KeyCode::KeyR)
            && let Some(c) = sel.clipboard.take()
        {
            sel.clipboard = Some(c.rotated_y90());
        }
        if mouse.just_pressed(MouseButton::Right)
            && let Some(hit) = target.0
        {
            ops.0.push(GroupOp::Paste(hit.place_pos()));
        }
        return;
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(hit) = target.0
    {
        if sel.a.is_none() || sel.b.is_some() {
            sel.a = Some(hit.pos);
            sel.b = None;
        } else {
            sel.b = Some(hit.pos);
            if let Some(area) = sel.area() {
                let s = area.size();
                notify.write(Notify::info(format!("Выделено {}×{}×{}", s.x, s.y, s.z)));
            }
        }
    }
    if sel.area().is_some() {
        if keys.just_pressed(KeyCode::KeyC) {
            ops.0.push(GroupOp::Copy);
        }
        if keys.just_pressed(KeyCode::KeyX) {
            ops.0.push(GroupOp::Cut);
        }
        if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
            ops.0.push(GroupOp::Delete);
        }
        if keys.just_pressed(KeyCode::KeyT) {
            ops.0.push(GroupOp::RotateInPlace);
        }
    }
}

fn notify(world: &mut World, n: Notify) {
    world.write_message(n);
}

/// Executes queued group operations.
pub fn apply_ops(world: &mut World) {
    let ops = std::mem::take(&mut world.resource_mut::<PendingOps>().0);
    for op in ops {
        let area = world.resource::<Selection>().area();
        match op {
            GroupOp::Paste(origin) => {
                let Some(bp) = world.resource::<Selection>().clipboard.clone() else {
                    continue;
                };
                match paste(world, &bp, origin) {
                    Ok(n) => notify(world, Notify::ok(format!("Построено элементов: {n}"))),
                    Err(PasteError::OutOfWorld) => {
                        notify(world, Notify::warn("Не помещается в мир"))
                    }
                    Err(PasteError::MissingItems(m)) => {
                        let list: Vec<String> = m
                            .iter()
                            .map(|(i, n)| format!("{} ×{n}", i.def().name))
                            .collect();
                        notify(
                            world,
                            Notify::warn(format!("Не хватает: {}", list.join(", "))),
                        );
                    }
                }
            }
            GroupOp::Fill(block) => {
                let Some(area) = area else { continue };
                if area.volume() > MAX_AREA_VOLUME {
                    notify(world, Notify::warn("Слишком большая область"));
                    continue;
                }
                let mut n = 0;
                for c in area.cells() {
                    if world
                        .resource::<crate::world::chunks::VoxelWorld>()
                        .get(c)
                        .is_replaceable()
                    {
                        world.write_message(crate::world::interaction::BlockEdit {
                            pos: c,
                            block,
                            cause: crate::world::chunks::EditCause::Creative,
                        });
                        n += 1;
                    }
                }
                notify(world, Notify::ok(format!("Заполнено блоков: {n}")));
            }
            op => {
                let Some(area) = area else { continue };
                if area.volume() > MAX_AREA_VOLUME {
                    notify(
                        world,
                        Notify::warn("Слишком большая область (максимум 32³)"),
                    );
                    continue;
                }
                match op {
                    GroupOp::Copy => {
                        let bp = capture(world, area);
                        let msg = format!(
                            "Скопировано: блоков {}, деталей {}",
                            bp.blocks.len(),
                            bp.parts.len()
                        );
                        world.resource_mut::<Selection>().clipboard = Some(bp);
                        notify(world, Notify::ok(msg));
                    }
                    GroupOp::Cut => {
                        let bp = capture(world, area);
                        clear_area(world, area);
                        world.resource_mut::<Selection>().clipboard = Some(bp);
                        notify(world, Notify::ok("Вырезано в буфер — V, чтобы вставить"));
                    }
                    GroupOp::Delete => {
                        let n = clear_area(world, area);
                        notify(world, Notify::ok(format!("Удалено элементов: {n}")));
                    }
                    GroupOp::RotateInPlace => {
                        let bp = capture(world, area).rotated_y90();
                        // Remove without refund, rebuild without cost.
                        let rules = *world.resource::<crate::core::rules::ModeRules>();
                        *world.resource_mut::<crate::core::rules::ModeRules>() =
                            crate::core::rules::ModeRules {
                                consume_items: false,
                                ..rules
                            };
                        clear_area(world, area);
                        let _ = paste(world, &bp, area.min);
                        *world.resource_mut::<crate::core::rules::ModeRules>() = rules;
                        let mut sel = world.resource_mut::<Selection>();
                        sel.a = Some(area.min);
                        sel.b = Some(area.min + bp.size - IVec3::ONE);
                        notify(world, Notify::ok("Повёрнуто на 90°"));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn draw_selection(
    sel: Res<Selection>,
    target: Res<BlockTarget>,
    mut ghosts: ResMut<Ghosts>,
    mut gizmos: Gizmos,
    active: Query<&Manipulator, With<Player>>,
    tool: Res<ActiveTool>,
) {
    let showing = *tool == ActiveTool::Manipulator
        && active
            .single()
            .is_ok_and(|m| m.mode == ManipulatorMode::Select);
    if !showing {
        return;
    }
    let corner_only = sel.a.zip(sel.b).is_none();
    if let Some(area) = sel.area().or_else(|| sel.a.map(|a| Area::new(a, a))) {
        let size = area.size().as_vec3();
        let center = area.min.as_vec3() + size * 0.5;
        let color = if corner_only {
            Color::srgb(1.0, 0.85, 0.3)
        } else {
            Color::srgb(0.4, 0.95, 1.0)
        };
        gizmos.cube(
            Transform::from_translation(center).with_scale(size + Vec3::splat(0.02)),
            color,
        );
    }
    if sel.paste_mode
        && let (Some(bp), Some(hit)) = (&sel.clipboard, target.0)
    {
        let origin = hit.place_pos().as_vec3();
        let size = bp.size.as_vec3();
        ghosts.0.push((
            GhostShape::Box(size),
            Transform::from_translation(origin + size * 0.5),
            Validity::Ok,
        ));
    }
}

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<PendingOps>()
            .add_systems(
                Update,
                (select_input.run_if(select_mode_active), draw_selection)
                    .after(crate::tools::update_entity_target)
                    .run_if(gameplay_active),
            )
            .add_systems(
                PostUpdate,
                apply_ops.before(crate::world::interaction::apply_block_edits),
            );
    }
}
