//! Headless physics regression tests for player-built assemblies
//! (no window, no renderer): the four-wheel cart must stay stable.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::components::{Engine, Part, PartKind, PowerCell};
use crate::physics::joints::{Connection, JointKind, LocalFrame, spawn_connection};

/// The frame lies on the ground (as placed by the player) while the engine
/// is lifted by its wheels: free wheels on the frame then touch the floor.
const FRAME_Y: f32 = 0.5;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::diagnostic::DiagnosticsPlugin,
    ))
    .init_asset::<Mesh>()
    .add_plugins(PhysicsPlugins::default())
    // Headless app: make sure Avian's diagnostics resources exist.
    .init_resource::<SpatialQueryDiagnostics>()
    .init_resource::<avian3d::collider_tree::ColliderTreeDiagnostics>()
    .init_resource::<avian3d::collision::CollisionDiagnostics>()
    .init_resource::<avian3d::dynamics::solver::SolverDiagnostics>()
    .insert_resource(Gravity(Vec3::NEG_Y * 19.0))
    .insert_resource(Time::<Fixed>::from_hz(60.0))
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / 60.0),
    ))
    .add_plugins((super::power::PowerPlugin, super::engines::EnginesPlugin, crate::electric::ElectricCorePlugin))
    .configure_sets(
        FixedUpdate,
        (super::MachinerySet::Power, super::MachinerySet::Drive).chain(),
    );
    app
}

fn body(app: &mut App, kind: PartKind, t: Transform) -> Entity {
    let mut e = app.world_mut().spawn((
        Part { kind },
        t,
        RigidBody::Dynamic,
        kind.collider(),
        Mass(kind.mass()),
        AngularDamping(0.35),
        Friction::new(if kind == PartKind::Wheel { 1.4 } else { 0.6 }),
        SpeculativeCcd::new(1.0),
        MaxLinearSpeed(80.0),
        MaxAngularSpeed(60.0),
    ));
    match kind {
        PartKind::Engine => {
            e.insert((Engine::default(), SleepingDisabled));
        }
        PartKind::PowerCell => {
            e.insert(PowerCell {
                energy: 600.0,
                capacity: 1000.0,
            });
        }
        _ => {}
    }
    e.id()
}

fn connect(app: &mut App, a: Entity, b: Entity, kind: JointKind, anchor: Vec3, basis: Quat) {
    let world = app.world_mut();
    let (pa, ra) = {
        let t = world.get::<Transform>(a).unwrap();
        (t.translation, t.rotation)
    };
    let (pb, rb) = {
        let t = world.get::<Transform>(b).unwrap();
        (t.translation, t.rotation)
    };
    let c = Connection {
        a,
        b,
        kind,
        frame_a: LocalFrame::from_world(pa, ra, anchor, basis),
        frame_b: LocalFrame::from_world(pb, rb, anchor, basis),
    };
    let mut commands = world.commands();
    spawn_connection(&mut commands, c);
    world.flush();
}

/// Builds the cart exactly like the player does (engine + frame, 2 driven
/// wheels on the engine, 2 free wheels on the frame, cell on top) on a
/// voxel floor. Returns (engine, all bodies).
fn build_cart(app: &mut App) -> (Entity, Vec<Entity>) {
    let coords: Vec<IVec3> = (-40..40)
        .flat_map(|x| (-40..40).map(move |z| IVec3::new(x, -1, z)))
        .collect();
    app.world_mut()
        .spawn((RigidBody::Static, Collider::voxels(Vec3::ONE, &coords)));
    let h = 0.6;
    let engine = body(app, PartKind::Engine, Transform::from_xyz(0.0, h, 0.0));
    let frame = body(
        app,
        PartKind::Frame,
        Transform::from_xyz(1.65, FRAME_Y, 0.0),
    );
    let cell = body(
        app,
        PartKind::PowerCell,
        Transform::from_xyz(0.0, h + 1.02, 0.0),
    );
    let wheel_rot = |side: f32| Quat::from_rotation_arc(Vec3::Y, Vec3::Z * side);
    let mut wheels = Vec::new();
    for (base, x, y) in [(engine, 0.0, h), (frame, 1.65, FRAME_Y)] {
        for side in [1.0f32, -1.0] {
            let w = body(
                app,
                PartKind::Wheel,
                Transform::from_xyz(x, y, side * 0.695).with_rotation(wheel_rot(-side)),
            );
            wheels.push((base, w, x, y, side));
        }
    }
    app.update();
    connect(
        app,
        engine,
        frame,
        JointKind::Weld,
        Vec3::new(0.51, h, 0.0),
        Quat::IDENTITY,
    );
    connect(
        app,
        engine,
        cell,
        JointKind::Weld,
        Vec3::new(0.0, h + 0.5, 0.0),
        Quat::IDENTITY,
    );
    let mut all = vec![engine, frame, cell];
    for (base, w, x, y, side) in wheels {
        let normal = Vec3::Z * side;
        connect(
            app,
            base,
            w,
            JointKind::Hinge,
            Vec3::new(x, y, side * 0.52),
            Quat::from_rotation_arc(Vec3::Z, normal),
        );
        all.push(w);
    }
    (engine, all)
}

