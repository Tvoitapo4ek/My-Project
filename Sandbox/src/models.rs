//! Models of mechanisms and tools — data (lists of pixel boxes), in the
//! style of the references: riveted dark steel, brass, dark wood, rubber,
//! glowing cores. Block models live in a cell centred on the origin
//! (−0.5..0.5); local +Y is the block's facing (axis), except `Horizontal`
//! blocks whose front is local +Z and stay upright.

use std::f32::consts::{FRAC_PI_4, TAU};

use bevy::prelude::*;

use crate::{
    content::{Mech, Placement, Tool},
    grid::{facing_dir, facing_rot},
    style::{Layer, Part, part},
};

/// Rotation of a block model for its facing.
pub fn block_rot(placement: Placement, facing: u8) -> Quat {
    match placement {
        Placement::Horizontal => {
            let d = facing_dir(facing).as_vec3();
            Quat::from_rotation_arc(Vec3::Z, Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::Z))
        }
        _ => facing_rot(facing),
    }
}

fn shaft(len: f32) -> Vec<Part> {
    vec![
        part([0.22, len, 0.22], "shaft", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
    ]
}

/// Gear: octagonal disk, teeth, hub, shaft (all spinning).
fn cog(radius: f32, teeth: usize, disk: f32) -> Vec<Part> {
    let mut v = vec![
        part([disk, 0.18, disk], "brass", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
        part([disk, 0.18, disk], "brass", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)).layer(Layer::Rotor),
        part([disk * 0.55, 0.22, disk * 0.55], "steel", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
        part([0.32, 0.26, 0.32], "brass", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
    ];
    v.extend(shaft(1.0));
    for i in 0..teeth {
        let a = i as f32 / teeth as f32 * TAU;
        v.push(part([0.18, 0.2, 0.16], "brass", [a.cos() * radius, 0.0, a.sin() * radius]).rot(Quat::from_rotation_y(-a)).layer(Layer::Rotor));
    }
    v
}

fn casing(mat: &'static str) -> Vec<Part> {
    vec![part([0.92, 0.92, 0.92], mat, [0.0, 0.0, 0.0])]
}

/// Shaft ends sticking out of a casing on both sides.
fn through_shaft() -> Vec<Part> {
    vec![
        part([0.22, 1.0, 0.22], "shaft", [0.0, 0.0, 0.0]).layer(Layer::Rotor),
        part([0.32, 0.06, 0.32], "brass", [0.0, 0.47, 0.0]).layer(Layer::Rotor),
        part([0.32, 0.06, 0.32], "brass", [0.0, -0.47, 0.0]).layer(Layer::Rotor),
    ]
}

/// Gearbox: a casing with a turning shaft stub on each of its four ports
/// (around its axis, local Y turned by `turn`) and brass gears showing in
/// windows on the axis faces.
fn gearbox_parts(turn: Quat) -> Vec<Part> {
    let mut v = casing("steel_casing");
    for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
        let d = turn * d;
        let r = Quat::from_rotation_arc(Vec3::Y, d);
        v.push(part([0.4, 0.06, 0.4], "brass", (d * 0.47).to_array()).rot(r));
        // The stub turns about the port's own axis (`Layer::Port`).
        let port = crate::grid::DIRS.iter().position(|x| x.as_vec3() == d.round()).unwrap_or(0) as u8;
        v.push(part([0.2, 0.12, 0.2], "shaft", [0.0, 0.0, 0.0]).layer(Layer::Port(port)));
        v.push(part([0.3, 0.04, 0.3], "brass", [0.0, -0.02, 0.0]).layer(Layer::Port(port)));
    }
    for y in [0.47f32, -0.47] {
        let at = turn * Vec3::new(0.0, y, 0.0);
        v.push(part([0.6, 0.03, 0.6], "screen", at.to_array()).rot(turn));
        v.push(part([0.42, 0.05, 0.42], "brass", at.to_array()).rot(turn));
        v.push(part([0.42, 0.05, 0.42], "brass", at.to_array()).rot(turn * Quat::from_rotation_y(FRAC_PI_4)));
        v.push(part([0.14, 0.07, 0.14], "steel", at.to_array()).rot(turn));
    }
    v
}

/// Tyre, rim and hub around local Y (the axle).
pub fn wheel_parts(layer: Layer) -> Vec<Part> {
    // Chunky tyre with offset tread lugs, a wide brass rim, a dark steel
    // disc and a bolted hub (the style of the references).
    let mut v = Vec::new();
    let n = 16;
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        let r = Quat::from_rotation_y(-a);
        v.push(part([0.22, 0.46, 0.2], "rubber", [a.cos() * 0.49, 0.0, a.sin() * 0.49]).rot(r).layer(layer));
        // Tread lugs, alternating sides.
        let side = if i % 2 == 0 { 0.11 } else { -0.11 };
        v.push(part([0.08, 0.2, 0.16], "rubber", [a.cos() * 0.6, side, a.sin() * 0.6]).rot(r).layer(layer));
    }
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU;
        v.push(part([0.12, 0.4, 0.2], "brass", [a.cos() * 0.34, 0.0, a.sin() * 0.34]).rot(Quat::from_rotation_y(-a)).layer(layer));
    }
    v.push(part([0.5, 0.34, 0.5], "steel", [0.0, 0.0, 0.0]).layer(layer));
    v.push(part([0.5, 0.34, 0.5], "steel", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)).layer(layer));
    v.push(part([0.24, 0.44, 0.24], "brass", [0.0, 0.0, 0.0]).layer(layer));
    v.push(part([0.12, 0.5, 0.12], "steel", [0.0, 0.0, 0.0]).layer(layer));
    for (x, z) in [(0.12f32, 0.0f32), (-0.12, 0.0), (0.0, 0.12), (0.0, -0.12)] {
        v.push(part([0.05, 0.4, 0.05], "brass", [x, 0.0, z]).layer(layer));
    }
    v
}

/// A probe clamped onto the face of the block it reads (+Y side): a
/// bolted plate with jaws over the block's edges, a gauge housing with its
/// dial facing out (−Y), a lens of `lens` and a cable of `cable` into the
/// block. Sensors, telemetry and threshold sensors share it.
fn probe(lens: &'static str, cable: &'static str) -> Vec<Part> {
    let mut v = vec![
        part([0.64, 0.07, 0.64], "steel", [0.0, 0.465, 0.0]),
        // Jaws over two edges of the block.
        part([0.06, 0.2, 0.5], "steel", [0.34, 0.46, 0.0]),
        part([0.06, 0.2, 0.5], "steel", [-0.34, 0.46, 0.0]),
        part([0.36, 0.2, 0.36], "steel_casing", [0.0, 0.33, 0.0]),
        part([0.3, 0.04, 0.3], "brass", [0.0, 0.22, 0.0]),
        part([0.24, 0.03, 0.24], "screen", [0.0, 0.2, 0.0]),
        part([0.1, 0.02, 0.1], lens, [0.0, 0.185, 0.0]),
        // Cable from the housing into the plate.
        part([0.05, 0.05, 0.22], cable, [0.12, 0.4, 0.24]),
        part([0.05, 0.12, 0.05], cable, [0.12, 0.33, 0.33]),
    ];
    for (x, z) in [(0.26f32, 0.26f32), (-0.26, 0.26), (0.26, -0.26), (-0.26, -0.26)] {
        v.push(part([0.06, 0.04, 0.06], "brass", [x, 0.42, z]));
    }
    v
}

