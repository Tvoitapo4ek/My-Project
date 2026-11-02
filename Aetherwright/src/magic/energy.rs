//! Aether as data: kinds of magic and the background field.
//!
//! Three kinds exist in the data model. Only their basic differences are
//! implemented in this prototype: colour, field contribution and the
//! agitated kind's instability (anomalies). Ancient magic is stronger and
//! resonates with machines (dynamo bonus) — deeper properties come later.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AetherKind {
    /// Ordinary, calm magic.
    #[default]
    Calm,
    /// Irritated / agitated: unstable, pushes things around.
    Agitated,
    /// Ancient: dense, slow, resonates with old structures.
    Ancient,
}

impl AetherKind {
    pub fn label(self) -> &'static str {
        match self {
            AetherKind::Calm => "спокойная",
            AetherKind::Agitated => "раздражённая",
            AetherKind::Ancient => "древняя",
        }
    }
    pub fn color(self) -> Color {
        match self {
            AetherKind::Calm => Color::srgb(0.35, 0.9, 1.0),
            AetherKind::Agitated => Color::srgb(1.0, 0.35, 0.75),
            AetherKind::Ancient => Color::srgb(0.4, 1.0, 0.8),
        }
    }
    /// Multiplier when converting to mechanical energy.
    pub fn potency(self) -> f32 {
        match self {
            AetherKind::Calm => 1.0,
            AetherKind::Agitated => 1.6,
            AetherKind::Ancient => 2.2,
        }
    }
}

/// Base field strength everywhere (aether units per second for collectors).
pub const BACKGROUND: f32 = 0.15;
/// Influence radius of a source.
pub const SOURCE_RADIUS: f32 = 18.0;

#[derive(Debug, Clone, Copy)]
pub struct FieldSample {
    pub strength: f32,
    pub kind: AetherKind,
}

/// Field at a point from a list of sources (pos, kind, strength).
pub fn sample_field(sources: &[(Vec3, AetherKind, f32)], p: Vec3) -> FieldSample {
    let mut total = BACKGROUND;
    let mut by_kind = [0.0f32; 3];
    for (sp, kind, s) in sources {
        let d2 = sp.distance_squared(p);
        if d2 > SOURCE_RADIUS * SOURCE_RADIUS {
            continue;
        }
        let c = s / (1.0 + d2 / 9.0);
        total += c;
        by_kind[*kind as usize] += c;
    }
    let kind = [AetherKind::Calm, AetherKind::Agitated, AetherKind::Ancient]
        .into_iter()
        .max_by(|a, b| by_kind[*a as usize].total_cmp(&by_kind[*b as usize]))
        .filter(|k| by_kind[*k as usize] > 0.0)
        .unwrap_or_default();
    FieldSample {
        strength: total,
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_falls_off_and_picks_kind() {
        let src = [
            (Vec3::ZERO, AetherKind::Agitated, 3.0),
            (Vec3::new(10.0, 0.0, 0.0), AetherKind::Calm, 1.0),
        ];
        let near = sample_field(&src, Vec3::new(0.5, 0.0, 0.0));
        let far = sample_field(&src, Vec3::new(5.0, 0.0, 0.0));
        let nowhere = sample_field(&src, Vec3::new(100.0, 0.0, 0.0));
        assert!(near.strength > far.strength && far.strength > nowhere.strength);
        assert_eq!(near.kind, AetherKind::Agitated);
        assert_eq!(nowhere.strength, BACKGROUND);
        assert_eq!(nowhere.kind, AetherKind::Calm);
    }
}
