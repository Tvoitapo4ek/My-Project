//! T3 checks: the holographic device. Clicks go through the real pipeline:
//! a screen position is computed for a UI node on a 3D panel, the cursor is
//! put there, the mouse button is pressed — the panel pointer ray, Bevy
//! picking and the section actions do the rest.

use bevy::{prelude::*, ui::UiGlobalTransform};

use super::{Step, StepFn, scenario::step, scenario4::pos_of, scenario5::field, screenshot};
use crate::{
    building::{blueprints::Blueprint, library::BlueprintLibrary, selection::Selection},
    core::{
        config::GameConfig,
        states::InputFocus,
    },
    holo::{
        HoloNav,
        device::{DeviceAnim, DeviceCommand, DeviceState},
        engineering::{EngineeringTarget, describe},
        panels::{HoloPanel, HoloRay, OverlayCamera, PanelRoot, panel_point},
        projection::FigurePart,
        sections::{
            NavButton,
            blueprints::BlueprintUi,
            inventory::InvUi,
            journal::{JournalState, JournalUi},
            map::{ExploredMap, MapMarkers},
        },
    },
    machinery::components::{PartAssets, PartKind, spawn_part},
    tools::scanning::Scanned,
    world::voxel::blocks,
};

/// Screen position of a UI node that lives on a holographic panel.
fn node_screen_pos(world: &mut World, node: Entity) -> Option<Vec2> {
    // Which panel: walk up to the PanelRoot.
    let mut root = node;
    while world.get::<PanelRoot>(root).is_none() {
        root = world.get::<ChildOf>(root)?.parent();
    }
    let slot = world.get::<PanelRoot>(root)?.0;
    let px = world.get::<UiGlobalTransform>(node)?.translation;
    let mut q = world.query::<(&HoloPanel, &GlobalTransform)>();
    let (panel, gt) = q.iter(world).find(|(p, _)| p.slot == slot)?;
    let p = panel_point(panel, gt, px);
    let mut cq = world.query_filtered::<(&Camera, &GlobalTransform), With<OverlayCamera>>();
    let (cam, cgt) = cq.iter(world).next()?;
    cam.world_to_viewport(cgt, p).ok()
}

/// Click script: frame 0 move, +2 press, +4 release. Returns true when done.
fn click(world: &mut World, frame: u32, start: u32, node: Entity) -> bool {
    let f = frame.saturating_sub(start);
    if frame < start {
        return false;
    }
    if f == 0 {
        let pos = node_screen_pos(world, node);
        world.resource_mut::<HoloRay>().cursor_override = pos;
    }
    let mut mouse = world.resource_mut::<ButtonInput<MouseButton>>();
    if f == 3 {
        mouse.press(MouseButton::Left);
    }
    if f == 6 {
        mouse.release(MouseButton::Left);
    }
    f >= 10
}

fn find<T: Component>(world: &mut World, pred: impl Fn(&T) -> bool) -> Option<Entity> {
    let mut q = world.query::<(Entity, &T)>();
    q.iter(world).find(|(_, c)| pred(c)).map(|(e, _)| e)
}

fn go(world: &mut World, id: &'static str) {
    world.resource_mut::<HoloNav>().go(id);
}