/// Wheel radius of a wheel block (m): small, standard, large.
pub fn wheel_radius(id: crate::content::BlockId) -> f32 {
    match id.def().key.as_str() {
        "wheel_small" => 0.45,
        "wheel_large" => 0.9,
        _ => 0.6,
    }
}

/// The model of a block (wheels come in sizes).
pub fn model_of(id: crate::content::BlockId) -> Vec<Part> {
    let Some(m) = id.def().mech else { return Vec::new() };
    if m == Mech::Wheel && id.def().key != "wheel" {
        let k = wheel_radius(id) / 0.6;
        // Wider with the size, pulled in towards the suspension.
        return wheel_parts(Layer::Rotor)
            .into_iter()
            .map(|mut p| {
                p.size = Vec3::new(p.size.x * k, p.size.y * k.sqrt(), p.size.z * k);
                p.at = Vec3::new(p.at.x * k, p.at.y * k.sqrt() - 0.24 * k.sqrt(), p.at.z * k);
                p
            })
            .collect();
    }
    block_model(m)
}

/// The model of a mechanism: `assets/models/<key>.bbmodel` if present,
/// otherwise the built-in one.
pub fn block_model(mech: Mech) -> Vec<Part> {
    crate::bbmodel::block_override(mech).unwrap_or_else(|| builtin_model(mech))
}

