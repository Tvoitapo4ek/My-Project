//! T2 checks: in-game texture editor (live painting, PNG save), texture
//! hot reload from disk, glTF model replacement with animation clips.

use bevy::prelude::*;

use super::{Step, StepFn, scenario::step, scenario4::pos_of, scenario5::field, screenshot};
use crate::{
    content::registry,
    core::states::InputFocus,
    machinery::components::{PartAssets, PartKind, spawn_part},
    models::{AnimationRequest, ModelAnimations, PrimitiveVisual},
    ui::texture_editor::{TextureEditor, Tool, commit, save},
    world::materials::{BlockMaterials, read_png, write_png},
};

const TEX: &str = "bricks";
const MOD_DIR: &str = "mods/autotest_models";

fn tex_index() -> usize {
    registry().texture_id(TEX).unwrap() as usize
}

fn image_pixel0(world: &World) -> [u8; 4] {
    let h = world.resource::<BlockMaterials>().images[tex_index()].clone();
    let img = world.resource::<Assets<Image>>().get(&h).unwrap();
    let d = img.data.as_ref().unwrap();
    [d[0], d[1], d[2], d[3]]
}

fn png_path() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("assets/textures/blocks/{TEX}.png"))
}

pub fn stage_t2(steps: &mut Vec<(String, StepFn)>) {
    steps.push(step("texture editor opens on a texture", |world, ctx| {
        if ctx.frames == 1 {
            // Keep the original file to restore it at the end.
            ctx.vars
                .insert("had_png".into(), png_path().exists() as u8 as f32);
            if let Ok(bytes) = std::fs::read(png_path()) {
                let _ = std::fs::write("target/autotest_tex_backup.png", bytes);
            }
            world.resource_mut::<TextureEditor>().texture = tex_index();
            world
                .resource_mut::<NextState<InputFocus>>()
                .set(InputFocus::TextureEditor);
        }
        if ctx.t > 1.5 && ctx.frames > 10 {
            let ed = world.resource::<TextureEditor>();
            let (w, h, n) = (ed.width, ed.height, ed.pixels.len());
            ctx.check(
                "editor loaded the texture",
                w == 16 && h == 16 && n == 256,
                format!("{w}×{h}, {n} px"),
            );
            screenshot(world, "22_texture_editor");
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step(
        "painting updates the world texture live",
        |world, ctx| {
            if ctx.frames == 1 {
                ctx.vars.insert("p0".into(), image_pixel0(world)[0] as f32);
                world.resource_scope(|world, mut ed: Mut<TextureEditor>| {
                    ed.tool = Tool::Pencil;
                    ed.color = [250, 20, 200, 255];
                    ed.apply(0);
                    let mats = BlockMaterials {
                        by_tex: Vec::new(),
                        images: world.resource::<BlockMaterials>().images.clone(),
                    };
                    let mut images = world.resource_mut::<Assets<Image>>();
                    commit(&mut ed, &mats, &mut images);
                });
            }
            if ctx.frames == 4 {
                let px = image_pixel0(world);
                ctx.check(
                    "live texture changed",
                    px == [250, 20, 200, 255],
                    format!("{px:?}"),
                );
                let r = save(world.resource::<TextureEditor>());
                let ok = r.is_ok()
                    && read_png(&png_path()).is_ok_and(|(_, _, d)| d[..4] == [250, 20, 200, 255]);
                ctx.check("Save writes the PNG", ok, format!("{r:?}"));
                world
                    .resource_mut::<NextState<InputFocus>>()
                    .set(InputFocus::Gameplay);
                return Step::Next;
            }
            Step::Wait
        },
    ));
    steps.push(step("external PNG edit is hot-reloaded", |world, ctx| {
        if ctx.frames == 1 {
            let (w, h, mut d) = read_png(&png_path()).unwrap();
            d[0..4].copy_from_slice(&[10, 240, 30, 255]);
            // Make sure the modification time moves forward.
            std::thread::sleep(std::time::Duration::from_millis(1100));
            write_png(&png_path(), w, h, &d).unwrap();
        }
        let px = image_pixel0(world);
        if px == [10, 240, 30, 255] || ctx.t > 5.0 {
            ctx.check(
                "hot reload picked up the file",
                px == [10, 240, 30, 255],
                format!("{px:?} after {:.1}s", ctx.t),
            );
            // Restore the original texture file.
            if ctx.vars["had_png"] > 0.5 {
                let _ = std::fs::copy("target/autotest_tex_backup.png", png_path());
            } else {
                let _ = std::fs::remove_file(png_path());
            }
            return Step::Next;
        }
        Step::Wait
    }));
    steps.push(step(
        "glTF model replaces primitives and animates",
        |world, ctx| {
            if ctx.frames == 1 {
                let dir = std::path::Path::new(MOD_DIR).join("models/parts");
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::copy("docs/examples/crate_demo.glb", dir.join("crate.glb"));
                let assets = world.remove_resource::<PartAssets>().unwrap();
                let f = field(world);
                let mut c = world.commands();
                let e = spawn_part(
                    &mut c,
                    &assets,
                    PartKind::Crate,
                    Transform::from_translation(f + Vec3::new(-2.0, 1.6, 2.0)),
                );
                world.flush();
                world.insert_resource(assets);
                ctx.entities.insert("model_crate".into(), e);
                let p = pos_of(world, e);
                super::scenario::teleport(world, p + Vec3::new(0.0, 0.8, 3.0));
                super::scenario3::aim_at(world, p);
            }
            let e = ctx.entities["model_crate"];
            let ready = world
                .get::<ModelAnimations>(e)
                .is_some_and(|a| a.player.is_some());
            if ready && !ctx.vars.contains_key("asked") {
                ctx.vars.insert("asked".into(), ctx.t);
                world.entity_mut(e).insert(AnimationRequest("work".into()));
            }
            if ctx.vars.get("asked").is_some_and(|t| ctx.t > t + 1.0) || ctx.t > 20.0 {
                let anims = world.get::<ModelAnimations>(e);
                let clips: Vec<String> = anims
                    .map(|a| a.clips.keys().cloned().collect())
                    .unwrap_or_default();
                ctx.check(
                    "model loaded with its clips",
                    clips.contains(&"work".to_string()),
                    format!("{clips:?}"),
                );
                let children: Vec<Entity> = world
                    .get::<Children>(e)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                let hidden = children
                    .iter()
                    .filter(|c| world.get::<PrimitiveVisual>(**c).is_some())
                    .all(|c| world.get::<Visibility>(*c) == Some(&Visibility::Hidden));
                ctx.check("primitives hidden", hidden, "");
                let playing = anims.and_then(|a| a.playing.clone());
                ctx.check(
                    "requested clip plays",
                    playing.as_deref() == Some("work"),
                    format!("{playing:?}"),
                );
                screenshot(world, "23_model");
                let _ = std::fs::remove_dir_all(MOD_DIR);
                return Step::Next;
            }
            Step::Wait
        },
    ));
}
