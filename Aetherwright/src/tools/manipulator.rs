//! The universal manipulator — the player's main tool.
//!
//! Modes (Q cycles while the manipulator is held):
//! * **Mine** — hold LMB: an energy beam mines the targeted block; time
//!   depends on hardness and drill tier; special materials need modules.
//! * **Scan** — LMB: detailed analysis of a block or object.
//! * **Cut** — hold LMB: cuts a block out of the terrain as a free physical
//!   body (needs the cutter module).
//! * **Mechanics** — LMB: interacts with machines (toggle / configure);
//!   handled by `machinery` through `MechanicsUse`.
//!
//! With a block item selected, LMB still mines and RMB places that block.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ActiveTool, EntityTarget};
use crate::{
    core::{events::Notify, resources::EyeRay, rules::ModeRules, states::gameplay_active},
    items::inventory::Inventory,
    player::Player,
    world::{
        chunks::EditCause,
        destruction::DetachedBlock,
        interaction::{BlockEdit, BlockTarget},
        voxel::blocks,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ManipulatorMode {
    #[default]
    Mine,
    Scan,
    Cut,
    Mechanics,
}

impl ManipulatorMode {
    pub const ALL: [ManipulatorMode; 4] =
        [ManipulatorMode::Mine, ManipulatorMode::Scan, ManipulatorMode::Cut, ManipulatorMode::Mechanics];
    pub fn label(self) -> &'static str {
        match self {
            ManipulatorMode::Mine => "Добыча",
            ManipulatorMode::Scan => "Сканирование",
            ManipulatorMode::Cut => "Резка",
            ManipulatorMode::Mechanics => "Механизмы",
        }
    }
    pub fn color(self) -> Color {
        match self {
            ManipulatorMode::Mine => Color::srgb(0.35, 0.9, 1.0),
            ManipulatorMode::Scan => Color::srgb(0.5, 1.0, 0.6),
            ManipulatorMode::Cut => Color::srgb(1.0, 0.55, 0.3),
            ManipulatorMode::Mechanics => Color::srgb(1.0, 0.85, 0.35),
        }
    }
}

/// Persistent manipulator state (on the player entity, saved).
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Manipulator {
    pub mode: ManipulatorMode,
    /// Installed drill tier (1 = base, 2 = module).
    pub drill_tier: u8,
    pub cutter: bool,
}

impl Default for Manipulator {
    fn default() -> Self {
        Self { mode: ManipulatorMode::Mine, drill_tier: 1, cutter: false }
    }
}

/// Transient mining progress.
#[derive(Resource, Default, Debug)]
pub struct MiningProgress {
    pub target: Option<IVec3>,
    pub progress: f32,
    pub required: f32,
    /// Cutting instead of mining.
    pub cutting: bool,
}

/// Mechanics-mode click on an entity (handled by machinery).
#[derive(Message, Debug, Clone, Copy)]
pub struct MechanicsUse {
    pub entity: Entity,
    /// Secondary (RMB) action.
    pub secondary: bool,
}

/// Scan request (handled by `scanning`).
#[derive(Message, Debug, Clone, Copy)]
pub enum ScanRequest {
    Block(IVec3),
    Entity(Entity),
}

fn cycle_mode(
    keys: Res<ButtonInput<KeyCode>>,
    tool: Res<ActiveTool>,
    mut q: Query<&mut Manipulator, With<Player>>,
    mut notify: MessageWriter<Notify>,
) {
    if *tool != ActiveTool::Manipulator || !keys.just_pressed(KeyCode::KeyQ) {
        return;
    }
    for mut m in &mut q {
        let i = ManipulatorMode::ALL.iter().position(|x| *x == m.mode).unwrap_or(0);
        m.mode = ManipulatorMode::ALL[(i + 1) % ManipulatorMode::ALL.len()];
        notify.write(Notify::info(format!("Манипулятор: {}", m.mode.label())));
    }
}

/// Mining time in seconds for the current tool, or an error message.
pub fn mining_time(block: crate::world::voxel::BlockId, tier: u8, hand: bool, cutting: bool) -> Result<f32, String> {
    let def = block.def();
    if def.hardness.is_infinite() {
        return Err(format!("{}: неразрушимо", def.name));
    }
    if def.tier > tier {
        return Err(format!("{}: нужен модуль бурения {}", def.name, roman(def.tier)));
    }
    let mut t = def.hardness;
    if tier >= 2 {
        t /= 1.6;
    }
    if hand {
        t *= 1.5;
    }
    if cutting {
        t *= 1.4;
    }
    Ok(t.max(0.05))
}

fn roman(n: u8) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        _ => "?",
    }
}

