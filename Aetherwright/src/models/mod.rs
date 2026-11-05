//! Replaceable 3D models and animations.
//!
//! Every entity with a visual role (part, creature, player) is built from
//! primitives by default. If a glTF file exists at
//! `models/<category>/<key>.glb` (in `mods/<mod>/` — last mod wins — or in
//! `assets/`), it replaces the primitives automatically:
//!
//! * the primitive children (marked `PrimitiveVisual`) are hidden;
//! * the file's default scene is spawned as a child;
//! * all named animation clips are put into an animation graph; gameplay
//!   asks for a clip by name through `AnimationRequest` ("idle", "walk",
//!   "run", "attack", "hit", "work", …). Missing clips are simply ignored.
//!
//! Physics never uses the model: colliders stay the gameplay shapes, so a
//! model cannot break balance. See `docs/MODELS_AND_ANIMATION.md`.

use std::{collections::HashMap, time::Duration};

use bevy::{
    gltf::Gltf,
    prelude::*,
    world_serialization::{WorldAssetRoot, WorldInstanceReady},
};

/// Marks primitive placeholder meshes that a loaded model replaces.
#[derive(Component, Clone, Copy)]
pub struct PrimitiveVisual;

/// Ask for a model `models/<category>/<key>.glb` on this entity.
#[derive(Component, Debug, Clone)]
pub struct ModelSlot {
    pub category: &'static str,
    pub key: String,
}

/// Desired animation clip name (set by gameplay systems).
#[derive(Component, Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimationRequest(pub String);

#[derive(Component)]
struct PendingModel(Handle<Gltf>);

/// Animation data of a spawned model.
#[derive(Component)]
pub struct ModelAnimations {
    pub graph: Handle<AnimationGraph>,
    pub clips: HashMap<String, AnimationNodeIndex>,
    pub player: Option<Entity>,
    pub playing: Option<String>,
}

/// Asset path for a model file, if one exists (mods via the `mods://` source).
pub fn model_asset_path(category: &str, key: &str) -> Option<String> {
    let rel = format!("models/{category}/{key}.glb");
    for root in crate::content::mod_roots().iter().rev() {
        if root.join(&rel).exists() {
            let name = root.file_name()?.to_string_lossy().to_string();
            return Some(format!("mods://{name}/{rel}"));
        }
    }
    std::path::Path::new("assets")
        .join(&rel)
        .exists()
        .then_some(rel)
}

fn request_models(
    mut commands: Commands,
    assets: Res<AssetServer>,
    added: Query<(Entity, &ModelSlot), Added<ModelSlot>>,
) {
    for (e, slot) in &added {
        if let Some(path) = model_asset_path(slot.category, &slot.key) {
            info!("model {} ← {path}", slot.key);
            commands.entity(e).insert(PendingModel(assets.load(path)));
        }
    }
}

fn spawn_loaded(
    mut commands: Commands,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    pending: Query<(Entity, &PendingModel)>,
    tree: Query<&Children>,
    mut primitives: Query<&mut Visibility, With<PrimitiveVisual>>,
) {
    for (e, p) in &pending {
        if !assets.is_loaded_with_dependencies(&p.0) {
            if assets.load_state(&p.0).is_failed() {
                warn!("model failed to load; keeping primitives");
                commands.entity(e).remove::<PendingModel>();
            }
            continue;
        }
        let Some(gltf) = gltfs.get(&p.0) else {
            continue;
        };
        let Some(scene) = gltf
            .default_scene
            .clone()
            .or_else(|| gltf.scenes.first().cloned())
        else {
            commands.entity(e).remove::<PendingModel>();
            continue;
        };
        // Whole hierarchy: limbs sit under pivots (grandchildren).
        for c in tree.iter_descendants(e) {
            if let Ok(mut v) = primitives.get_mut(c) {
                *v = Visibility::Hidden;
            }
        }
        let names: Vec<String> = gltf
            .named_animations
            .keys()
            .map(|k| k.to_string())
            .collect();
        let (graph, nodes) = AnimationGraph::from_clips(
            names
                .iter()
                .map(|n| gltf.named_animations[n.as_str()].clone()),
        );
        let clips = names.into_iter().zip(nodes).collect();
        commands
            .entity(e)
            .remove::<PendingModel>()
            .insert(ModelAnimations {
                graph: graphs.add(graph),
                clips,
                player: None,
                playing: None,
            });
        let model = commands
            .spawn((Name::new("Model"), WorldAssetRoot(scene)))
            .observe(on_model_ready)
            .id();
        commands.entity(e).add_child(model);
    }
}

/// When the scene instance exists, find its animation player.
fn on_model_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    mut owners: Query<&mut ModelAnimations>,
) {
    let model = ready.entity;
    let Ok(owner) = parents.get(model).map(|p| p.parent()) else {
        return;
    };
    let Ok(mut anims) = owners.get_mut(owner) else {
        return;
    };
    if let Some(player) = children
        .iter_descendants(model)
        .find(|c| players.get(*c).is_ok())
    {
        commands.entity(player).insert((
            AnimationGraphHandle(anims.graph.clone()),
            AnimationTransitions::new(),
        ));
        anims.player = Some(player);
    }
}

/// Plays the requested clip (cross-fade 0.2 s), looping.
fn play_requested(
    mut owners: Query<(&AnimationRequest, &mut ModelAnimations)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (req, mut anims) in &mut owners {
        if anims.playing.as_deref() == Some(req.0.as_str()) {
            continue;
        }
        let (Some(player), Some(&node)) = (anims.player, anims.clips.get(&req.0)) else {
            continue;
        };
        if let Ok((mut p, mut t)) = players.get_mut(player) {
            t.play(&mut p, node, Duration::from_millis(200)).repeat();
            anims.playing = Some(req.0.clone());
        }
    }
}

pub struct ModelsPlugin;

impl Plugin for ModelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (request_models, spawn_loaded, play_requested).chain(),
        );
    }
}
