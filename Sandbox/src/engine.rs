//! Diesel engine: a motor that needs fuel and air.
//!
//! * Burns `BURN` mB of diesel per second from its own small tank, which
//!   refills from the pipe network it is connected to (any side but the
//!   output shaft at its front).
//! * Needs air: the exhaust on top must be free (no block above) and out
//!   of the water. Otherwise it stalls.
//! * While running it is a source in the kinetic network (speed and
//!   direction with the wrench, like the creative motor), puffs exhaust
//!   smoke and rumbles (`sound`).

use bevy::{light::NotShadowCaster, prelude::*};

use crate::{
    content::Mech,
    fluids::{Fluid, FluidStore},
    grid::VoxelGrid,
    mech::{BlockEntity, MechSet, Params},
    style::{BoxCache, Materials},
    water::Water,
};

/// Diesel burnt while running (mB/s).
pub const BURN: f32 = 10.0;

/// Why an engine is not running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stall {
    Off,
    NoFuel,
    NoAir,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Engine {
    pub running: bool,
    pub stall: Option<Stall>,
    puff: f32,
}

impl Default for Engine {
    fn default() -> Self {
        Self { running: false, stall: Some(Stall::NoFuel), puff: 0.0 }
    }
}

/// Runs or not: wanted speed, fuel, free exhaust, exhaust out of the water.
pub fn state(speed: f32, fuel: f32, exhaust_free: bool, exhaust_depth: f32) -> Option<Stall> {
    if speed <= 0.0 {
        Some(Stall::Off)
    } else if fuel <= 0.0 {
        Some(Stall::NoFuel)
    } else if !exhaust_free || exhaust_depth > 0.0 {
        Some(Stall::NoAir)
    } else {
        None
    }
}

fn add_engines(mut commands: Commands, added: Query<(Entity, &BlockEntity), (Added<BlockEntity>, Without<Engine>)>) {
    for (e, b) in &added {
        if b.mech() == Some(Mech::DieselEngine) {
            commands.entity(e).insert(Engine::default());
        }
    }
}

/// Exhaust top in world space.
fn exhaust(gt: &GlobalTransform, b: &BlockEntity) -> Vec3 {
    gt.transform_point(b.cell.as_vec3() + Vec3::new(0.5, 1.05, 0.5))
}

fn run(time: Res<Time>, water: Res<Water>, grids: Query<(&VoxelGrid, &GlobalTransform)>, mut engines: Query<(&BlockEntity, &Params, &mut FluidStore, &mut Engine)>) {
    let dt = time.delta_secs();
    for (b, p, mut fuel, mut eng) in &mut engines {
        let Ok((grid, gt)) = grids.get(b.grid) else { continue };
        let diesel = if fuel.fluid == Some(Fluid::Diesel) { fuel.amount } else { 0.0 };
        let free = grid.get(b.cell + IVec3::Y).is_air();
        let stall = state(p.get(b.id, "speed"), diesel, free, water.depth(exhaust(gt, b)));
        eng.running = stall.is_none();
        eng.stall = stall;
        if eng.running {
            fuel.take(Fluid::Diesel, BURN * dt);
        }
    }
}

#[derive(Component)]
struct Smoke {
    age: f32,
    vel: Vec3,
}

const SMOKE_LIFE: f32 = 2.2;

#[allow(clippy::too_many_arguments)]
fn smoke(
    mut commands: Commands,
    time: Res<Time>,
    grids: Query<&GlobalTransform, With<VoxelGrid>>,
    mut engines: Query<(&BlockEntity, &mut Engine)>,
    mut puffs: Query<(Entity, &mut Smoke, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BoxCache>,
    mats: Option<Res<Materials>>,
) {
    let Some(mats) = mats else { return };
    let dt = time.delta_secs();
    let count = puffs.iter().count();
    for (b, mut eng) in &mut engines {
        if !eng.running {
            continue;
        }
        eng.puff -= dt;
        if eng.puff > 0.0 || count > 240 {
            continue;
        }
        eng.puff = 0.18;
        let Ok(gt) = grids.get(b.grid) else { continue };
        let at = exhaust(gt, b);
        let r = (time.elapsed_secs() * 12.9 + b.cell.x as f32 * 3.1).sin();
        commands.spawn((
            Smoke { age: 0.0, vel: Vec3::new(r * 0.25, 1.1, (r * 1.7).cos() * 0.25) },
            Mesh3d(cache.get(&mut meshes, Vec3::splat(0.16))),
            MeshMaterial3d(mats.get("smoke")),
            Transform::from_translation(at),
            NotShadowCaster,
        ));
    }
    for (e, mut s, mut t) in &mut puffs {
        s.age += dt;
        if s.age > SMOKE_LIFE {
            commands.entity(e).despawn();
            continue;
        }
        t.translation += s.vel * dt;
        s.vel *= 1.0 - 0.6 * dt;
        // Grows, then thins out.
        let k = s.age / SMOKE_LIFE;
        t.scale = Vec3::splat((1.0 + k * 3.0) * (1.0 - k * k));
    }
}

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_engines, run).chain().before(MechSet)).add_systems(Update, smoke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_needs_fuel_and_air() {
        assert_eq!(state(64.0, 500.0, true, -1.0), None);
        assert_eq!(state(64.0, 0.0, true, -1.0), Some(Stall::NoFuel));
        assert_eq!(state(64.0, 500.0, false, -1.0), Some(Stall::NoAir), "exhaust blocked");
        assert_eq!(state(64.0, 500.0, true, 0.5), Some(Stall::NoAir), "exhaust under water");
        assert_eq!(state(0.0, 500.0, true, -1.0), Some(Stall::Off));
    }
}
