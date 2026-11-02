//! Stage 7 checks: needs, eating, drinking, fall damage, creature AI,
//! melee / ranged / magic damage, creature death and player respawn.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    scenario::{player_pos, step, teleport},
    scenario3::{aim_at, inv, key, mouse, select, with_inv},
    scenario4::pos_of,
    scenario5::field,
    screenshot, Ctx, Step, StepFn,
};
use crate::{
    core::resources::SpawnPoint,
    items::{drops::ItemDrop, ItemId},
    magic::spells::{AetherReserve, Spell, SpellState},
    player::Player,
    survival::{
        combat::WeaponCooldown,
        creatures::{spawn_hollowling, Creature},
        damage::{Damage, DamageSource, Health},
        needs::Vitals,
    },
    world::chunks::VoxelWorld,
};

fn player_e(world: &mut World) -> Entity {
    world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap()
}

fn vitals(world: &mut World) -> Vitals {
    let e = player_e(world);
    world.get::<Vitals>(e).cloned().unwrap()
}

fn hp(world: &World, e: Entity) -> f32 {
    world.get::<Health>(e).map(|h| h.hp).unwrap_or(-1.0)
}

fn spawn_creature(world: &mut World, at: Vec3) -> Entity {
    let mut c = world.commands();
    let e = spawn_hollowling(&mut c, at);
    world.flush();
    e
}

/// Aims at a target entity for a while and clicks once (time-robust).
fn aim_and_click(world: &mut World, ctx: &mut Ctx, target: Entity, button: MouseButton) -> bool {
    let p = pos_of(world, target);
    aim_at(world, p);
    if ctx.frames < 10 || ctx.t < 0.8 {
        return false;
    }
    if !ctx.vars.contains_key("clicked") {
        ctx.vars.insert("clicked".into(), ctx.t);
        mouse(world, button, true);
        return false;
    }
    mouse(world, button, false);
    ctx.t > ctx.vars["clicked"] + 0.5
}