pub fn builtin_model(mech: Mech) -> Vec<Part> {
    match mech {
        Mech::Shaft => {
            // A clean faceted andesite bar, brass collars only at the ends.
            let mut v = shaft(1.0);
            v.push(part([0.3, 0.06, 0.3], "brass", [0.0, 0.47, 0.0]).layer(Layer::Rotor));
            v.push(part([0.3, 0.06, 0.3], "brass", [0.0, -0.47, 0.0]).layer(Layer::Rotor));
            v
        }
        // Teeth reach just past the cell so neighbours interleave.
        Mech::Cog => cog(0.44, 8, 0.72),
        Mech::LargeCog => cog(0.92, 16, 1.5),
        Mech::Gearbox => gearbox_parts(Quat::IDENTITY),
        // Axis across the player's view: ports up, down and to the sides.
        Mech::VerticalGearbox => gearbox_parts(Quat::from_rotation_arc(Vec3::Y, Vec3::Z)),
        Mech::Clutch => {
            let mut v = vec![part([0.86, 0.6, 0.86], "steel_casing", [0.0, 0.0, 0.0]), part([0.9, 0.12, 0.9], "red_grip", [0.0, 0.0, 0.0])];
            v.extend(through_shaft());
            v
        }
        Mech::Gearshift => {
            let mut v = vec![part([0.86, 0.6, 0.86], "brass_casing", [0.0, 0.0, 0.0]), part([0.9, 0.12, 0.9], "steel", [0.0, 0.0, 0.0])];
            v.extend(through_shaft());
            v
        }
        Mech::SpeedController => {
            let mut v = vec![
                part([0.92, 0.7, 0.92], "steel_casing", [0.0, -0.1, 0.0]),
                part([0.5, 0.08, 0.2], "brass", [0.0, -0.1, 0.47]),
                part([0.08, 0.06, 0.08], "glow_green", [0.12, -0.08, 0.52]),
            ];
            v.push(part([0.22, 0.3, 0.22], "shaft", [0.0, 0.36, 0.0]).layer(Layer::Rotor));
            v.push(part([0.36, 0.08, 0.36], "brass", [0.0, 0.28, 0.0]).layer(Layer::Rotor));
            v
        }
        Mech::Motor => {
            // Engine block: steel base with brass bolts, cased body with
            // copper cooling fins, exhaust stubs, a glowing status window and
            // the output shaft with a flywheel (+Y).
            let mut v = vec![
                part([0.92, 0.16, 0.92], "steel", [0.0, -0.42, 0.0]),
                part([0.74, 0.5, 0.74], "steel_casing", [0.0, -0.1, 0.0]),
                part([0.6, 0.1, 0.6], "brass", [0.0, 0.18, 0.0]),
                part([0.3, 0.18, 0.06], "glow_amber", [0.0, -0.12, 0.38]),
                part([0.36, 0.24, 0.04], "brass", [0.0, -0.12, 0.37]),
            ];
            for y in [-0.22f32, 0.02] {
                v.push(part([0.8, 0.035, 0.8], "copper", [0.0, y, 0.0]));
            }
            for (x, z) in [(0.4f32, 0.4f32), (-0.4, 0.4), (0.4, -0.4), (-0.4, -0.4)] {
                v.push(part([0.08, 0.06, 0.08], "brass", [x, -0.33, z]));
            }
            for x in [0.26f32, -0.26] {
                v.push(part([0.1, 0.36, 0.1], "copper", [x, 0.02, -0.33]));
                v.push(part([0.14, 0.06, 0.14], "steel", [x, 0.22, -0.33]));
            }
            v.push(part([0.22, 0.34, 0.22], "shaft", [0.0, 0.33, 0.0]).layer(Layer::Rotor));
            v.push(part([0.46, 0.08, 0.46], "brass", [0.0, 0.27, 0.0]).layer(Layer::Rotor));
            v.push(part([0.46, 0.08, 0.46], "brass", [0.0, 0.27, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)).layer(Layer::Rotor));
            v.push(part([0.3, 0.06, 0.3], "brass", [0.0, 0.47, 0.0]).layer(Layer::Rotor));
            v
        }
        Mech::Suspension => {
            // Like the reference: a bolted chassis bracket (−Y), a copper
            // coil-over (spring around a damper) standing beside the hub, a
            // trailing arm from the bracket to the knuckle, the knuckle with
            // the axle stub towards the wheel (+Y). The spring stretches and
            // the arm drops with the wheel's travel (`vehicle`).
            let mut v = vec![
                part([0.86, 0.12, 0.86], "steel", [0.0, -0.44, 0.0]),
                part([0.2, 0.06, 0.2], "brass", [0.3, -0.37, 0.3]),
                part([0.2, 0.06, 0.2], "brass", [-0.3, -0.37, 0.3]),
                part([0.2, 0.06, 0.2], "brass", [0.3, -0.37, -0.3]),
                part([0.2, 0.06, 0.2], "brass", [-0.3, -0.37, -0.3]),
                // Upper spring seat on a post.
                part([0.14, 0.5, 0.14], "steel", [0.0, -0.18, -0.32]),
                part([0.3, 0.06, 0.3], "steel", [0.0, 0.08, -0.26]),
            ];
            // Coil-over: damper body + coils (stretch together).
            v.push(part([0.09, 0.5, 0.09], "steel_casing", [0.0, -0.16, -0.06]).layer(Layer::Spring));
            for i in 0..5 {
                v.push(part([0.26, 0.05, 0.26], "copper", [0.0, -0.36 + i as f32 * 0.09, -0.06]).rot(Quat::from_rotation_y(i as f32 * 0.5)).layer(Layer::Spring));
            }
            // Trailing arm and the knuckle with the axle stub.
            v.push(part([0.12, 0.12, 0.62], "steel", [0.22, -0.3, 0.02]).layer(Layer::Arm));
            v.push(part([0.12, 0.12, 0.62], "steel", [-0.22, -0.3, 0.02]).layer(Layer::Arm));
            v.push(part([0.42, 0.24, 0.42], "steel_casing", [0.0, 0.18, 0.0]).layer(Layer::Steer));
            v.push(part([0.48, 0.06, 0.48], "brass", [0.0, 0.31, 0.0]).layer(Layer::Steer));
            v.push(part([0.2, 0.36, 0.2], "shaft", [0.0, 0.36, 0.0]).layer(Layer::Rotor));
            v
        }
        // Pulled in towards the suspension (local −Y), no gap at the axle.
        Mech::Wheel => wheel_parts(Layer::Rotor).into_iter().map(|mut p| {
            p.at.y -= 0.24;
            p
        }).collect(),
        Mech::Seat => vec![
            part([0.86, 0.12, 0.86], "steel", [0.0, -0.4, 0.0]),
            part([0.1, 0.3, 0.1], "steel", [0.34, -0.25, 0.34]),
            part([0.1, 0.3, 0.1], "steel", [-0.34, -0.25, 0.34]),
            part([0.1, 0.3, 0.1], "steel", [0.34, -0.25, -0.34]),
            part([0.1, 0.3, 0.1], "steel", [-0.34, -0.25, -0.34]),
            part([0.84, 0.14, 0.84], "planks", [0.0, -0.06, 0.02]),
            part([0.84, 0.7, 0.14], "planks", [0.0, 0.3, -0.38]),
            part([0.9, 0.08, 0.18], "steel", [0.0, 0.66, -0.38]),
        ],
        Mech::ControlPanel => vec![
            part([0.8, 0.5, 0.6], "steel_casing", [0.0, -0.25, -0.1]),
            part([0.86, 0.1, 0.7], "brass", [0.0, 0.05, -0.05]).rot(Quat::from_rotation_x(0.45)),
            part([0.12, 0.06, 0.12], "glow_green", [-0.25, 0.12, 0.05]).rot(Quat::from_rotation_x(0.45)),
            part([0.12, 0.06, 0.12], "glow_red", [0.0, 0.12, 0.05]).rot(Quat::from_rotation_x(0.45)),
            part([0.12, 0.06, 0.12], "glow_amber", [0.25, 0.12, 0.05]).rot(Quat::from_rotation_x(0.45)),
            part([0.06, 0.34, 0.06], "steel", [-0.2, 0.25, -0.2]),
            part([0.12, 0.12, 0.12], "red_grip", [-0.2, 0.42, -0.2]),
            part([0.06, 0.34, 0.06], "steel", [0.2, 0.25, -0.2]),
            part([0.12, 0.12, 0.12], "red_grip", [0.2, 0.42, -0.2]),
        ],
        Mech::Sensor => probe("glow_green", "brass"),
        Mech::Speedometer => {
            // Dark casing, brass rim, an impeller in a side window and a
            // pitot tube on top.
            let mut v = vec![
                part([0.86, 0.7, 0.86], "steel_casing", [0.0, -0.15, 0.0]),
                part([0.9, 0.08, 0.9], "brass", [0.0, 0.2, 0.0]),
                part([0.5, 0.42, 0.06], "screen", [0.0, -0.12, 0.43]),
                part([0.08, 0.3, 0.08], "steel", [0.25, 0.38, 0.0]),
                part([0.08, 0.08, 0.3], "brass", [0.25, 0.5, 0.12]),
                part([0.12, 0.12, 0.08], "copper", [0.25, 0.5, 0.29]),
            ];
            for i in 0..6 {
                let a = i as f32 / 6.0 * TAU;
                v.push(part([0.05, 0.17, 0.03], "steel", [a.sin() * 0.09, -0.12 + a.cos() * 0.09, 0.47]).rot(Quat::from_rotation_z(-a)));
            }
            v.push(part([0.07, 0.07, 0.04], "red_grip", [0.0, -0.12, 0.48]));
            v
        }
        Mech::TelemetrySensor => probe("glow_cyan", "copper"),
        Mech::MechanicalDisplay => {
            // Front = local +Z. The face (digits / dial) is added by
            // `telemetry`; here: casing, brass frame and side gear rings.
            let mut v = vec![
                part([0.84, 0.84, 0.8], "steel", [0.0, 0.0, -0.04]),
                part([0.94, 0.08, 0.1], "brass", [0.0, 0.43, 0.42]),
                part([0.94, 0.08, 0.1], "brass", [0.0, -0.43, 0.42]),
                part([0.08, 0.8, 0.1], "brass", [0.43, 0.0, 0.42]),
                part([0.08, 0.8, 0.1], "brass", [-0.43, 0.0, 0.42]),
                part([0.78, 0.78, 0.04], "screen", [0.0, 0.0, 0.4]),
                part([0.9, 0.12, 0.7], "planks", [0.0, 0.44, -0.08]),
            ];
            for x in [-0.46f32, 0.46] {
                v.push(part([0.1, 0.5, 0.5], "brass", [x, 0.0, -0.05]).layer(Layer::RotorX));
                v.push(part([0.1, 0.5, 0.5], "brass", [x, 0.0, -0.05]).rot(Quat::from_rotation_x(FRAC_PI_4)).layer(Layer::RotorX));
                for i in 0..8 {
                    let a = i as f32 / 8.0 * TAU;
                    v.push(part([0.1, 0.12, 0.12], "brass", [x, a.cos() * 0.29, -0.05 + a.sin() * 0.29]).rot(Quat::from_rotation_x(-a)).layer(Layer::RotorX));
                }
                v.push(part([0.12, 0.16, 0.16], "steel", [x, 0.0, -0.05]).layer(Layer::RotorX));
            }
            v
        }
        Mech::Headlight => vec![
            // Bolted to the face behind it (−Y), shining along +Y. The lens
            // (lit by `lights`) is added separately.
            part([0.7, 0.14, 0.7], "steel", [0.0, -0.43, 0.0]),
            part([0.56, 0.3, 0.56], "steel_casing", [0.0, -0.22, 0.0]),
            part([0.62, 0.08, 0.62], "brass", [0.0, -0.05, 0.0]),
            part([0.1, 0.2, 0.1], "copper", [0.3, -0.3, 0.3]),
        ],
        Mech::Bearing => {
            // Casing with the input shaft at the back (−Y); a brass turntable
            // on the face (+Y) follows the structure's angle.
            let mut v = vec![
                part([0.92, 0.66, 0.92], "steel_casing", [0.0, -0.13, 0.0]),
                part([0.96, 0.08, 0.96], "brass", [0.0, 0.18, 0.0]),
                part([0.22, 0.12, 0.22], "shaft", [0.0, -0.47, 0.0]).layer(Layer::Rotor),
                part([0.32, 0.05, 0.32], "brass", [0.0, -0.45, 0.0]).layer(Layer::Rotor),
            ];
            v.push(part([0.86, 0.14, 0.86], "planks", [0.0, 0.31, 0.0]).layer(Layer::Mount));
            v.push(part([0.9, 0.06, 0.16], "steel", [0.0, 0.4, 0.0]).layer(Layer::Mount));
            v.push(part([0.16, 0.06, 0.9], "steel", [0.0, 0.4, 0.0]).layer(Layer::Mount));
            v.push(part([0.26, 0.08, 0.26], "brass", [0.0, 0.43, 0.0]).layer(Layer::Mount));
            for (x, z) in [(0.32f32, 0.32f32), (-0.32, 0.32), (0.32, -0.32), (-0.32, -0.32)] {
                v.push(part([0.08, 0.06, 0.08], "brass", [x, 0.4, z]).layer(Layer::Mount));
            }
            v
        }
        Mech::Piston => {
            // Casing with brass bands, input shaft at the back; the head
            // plate moves out with the extension, the pole stretches.
            let mut v = vec![
                part([0.92, 0.78, 0.92], "steel_casing", [0.0, -0.07, 0.0]),
                part([0.96, 0.08, 0.96], "brass", [0.0, -0.3, 0.0]),
                part([0.96, 0.08, 0.96], "brass", [0.0, 0.18, 0.0]),
                part([0.22, 0.12, 0.22], "shaft", [0.0, -0.47, 0.0]).layer(Layer::Rotor),
                part([0.32, 0.05, 0.32], "brass", [0.0, -0.45, 0.0]).layer(Layer::Rotor),
            ];
            v.push(part([0.9, 0.12, 0.9], "planks", [0.0, 0.44, 0.0]).layer(Layer::Mount));
            v.push(part([0.5, 0.08, 0.5], "steel", [0.0, 0.36, 0.0]).layer(Layer::Mount));
            for (x, z) in [(0.34f32, 0.34f32), (-0.34, 0.34), (0.34, -0.34), (-0.34, -0.34)] {
                v.push(part([0.08, 0.06, 0.08], "brass", [x, 0.5, z]).layer(Layer::Mount));
            }
            v.push(part([0.3, 1.0, 0.3], "shaft", [0.0, 0.0, 0.0]).layer(Layer::Pole));
            v
        }
        // Drawn by the piston (its pole stretches through these cells).
        Mech::PistonPole => Vec::new(),
        Mech::CreativeTank => {
            // Like a tank, with brass bands and a glowing core: never empties.
            let mut v = block_model(Mech::FluidTank);
            v.push(part([1.0, 0.1, 1.0], "brass", [0.0, 0.2, 0.0]));
            v.push(part([1.0, 0.1, 1.0], "brass", [0.0, -0.2, 0.0]));
            v.push(part([0.2, 0.2, 0.2], "glow_amber", [0.0, 0.0, 0.0]));
            v
        }
        Mech::ThresholdSensor => {
            let mut v = probe("glow_amber", "brass");
            // A red needle set to the threshold.
            v.push(part([0.03, 0.02, 0.14], "red_grip", [0.03, 0.145, 0.02]).rot(Quat::from_rotation_y(0.6)));
            v
        }
        Mech::RadioTransmitter | Mech::RadioReceiver => {
            // A steel box with an antenna; front lamp (red: sends, green:
            // receives).
            let tx = mech == Mech::RadioTransmitter;
            vec![
                part([0.8, 0.6, 0.8], "steel_casing", [0.0, -0.2, 0.0]),
                part([0.84, 0.06, 0.84], "brass", [0.0, 0.1, 0.0]),
                part([0.06, 0.5, 0.06], "steel", [0.25, 0.4, 0.25]),
                part([0.12, 0.08, 0.12], if tx { "copper" } else { "brass" }, [0.25, 0.66, 0.25]),
                part([0.4, 0.2, 0.04], "screen", [0.0, -0.2, 0.42]),
            ]
        }
        Mech::Lamp => vec![
            // A glass cube in a steel cage, bolted to the face behind it.
            part([0.66, 0.1, 0.66], "steel", [0.0, -0.43, 0.0]),
            part([0.56, 0.56, 0.56], "glass_lens", [0.0, -0.08, 0.0]),
            part([0.06, 0.62, 0.06], "steel", [0.3, -0.08, 0.3]),
            part([0.06, 0.62, 0.06], "steel", [-0.3, -0.08, 0.3]),
            part([0.06, 0.62, 0.06], "steel", [0.3, -0.08, -0.3]),
            part([0.06, 0.62, 0.06], "steel", [-0.3, -0.08, -0.3]),
            part([0.66, 0.06, 0.66], "brass", [0.0, 0.24, 0.0]),
        ],
        Mech::BeltPulley => {
            // A shaft with a rubber drum and brass flanges (turning).
            let mut v = shaft(1.0);
            v.push(part([0.72, 0.8, 0.72], "rubber", [0.0, 0.0, 0.0]).layer(Layer::Rotor));
            v.push(part([0.72, 0.8, 0.72], "rubber", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)).layer(Layer::Rotor));
            for y in [0.42f32, -0.42] {
                v.push(part([0.86, 0.06, 0.86], "brass", [0.0, y, 0.0]).layer(Layer::Rotor));
            }
            v
        }
        // Drawn by `belts` (its run depends on the neighbours).
        Mech::Belt => Vec::new(),
        Mech::TrackDrive => crate::tracks::roller_parts(true),
        Mech::TrackRoller => crate::tracks::roller_parts(false),
        Mech::SlidingDoor => vec![
            // Front = local +Z. A steel frame post on each side; the panel
            // (planks with steel bands, a brass handle and a window up top)
            // slides along local +X (`doors`).
            part([0.08, 1.0, 0.26], "steel", [-0.46, 0.0, 0.0]),
            part([0.08, 1.0, 0.26], "steel", [0.46, 0.0, 0.0]),
            part([0.86, 1.0, 0.12], "planks", [0.0, 0.0, 0.0]).layer(Layer::Mount),
            part([0.88, 0.07, 0.14], "steel", [0.0, 0.3, 0.0]).layer(Layer::Mount),
            part([0.88, 0.07, 0.14], "steel", [0.0, -0.3, 0.0]).layer(Layer::Mount),
            part([0.5, 0.36, 0.14], "glass_lens", [0.0, 0.0, 0.0]).layer(Layer::Mount),
            part([0.06, 0.16, 0.06], "brass", [-0.32, 0.0, 0.1]).layer(Layer::Mount),
            part([0.06, 0.16, 0.06], "brass", [-0.32, 0.0, -0.1]).layer(Layer::Mount),
        ],
        Mech::Flywheel => {
            // A heavy brass disc with a steel rim and counterweights on the
            // shaft (everything turns).
            let mut v = shaft(1.0);
            v.push(part([0.92, 0.22, 0.92], "brass", [0.0, 0.0, 0.0]).layer(Layer::Rotor));
            v.push(part([0.92, 0.22, 0.92], "brass", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)).layer(Layer::Rotor));
            for i in 0..16 {
                let a = i as f32 / 16.0 * TAU;
                v.push(part([0.16, 0.3, 0.14], "steel", [a.cos() * 0.48, 0.0, a.sin() * 0.48]).rot(Quat::from_rotation_y(-a)).layer(Layer::Rotor));
            }
            for a in [0.0f32, std::f32::consts::PI] {
                v.push(part([0.22, 0.3, 0.16], "steel_casing", [a.cos() * 0.26, 0.0, a.sin() * 0.26]).layer(Layer::Rotor));
            }
            v.push(part([0.3, 0.3, 0.3], "steel", [0.0, 0.0, 0.0]).layer(Layer::Rotor));
            v
        }
        Mech::FluidTank => {
            // Steel edges with brass corner bolts, glass sides; the fluid
            // level inside is added by `fluids`.
            let mut v = Vec::new();
            let e = 0.47;
            for (a, b) in [(e, e), (e, -e), (-e, e), (-e, -e)] {
                v.push(part([0.08, 1.0, 0.08], "steel", [a, 0.0, b]));
                v.push(part([1.0, 0.08, 0.08], "steel", [0.0, a, b]));
                v.push(part([0.08, 0.08, 1.0], "steel", [a, b, 0.0]));
            }
            for (x, y, z) in [(1.0f32, 1.0f32, 1.0f32), (-1.0, 1.0, 1.0), (1.0, -1.0, 1.0), (-1.0, -1.0, 1.0), (1.0, 1.0, -1.0), (-1.0, 1.0, -1.0), (1.0, -1.0, -1.0), (-1.0, -1.0, -1.0)] {
                v.push(part([0.12, 0.12, 0.12], "brass", [x * 0.46, y * 0.46, z * 0.46]));
            }
            for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let r = Quat::from_rotation_arc(Vec3::Z, d);
                v.push(part([0.86, 0.86, 0.03], "glass", (d * 0.455).to_array()).rot(r));
            }
            v.push(part([0.88, 0.04, 0.88], "steel", [0.0, -0.46, 0.0]));
            v
        }
        Mech::Wire => crate::logic::bus_parts(1, 0, &[(IVec3::X, 1), (IVec3::NEG_X, 1)]),
        Mech::Button => vec![
            // On the face behind it (+Y); the cap moves in when pressed.
            part([0.44, 0.06, 0.34], "steel", [0.0, 0.46, 0.0]),
            part([0.06, 0.04, 0.06], "brass", [0.17, 0.42, 0.12]),
            part([0.06, 0.04, 0.06], "brass", [-0.17, 0.42, -0.12]),
            part([0.26, 0.12, 0.18], "red_grip", [0.0, 0.38, 0.0]).layer(Layer::Mount),
        ],
        Mech::Lever => vec![
            part([0.5, 0.08, 0.64], "steel_casing", [0.0, 0.45, 0.0]),
            part([0.16, 0.08, 0.3], "steel", [0.0, 0.39, 0.0]),
            part([0.07, 0.42, 0.07], "steel", [0.0, 0.2, 0.0]).layer(Layer::Mount),
            part([0.13, 0.13, 0.13], "red_grip", [0.0, -0.02, 0.0]).layer(Layer::Mount),
        ],
        Mech::Timer => {
            // Front (local +Z): a dial with a turning hand and a lamp.
            let mut v = vec![
                part([0.86, 0.86, 0.86], "steel_casing", [0.0, 0.0, 0.0]),
                part([0.62, 0.62, 0.04], "screen", [0.0, -0.04, 0.44]),
                part([0.66, 0.05, 0.05], "brass", [0.0, 0.3, 0.45]),
                part([0.66, 0.05, 0.05], "brass", [0.0, -0.38, 0.45]),
                part([0.03, 0.26, 0.03], "red_grip", [0.0, 0.09, 0.47]).layer(Layer::Mount),
                part([0.07, 0.07, 0.04], "brass", [0.0, -0.04, 0.47]),
            ];
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU;
                v.push(part([0.04, 0.04, 0.03], "brass", [a.sin() * 0.24, -0.04 + a.cos() * 0.24, 0.46]));
            }
            v
        }
        Mech::PulseGenerator => vec![
            part([0.92, 0.3, 0.92], "steel", [0.0, -0.35, 0.0]),
            part([0.8, 0.06, 0.8], "brass", [0.0, -0.18, 0.0]),
            part([0.3, 0.2, 0.3], "copper", [0.0, -0.06, -0.1]),
            part([0.12, 0.06, 0.32], "red_grip", [0.0, -0.13, 0.26]),
            part([0.06, 0.12, 0.06], "glow_cyan", [0.0, 0.08, -0.1]),
        ],
        Mech::LogicGate => vec![
            // A plate with two input posts (torch-like) and a red arrow to
            // the output at the front (local +Z).
            part([0.92, 0.24, 0.92], "steel", [0.0, -0.38, 0.0]),
            part([0.8, 0.05, 0.8], "brass", [0.0, -0.24, 0.0]),
            part([0.1, 0.3, 0.1], "copper", [0.26, -0.08, -0.18]),
            part([0.1, 0.3, 0.1], "copper", [-0.26, -0.08, -0.18]),
            part([0.12, 0.1, 0.12], "glow_amber", [0.26, 0.11, -0.18]),
            part([0.12, 0.1, 0.12], "glow_amber", [-0.26, 0.11, -0.18]),
            part([0.3, 0.05, 0.1], "red_grip", [0.0, -0.2, 0.1]),
            part([0.18, 0.05, 0.1], "red_grip", [0.0, -0.2, 0.2]),
            part([0.06, 0.05, 0.1], "red_grip", [0.0, -0.2, 0.3]),
        ],
        Mech::Flasher => vec![
            // Bolted to the face behind it (−Y); a glass dome on top.
            part([0.62, 0.14, 0.62], "steel", [0.0, -0.43, 0.0]),
            part([0.5, 0.06, 0.5], "brass", [0.0, -0.34, 0.0]),
            part([0.46, 0.4, 0.46], "glass_lens", [0.0, -0.11, 0.0]),
            part([0.5, 0.05, 0.5], "brass", [0.0, 0.11, 0.0]),
            part([0.06, 0.48, 0.06], "brass", [0.24, -0.11, 0.24]),
            part([0.06, 0.48, 0.06], "brass", [-0.24, -0.11, -0.24]),
        ],
        Mech::Pipe => crate::fluids::pipe_parts(&[], false),
        Mech::ItemTube => crate::tubes::tube_parts(&[]),
        Mech::HandCrank => vec![
            // On the face of the block behind it (−Y): a hub, an arm, a grip.
            part([0.3, 0.12, 0.3], "steel", [0.0, -0.42, 0.0]).layer(Layer::Rotor),
            part([0.16, 0.2, 0.16], "brass", [0.0, -0.3, 0.0]).layer(Layer::Rotor),
            part([0.12, 0.08, 0.5], "steel", [0.0, -0.2, 0.18]).layer(Layer::Rotor),
            part([0.12, 0.26, 0.12], "red_grip", [0.0, -0.04, 0.4]).layer(Layer::Rotor),
        ],
        Mech::Winch => {
            // A frame with a drum across (local X), rope wound on it, the
            // rope leaving through a brass guide on the face (+Y).
            let mut v = vec![
                part([0.9, 0.12, 0.9], "steel_casing", [0.0, -0.44, 0.0]),
                part([0.1, 0.8, 0.5], "steel", [0.42, -0.02, 0.0]),
                part([0.1, 0.8, 0.5], "steel", [-0.42, -0.02, 0.0]),
                part([0.2, 0.12, 0.2], "brass", [0.0, 0.44, 0.0]),
                part([0.1, 0.2, 0.1], "leather", [0.0, 0.32, 0.0]),
            ];
            for r in [0.0f32, FRAC_PI_4] {
                v.push(part([0.66, 0.5, 0.5], "leather", [0.0, 0.02, 0.0]).rot(Quat::from_rotation_x(r)).layer(Layer::RotorX));
                v.push(part([0.06, 0.64, 0.64], "brass", [0.34, 0.02, 0.0]).rot(Quat::from_rotation_x(r)).layer(Layer::RotorX));
                v.push(part([0.06, 0.64, 0.64], "brass", [-0.34, 0.02, 0.0]).rot(Quat::from_rotation_x(r)).layer(Layer::RotorX));
            }
            v.push(part([0.9, 0.16, 0.16], "shaft", [0.0, 0.02, 0.0]).layer(Layer::RotorX));
            v
        }
        Mech::RopeAnchor => vec![
            // A plate on the face of the block it is attached to (+Y), a
            // brass eye sticking out.
            part([0.4, 0.06, 0.4], "steel", [0.0, 0.47, 0.0]),
            part([0.08, 0.2, 0.08], "brass", [-0.1, 0.36, 0.0]),
            part([0.08, 0.2, 0.08], "brass", [0.1, 0.36, 0.0]),
            part([0.28, 0.08, 0.08], "brass", [0.0, 0.24, 0.0]),
        ],
        Mech::ItemDrive => {
            // A steel case on a glass tube, a brass ring around it (meshes
            // like a cog) and a green arrow of the way items go.
            let mut v = cog(0.4, 8, 0.66);
            v.retain(|p| p.mat != "shaft");
            v.push(part([0.56, 1.0, 0.56], "glass", [0.0, 0.0, 0.0]));
            v.push(part([0.66, 0.4, 0.66], "steel_casing", [0.0, 0.22, 0.0]));
            v.push(part([0.64, 0.07, 0.64], "brass", [0.0, 0.465, 0.0]));
            v.push(part([0.64, 0.07, 0.64], "brass", [0.0, -0.465, 0.0]));
            v.push(part([0.06, 0.18, 0.04], "glow_green", [0.0, 0.26, 0.34]));
            v.push(part([0.14, 0.06, 0.04], "glow_green", [0.0, 0.34, 0.34]));
            v
        }
        Mech::Chest => {
            // Dark wood with steel bands and a brass lock at the front (+Z).
            let mut v = vec![
                part([0.86, 0.6, 0.86], "planks", [0.0, -0.13, 0.0]),
                part([0.88, 0.26, 0.88], "planks", [0.0, 0.3, 0.0]),
                part([0.9, 0.04, 0.9], "steel", [0.0, 0.17, 0.0]),
                part([0.14, 0.2, 0.06], "brass", [0.0, 0.14, 0.45]),
                part([0.06, 0.06, 0.04], "screen", [0.0, 0.12, 0.48]),
            ];
            for (x, z) in [(0.44f32, 0.44f32), (-0.44, 0.44), (0.44, -0.44), (-0.44, -0.44)] {
                v.push(part([0.06, 0.88, 0.06], "steel", [x, 0.0, z]));
            }
            for x in [-0.28f32, 0.28] {
                v.push(part([0.07, 0.28, 0.9], "steel", [x, 0.3, 0.0]));
            }
            v
        }
        Mech::Barrel => {
            // Staves as an octagon, steel hoops, a brass bung on top (+Y).
            let mut v = vec![
                part([0.8, 0.98, 0.8], "planks", [0.0, 0.0, 0.0]),
                part([0.8, 0.98, 0.8], "planks", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)),
                part([0.7, 0.04, 0.7], "planks", [0.0, 0.49, 0.0]),
                part([0.16, 0.05, 0.16], "brass", [0.18, 0.51, 0.0]),
            ];
            for y in [-0.33f32, 0.0, 0.33] {
                v.push(part([0.84, 0.06, 0.84], "steel", [0.0, y, 0.0]));
                v.push(part([0.84, 0.06, 0.84], "steel", [0.0, y, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)));
            }
            v
        }
        Mech::Container => {
            // A wooden crate in a riveted steel frame; joined ones look like one.
            let mut v = vec![part([0.92, 0.92, 0.92], "planks", [0.0, 0.0, 0.0])];
            let e = 0.47;
            for (a, b) in [(e, e), (e, -e), (-e, e), (-e, -e)] {
                v.push(part([0.07, 1.0, 0.07], "steel", [a, 0.0, b]));
                v.push(part([1.0, 0.07, 0.07], "steel", [0.0, a, b]));
                v.push(part([0.07, 0.07, 1.0], "steel", [a, b, 0.0]));
            }
            for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let r = Quat::from_rotation_arc(Vec3::Z, d);
                v.push(part([0.07, 0.9, 0.03], "steel", (d * 0.465).to_array()).rot(r * Quat::from_rotation_z(FRAC_PI_4)));
                v.push(part([0.3, 0.12, 0.03], "brass", (d * 0.47 + Vec3::Y * 0.3).to_array()).rot(r));
            }
            v
        }
        Mech::CasedPipe => crate::fluids::pipe_parts(&[IVec3::Y, IVec3::NEG_Y], true),
        Mech::CasedShaft => {
            let mut v = casing("steel_casing");
            v.extend(through_shaft());
            v
        }
        Mech::Pump => {
            // A copper pipe body along the axis with a brass cog around it
            // (powered like a cogwheel) and an arrow of the flow.
            let mut v = cog(0.38, 8, 0.62);
            v.retain(|p| p.mat != "shaft");
            v.push(part([0.42, 1.0, 0.42], "copper", [0.0, 0.0, 0.0]));
            v.push(part([0.5, 0.1, 0.5], "brass", [0.0, 0.45, 0.0]));
            v.push(part([0.5, 0.1, 0.5], "brass", [0.0, -0.45, 0.0]));
            v.push(part([0.56, 0.3, 0.56], "steel_casing", [0.0, 0.25, 0.0]));
            v.push(part([0.12, 0.08, 0.04], "glow_green", [0.0, 0.3, 0.29]));
            v
        }
        Mech::FluidPort => vec![
            // Casing with a funnel and a hatch on the face (+Y).
            part([0.9, 0.78, 0.9], "steel_casing", [0.0, -0.06, 0.0]),
            part([0.6, 0.1, 0.6], "copper", [0.0, 0.36, 0.0]),
            part([0.46, 0.06, 0.46], "brass", [0.0, 0.43, 0.0]),
            part([0.3, 0.04, 0.3], "screen", [0.0, 0.46, 0.0]),
            part([0.3, 0.06, 0.08], "red_grip", [0.0, 0.48, 0.16]),
            part([0.08, 0.08, 0.08], "brass", [0.25, 0.45, -0.22]),
        ],
        Mech::DieselEngine => {
            // One module of an inline diesel (front = local +Z): oil pan,
            // block and head run the full length so modules in a row read
            // as one long engine. Two cylinders per module: injectors on the
            // valve cover, exhaust manifold on the left (+X) into a stack,
            // intake on the right. The front cap carries the flywheel, the
            // back cap an end plate; `engine` hides them inside a row.
            let mut v = vec![
                part([0.8, 0.22, 1.0], "steel", [0.0, -0.39, 0.0]),
                part([0.9, 0.06, 1.0], "steel", [0.0, -0.47, 0.0]),
                part([0.72, 0.4, 1.0], "steel_casing", [0.0, -0.08, 0.0]),
                part([0.78, 0.12, 1.0], "steel", [0.0, 0.18, 0.0]),
                part([0.56, 0.1, 0.92], "red_grip", [0.0, 0.29, 0.0]),
                part([0.58, 0.03, 0.94], "steel", [0.0, 0.25, 0.0]),
            ];
            for z in [-0.25f32, 0.25] {
                // Injector + fuel line, exhaust runner, intake runner.
                v.push(part([0.08, 0.12, 0.08], "brass", [0.0, 0.39, z]));
                v.push(part([0.04, 0.04, 0.2], "copper", [0.08, 0.36, z]));
                v.push(part([0.16, 0.12, 0.14], "copper", [0.42, 0.1, z]));
                v.push(part([0.1, 0.16, 0.1], "steel", [-0.41, 0.06, z]));
                // Cooling fins on the block.
                v.push(part([0.76, 0.03, 0.2], "steel", [0.0, -0.06, z]));
                v.push(part([0.76, 0.03, 0.2], "steel", [0.0, -0.16, z]));
            }
            v.push(part([0.12, 0.14, 1.0], "copper", [0.49, 0.1, 0.0]));
            v.push(part([0.12, 0.08, 1.0], "steel", [-0.46, 0.15, 0.0]));
            // Exhaust stack up to the top face.
            v.push(part([0.14, 0.4, 0.14], "steel", [0.3, 0.32, 0.0]));
            v.push(part([0.18, 0.06, 0.18], "brass", [0.3, 0.5, 0.0]));
            v.push(part([0.2, 0.05, 0.1], "copper", [0.4, 0.14, 0.0]));
            // Front: timing cover + flywheel (output).
            v.push(part([0.7, 0.6, 0.08], "steel_casing", [0.0, -0.1, 0.46]).layer(Layer::CapFront));
            v.push(part([0.26, 0.16, 0.04], "glow_amber", [-0.18, 0.12, 0.5]).layer(Layer::CapFront));
            v.push(part([0.22, 0.22, 0.2], "shaft", [0.0, -0.12, 0.44]).layer(Layer::RotorZ));
            v.push(part([0.52, 0.52, 0.06], "brass", [0.0, -0.12, 0.49]).layer(Layer::RotorZ));
            v.push(part([0.52, 0.52, 0.06], "brass", [0.0, -0.12, 0.49]).rot(Quat::from_rotation_z(FRAC_PI_4)).layer(Layer::RotorZ));
            v.push(part([0.2, 0.2, 0.08], "steel", [0.0, -0.12, 0.52]).layer(Layer::RotorZ));
            // Back: end plate with bolts.
            v.push(part([0.74, 0.66, 0.06], "steel", [0.0, -0.1, -0.47]).layer(Layer::CapBack));
            for (x, y) in [(-0.3f32, 0.16f32), (0.3, 0.16), (-0.3, -0.36), (0.3, -0.36)] {
                v.push(part([0.06, 0.06, 0.04], "brass", [x, y, -0.5]).layer(Layer::CapBack));
            }
            v
        }
        Mech::Valve => {
            // A pipe along the axis through a steel body; the red handwheel
            // on top is turned in line (open) or across (closed).
            let mut v = crate::fluids::pipe_parts(&[IVec3::Y, IVec3::NEG_Y], false);
            v.push(part([0.5, 0.42, 0.5], "steel_casing", [0.0, 0.0, 0.0]));
            v.push(part([0.12, 0.2, 0.12], "steel", [0.0, 0.0, 0.3]));
            v.push(part([0.44, 0.05, 0.05], "red_grip", [0.0, 0.0, 0.42]).rot(Quat::from_rotation_z(FRAC_PI_4)));
            v.push(part([0.44, 0.05, 0.05], "red_grip", [0.0, 0.0, 0.42]).rot(Quat::from_rotation_z(-FRAC_PI_4)));
            v.push(part([0.1, 0.1, 0.06], "brass", [0.0, 0.0, 0.44]));
            v
        }
        Mech::PhysicsToggle => vec![
            part([0.96, 0.96, 0.96], "steel_casing", [0.0, 0.0, 0.0]),
            part([1.0, 0.12, 0.3], "brass", [0.0, 0.42, 0.0]),
            part([0.3, 0.12, 1.0], "brass", [0.0, 0.42, 0.0]),
            part([0.34, 0.14, 0.34], "glow_amber", [0.0, 0.46, 0.0]),
        ],
    }
}

