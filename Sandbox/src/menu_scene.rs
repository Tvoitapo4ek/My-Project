//! The main menu's background: a small workshop diorama (an inline diesel
//! turning a gearbox and cogs, a car on its suspension, tanks and pipes,
//! storages and item tubes, a winch, a lamp on a wire bus), built far out
//! in the void while the menu is open; the camera circles it slowly.
//! It is a plain static grid: not part of any saved world.

use bevy::prelude::*;

use crate::{
    content::BlockId,
    core::Ui,
    grid::{Cell, VoxelGrid},
};

/// Where the diorama stands (far from the proving ground).
pub const AT: Vec3 = Vec3::new(400.0, 10.0, 400.0);

#[derive(Component)]
pub struct MenuScene;

fn put(g: &mut VoxelGrid, x: i32, y: i32, z: i32, key: &str, facing: u8) {
    if let Some(id) = crate::content::registry().block(key) {
        g.set(IVec3::new(x, y, z), Cell::new(id, facing));
    }
}

/// The blocks of the diorama.
pub fn diorama() -> VoxelGrid {
    let mut g = VoxelGrid::default();
    let floor = BlockId::k("concrete");
    let line = BlockId::k("concrete_line");
    for x in -9..=9 {
        for z in -7..=7 {
            let id = if x == 0 || z == 0 { line } else { floor };
            g.set(IVec3::new(x, -1, z), Cell::new(id, 2));
        }
    }
    // Three-module diesel → shaft → gearbox → cogs.
    for z in -4..=-2 {
        put(&mut g, -6, 0, z, "diesel_engine", 4);
    }
    put(&mut g, -6, 0, -1, "shaft", 4);
    put(&mut g, -6, 0, 0, "gearbox", 4);
    put(&mut g, -5, 0, 0, "shaft", 0);
    put(&mut g, -4, 0, 0, "cogwheel", 0);
    put(&mut g, -4, 1, 0, "cogwheel", 0);
    put(&mut g, -3, 1, 0, "large_cogwheel", 0);
    // A diesel tank feeding the engines.
    put(&mut g, -8, 0, -3, "fluid_tank", 2);
    put(&mut g, -8, 1, -3, "fluid_tank", 2);
    put(&mut g, -7, 0, -3, "fluid_pipe", 0);
    // The car: chassis, seat, panel, four wheels on suspensions.
    for x in 1..=4 {
        for z in -1..=1 {
            put(&mut g, x, 1, z, "steel_frame", 2);
        }
    }
    put(&mut g, 2, 2, 0, "seat", 0);
    put(&mut g, 3, 2, 0, "control_panel", 1);
    put(&mut g, 4, 2, -1, "headlight", 0);
    put(&mut g, 4, 2, 1, "headlight", 0);
    for (x, z, f) in [(1, -2, 5u8), (4, -2, 5), (1, 2, 4), (4, 2, 4)] {
        put(&mut g, x, 1, z, "suspension", f);
        let dz = if f == 4 { 1 } else { -1 };
        put(&mut g, x, 1, z + dz, "wheel_large", f);
    }
    // Storage: chest and barrel joined by a tube through a drive.
    put(&mut g, 6, 0, -5, "chest", 4);
    put(&mut g, 7, 0, -5, "item_tube", 0);
    put(&mut g, 8, 0, -5, "item_drive", 0);
    put(&mut g, 8, 0, -4, "cogwheel", 0);
    put(&mut g, 7, 0, -4, "creative_motor", 0);
    put(&mut g, 9, 0, -5, "barrel", 2);
    for x in 6..=8 {
        put(&mut g, x, 0, 5, "item_container", 2);
        put(&mut g, x, 1, 5, "item_container", 2);
    }
    // A gantry with a winch.
    for y in 0..5 {
        put(&mut g, -8, y, 5, "steel_block", 2);
    }
    put(&mut g, -7, 4, 5, "steel_block", 2);
    put(&mut g, -6, 4, 5, "steel_block", 2);
    put(&mut g, -6, 3, 5, "winch", 3);
    // A lever and a lamp on a short wire bus.
    put(&mut g, -2, 0, 5, "lever", 3);
    for x in -1..=1 {
        put(&mut g, x, 0, 5, "wire", 2);
    }
    put(&mut g, 2, 0, 5, "lamp", 2);
    g
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Menu diorama"), MenuScene, diorama(), crate::tools::glue::Glue::default(), crate::chains::Chains::default(), Transform::from_translation(AT), Visibility::default(), avian3d::prelude::RigidBody::Static));
}

/// Fill the engines and switch the lever on once the blocks exist.
fn liven(scene: Query<&VoxelGrid, With<MenuScene>>, mut stores: Query<&mut crate::fluids::FluidStore>, mut logic: Query<&mut crate::logic::LogicState>, mut params: Query<(&crate::mech::BlockEntity, &mut crate::mech::Params)>) {
    let Ok(g) = scene.single() else { return };
    for (c, e) in &g.entities {
        if let Ok(mut s) = stores.get_mut(*e)
            && s.amount < 100.0
        {
            s.add(crate::fluids::Fluid::Diesel, 200.0);
        }
        if g.get(*c).id.def().key == "lever"
            && let Ok(mut l) = logic.get_mut(*e)
        {
            l.memory = true;
        }
        if let Ok((b, mut p)) = params.get_mut(*e)
            && b.id.def().key == "creative_motor"
            && let Some((i, _)) = b.id.def().param("speed")
            && p.0[i] != 24.0
        {
            p.0[i] = 24.0;
        }
    }
}

fn despawn(mut commands: Commands, q: Query<Entity, With<MenuScene>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// The menu camera: a slow circle around the diorama.
pub fn camera(t: f32) -> Transform {
    let a = t * 0.06 + 0.8;
    let p = AT + Vec3::new(a.cos() * 17.0, 7.5 + (t * 0.11).sin() * 1.0, a.sin() * 17.0);
    Transform::from_translation(p).looking_at(AT + Vec3::new(0.0, 0.5, 0.0), Vec3::Y)
}

pub struct MenuScenePlugin;

impl Plugin for MenuScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Ui::Menu), spawn).add_systems(OnExit(Ui::Menu), despawn).add_systems(Update, liven.run_if(in_state(Ui::Menu)));
    }
}