pub fn stage7(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("needs decrease over time", |world, ctx| {
        if ctx.frames == 1 {
            let v = vitals(world);
            ctx.vars.insert("h0".into(), v.hunger);
            ctx.vars.insert("t0".into(), v.thirst);
        }
        if ctx.t > 2.0 {
            let v = vitals(world);
            ctx.check("hunger and thirst drop slowly", v.hunger < ctx.vars["h0"] && v.thirst < ctx.vars["t0"] && ctx.vars["h0"] - v.hunger < 1.0, format!("hunger {:.3}→{:.3}", ctx.vars["h0"], v.hunger));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("eat berries", |world, ctx| {
        if ctx.frames == 1 {
            let e = player_e(world);
            world.get_mut::<Vitals>(e).unwrap().hunger = 40.0;
            with_inv(world, |i| {
                i.add(ItemId::k("berries"), 2);
            });
            select(world, "berries");
            ctx.vars.insert("b0".into(), inv(world).count(ItemId::k("berries")) as f32);
        }
        if ctx.frames == 5 {
            mouse(world, MouseButton::Right, true);
        }
        if ctx.frames == 6 {
            mouse(world, MouseButton::Right, false);
        }
        if ctx.frames == 10 {
            let v = vitals(world);
            let b = inv(world).count(ItemId::k("berries")) as f32;
            ctx.check("eating restores hunger and uses the item", v.hunger > 50.0 && b == ctx.vars["b0"] - 1.0, format!("hunger {:.1}, berries {b}", v.hunger));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("drink from a lake", |world, ctx| {
        if ctx.frames == 1 {
            let e = player_e(world);
            world.get_mut::<Vitals>(e).unwrap().thirst = 30.0;
            let w = world.resource::<VoxelWorld>();
            let (size, sea) = (w.size_blocks(), w.sea_level);
            let mut found = None;
            'o: for z in (8..size.z - 8).step_by(2) {
                for x in (8..size.x - 8).step_by(2) {
                    if (0..2).all(|d| w.get(IVec3::new(x, sea - d, z)).is_liquid()) {
                        found = Some(Vec3::new(x as f32 + 0.5, sea as f32 + 0.6, z as f32 + 0.5));
                        break 'o;
                    }
                }
            }
            let p = found.unwrap();
            teleport(world, p);
            ctx.vars.insert("wx".into(), p.x);
            ctx.vars.insert("wy".into(), p.y);
            ctx.vars.insert("wz".into(), p.z);
        }
        let water = Vec3::new(ctx.vars["wx"] + 1.0, ctx.vars["wy"] - 1.0, ctx.vars["wz"]);
        aim_at(world, water);
        if ctx.frames == 15 {
            key(world, KeyCode::KeyE, true);
        }
        if ctx.frames == 16 {
            key(world, KeyCode::KeyE, false);
        }
        if ctx.frames == 20 {
            let v = vitals(world);
            ctx.check("drinking water restores thirst", v.thirst > 50.0, format!("thirst {:.1}", v.thirst));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("fall damage", |world, ctx| {
        if ctx.frames == 1 {
            let s = world.resource::<SpawnPoint>().0;
            let e = player_e(world);
            world.get_mut::<Health>(e).unwrap().hp = 100.0;
            teleport(world, s + Vec3::Y * 14.0);
        }
        if ctx.t > 3.0 {
            let e = player_e(world);
            let h = hp(world, e);
            ctx.check("falling 14 m hurts", h < 95.0 && h > 0.0, format!("hp {h:.1}"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("creature notices, chases and bites", |world, ctx| {
        if ctx.frames == 1 {
            let f = field(world);
            teleport(world, f + Vec3::new(0.0, 2.0, 3.0));
            let e = player_e(world);
            world.get_mut::<Health>(e).unwrap().hp = 100.0;
            let c = spawn_creature(world, f + Vec3::new(0.0, 2.0, -5.0));
            ctx.entities.insert("c1".into(), c);
            ctx.vars.insert("d0".into(), 9.0);
        }
        let c = ctx.entities["c1"];
        let pe = player_e(world);
        if ctx.t > 8.0 || hp(world, pe) < 100.0 && ctx.t > 1.0 {
            let pe = player_e(world);
            let d = pos_of(world, c).distance(pos_of(world, pe));
            ctx.check("creature closes in", d < 4.0, format!("distance {d:.1}"));
            let h = hp(world, pe);
            ctx.check("creature bites the player", h < 100.0, format!("hp {h:.1}"));
            screenshot(world, "16_creature");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("melee hit", |world, ctx| {
        let c = ctx.entities["c1"];
        if ctx.frames == 1 {
            select(world, "blade");
            world.resource_mut::<WeaponCooldown>().0 = 0.0;
            ctx.vars.insert("ch0".into(), hp(world, c));
            ctx.vars.remove("clicked");
        }
        // Stay close to it.
        if ctx.frames < 10 {
            let p = pos_of(world, c);
            let pe = player_e(world);
            let me = pos_of(world, pe);
            let back = (me - p).normalize_or_zero();
            teleport(world, p + Vec3::new(back.x, 0.0, back.z).normalize_or_zero() * 1.8 + Vec3::Y * 0.5);
        }
        if aim_and_click(world, ctx, c, MouseButton::Left) {
            let now = hp(world, c);
            ctx.check("blade damages the creature", now < ctx.vars["ch0"], format!("{:.0} → {now:.0}", ctx.vars["ch0"]));
            ctx.vars.remove("clicked");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("ranged hit", |world, ctx| {
        if ctx.frames == 1 {
            // Fresh target at a distance, player out of its reach (on a pillar-free spot).
            let f = field(world);
            teleport(world, f + Vec3::new(4.0, 2.0, 5.0));
            let c = spawn_creature(world, f + Vec3::new(4.0, 2.0, -3.0));
            ctx.entities.insert("c2".into(), c);
            select(world, "arc_caster");
            world.resource_mut::<WeaponCooldown>().0 = 0.0;
            with_inv(world, |i| {
                i.add(ItemId::k("bolt"), 5);
            });
            ctx.vars.insert("bolts".into(), inv(world).count(ItemId::k("bolt")) as f32);
            ctx.vars.remove("clicked");
        }
        let c = ctx.entities["c2"];
        if ctx.frames == 2 {
            ctx.vars.insert("ch0".into(), hp(world, c));
        }
        if aim_and_click(world, ctx, c, MouseButton::Left) {
            if ctx.t < ctx.vars["clicked"] + 1.5 {
                return Step::Wait;
            }
            let now = hp(world, c);
            let bolts = inv(world).count(ItemId::k("bolt")) as f32;
            ctx.check("bolt hits the creature", now < ctx.vars["ch0"], format!("{:.0} → {now:.0}", ctx.vars["ch0"]));
            ctx.check("shooting uses a bolt", bolts == ctx.vars["bolts"] - 1.0, format!("bolts {bolts}"));
            ctx.vars.remove("clicked");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("magic discharge kills the creature", |world, ctx| {
        let c = ctx.entities["c2"];
        if ctx.frames == 1 {
            select(world, "aether_focus");
            world.resource_mut::<SpellState>().selected = Spell::Discharge;
            world.resource_mut::<SpellState>().cooldown = 0.0;
            let e = player_e(world);
            world.get_mut::<AetherReserve>(e).unwrap().amount = 100.0;
            if let Some(mut h) = world.get_mut::<Health>(c) {
                h.hp = 20.0;
            }
            ctx.vars.remove("clicked");
            let mut q = world.query::<&ItemDrop>();
            ctx.vars.insert("drops0".into(), q.iter(world).filter(|d| d.stack.item == ItemId::k("agitated_shard")).count() as f32);
        }
        if world.get_entity(c).is_err() || world.get::<Creature>(c).is_none() {
            let mut q = world.query::<&ItemDrop>();
            let drops = q.iter(world).filter(|d| d.stack.item == ItemId::k("agitated_shard")).count() as f32;
            ctx.check("creature dies from magic", true, "despawned");
            ctx.check("creature drops loot", drops > ctx.vars["drops0"], format!("{drops} shard drops"));
            return Step::Next;
        }
        if aim_and_click(world, ctx, c, MouseButton::Left) {
            if ctx.t > 8.0 {
                ctx.check("creature dies from magic", false, format!("hp {}", hp(world, c)));
                return Step::Next;
            }
            // Missed a moving target: try again.
            ctx.vars.remove("clicked");
            world.resource_mut::<SpellState>().cooldown = 0.0;
        }
        Step::Wait
    }));
    steps.push(step("player death and respawn", |world, ctx| {
        if ctx.frames == 1 {
            // Remove remaining creatures so they cannot bite the respawned player.
            let mut q = world.query_filtered::<Entity, With<Creature>>();
            let all: Vec<Entity> = q.iter(world).collect();
            for c in all {
                world.despawn(c);
            }
        }
        if ctx.frames == 3 {
            let e = player_e(world);
            world.write_message(Damage { target: e, amount: 500.0, source: DamageSource::Fall });
        }
        if ctx.frames == 8 {
            let e = player_e(world);
            let spawn = world.resource::<SpawnPoint>().0;
            let d = player_pos(world).unwrap().distance(spawn);
            ctx.check("dead player respawns with full health", hp(world, e) == 100.0 && d < 2.0, format!("hp {}, {d:.1} m from spawn", hp(world, e)));
            let _ = world.query::<&RigidBody>();
            return Step::Next;
        }
        Step::Wait
    }));
}