pub fn stage_t3(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("device opens with its animation", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Tab);
        }
        if ctx.frames == 2 {
            world.resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Tab);
        }
        if ctx.frames == 3 {
            let t = world.resource::<DeviceAnim>().t;
            ctx.vars.insert("t_early".into(), t);
        }
        let open = *world.resource::<DeviceState>() == DeviceState::Open;
        if (open && ctx.t > 1.0) || ctx.t > 8.0 {
            let focus = *world.resource::<State<InputFocus>>().get();
            ctx.check("Tab opens the device", open && focus == InputFocus::Device, format!("{focus:?}"));
            ctx.check("opening is animated", ctx.vars["t_early"] < 1.0, format!("t={}", ctx.vars["t_early"]));
            ctx.check("gameplay actions blocked while open", focus != InputFocus::Gameplay, "");
            let navs = world.query::<&NavButton>().iter(world).count();
            ctx.check("navigation lists the sections", navs >= 9, format!("{navs}"));
            screenshot(world, "24_holo_home");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("mouse click on a 3D panel switches the section", |world, ctx| {
        let Some(btn) = find::<NavButton>(world, |n| n.0 == "inventory") else {
            return if ctx.t > 5.0 { ctx.check("nav button exists", false, ""); Step::Next } else { Step::Wait };
        };
        if ctx.frames == 1 {
            ctx.check("button projects onto the screen", node_screen_pos(world, btn).is_some(), "");
        }
        let done = click(world, ctx.frames, 1, btn);
        if done && ctx.frames > 20 {
            let hit = world.resource::<HoloRay>().hit;
            let cur = world.resource::<HoloNav>().current;
            ctx.check("pointer ray hits the navigation panel", hit.is_some(), format!("{hit:?}"));
            ctx.check("click opened the inventory", cur == "inventory", cur);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("inventory slot is selected by clicking it", |world, ctx| {
        if ctx.frames == 1 {
            *world.resource_mut::<InvUi>() = InvUi::default();
        }
        if ctx.frames < 5 {
            return Step::Wait;
        }
        let Some(target) = crate::holo::sections::inventory::slot_entity(world, 0) else {
            ctx.check("slot node exists", false, "");
            return Step::Next;
        };
        if click(world, ctx.frames, 5, target) && ctx.frames > 22 {
            let sel = world.resource::<InvUi>().selected;
            ctx.check("slot selected through the panel", sel == Some(0), format!("{sel:?}"));
            screenshot(world, "25_holo_inventory");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("equipment shows the 3D figure", |world, ctx| {
        if ctx.frames == 1 {
            go(world, "equipment");
        }
        if ctx.t > 1.2 {
            let parts = world.query::<&FigurePart>().iter(world).count();
            ctx.check("character projection built", parts > 6, format!("{parts} parts"));
            screenshot(world, "26_holo_equipment");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("map projects explored terrain and markers", |world, ctx| {
        if ctx.frames == 1 {
            let p = {
                let mut q = world.query_filtered::<&Transform, With<crate::player::Player>>();
                q.iter(world).next().map(|t| t.translation).unwrap_or_default()
            };
            world.resource_mut::<MapMarkers>().0.push(crate::holo::sections::map::Marker { position: p + Vec3::X * 6.0, label: "Тест".into() });
            go(world, "map");
        }
        if ctx.t > 1.5 {
            let pct = world.resource::<ExploredMap>().percent();
            ctx.check("area around the player explored", pct > 0.5, format!("{pct:.1}%"));
            let saved = crate::save::world_save::capture(world);
            let has = saved.extensions.get("map").is_some_and(|v| v.to_string().contains("Тест"));
            ctx.check("markers and explored map go into the save", has, "");
            screenshot(world, "27_holo_map");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("journal entry opens by scanning", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<MapMarkers>().0.clear();
            world.resource_mut::<JournalState>().unlocked.remove("iron_ore");
            world.write_message(Scanned { block: Some("iron_ore".into()), title: "Железная руда".into() });
            *world.resource_mut::<JournalUi>() = JournalUi { category: crate::content::JournalCategory::Materials, selected: Some("iron_ore".into()) };
            go(world, "journal");
        }
        if ctx.t > 1.2 {
            let st = world.resource::<JournalState>();
            ctx.check("scan unlocked the entry", st.unlocked.contains("iron_ore"), "");
            ctx.check("start entries known", st.unlocked.contains("stone"), "");
            screenshot(world, "28_holo_journal");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("blueprint library: save, version, preview", |world, ctx| {
        if ctx.frames == 1 {
            let bp = Blueprint {
                size: IVec3::new(3, 2, 2),
                blocks: vec![(IVec3::ZERO, blocks::STONE), (IVec3::X, blocks::PLANKS), (IVec3::new(2, 0, 0), blocks::STONE), (IVec3::new(1, 1, 1), blocks::GLASS)],
                parts: vec![],
                joints: vec![],
            };
            let backup = world.resource::<BlueprintLibrary>().clone();
            world.insert_resource(BlueprintBackup(backup));
            let mut lib = BlueprintLibrary::default(); // path None: nothing is written
            let i = lib.add("Тестовый чертёж", &bp);
            lib.add_version(i, &bp);
            world.insert_resource(lib);
            world.resource_mut::<Selection>().clipboard = Some(bp);
            *world.resource_mut::<BlueprintUi>() = BlueprintUi { selected: Some(i), version: None };
            go(world, "blueprints");
        }
        if ctx.t > 1.2 {
            let lib = world.resource::<BlueprintLibrary>();
            ctx.check("two versions stored", lib.entries[0].versions.len() == 2, "");
            screenshot(world, "29_holo_blueprints");
            let b = world.remove_resource::<BlueprintBackup>().unwrap();
            world.insert_resource(b.0);
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("settings change the HUD live", |world, ctx| {
        if ctx.frames == 1 {
            go(world, "settings");
            world.resource_mut::<GameConfig>().hud.opacity = 0.5;
        }
        if ctx.t > 1.0 {
            let mut q = world.query::<(&crate::ui::hud::HudPanel, &Children)>();
            let any_dim = q.iter(world).count() > 0;
            ctx.check("HUD panels present", any_dim, "");
            screenshot(world, "30_holo_settings");
            world.resource_mut::<GameConfig>().hud.opacity = 1.0;
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("Escape closes the device (not the pause menu)", |world, ctx| {
        if ctx.frames == 1 {
            world.resource_mut::<HoloRay>().cursor_override = None;
            world.resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        }
        if ctx.frames == 2 {
            world.resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Escape);
        }
        let closed = *world.resource::<DeviceState>() == DeviceState::Closed;
        if (closed && ctx.t > 0.5) || ctx.t > 6.0 {
            let focus = *world.resource::<State<InputFocus>>().get();
            ctx.check("device closed back to gameplay", closed && focus == InputFocus::Gameplay, format!("{focus:?}"));
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("engineering panel shows real machine data", |world, ctx| {
        if ctx.frames == 1 {
            let assets = world.remove_resource::<PartAssets>().unwrap();
            let f = field(world);
            let mut c = world.commands();
            let e = spawn_part(&mut c, &assets, PartKind::Engine, Transform::from_translation(f + Vec3::new(2.0, 1.6, 2.0)));
            world.flush();
            world.insert_resource(assets);
            ctx.entities.insert("eng_engine".into(), e);
            let p = pos_of(world, e);
            super::scenario::teleport(world, p + Vec3::new(0.0, 0.8, 3.5));
            super::scenario3::aim_at(world, p + Vec3::Y * 0.8);
        }
        let e = ctx.entities["eng_engine"];
        if ctx.frames == 6 {
            world.resource_mut::<EngineeringTarget>().pin(e, 10.0);
        }
        if ctx.t > 2.0 {
            let text = describe(world, e);
            let ok = text.as_ref().is_some_and(|(title, body, diag)| {
                title == "Двигатель" && body.contains("об/мин") && body.contains("Соединения") && diag.contains("выключен")
            });
            ctx.check("engine data and diagnostics", ok, format!("{text:?}"));
            let shown = world.query::<(&Name, &Visibility)>().iter(world).any(|(n, v)| n.as_str() == "Engineering Panel" && *v == Visibility::Visible);
            ctx.check("panel visible above the machine", shown, "");
            screenshot(world, "31_engineering_panel");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step("device command API opens a given section", |world, ctx| {
        if ctx.frames == 1 {
            world.write_message(DeviceCommand::Open(Some("character")));
        }
        if ctx.t > 1.0 && ctx.frames > 5 {
            let ok = world.resource::<HoloNav>().current == "character" && *world.resource::<State<InputFocus>>().get() == InputFocus::Device;
            ctx.check("opened on «Персонаж»", ok, "");
            screenshot(world, "32_holo_character");
            world.write_message(DeviceCommand::Close);
            return Step::Next;
        }
        Step::Wait
    }));
}

#[derive(Resource)]
struct BlueprintBackup(BlueprintLibrary);
