//! Water: the test pool of the proving ground, swimming, underwater fog and
//! buoyancy of physical contraptions (Archimedes).
//!
//! * Water is a volume (not blocks): `Water::depth(point)`.
//! * Every block of a contraption under water is pushed up by the water it
//!   displaces: `buoyancy` in `blocks.ron` (kg of lift when fully under
//!   water; default 40 % of the block's mass, so solid machinery sinks,
//!   while planks, glass, frames and empty tanks float). Fluid inside tanks
//!   adds its weight, so filling ballast tanks with water makes a
//!   submarine sink.
//! * Water slows bodies down (drag), more vertically than horizontally.

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::{
    content::BlockDef,
    contraption::Contraption,
    fluids::FluidStore,
    grid::VoxelGrid,
};

/// Water volumes: (min corner, max corner); the top is the surface.
#[derive(Resource, Debug, Clone, Default)]
pub struct Water {
    pub pools: Vec<(Vec3, Vec3)>,
}

impl Water {
    /// How deep a point is under the surface (≤ 0 out of the water).
    pub fn depth(&self, p: Vec3) -> f32 {
        self.pools
            .iter()
            .filter(|(lo, hi)| p.x >= lo.x && p.x <= hi.x && p.z >= lo.z && p.z <= hi.z && p.y >= lo.y)
            .map(|(_, hi)| hi.y - p.y)
            .fold(f32::MIN, f32::max)
            .max(-1.0)
    }
}

/// Lift of a fully submerged block (kg).
pub fn lift(def: &BlockDef) -> f32 {
    def.buoyancy.unwrap_or(def.mass * 0.4)
}

/// Share of a block (centre `p`, ±0.5) under water.
pub fn submerged(depth_of_centre: f32) -> f32 {
    (depth_of_centre + 0.5).clamp(0.0, 1.0)
}

/// Water drag per submerged block (N·s/m).
const DRAG_UP: f32 = 300.0;
const DRAG_SIDE: f32 = 60.0;

fn buoyancy(water: Res<Water>, gravity: Res<Gravity>, mut bodies: Query<(&VoxelGrid, &GlobalTransform, &ComputedMass, Forces), With<Contraption>>, stores: Query<&FluidStore>) {
    let g = gravity.0.length();
    for (grid, gt, mass, mut forces) in &mut bodies {
        if !mass.value().is_finite() || mass.value() < 1.0 {
            continue;
        }
        let t = gt.compute_transform();
        for (c, cell) in grid.cells() {
            let p = t.transform_point(c.as_vec3() + Vec3::splat(0.5));
            // Fluid in tanks weighs.
            if let Some(s) = grid.entities.get(&c).and_then(|e| stores.get(*e).ok())
                && let Some(f) = s.fluid
            {
                forces.apply_force_at_point(Vec3::NEG_Y * s.amount * f.kg_per_mb() * g, p);
            }
            let k = submerged(water.depth(p));
            if k <= 0.0 {
                continue;
            }
            forces.apply_force_at_point(Vec3::Y * lift(cell.id.def()) * g * k, p);
            let v = forces.velocity_at_point(p);
            let drag = Vec3::new(v.x * DRAG_SIDE, v.y * DRAG_UP, v.z * DRAG_SIDE) * -k;
            forces.apply_force_at_point(drag, p);
        }
    }
}

/// A flat water surface over each pool (seen from both sides).
fn spawn_surfaces(mut commands: Commands, water: Res<Water>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
    use crate::grid::render::{TILE, load_texture, pixel_image};
    let mut img = pixel_image(TILE, TILE, load_texture("water"));
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::Repeat, address_mode_v: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::nearest() });
    let mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.9, 1.0, 0.62),
        base_color_texture: Some(images.add(img)),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.12,
        reflectance: 0.5,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    for (lo, hi) in &water.pools {
        let (x0, x1, z0, z1, y) = (lo.x, hi.x, lo.z, hi.z, hi.y);
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]]);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4]);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]);
        m.insert_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]));
        commands.spawn((Name::new("Water surface"), Mesh3d(meshes.add(m)), MeshMaterial3d(mat.clone()), Transform::default(), bevy::light::NotShadowCaster));
    }
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Water { pools: vec![crate::world::pool()] })
            .add_systems(Startup, spawn_surfaces)
            .add_systems(FixedUpdate, buoyancy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    #[test]
    fn archimedes() {
        let w = Water { pools: vec![(Vec3::new(0.0, -10.0, 0.0), Vec3::new(10.0, -1.0, 10.0))] };
        assert_eq!(w.depth(Vec3::new(5.0, -3.0, 5.0)), 2.0);
        assert!(w.depth(Vec3::new(20.0, -3.0, 5.0)) <= 0.0, "outside the pool");
        assert!(w.depth(Vec3::new(5.0, 0.0, 5.0)) <= 0.0, "above the surface");
        // Half a block under the surface → half the lift.
        assert_eq!(submerged(0.0), 0.5);
        assert_eq!(submerged(2.0), 1.0);
        // Planks and empty tanks float, solid steel sinks.
        let floats = |k: &str| lift(BlockId::k(k).def()) > BlockId::k(k).def().mass;
        assert!(floats("planks") && floats("fluid_tank") && floats("steel_frame"));
        assert!(!floats("steel_block") && !floats("creative_motor"));
        // A full water tank block is neutral: its water weighs what it lifts.
        let full = crate::fluids::TANK * crate::fluids::Fluid::Water.kg_per_mb();
        assert_eq!(full, 1000.0);
    }
}