fn run(app: &mut App, seconds: f32, bodies: &[Entity]) -> f32 {
    let mut max_speed: f32 = 0.0;
    let steps = (seconds * 60.0) as usize;
    for _ in 0..steps {
        app.update();
        for (i, e) in bodies.iter().enumerate() {
            if let Some(v) = app.world().get::<LinearVelocity>(*e) {
                if v.0.length() > max_speed && v.0.length() > 15.0 {
                    let w = app.world().get::<AngularVelocity>(*e).unwrap().0;
                    eprintln!("body {i} v {:.1} ω {:.1}", v.0.length(), w.length());
                }
                max_speed = max_speed.max(v.0.length());
            }
        }
    }
    max_speed
}

#[test]
fn four_wheel_cart_is_stable_at_rest() {
    let mut app = app();
    let (_, bodies) = build_cart(&mut app);
    let y0 = app.world().get::<Position>(bodies[0]).unwrap().0.y;
    let v = run(&mut app, 4.0, &bodies);
    let y1 = app.world().get::<Position>(bodies[0]).unwrap().0.y;
    assert!(
        (y1 - y0).abs() < 0.5 && y1 > 0.3,
        "physics must run and the cart must rest on its wheels: y {y0:.2} → {y1:.2}"
    );
    assert!(v < 6.0, "assembly blew up at rest: max speed {v:.1} m/s");
}

#[test]
fn four_wheel_cart_drives_stably() {
    let mut app = app();
    let (engine, bodies) = build_cart(&mut app);
    run(&mut app, 1.0, &bodies);
    let x0 = app.world().get::<Position>(engine).unwrap().0;
    app.world_mut().get_mut::<Engine>(engine).unwrap().on = true;
    let v = run(&mut app, 4.0, &bodies);
    let x1 = app.world().get::<Position>(engine).unwrap().0;
    let moved = Vec2::new(x1.x - x0.x, x1.z - x0.z).length();
    assert!(v < 15.0, "driving blew up: max speed {v:.1} m/s");
    assert!(moved > 2.0, "cart should drive: moved {moved:.2} m");
}

#[test]
fn impulse_wakes_a_sleeping_body() {
    let mut app = app();
    app.add_plugins(crate::physics::impulses::ImpulsesPlugin);
    let coords: Vec<IVec3> = (-5..5)
        .flat_map(|x| (-5..5).map(move |z| IVec3::new(x, -1, z)))
        .collect();
    app.world_mut()
        .spawn((RigidBody::Static, Collider::voxels(Vec3::ONE, &coords)));
    let crate_e = body(
        &mut app,
        PartKind::Crate,
        Transform::from_xyz(0.5, 0.6, 0.5),
    );
    // Let it settle and fall asleep.
    run(&mut app, 6.0, &[crate_e]);
    let asleep = app.world().get::<Sleeping>(crate_e).is_some();
    assert!(asleep, "precondition: the crate should be asleep");
    let y0 = app.world().get::<Position>(crate_e).unwrap().0.y;
    app.world_mut()
        .resource_mut::<crate::physics::impulses::ImpulseQueue>()
        .push(crate_e, Vec3::Y * 6.0);
    let mut peak: f32 = 0.0;
    for _ in 0..30 {
        app.update();
        peak = peak.max(app.world().get::<Position>(crate_e).unwrap().0.y - y0);
    }
    assert!(
        peak > 0.4,
        "impulse must lift the body (was asleep: {asleep}); rose {peak:.2} m"
    );
}