#[allow(clippy::too_many_arguments)]
fn use_manipulator(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    block_target: Res<BlockTarget>,
    entity_target: Res<EntityTarget>,
    player: Query<&Manipulator, With<Player>>,
    mut progress: ResMut<MiningProgress>,
    mut edits: MessageWriter<BlockEdit>,
    mut detached: MessageWriter<DetachedBlock>,
    mut mechanics: MessageWriter<MechanicsUse>,
    mut scans: MessageWriter<ScanRequest>,
    mut notify: MessageWriter<Notify>,
    mut warned: Local<Option<IVec3>>,
) {
    let Ok(m) = player.single() else { return };
    let mode = if *tool == ActiveTool::Manipulator { m.mode } else { ManipulatorMode::Mine };
    if !tool.mines() {
        progress.target = None;
        return;
    }

    match mode {
        ManipulatorMode::Scan => {
            if mouse.just_pressed(MouseButton::Left) {
                if let Some(e) = entity_target.entity {
                    scans.write(ScanRequest::Entity(e));
                } else if let Some(hit) = block_target.0 {
                    scans.write(ScanRequest::Block(hit.pos));
                }
            }
            return;
        }
        ManipulatorMode::Mechanics => {
            for (button, secondary) in [(MouseButton::Left, false), (MouseButton::Right, true)] {
                if mouse.just_pressed(button)
                    && let Some(e) = entity_target.entity
                {
                    mechanics.write(MechanicsUse { entity: e, secondary });
                }
            }
            return;
        }
        ManipulatorMode::Mine | ManipulatorMode::Cut => {}
    }

    let cutting = mode == ManipulatorMode::Cut;
    if cutting && !m.cutter {
        if mouse.just_pressed(MouseButton::Left) {
            notify.write(Notify::warn("Для резки нужен модуль резки"));
        }
        progress.target = None;
        return;
    }
    let hit = match (block_target.0, entity_target.entity) {
        (Some(h), None) => h,
        _ => {
            progress.target = None;
            return;
        }
    };
    if !mouse.pressed(MouseButton::Left) {
        progress.target = None;
        *warned = None;
        return;
    }
    let hand = *tool == ActiveTool::Hand;
    let required = if rules.instant_break && !cutting {
        Ok(0.0)
    } else {
        mining_time(hit.block, m.drill_tier, hand, cutting)
    };
    let required = match required {
        Ok(t) => t,
        Err(msg) => {
            if *warned != Some(hit.pos) {
                notify.write(Notify::warn(msg));
                *warned = Some(hit.pos);
            }
            progress.target = None;
            return;
        }
    };
    if progress.target != Some(hit.pos) || progress.cutting != cutting {
        *progress = MiningProgress { target: Some(hit.pos), progress: 0.0, required, cutting };
    }
    progress.progress += time.delta_secs();
    if progress.progress >= progress.required {
        if cutting {
            edits.write(BlockEdit { pos: hit.pos, block: blocks::AIR, cause: EditCause::Cut });
            detached.write(DetachedBlock { pos: hit.pos, block: hit.block });
        } else {
            edits.write(BlockEdit { pos: hit.pos, block: blocks::AIR, cause: EditCause::Mined });
        }
        progress.target = None;
    }
}

/// RMB with a block item: place it on the targeted face.
fn place_block(
    mouse: Res<ButtonInput<MouseButton>>,
    tool: Res<ActiveTool>,
    rules: Res<ModeRules>,
    target: Res<BlockTarget>,
    entity_target: Res<EntityTarget>,
    mut player: Query<&mut Inventory, With<Player>>,
    mut edits: MessageWriter<BlockEdit>,
    mut pending: Local<Vec<(IVec3, crate::items::ItemId, u8)>>,
    mut changed: MessageReader<crate::world::interaction::BlockChanged>,
) {
    // Consume the item only when the edit was actually applied.
    let Ok(mut inv) = player.single_mut() else { return };
    for c in changed.read() {
        if c.cause == EditCause::Placed
            && let Some(i) = pending.iter().position(|(p, _, _)| *p == c.pos)
        {
            let (_, item, _) = pending.swap_remove(i);
            if rules.consume_items {
                inv.remove(item, 1);
            }
        }
    }
    // Requests that were rejected never produce a change; forget them.
    pending.retain_mut(|(_, _, age)| {
        *age += 1;
        *age < 10
    });
    let ActiveTool::PlaceBlock(item, block) = *tool else { return };
    if !mouse.just_pressed(MouseButton::Right) || entity_target.entity.is_some() {
        return;
    }
    if let Some(hit) = target.0 {
        let pos = if hit.block.is_replaceable() { hit.pos } else { hit.place_pos() };
        edits.write(BlockEdit { pos, block, cause: EditCause::Placed });
        pending.push((pos, item, 0));
    }
}

/// Beam from the device to the target + shrinking cube while mining.
fn draw_beam(
    progress: Res<MiningProgress>,
    eye: Res<EyeRay>,
    player: Query<&Manipulator, With<Player>>,
    tool: Res<ActiveTool>,
    mut gizmos: Gizmos,
) {
    let Some(pos) = progress.target else { return };
    let mode = player.single().map(|m| m.mode).unwrap_or_default();
    let color = if progress.cutting { ManipulatorMode::Cut.color() } else if *tool == ActiveTool::Manipulator { mode.color() } else { ManipulatorMode::Mine.color() };
    let right = eye.direction.cross(Vec3::Y).normalize_or_zero();
    let start = eye.origin + eye.direction * 0.5 + right * 0.28 - Vec3::Y * 0.22;
    let center = pos.as_vec3() + Vec3::splat(0.5);
    gizmos.line(start, center, color);
    let k = 1.0 - (progress.progress / progress.required.max(1e-3)).clamp(0.0, 1.0);
    gizmos.cube(Transform::from_translation(center).with_scale(Vec3::splat(0.3 + 0.7 * k)), color);
}

pub struct ManipulatorPlugin;

impl Plugin for ManipulatorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MiningProgress>()
            .add_message::<MechanicsUse>()
            .add_message::<ScanRequest>()
            .add_systems(
                Update,
                (cycle_mode, use_manipulator, place_block, draw_beam)
                    .chain()
                    .after(super::update_entity_target)
                    .run_if(gameplay_active),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_gates_special_materials() {
        assert!(mining_time(blocks::ANCIENT_STONE, 1, false, false).is_err());
        assert!(mining_time(blocks::ANCIENT_STONE, 2, false, false).is_ok());
        assert!(mining_time(blocks::BEDROCK, 3, false, false).is_err());
        let base = mining_time(blocks::STONE, 1, false, false).unwrap();
        assert!(mining_time(blocks::STONE, 2, false, false).unwrap() < base);
        assert!(mining_time(blocks::STONE, 1, true, false).unwrap() > base);
    }
}
