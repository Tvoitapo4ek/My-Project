//! Diegetic telemetry: values are shown on blocks in the world, not on the
//! HUD.
//!
//! * **Speedometer** — speed of the physical contraption it is on (km/h);
//!   0 in the world grid.
//! * **Telemetry sensor** — attached to a block (faces into it), reads:
//!   speedometer → km/h, suspension → steering angle (°), any other
//!   mechanism → |RPM|. Wrench: LMB on the sensor, then LMB on a display.
//!   The link is a `Linked` (grid + cell), so it moves with contraptions.
//! * **Mechanical display** — shows the value of the sensors linked to it
//!   (the largest if several). Works only while it turns (rotation from its
//!   side); unpowered it goes dark and the needle drops to zero. Wrench
//!   settings: «Вид» (digits / needle) and «Максимум шкалы».

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use crate::{
    content::Mech,
    grid::VoxelGrid,
    mech::{BlockEntity, Params, Spin},
    signal::Linked,
    style::{BoxCache, Materials},
    vehicle::Steer,
};

/// The value a display received this frame (before power is checked).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Shown(pub f32);

/// What a telemetry sensor reads from the block it faces.
pub fn read_value(target: Option<Mech>, rpm: f32, steer_deg: f32, kmh: f32) -> f32 {
    match target {
        Some(Mech::Speedometer) => kmh,
        Some(Mech::Suspension) => steer_deg.abs(),
        Some(_) => rpm.abs(),
        None => 0.0,
    }
}

/// A display is on while it turns.
pub fn powered(rpm: f32) -> bool {
    rpm.abs() > 1e-3
}

/// Needle angle (degrees, −120 … +120) for a value.
pub fn needle_angle(value: f32, max: f32, on: bool) -> f32 {
    if !on {
        return -120.0;
    }
    -120.0 + 240.0 * (value / max.max(1.0)).clamp(0.0, 1.0)
}

/// Digits shown (hundreds, tens, ones); None = blank (leading zeros).
pub fn digits(value: f32) -> [Option<u8>; 3] {
    let v = value.round().clamp(0.0, 999.0) as u32;
    let d = [(v / 100) as u8, ((v / 10) % 10) as u8, (v % 10) as u8];
    [(v >= 100).then_some(d[0]), (v >= 10).then_some(d[1]), Some(d[2])]
}

/// Lit segments of a digit: a b c d e f g (top, upper-right, lower-right,
/// bottom, lower-left, upper-left, middle).
pub fn segments(d: u8) -> [bool; 7] {
    const S: [u8; 10] = [0b0111111, 0b0000110, 0b1011011, 0b1001111, 0b1100110, 0b1101101, 0b1111101, 0b0000111, 0b1111111, 0b1101111];
    let m = S[(d as usize).min(9)];
    std::array::from_fn(|i| m & (1 << i) != 0)
}

// ------------------------------------------------------------ reading

fn read_sensors(
    sensors: Query<(&BlockEntity, &Linked)>,
    grids: Query<(&VoxelGrid, Option<&LinearVelocity>)>,
    blocks: Query<(&BlockEntity, &Spin, Option<&Steer>)>,
    mut displays: Query<&mut Shown>,
) {
    for mut s in &mut displays {
        s.0 = 0.0;
    }
    for (b, link) in &sensors {
        if b.mech() != Some(Mech::TelemetrySensor) {
            continue;
        }
        let Some(target_ref) = link.0 else { continue };
        let Ok((grid, vel)) = grids.get(b.grid) else { continue };
        let value = match grid.entities.get(&(b.cell + b.dir())).and_then(|e| blocks.get(*e).ok()) {
            Some((t, spin, steer)) => {
                let kmh = vel.map(|v| v.0.length() * 3.6).unwrap_or(0.0);
                read_value(t.mech(), spin.rpm, steer.map(|s| s.0.to_degrees()).unwrap_or(0.0), kmh)
            }
            None => 0.0,
        };
        let Some(&d) = grids.get(target_ref.grid).ok().and_then(|(g, _)| g.entities.get(&target_ref.cell)) else { continue };
        if let Ok(mut s) = displays.get_mut(d) {
            s.0 = s.0.max(value);
        }
    }
}

