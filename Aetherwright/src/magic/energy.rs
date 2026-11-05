//! Aether as data: kinds of magic and the background field.
//!
//! Three kinds exist in the data model. Only their basic differences are
//! implemented in this prototype: colour, field contribution and the
//! agitated kind's instability (anomalies). Ancient magic is stronger and
//! resonates with machines (dynamo bonus) — deeper properties come later.

use bevy::prelude::*;

pub use crate::content::AetherKind;

impl AetherKind {
    pub const ALL: [AetherKind; 5] = [
        AetherKind::Calm,
        AetherKind::Agitated,
        AetherKind::Condensed,
        AetherKind::Ancient,
        AetherKind::Unique,
    ];
    pub fn color(self) -> Color {
        let [r, g, b] = self.rgb();
        Color::srgb(r, g, b)
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
    let mut by_kind = [0.0f32; 5];
    for (sp, kind, s) in sources {
        let d2 = sp.distance_squared(p);
        if d2 > SOURCE_RADIUS * SOURCE_RADIUS {
            continue;
        }
        let c = s / (1.0 + d2 / 9.0);
        total += c;
        by_kind[*kind as usize] += c;
    }
    let kind = AetherKind::ALL
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
