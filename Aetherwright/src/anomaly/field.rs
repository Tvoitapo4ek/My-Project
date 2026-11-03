//! Anomaly rules — pure functions, unit-tested.
//!
//! * **Effect**: inside the sphere gravity is multiplied by `g`; the effect
//!   is full in the inner 60 % of the radius and fades to normal at the
//!   edge. Only the pull changes — inertia stays (bodies keep their mass).
//! * **Load**: how hard an anomaly is to hold — bigger and further from
//!   normal gravity is harder.
//! * **Containment** (0..1): stabilizers' holding strength / load. It does
//!   not change the anomaly's set values; it removes the wandering around
//!   them and keeps the radius from swelling (predictability).
//! * **Shaping**: the generator's result is a roll — success depends on
//!   how unstable the surroundings are, and the shaped radius and strength
//!   spread around the settings.

/// Deterministic random number in 0..1 from a seed (splitmix).
pub fn rand01(seed: u64) -> f32 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth wandering −1..1 (sum of incommensurate sines, seeded).
pub fn wander(t: f32, seed: u32) -> f32 {
    // Keep the phase small: f32 loses `t` next to a huge offset.
    let s = (seed % 9973) as f32 * 0.137;
    ((t * 0.37 + s).sin() * 0.6 + (t * 0.91 + s * 2.3).sin() * 0.3 + (t * 1.73 + s * 0.7).sin() * 0.1).clamp(-1.0, 1.0)
}

/// Share of the effect at distance `d` from the centre (1 inside, 0 out).
pub fn falloff(d: f32, radius: f32) -> f32 {
    if radius <= 0.0 || d >= radius {
        return 0.0;
    }
    let inner = radius * 0.6;
    if d <= inner {
        return 1.0;
    }
    let x = 1.0 - (d - inner) / (radius - inner);
    x * x * (3.0 - 2.0 * x)
}

/// Gravity multiplier at distance `d`.
pub fn gravity_at(d: f32, radius: f32, g: f32) -> f32 {
    1.0 + (g - 1.0) * falloff(d, radius)
}

/// How hard the anomaly is to hold (1 = a 6 m anomaly that halves gravity).
pub fn load(radius: f32, g: f32) -> f32 {
    (radius / 6.0) * ((g - 1.0).abs() + 0.5) / 1.0
}

/// Containment 0..1 from the stabilizers' holding strengths.
pub fn containment(holds: f32, load: f32) -> f32 {
    if load <= 1e-4 {
        return 1.0;
    }
    (holds / load).clamp(0.0, 1.0)
}

/// What the anomaly does right now: (radius, g) after wandering.
/// `jitter` is the share of the deviation that wanders when unheld.
pub fn current(radius: f32, g: f32, jitter: f32, containment: f32, t: f32, seed: u32) -> (f32, f32) {
    let free = 1.0 - containment.clamp(0.0, 1.0);
    // Wanders by a share of its deviation, at least ±0.15 g.
    let amp = jitter * ((g - 1.0).abs().max(0.3)) * free;
    let g_now = g + amp * wander(t, seed);
    // Unheld anomalies swell and shrink; held ones keep their size.
    let r_now = radius * (1.0 + 0.2 * jitter * free * (0.5 + 0.5 * wander(t * 0.6, seed ^ 0x5bd1)));
    (r_now, g_now)
}

/// Whether the environment allows shaping, and the success chance.
/// `instability` is the unstable field strength at the generator.
pub fn shaping_chance(instability: f32, min: f32, success: f32) -> Option<f32> {
    if instability < min || min <= 0.0 && instability <= 0.0 {
        return None;
    }
    let over = if min > 0.0 { (instability / min - 1.0).clamp(0.0, 1.0) } else { 1.0 };
    Some(success + (1.0 - success) * over)
}

/// The shaped result: (radius, g) spread around the settings.
pub fn shaped(radius: f32, g: f32, spread: f32, seed: u64) -> (f32, f32) {
    let a = rand01(seed) * 2.0 - 1.0;
    let b = rand01(seed ^ 0xA5A5) * 2.0 - 1.0;
    let r = radius * (1.0 + spread * a);
    // Spread acts on the deviation from normal gravity.
    let dev = (g - 1.0) * (1.0 + spread * b);
    (r.max(1.0), 1.0 + dev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_is_full_inside_and_zero_outside() {
        assert_eq!(falloff(0.0, 10.0), 1.0);
        assert_eq!(falloff(5.9, 10.0), 1.0);
        assert!(falloff(8.0, 10.0) > 0.0 && falloff(8.0, 10.0) < 1.0);
        assert_eq!(falloff(10.0, 10.0), 0.0);
        assert_eq!(gravity_at(0.0, 6.0, 0.0), 0.0);
        assert_eq!(gravity_at(7.0, 6.0, 0.0), 1.0);
        assert!((gravity_at(1.0, 6.0, -0.6) + 0.6).abs() < 1e-6);
    }

    #[test]
    fn held_anomaly_does_not_wander() {
        for t in [0.0, 1.3, 7.7, 40.0] {
            let (r, g) = current(6.0, 0.0, 0.5, 1.0, t, 7);
            assert!((r - 6.0).abs() < 1e-5 && g.abs() < 1e-5);
        }
        // Unheld: it wanders around its value, and the radius only swells.
        let mut spread = 0.0f32;
        for i in 0..200 {
            let (r, g) = current(6.0, 0.0, 0.5, 0.0, i as f32 * 0.25, 7);
            assert!(r >= 6.0 - 1e-4 && r <= 6.0 * 1.1 + 1e-4);
            assert!(g.abs() <= 0.5 + 1e-4);
            spread = spread.max(g.abs());
        }
        assert!(spread > 0.2, "should wander: {spread}");
    }

    #[test]
    fn containment_grows_with_stabilizers_and_load_with_size() {
        let l = load(6.0, 0.5);
        assert!((l - 1.0).abs() < 1e-5);
        assert!(load(12.0, 0.5) > l && load(6.0, -0.6) > l);
        assert_eq!(containment(0.0, l), 0.0);
        assert_eq!(containment(0.5, l), 0.5);
        assert_eq!(containment(3.0, l), 1.0);
    }

    #[test]
    fn shaping_needs_unstable_environment() {
        assert_eq!(shaping_chance(1.0, 1.5, 0.7), None);
        assert_eq!(shaping_chance(1.5, 1.5, 0.7), Some(0.7));
        assert_eq!(shaping_chance(3.0, 1.5, 0.7), Some(1.0));
        let (r, g) = shaped(6.0, 0.0, 0.25, 42);
        assert!((4.5..=7.5).contains(&r) && (-0.25..=0.25).contains(&g));
        // Deterministic.
        assert_eq!(shaped(6.0, 0.0, 0.25, 42), (r, g));
        let mean: f32 = (0..1000).map(|i| rand01(i)).sum::<f32>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05);
    }
}
