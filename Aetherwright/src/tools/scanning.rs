//! Scanner: produces a detailed description of a block or object. Other
//! subsystems describe their entities through the `Scannable` component
//! (kept up to date by them), so the scanner needs no knowledge of them.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::manipulator::ScanRequest;
use crate::world::chunks::VoxelWorld;

/// Description of an entity for the scanner and the target info panel.
#[derive(Component, Debug, Clone, Default)]
pub struct Scannable {
    pub title: String,
    pub info: String,
}

/// Last scan result shown by the HUD.
#[derive(Resource, Debug, Clone, Default)]
pub struct ScanResult {
    pub title: String,
    pub lines: Vec<String>,
    /// Seconds remaining on screen.
    pub ttl: f32,
}

/// Extra lines appended to block scans by other systems (e.g. the local
/// aether field from `magic`). Function pointer keeps modules decoupled.
#[derive(Resource, Default)]
pub struct ScanExtras(pub Vec<fn(&World, Vec3) -> Option<String>>);

fn handle_scans(world: &mut World) {
    let requests: Vec<ScanRequest> = world.resource_mut::<Messages<ScanRequest>>().drain().collect();
    for r in requests {
        let mut result = ScanResult { ttl: 8.0, ..default() };
        let probe_point;
        match r {
            ScanRequest::Block(pos) => {
                let b = world.resource::<VoxelWorld>().get(pos);
                let d = b.def();
                result.title = d.name.to_string();
                result.lines.push(d.info.to_string());
                if d.hardness.is_finite() {
                    result.lines.push(format!("Твёрдость: {:.1}  ·  уровень бурения: {}", d.hardness, d.tier));
                } else {
                    result.lines.push("Неразрушимо".into());
                }
                if let Some(item) = crate::items::block_drop(b) {
                    result.lines.push(format!("Даёт: {}", item.def().name));
                }
                if d.aether > 0.0 {
                    result.lines.push(format!("Излучение эфира: {:.1} ед/с", d.aether));
                }
                result.lines.push(format!("Координаты: {} {} {}", pos.x, pos.y, pos.z));
                probe_point = Some(pos.as_vec3() + Vec3::splat(0.5));
            }
            ScanRequest::Entity(e) => {
                let Ok(er) = world.get_entity(e) else { continue };
                if let Some(s) = er.get::<Scannable>() {
                    result.title = s.title.clone();
                    result.lines.extend(s.info.lines().map(str::to_string));
                } else if let Some(n) = er.get::<Name>() {
                    result.title = n.as_str().to_string();
                } else {
                    result.title = "Объект".into();
                }
                if let Some(m) = er.get::<ComputedMass>() {
                    result.lines.push(format!("Масса: {:.1} кг", m.value()));
                }
                if let Some(v) = er.get::<LinearVelocity>() {
                    result.lines.push(format!("Скорость: {:.1} м/с", v.0.length()));
                }
                if let Some(rb) = er.get::<RigidBody>() {
                    result.lines.push(format!("Тело: {}", if rb.is_static() { "закреплено" } else { "свободное" }));
                }
                probe_point = er.get::<Transform>().map(|t| t.translation);
            }
        }
        if let Some(p) = probe_point {
            let extras: Vec<_> = world.resource::<ScanExtras>().0.clone();
            for f in extras {
                if let Some(line) = f(world, p) {
                    result.lines.push(line);
                }
            }
        }
        world.insert_resource(result);
    }
}

fn tick(time: Res<Time>, mut r: ResMut<ScanResult>) {
    r.ttl = (r.ttl - time.delta_secs()).max(0.0);
}

pub struct ScanningPlugin;

impl Plugin for ScanningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScanResult>()
            .init_resource::<ScanExtras>()
            .add_systems(PostUpdate, handle_scans)
            .add_systems(Update, tick);
    }
}