// ------------------------------------------------------------ face

#[derive(Component)]
struct DigitsFace;
#[derive(Component)]
struct DialFace;
#[derive(Component, Clone, Copy)]
struct Segment {
    digit: usize,
    seg: usize,
}
#[derive(Component, Clone, Copy)]
struct BarCell(usize);
#[derive(Component, Clone, Copy)]
struct Tick {
    red: bool,
}
#[derive(Component)]
struct Needle(f32);
#[derive(Component)]
struct PowerLed;
/// Current lit state, so materials change only when needed.
#[derive(Component, Default)]
struct Lit(bool);

const BAR: usize = 10;
/// Face plane (local z) of the display.
const FZ: f32 = 0.43;

fn attach_face(mut commands: Commands, added: Query<(Entity, &BlockEntity), Added<BlockEntity>>, mut meshes: ResMut<Assets<Mesh>>, mut cache: ResMut<BoxCache>, mats: Option<Res<Materials>>) {
    let Some(mats) = mats else { return };
    for (e, b) in &added {
        if b.mech() != Some(Mech::MechanicalDisplay) {
            continue;
        }
        commands.entity(e).insert(Shown::default());
        let off = mats.get("seg_off");
        let mut bx = |x: f32, y: f32| cache.get(&mut meshes, Vec3::new(x, y, 0.03));
        let (hseg, vseg) = (bx(0.13, 0.035), bx(0.035, 0.13));
        // Digits: 3 × 7 segments + a 10-cell bar.
        let digits_node = commands.spawn((DigitsFace, Transform::default(), Visibility::default(), ChildOf(e))).id();
        for digit in 0..3 {
            let cx = -0.22 + digit as f32 * 0.22;
            let cy = 0.06;
            let at = [(0.0, 0.16), (0.08, 0.08), (0.08, -0.08), (0.0, -0.16), (-0.08, -0.08), (-0.08, 0.08), (0.0, 0.0)];
            for (seg, (x, y)) in at.into_iter().enumerate() {
                let mesh = if matches!(seg, 0 | 3 | 6) { hseg.clone() } else { vseg.clone() };
                commands.spawn((Segment { digit, seg }, Lit(false), Mesh3d(mesh), MeshMaterial3d(off.clone()), Transform::from_xyz(cx + x, cy + y, FZ), ChildOf(digits_node)));
            }
        }
        let cell = bx(0.05, 0.05);
        for i in 0..BAR {
            commands.spawn((BarCell(i), Lit(false), Mesh3d(cell.clone()), MeshMaterial3d(off.clone()), Transform::from_xyz(-0.27 + i as f32 * 0.06, -0.24, FZ), ChildOf(digits_node)));
        }
        // Dial: ticks along the arc, brass hub, needle.
        let dial = commands.spawn((DialFace, Transform::default(), Visibility::Hidden, ChildOf(e))).id();
        let tick = bx(0.03, 0.07);
        for i in 0..9 {
            let a = (-120.0 + i as f32 * 30.0f32).to_radians();
            let (x, y) = (a.sin() * 0.27, -0.04 + a.cos() * 0.27);
            commands.spawn((Tick { red: i >= 7 }, Lit(false), Mesh3d(tick.clone()), MeshMaterial3d(off.clone()), Transform::from_xyz(x, y, FZ).with_rotation(Quat::from_rotation_z(-a)), ChildOf(dial)));
        }
        let needle = commands.spawn((Needle(-120.0), Transform::from_xyz(0.0, -0.04, FZ + 0.01), Visibility::default(), ChildOf(dial))).id();
        commands.spawn((Mesh3d(cache.get(&mut meshes, Vec3::new(0.035, 0.24, 0.02))), MeshMaterial3d(mats.get("red_grip")), Transform::from_xyz(0.0, 0.1, 0.0), ChildOf(needle)));
        commands.spawn((Mesh3d(cache.get(&mut meshes, Vec3::new(0.09, 0.09, 0.05))), MeshMaterial3d(mats.get("brass")), Transform::from_xyz(0.0, -0.04, FZ + 0.02), ChildOf(dial)));
        // Power lamp (both modes), top-right corner.
        commands.spawn((PowerLed, Lit(false), Mesh3d(cache.get(&mut meshes, Vec3::new(0.05, 0.05, 0.03))), MeshMaterial3d(off.clone()), Transform::from_xyz(0.31, 0.31, FZ), ChildOf(e)));
    }
}