/// Engineer's goggles: a brass frame, two cyan lenses, a leather strap.
pub fn goggles_parts() -> Vec<Part> {
    vec![
        part([0.13, 0.11, 0.06], "brass", [-0.08, 0.0, 0.0]),
        part([0.13, 0.11, 0.06], "brass", [0.08, 0.0, 0.0]),
        part([0.09, 0.07, 0.03], "glow_cyan", [-0.08, 0.0, -0.025]),
        part([0.09, 0.07, 0.03], "glow_cyan", [0.08, 0.0, -0.025]),
        part([0.05, 0.03, 0.04], "brass", [0.0, 0.02, 0.0]),
        part([0.36, 0.04, 0.03], "leather", [0.0, 0.0, 0.02]),
    ]
}

/// Hand-held tool models (local −Z = forward, origin at the grip).
pub fn tool_model(tool: Tool) -> Vec<Part> {
    match tool {
        Tool::Wrench => vec![
            part([0.07, 0.07, 0.3], "red_grip", [0.0, 0.0, 0.05]),
            part([0.09, 0.09, 0.05], "brass", [0.0, 0.0, 0.22]),
            part([0.09, 0.09, 0.05], "brass", [0.0, 0.0, -0.11]),
            part([0.05, 0.05, 0.22], "steel", [0.0, 0.0, -0.22]),
            part([0.16, 0.06, 0.07], "steel", [0.0, 0.0, -0.34]),
            part([0.05, 0.06, 0.12], "steel", [0.06, 0.0, -0.42]),
            part([0.05, 0.06, 0.1], "brass", [-0.06, 0.0, -0.41]),
        ],
        Tool::Glue => vec![
            part([0.14, 0.18, 0.14], "brass", [0.0, 0.0, 0.0]),
            part([0.16, 0.04, 0.16], "steel", [0.0, 0.1, 0.0]),
            part([0.05, 0.08, 0.05], "steel", [0.0, 0.15, 0.0]),
            part([0.1, 0.08, 0.1], "glow_green", [0.0, -0.02, 0.06]),
        ],
        Tool::Goggles => goggles_parts(),
        Tool::WaterBucket => vec![
            part([0.2, 0.2, 0.2], "steel", [0.0, 0.0, 0.0]),
            part([0.16, 0.02, 0.16], "water", [0.0, 0.1, 0.0]),
            part([0.22, 0.03, 0.22], "brass", [0.0, 0.1, 0.0]),
            part([0.02, 0.14, 0.02], "brass", [0.1, 0.17, 0.0]),
            part([0.02, 0.14, 0.02], "brass", [-0.1, 0.17, 0.0]),
            part([0.22, 0.02, 0.02], "brass", [0.0, 0.24, 0.0]),
        ],
        Tool::Crowbar => vec![
            // A steel bar with a curved claw at the front (−Z), red grip.
            part([0.05, 0.05, 0.62], "steel", [0.0, 0.0, -0.2]),
            part([0.05, 0.14, 0.05], "steel", [0.0, 0.06, -0.52]).rot(Quat::from_rotation_x(-0.5)),
            part([0.06, 0.04, 0.08], "steel", [0.0, 0.13, -0.47]),
            part([0.07, 0.07, 0.22], "red_grip", [0.0, 0.0, 0.08]),
            part([0.06, 0.03, 0.07], "steel", [0.0, -0.03, 0.18]),
        ],
        Tool::Flashlight => vec![
            // A steel tube, brass head and switch, a warm lens at the front.
            part([0.09, 0.09, 0.3], "steel", [0.0, 0.0, -0.05]),
            part([0.13, 0.13, 0.08], "brass", [0.0, 0.0, -0.22]),
            part([0.09, 0.09, 0.02], "glow_lamp", [0.0, 0.0, -0.265]),
            part([0.03, 0.03, 0.05], "red_grip", [0.0, 0.055, 0.0]),
            part([0.1, 0.1, 0.03], "brass", [0.0, 0.0, 0.1]),
        ],
        Tool::Rope => vec![
            // A coil of rope with a brass hook.
            part([0.22, 0.22, 0.1], "leather", [0.0, 0.0, 0.0]),
            part([0.22, 0.22, 0.1], "leather", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_z(FRAC_PI_4)),
            part([0.1, 0.1, 0.12], "planks", [0.0, 0.0, 0.0]),
            part([0.04, 0.12, 0.04], "brass", [0.0, -0.16, 0.0]),
            part([0.08, 0.04, 0.04], "brass", [0.03, -0.22, 0.0]),
        ],
        Tool::Belt => vec![
            // A rolled-up belt.
            part([0.2, 0.2, 0.12], "rubber", [0.0, 0.0, 0.0]),
            part([0.2, 0.2, 0.12], "rubber", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_z(FRAC_PI_4)),
            part([0.08, 0.08, 0.14], "steel", [0.0, 0.0, 0.0]),
        ],
        Tool::Chain => {
            // A short length of chain: alternating steel and brass links.
            let mut v = Vec::new();
            for i in 0..6 {
                let (mat, s) = if i % 2 == 0 { ("steel", [0.05, 0.1, 0.12]) } else { ("brass", [0.1, 0.05, 0.12]) };
                v.push(part(s, mat, [0.0, 0.0, 0.1 - i as f32 * 0.09]));
            }
            v
        }
        Tool::DieselBucket => vec![
            // A steel bucket with an amber fill and a brass handle.
            part([0.2, 0.2, 0.2], "steel", [0.0, 0.0, 0.0]),
            part([0.16, 0.02, 0.16], "diesel", [0.0, 0.1, 0.0]),
            part([0.22, 0.03, 0.22], "brass", [0.0, 0.1, 0.0]),
            part([0.02, 0.14, 0.02], "brass", [0.1, 0.17, 0.0]),
            part([0.02, 0.14, 0.02], "brass", [-0.1, 0.17, 0.0]),
            part([0.22, 0.02, 0.02], "brass", [0.0, 0.24, 0.0]),
        ],
        Tool::Blueprint => vec![
            // An open book of blue drawings.
            part([0.34, 0.03, 0.26], "leather", [0.0, -0.02, 0.0]),
            part([0.15, 0.03, 0.24], "blueprint", [-0.08, 0.0, 0.0]).rot(Quat::from_rotation_z(0.12)),
            part([0.15, 0.03, 0.24], "blueprint", [0.08, 0.0, 0.0]).rot(Quat::from_rotation_z(-0.12)),
            part([0.02, 0.04, 0.26], "brass", [0.0, 0.0, 0.0]),
        ],
        Tool::GravityGun | Tool::PhysGun => {
            let glow = if tool == Tool::GravityGun { "glow_amber" } else { "glow_cyan" };
            let claw = if tool == Tool::GravityGun { "brass" } else { "steel" };
            let mut v = vec![
                part([0.16, 0.16, 0.4], "steel", [0.0, 0.0, -0.1]),
                part([0.2, 0.2, 0.06], "brass", [0.0, 0.0, 0.06]),
                part([0.2, 0.2, 0.06], "brass", [0.0, 0.0, -0.12]),
                part([0.2, 0.2, 0.06], "brass", [0.0, 0.0, -0.28]),
                part([0.17, 0.08, 0.1], glow, [0.0, 0.06, -0.04]),
                part([0.17, 0.08, 0.1], glow, [0.0, 0.06, -0.2]),
                part([0.1, 0.1, 0.04], glow, [0.0, 0.0, -0.32]),
                part([0.07, 0.18, 0.08], "steel", [0.0, -0.14, 0.08]),
                part([0.18, 0.04, 0.04], "steel", [0.0, 0.14, -0.05]),
            ];
            for a in [0.0f32, 2.1, 4.2] {
                let (s, c) = a.sin_cos();
                v.push(part([0.03, 0.03, 0.16], claw, [s * 0.09, c * 0.09, -0.4]).rot(Quat::from_axis_angle(Vec3::new(c, -s, 0.0), 0.35)));
            }
            v
        }
    }
}
