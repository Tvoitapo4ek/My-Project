use glam::Vec3;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

use crate::Physics;

/// Персонаж-капсула. Умеет стоять и ходить по движущимся кораблям:
/// скорость опоры под ногами добавляется к собственной.
pub struct Character {
    pub body: RigidBodyHandle,
    pub radius: f32,
    pub half_height: f32,
    pub on_ground: bool,
}

impl Character {
    /// `half_height` — половина цилиндрической части капсулы.
    pub fn spawn(phys: &mut Physics, pos: Vec3, radius: f32, half_height: f32, mass: f32) -> Self {
        let body = RigidBodyBuilder::dynamic().translation(pos).lock_rotations().ccd_enabled(true).can_sleep(false);
        let collider = ColliderBuilder::capsule_y(half_height, radius)
            .mass(mass)
            .friction(0.0)
            .friction_combine_rule(CoefficientCombineRule::Min);
        let (body, _) = phys.world.insert(body, collider);
        Character { body, radius, half_height, on_ground: false }
    }

    /// Центр капсулы.
    pub fn position(&self, phys: &Physics) -> Vec3 {
        phys.world.bodies[self.body].translation()
    }

    pub fn velocity(&self, phys: &Physics) -> Vec3 {
        phys.world.bodies[self.body].linvel()
    }

    pub fn teleport(&self, phys: &mut Physics, pos: Vec3) {
        let b = &mut phys.world.bodies[self.body];
        b.set_translation(pos, true);
        b.set_linvel(Vec3::ZERO, true);
    }

    /// Вызывать перед каждым шагом физики.
    /// * `wish` — желаемая горизонтальная скорость в мире;
    /// * `jump` — скорость прыжка (если прыгаем);
    /// * `fly` — `Some(вертикальная скорость)` в режиме полёта.
    pub fn drive(&mut self, phys: &mut Physics, wish: Vec3, jump: Option<f32>, fly: Option<f32>) {
        let pos = self.position(phys);
        let feet = pos - Vec3::Y * (self.half_height + self.radius);

        // Опора: шар чуть меньше капсулы, опускаем вниз.
        let probe = Ball::new(self.radius * 0.9);
        let opts = ShapeCastOptions::with_max_time_of_impact(self.half_height + self.radius * 0.1 + 0.08);
        let filter = QueryFilter::default().exclude_rigid_body(self.body);
        let ground = phys.world.cast_shape(&Pose::from_translation(pos), Vec3::NEG_Y, &probe, opts, filter);
        let support = ground.and_then(|(c, _)| phys.world.colliders[c].parent());
        self.on_ground = support.is_some();
        let base = support.map_or(Vec3::ZERO, |b| phys.world.bodies[b].velocity_at_point(feet));

        let dt = phys.world.integration_parameters.dt;
        let body = &mut phys.world.bodies[self.body];
        let mut vel = body.linvel();
        // Разгон ограничен, как трение ног. Если задавать скорость напрямую, персонаж
        // в контакте ведёт себя как тело бесконечной массы и раскачивает корабль.
        let accel = if self.on_ground || fly.is_some() { 60.0 } else { 10.0 };
        let target = base + Vec3::new(wish.x, 0.0, wish.z);
        let dv = Vec3::new(target.x - vel.x, 0.0, target.z - vel.z).clamp_length_max(accel * dt);
        vel += dv;
        match (fly, jump) {
            (Some(vy), _) => vel.y = vy,
            (None, Some(j)) if self.on_ground => vel.y = base.y + j,
            _ => {}
        }
        body.set_gravity_scale(if fly.is_some() { 0.0 } else { 1.0 }, true);
        body.set_linvel(vel, true);
    }
}