fn set_lit(lit: &mut Lit, mat: &mut MeshMaterial3d<StandardMaterial>, want: bool, on: &Handle<StandardMaterial>, off: &Handle<StandardMaterial>) {
    if lit.0 != want {
        lit.0 = want;
        mat.0 = if want { on.clone() } else { off.clone() };
    }
}

#[allow(clippy::type_complexity)]
fn draw_faces(
    time: Res<Time>,
    mats: Option<Res<Materials>>,
    displays: Query<(Entity, &BlockEntity, &Params, &Spin, &Shown)>,
    mut faces: ParamSet<(Query<(&ChildOf, &mut Visibility), With<DigitsFace>>, Query<(&ChildOf, &mut Visibility), With<DialFace>>)>,
    mut lamps: Query<(&ChildOf, &mut Lit, &mut MeshMaterial3d<StandardMaterial>, Option<&Segment>, Option<&BarCell>, Option<&Tick>, Has<PowerLed>)>,
    mut needles: Query<(&ChildOf, &mut Needle, &mut Transform)>,
    parents: Query<&ChildOf>,
) {
    let Some(mats) = mats else { return };
    let (cyan, amber, red, green, off) = (mats.get("glow_cyan"), mats.get("glow_amber"), mats.get("glow_red"), mats.get("glow_green"), mats.get("seg_off"));
    let dt = time.delta_secs();
    for (e, b, p, _, _) in &displays {
        let needle_mode = p.get(b.id, "style") >= 0.5;
        for (c, mut v) in faces.p0().iter_mut() {
            if c.parent() == e {
                v.set_if_neq(if needle_mode { Visibility::Hidden } else { Visibility::Inherited });
            }
        }
        for (c, mut v) in faces.p1().iter_mut() {
            if c.parent() == e {
                v.set_if_neq(if needle_mode { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
    }
    // Lamps: find their display through the parent chain.
    for (c, mut lit, mut mat, seg, bar, tick, led) in &mut lamps {
        let display = if led { Some(c.parent()) } else { parents.get(c.parent()).ok().map(|pc| pc.parent()) };
        let Some(Ok((_, b, p, spin, shown))) = display.map(|d| displays.get(d)) else { continue };
        let on = powered(spin.rpm);
        let max = p.get(b.id, "max_val");
        if led {
            set_lit(&mut lit, &mut mat, on, &green, &off);
        } else if let Some(s) = seg {
            let want = on && digits(shown.0)[s.digit].is_some_and(|d| segments(d)[s.seg]);
            set_lit(&mut lit, &mut mat, want, &cyan, &off);
        } else if let Some(bc) = bar {
            let n = ((shown.0 / max.max(1.0)).clamp(0.0, 1.0) * BAR as f32).round() as usize;
            set_lit(&mut lit, &mut mat, on && bc.0 < n, &amber, &off);
        } else if let Some(t) = tick {
            let col = if t.red { &red } else { &cyan };
            set_lit(&mut lit, &mut mat, on, col, &off);
        }
    }
    for (c, mut n, mut t) in &mut needles {
        // needle → dial → display
        let Some(Ok((_, b, p, spin, shown))) = parents.get(c.parent()).ok().map(|d| displays.get(d.parent())) else { continue };
        let target = needle_angle(shown.0, p.get(b.id, "max_val"), powered(spin.rpm));
        n.0 += (target - n.0) * (dt * 8.0).min(1.0);
        t.rotation = Quat::from_rotation_z(-n.0.to_radians());
    }
}

pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach_face, read_sensors.after(crate::mech::MechSet), draw_faces).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_reads_by_target() {
        assert_eq!(read_value(Some(Mech::Speedometer), 0.0, 0.0, 54.0), 54.0);
        assert_eq!(read_value(Some(Mech::Motor), -64.0, 0.0, 0.0), 64.0);
        assert_eq!(read_value(Some(Mech::Suspension), 32.0, -30.0, 0.0), 30.0);
        assert_eq!(read_value(None, 10.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn display_needs_rotation() {
        assert!(powered(-1.0) && !powered(0.0));
        assert_eq!(needle_angle(64.0, 128.0, true), 0.0);
        assert_eq!(needle_angle(500.0, 128.0, true), 120.0);
        assert_eq!(needle_angle(64.0, 128.0, false), -120.0);
    }

    /// Speedometer and motor → telemetry sensors → displays, through the
    /// real reading system; the readout drops to zero when things stop.
    #[test]
    fn sensors_feed_linked_displays() {
        use crate::{content::BlockId, grid::facing_of, signal::PanelRef};
        use bevy::ecs::system::RunSystemOnce;
        let mut w = World::new();
        let g = w.spawn((VoxelGrid::default(), LinearVelocity(Vec3::new(15.0, 0.0, 0.0)))).id();
        let put = |w: &mut World, cell: IVec3, key: &str, facing: u8, rpm: f32| {
            let e = w.spawn((BlockEntity { grid: g, cell, id: BlockId::k(key), facing }, Spin { rpm, angle: 0.0 })).id();
            w.get_mut::<VoxelGrid>(g).unwrap().entities.insert(cell, e);
            e
        };
        let into_z = facing_of(IVec3::NEG_Z);
        put(&mut w, IVec3::new(0, 0, 0), "speedometer", 2, 0.0);
        let s1 = put(&mut w, IVec3::new(0, 0, 1), "telemetry_sensor", into_z, 0.0);
        let d1 = put(&mut w, IVec3::new(2, 0, 0), "mechanical_display", 2, 32.0);
        let motor = put(&mut w, IVec3::new(5, 0, 0), "creative_motor", 2, -64.0);
        let s2 = put(&mut w, IVec3::new(5, 0, 1), "telemetry_sensor", into_z, 0.0);
        let d2 = put(&mut w, IVec3::new(7, 0, 0), "mechanical_display", 2, 32.0);
        w.entity_mut(s1).insert(Linked(Some(PanelRef { grid: g, cell: IVec3::new(2, 0, 0) })));
        w.entity_mut(s2).insert(Linked(Some(PanelRef { grid: g, cell: IVec3::new(7, 0, 0) })));
        w.entity_mut(d1).insert(Shown::default());
        w.entity_mut(d2).insert(Shown::default());
        w.run_system_once(read_sensors).unwrap();
        assert!((w.get::<Shown>(d1).unwrap().0 - 54.0).abs() < 1e-3, "15 m/s = 54 km/h");
        assert_eq!(w.get::<Shown>(d2).unwrap().0, 64.0);
        // The motor and the car stop → both read 0.
        w.get_mut::<Spin>(motor).unwrap().rpm = 0.0;
        w.get_mut::<LinearVelocity>(g).unwrap().0 = Vec3::ZERO;
        w.run_system_once(read_sensors).unwrap();
        assert_eq!(w.get::<Shown>(d1).unwrap().0, 0.0);
        assert_eq!(w.get::<Shown>(d2).unwrap().0, 0.0);
        // A display whose side cog stops goes dark, whatever it receives.
        w.get_mut::<Spin>(motor).unwrap().rpm = 64.0;
        w.get_mut::<Spin>(d2).unwrap().rpm = 0.0;
        w.run_system_once(read_sensors).unwrap();
        let (shown, rpm) = (w.get::<Shown>(d2).unwrap().0, w.get::<Spin>(d2).unwrap().rpm);
        assert_eq!(shown, 64.0);
        assert_eq!(needle_angle(shown, 128.0, powered(rpm)), -120.0);
    }

    #[test]
    fn seven_segments() {
        assert_eq!(digits(7.4), [None, None, Some(7)]);
        assert_eq!(digits(128.0), [Some(1), Some(2), Some(8)]);
        assert_eq!(digits(5000.0), [Some(9), Some(9), Some(9)]);
        assert_eq!(segments(8), [true; 7]);
        assert_eq!(segments(1), [false, true, true, false, false, false, false]);
        assert_eq!(segments(0)[6], false);
    }
}
