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

/// Tyre, rim and hub around local Y (the axle).
pub fn wheel_parts(layer: Layer) -> Vec<Part> {
    let mut v = Vec::new();
    let n = 12;
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        v.push(part([0.3, 0.42, 0.24], "rubber", [a.cos() * 0.47, 0.0, a.sin() * 0.47]).rot(Quat::from_rotation_y(-a)).layer(layer));
    }
    v.push(part([0.7, 0.36, 0.7], "brass", [0.0, 0.0, 0.0]).layer(layer));
    v.push(part([0.7, 0.36, 0.7], "brass", [0.0, 0.0, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)).layer(layer));
    v.push(part([0.3, 0.44, 0.3], "steel", [0.0, 0.0, 0.0]).layer(layer));
    v.push(part([0.12, 0.48, 0.12], "brass", [0.0, 0.0, 0.0]).layer(layer));
    v
}

pub fn block_model(mech: Mech) -> Vec<Part> {
    match mech {
        Mech::Shaft => {
            let mut v = shaft(1.0);
            v.push(part([0.3, 0.07, 0.3], "brass", [0.0, 0.36, 0.0]).layer(Layer::Rotor));
            v.push(part([0.3, 0.07, 0.3], "brass", [0.0, -0.36, 0.0]).layer(Layer::Rotor));
            v
        }
        Mech::Cog => cog(0.38, 8, 0.62),
        Mech::LargeCog => cog(0.84, 16, 1.36),
        Mech::Gearbox => {
            let mut v = casing("steel_casing");
            for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let r = Quat::from_rotation_arc(Vec3::Y, d);
                v.push(part([0.4, 0.06, 0.4], "brass", (d * 0.47).to_array()).rot(r));
                v.push(part([0.2, 0.08, 0.2], "shaft", (d * 0.5).to_array()).rot(r));
            }
            v
        }
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
            let mut v = vec![
                part([0.8, 0.62, 0.8], "steel", [0.0, -0.16, 0.0]),
                part([0.8, 0.62, 0.8], "steel", [0.0, -0.16, 0.0]).rot(Quat::from_rotation_y(FRAC_PI_4)),
                part([0.9, 0.1, 0.9], "brass", [0.0, -0.4, 0.0]),
                part([0.86, 0.08, 0.86], "brass", [0.0, 0.1, 0.0]),
                part([0.84, 0.16, 0.24], "copper", [0.0, -0.16, 0.0]),
                part([0.24, 0.16, 0.84], "copper", [0.0, -0.16, 0.0]),
                part([0.3, 0.2, 0.06], "glow_amber", [0.0, -0.16, 0.42]),
            ];
            v.push(part([0.22, 0.4, 0.22], "shaft", [0.0, 0.3, 0.0]).layer(Layer::Rotor));
            v.push(part([0.34, 0.08, 0.34], "brass", [0.0, 0.2, 0.0]).layer(Layer::Rotor));
            v
        }
        Mech::Suspension => {
            // Bracket (bolted to the chassis), coil spring, knuckle with
            // the axle stub pointing at the wheel (+Y).
            let mut v = vec![
                part([0.8, 0.16, 0.8], "steel", [0.0, -0.42, 0.0]),
                part([0.16, 0.7, 0.16], "steel", [0.0, -0.1, -0.3]),
            ];
            for i in 0..4 {
                v.push(part([0.3, 0.06, 0.3], "copper", [0.0, -0.3 + i as f32 * 0.12, 0.05]).rot(Quat::from_rotation_y(i as f32 * 0.4)));
            }
            v.push(part([0.42, 0.24, 0.42], "steel_casing", [0.0, 0.18, 0.0]).layer(Layer::Steer));
            v.push(part([0.2, 0.36, 0.2], "shaft", [0.0, 0.36, 0.0]).layer(Layer::Rotor));
            v
        }
        Mech::Wheel => wheel_parts(Layer::Rotor),
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
        Mech::Sensor => vec![
            // A plate on the face of the block it controls (+Y side).
            part([0.6, 0.12, 0.6], "steel", [0.0, 0.44, 0.0]),
            part([0.44, 0.08, 0.44], "brass", [0.0, 0.36, 0.0]),
            part([0.16, 0.08, 0.16], "glow_green", [0.0, 0.3, 0.0]),
            part([0.05, 0.3, 0.05], "steel", [0.18, 0.22, 0.18]),
        ],
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
        Mech::TelemetrySensor => vec![
            // A plate on the face of the block it reads (+Y side), with a
            // cyan lens and a cable stub.
            part([0.6, 0.12, 0.6], "steel", [0.0, 0.44, 0.0]),
            part([0.44, 0.08, 0.44], "copper", [0.0, 0.36, 0.0]),
            part([0.18, 0.08, 0.18], "glow_cyan", [0.0, 0.3, 0.0]),
            part([0.1, 0.2, 0.1], "brass", [-0.2, 0.26, 0.2]),
        ],
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
        Mech::PhysicsToggle => vec![
            part([0.96, 0.96, 0.96], "steel_casing", [0.0, 0.0, 0.0]),
            part([1.0, 0.12, 0.3], "brass", [0.0, 0.42, 0.0]),
            part([0.3, 0.12, 1.0], "brass", [0.0, 0.42, 0.0]),
            part([0.34, 0.14, 0.34], "glow_amber", [0.0, 0.46, 0.0]),
        ],
    }
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
