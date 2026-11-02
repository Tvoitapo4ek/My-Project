//! Spatial panels: Bevy UI rendered into textures shown on translucent
//! planes in front of the character, plus the pointer that drives them.
//!
//! * Each panel owns an image, a 2D camera rendering into it and a UI root
//!   (`PanelRoot`) that sections fill.
//! * The planes live on their own render layer (`HOLO_LAYER`) drawn by an
//!   overlay camera that copies the world camera every frame, so the
//!   projection is never hidden inside walls yet keeps real perspective.
//! * `drive_pointer` intersects the mouse ray with the planes and sends
//!   `PointerInput` for a dedicated pointer: hover, click, drag and scroll
//!   then reach the UI nodes through normal Bevy picking.

use bevy::{
    camera::{ImageRenderTarget, NormalizedRenderTarget, RenderTarget, visibility::RenderLayers},
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::AccumulatedMouseScroll,
    input::touch::TouchPhase,
    light::NotShadowCaster,
    picking::{
        PickingSystems,
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    },
    prelude::*,
    render::render_resource::TextureFormat,
    transform::TransformSystems,
    window::PrimaryWindow,
};

use super::device::{DeviceAnim, DeviceState};
use crate::{
    core::resources::{EyeRay, MainCamera},
    player::camera::{CameraMode, CameraRig},
};

/// Render layer of everything that belongs to the personal projection.
pub const HOLO_LAYER: usize = 3;
/// Pixels per metre of panel surface.
pub const PX_PER_M: f32 = 1000.0;
/// Distance from the eyes to the centre panel (first person).
pub const PANEL_DISTANCE: f32 = 0.95;
/// The projection pointer.
pub const HOLO_POINTER: PointerId = PointerId::Custom(bevy::asset::uuid::Uuid::from_u128(
    0x5a1e_7e11_0000_4000_8000_00000000a24e,
));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSlot {
    /// Left: section navigation.
    Nav,
    /// Centre: section content.
    Content,
    /// Right: details of the selection.
    Detail,
}

impl PanelSlot {
    pub const ALL: [PanelSlot; 3] = [PanelSlot::Nav, PanelSlot::Content, PanelSlot::Detail];
    /// (size in metres, local offset, yaw towards the user)
    fn layout(self) -> (Vec2, Vec3, f32) {
        match self {
            PanelSlot::Nav => (Vec2::new(0.34, 0.62), Vec3::new(-0.72, 0.0, 0.08), 0.42),
            PanelSlot::Content => (Vec2::new(1.0, 0.62), Vec3::ZERO, 0.0),
            PanelSlot::Detail => (Vec2::new(0.42, 0.62), Vec3::new(0.76, 0.0, 0.10), -0.42),
        }
    }
}

/// A projected plane (the 3D side).
#[derive(Component, Debug, Clone)]
pub struct HoloPanel {
    pub slot: PanelSlot,
    /// Size in metres (mesh units).
    pub size: Vec2,
    /// Texture size in pixels.
    pub px: UVec2,
    pub target: NormalizedRenderTarget,
    pub camera: Entity,
}

/// The UI root inside a panel texture (the 2D side). Sections fill these.
#[derive(Component, Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct PanelRoot(pub PanelSlot);

/// Parent of all projection planes; placed in front of the character.
#[derive(Component)]
pub struct HoloRoot;

/// Where sections put 3D holograms (in front of the centre panel).
#[derive(Component)]
pub struct ProjectionAnchor;

/// The camera that draws the projection layer over the world.
#[derive(Component)]
pub struct OverlayCamera;

/// Entities other modules need.
#[derive(Resource, Debug, Clone, Copy)]
pub struct HoloEntities {
    pub root: Entity,
    pub projection: Entity,
    pub nav: Entity,
    pub content: Entity,
    pub detail: Entity,
}

/// Facing of the projection; it follows the view lazily so the user can
/// look at a side panel without the panels running away.
#[derive(Resource, Debug, Default)]
pub struct HoloAnchor {
    pub yaw: f32,
}

/// Mouse state seen by the projection this frame.
#[derive(Resource, Debug, Default, Clone)]
pub struct HoloRay {
    /// World ray under the cursor (device open, not looking around).
    pub ray: Option<Ray3d>,
    /// Panel under the cursor and pixel position on it.
    pub hit: Option<(PanelSlot, Vec2)>,
    /// Cursor position override (autotest / gamepad virtual cursor).
    pub cursor_override: Option<Vec2>,
}

fn spawn_panels(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let layer = RenderLayers::layer(HOLO_LAYER);
    let root = commands
        .spawn((
            Name::new("Holo Root"),
            HoloRoot,
            Transform::default(),
            Visibility::Hidden,
            layer.clone(),
        ))
        .id();
    let projection = commands
        .spawn((
            Name::new("Holo Projection Anchor"),
            ProjectionAnchor,
            Transform::from_xyz(0.0, -0.02, 0.16),
            Visibility::Inherited,
            layer.clone(),
            ChildOf(root),
        ))
        .id();

    let plate = materials.add(StandardMaterial {
        base_color: Color::srgba(0.25, 0.85, 1.0, 0.07),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let edge = materials.add(StandardMaterial {
        base_color: Color::srgba(0.5, 0.95, 1.0, 0.85),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let mut ui_roots = Vec::new();
    for (i, slot) in PanelSlot::ALL.into_iter().enumerate() {
        let (size, offset, yaw) = slot.layout();
        let px = (size * PX_PER_M).round().as_uvec2();
        let mut image = Image::new_target_texture(px.x, px.y, TextureFormat::Bgra8UnormSrgb, None);
        image.sampler = bevy::image::ImageSampler::linear();
        let handle = images.add(image);
        let target = RenderTarget::Image(ImageRenderTarget {
            handle: handle.clone(),
            scale_factor: 1.0,
        });
        let normalized = target.normalize(None).expect("image target");
        let camera = commands
            .spawn((
                Name::new(format!("Holo UI Camera {slot:?}")),
                Camera2d,
                Camera {
                    order: -30 + i as isize,
                    is_active: false,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                target,
            ))
            .id();
        let ui = commands
            .spawn((
                Name::new(format!("Holo UI {slot:?}")),
                PanelRoot(slot),
                Node {
                    width: px_val(px.x),
                    height: px_val(px.y),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BackgroundColor(Color::NONE),
                UiTargetCamera(camera),
            ))
            .id();
        ui_roots.push(ui);

        let material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(handle),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let panel = commands
            .spawn((
                Name::new(format!("Holo Panel {slot:?}")),
                HoloPanel { slot, size, px, target: normalized, camera },
                Mesh3d(meshes.add(Rectangle::new(size.x, size.y))),
                MeshMaterial3d(material),
                Transform::from_translation(offset).with_rotation(Quat::from_rotation_y(yaw)),
                NotShadowCaster,
                layer.clone(),
                ChildOf(root),
            ))
            .id();
        // Depth: a glass plate behind the surface and glowing edges.
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x + 0.02, size.y + 0.02, 0.01))),
            MeshMaterial3d(plate.clone()),
            Transform::from_xyz(0.0, 0.0, -0.018),
            NotShadowCaster,
            layer.clone(),
            ChildOf(panel),
        ));
        for (w, h, x, y) in [
            (size.x + 0.03, 0.004, 0.0, size.y / 2.0 + 0.012),
            (size.x + 0.03, 0.004, 0.0, -size.y / 2.0 - 0.012),
            (0.004, size.y * 0.35, -size.x / 2.0 - 0.012, size.y * 0.3),
            (0.004, size.y * 0.35, size.x / 2.0 + 0.012, -size.y * 0.3),
        ] {
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(w, h, 0.004))),
                MeshMaterial3d(edge.clone()),
                Transform::from_xyz(x, y, -0.01),
                NotShadowCaster,
                layer.clone(),
                ChildOf(panel),
            ));
        }
    }

    commands.spawn((
        Name::new("Holo Overlay Camera"),
        OverlayCamera,
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            // Drawn only while the projection is visible.
            is_active: false,
            ..default()
        },
        Tonemapping::None,
        layer,
        Transform::default(),
    ));
    commands.spawn(HOLO_POINTER);

    commands.insert_resource(HoloEntities {
        root,
        projection,
        nav: ui_roots[0],
        content: ui_roots[1],
        detail: ui_roots[2],
    });
}

fn px_val(v: u32) -> Val {
    Val::Px(v as f32)
}

/// Wraps an angle to (-π, π].
fn wrap(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
}

/// Places the projection in front of the character, facing the camera.
pub fn place_root(
    time: Res<Time>,
    rig: Res<CameraRig>,
    eye: Res<EyeRay>,
    anim: Res<DeviceAnim>,
    state: Res<DeviceState>,
    mut anchor: ResMut<HoloAnchor>,
    cam: Query<&Transform, (With<MainCamera>, Without<HoloRoot>)>,
    mut root: Query<(&mut Transform, &mut Visibility), With<HoloRoot>>,
    panels: Query<&HoloPanel>,
    mut ui_cams: Query<&mut Camera, (Without<MainCamera>, Without<OverlayCamera>)>,
    mut overlays: Query<(&mut Camera, &OverlayCamera), Without<MainCamera>>,
) {
    let Ok((mut t, mut vis)) = root.single_mut() else {
        return;
    };
    let shown = anim.t > 0.001;
    let want = if shown { Visibility::Visible } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    for (mut c, _) in &mut overlays {
        if c.is_active != shown {
            c.is_active = shown;
        }
    }
    for panel in &panels {
        if let Ok(mut c) = ui_cams.get_mut(panel.camera)
            && c.is_active != shown
        {
            c.is_active = shown;
        }
    }
    if !shown {
        anchor.yaw = rig.yaw;
        return;
    }
    let Ok(cam) = cam.single() else {
        return;
    };
    // Follow the view only once it leaves a comfortable cone.
    if *state == DeviceState::Opening {
        anchor.yaw = rig.yaw;
    } else {
        let diff = wrap(rig.yaw - anchor.yaw);
        let excess = diff - diff.clamp(-0.65, 0.65);
        anchor.yaw = wrap(anchor.yaw + excess * (1.0 - (-6.0 * time.delta_secs()).exp()));
    }
    let fwd = Quat::from_rotation_y(anchor.yaw) * Vec3::NEG_Z;
    let lift = match rig.mode {
        CameraMode::FirstPerson => -0.05,
        CameraMode::ThirdPerson => 0.2,
    };
    let base = eye.origin + fwd * PANEL_DISTANCE + Vec3::Y * lift;
    let to_cam = cam.translation - base;
    let dist = to_cam.length().max(0.1);
    let mut look = to_cam;
    look.y *= 0.35; // keep the panels mostly upright
    let rot = Transform::IDENTITY.looking_to(-look, Vec3::Y).rotation;
    // Unfold animation: a thin line that opens vertically.
    let p = anim.eased();
    let s = dist / PANEL_DISTANCE;
    *t = Transform {
        translation: base,
        rotation: rot,
        scale: Vec3::new(s * (0.8 + 0.2 * p), s * p.max(0.02), s),
    };
}

/// The overlay camera mirrors the world camera.
fn sync_overlay_camera(
    main: Query<(&Transform, &Projection), (With<MainCamera>, Without<OverlayCamera>)>,
    mut overlay: Query<(&mut Transform, &mut Projection), With<OverlayCamera>>,
) {
    let (Ok((mt, mp)), Ok((mut ot, mut op))) = (main.single(), overlay.single_mut()) else {
        return;
    };
    *ot = *mt;
    if let (Projection::Perspective(o), Projection::Perspective(m)) = (&*op, mp)
        && o.fov == m.fov
        && o.aspect_ratio == m.aspect_ratio
        && o.near == m.near
    {
        return;
    }
    *op = mp.clone();
}

#[derive(Default)]
struct PointerState {
    /// Target and pixel position the pointer was last reported at.
    last: Option<(NormalizedRenderTarget, Vec2)>,
    pressed: Vec<PointerButton>,
}

/// Intersects a ray with a panel plane. Returns pixel coordinates.
pub fn ray_panel(ray: Ray3d, panel: &HoloPanel, gt: &GlobalTransform) -> Option<(f32, Vec2)> {
    let n = gt.affine().transform_vector3(Vec3::Z).normalize_or_zero();
    let origin = gt.translation();
    let denom = ray.direction.dot(n);
    if denom.abs() < 1e-5 {
        return None;
    }
    let dist = (origin - ray.origin).dot(n) / denom;
    if dist <= 0.0 {
        return None;
    }
    let hit = ray.origin + *ray.direction * dist;
    let local = gt.affine().inverse().transform_point3(hit);
    let (hx, hy) = (panel.size.x / 2.0, panel.size.y / 2.0);
    if local.x.abs() > hx || local.y.abs() > hy {
        return None;
    }
    let u = local.x / panel.size.x + 0.5;
    let v = 0.5 - local.y / panel.size.y;
    Some((dist, Vec2::new(u * panel.px.x as f32, v * panel.px.y as f32)))
}

/// Pixel on a panel → world point (for tests and for anchoring effects).
pub fn panel_point(panel: &HoloPanel, gt: &GlobalTransform, px: Vec2) -> Vec3 {
    let lx = (px.x / panel.px.x as f32 - 0.5) * panel.size.x;
    let ly = (0.5 - px.y / panel.px.y as f32) * panel.size.y;
    gt.transform_point(Vec3::new(lx, ly, 0.0))
}

#[allow(clippy::too_many_arguments)]
fn drive_pointer(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<OverlayCamera>>,
    panels: Query<(&HoloPanel, &GlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    state: Res<DeviceState>,
    mut holo_ray: ResMut<HoloRay>,
    mut st: Local<PointerState>,
    mut out: MessageWriter<PointerInput>,
) {
    let looking = mouse.pressed(MouseButton::Right);
    let cursor = holo_ray
        .cursor_override
        .or_else(|| window.single().ok().and_then(|w| w.cursor_position()));
    let mut ray = None;
    let mut hit: Option<(f32, PanelSlot, NormalizedRenderTarget, Vec2)> = None;
    if *state == DeviceState::Open
        && !looking
        && let (Some(c), Ok((cam, cam_gt))) = (cursor, camera.single())
        && let Ok(r) = cam.viewport_to_world(cam_gt, c)
    {
        ray = Some(r);
        for (panel, gt) in &panels {
            if let Some((d, px)) = ray_panel(r, panel, gt)
                && hit.as_ref().is_none_or(|h| d < h.0)
            {
                hit = Some((d, panel.slot, panel.target.clone(), px));
            }
        }
    }
    holo_ray.ray = ray;
    holo_ray.hit = hit.as_ref().map(|h| (h.1, h.3));

    let send = |out: &mut MessageWriter<PointerInput>, target: &NormalizedRenderTarget, pos: Vec2, action| {
        out.write(PointerInput::new(
            HOLO_POINTER,
            Location { target: target.clone(), position: pos },
            action,
        ));
    };
    const AWAY: Vec2 = Vec2::new(-1000.0, -1000.0);
    // Leaving a panel: move the pointer off it so hover ends there.
    if let Some((last_t, last_p)) = st.last.clone()
        && hit.as_ref().is_none_or(|h| h.2 != last_t)
    {
        // Finish presses that started on this panel (ends drags cleanly).
        for b in std::mem::take(&mut st.pressed) {
            send(&mut out, &last_t, AWAY, PointerAction::Release(b));
        }
        send(&mut out, &last_t, AWAY, PointerAction::Move { delta: AWAY - last_p });
        st.last = None;
    }
    let Some((_, _, target, pos)) = hit else {
        return;
    };
    let delta = st.last.as_ref().map(|l| pos - l.1).unwrap_or(Vec2::ZERO);
    if st.last.is_none() || delta != Vec2::ZERO {
        send(&mut out, &target, pos, PointerAction::Move { delta });
    }
    st.last = Some((target.clone(), pos));
    for (mb, pb) in [
        (MouseButton::Left, PointerButton::Primary),
        (MouseButton::Middle, PointerButton::Middle),
    ] {
        if mouse.just_pressed(mb) {
            send(&mut out, &target, pos, PointerAction::Press(pb));
            st.pressed.push(pb);
        }
        if mouse.just_released(mb) && st.pressed.contains(&pb) {
            send(&mut out, &target, pos, PointerAction::Release(pb));
            st.pressed.retain(|b| *b != pb);
        }
    }
    if scroll.delta != Vec2::ZERO {
        send(
            &mut out,
            &target,
            pos,
            PointerAction::Scroll {
                unit: scroll.unit,
                x: scroll.delta.x,
                y: scroll.delta.y,
                phase: TouchPhase::Moved,
            },
        );
    }
}

/// Scrollable list inside a panel (wheel over it scrolls).
#[derive(Component, Default)]
pub struct HoloScroll;

fn on_scroll(
    mut ev: On<bevy::picking::events::PointerScroll>,
    mut q: Query<(&mut ScrollPosition, &ComputedNode), With<HoloScroll>>,
) {
    let Ok((mut pos, node)) = q.get_mut(ev.event_target()) else {
        return;
    };
    let step = match ev.unit {
        bevy::input::mouse::MouseScrollUnit::Line => ev.y * 48.0,
        bevy::input::mouse::MouseScrollUnit::Pixel => ev.y,
    };
    let max = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
    pos.0.y = (pos.0.y - step).clamp(0.0, max);
    ev.propagate(false);
}

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloAnchor>()
            .init_resource::<HoloRay>()
            .add_systems(Startup, spawn_panels)
            .add_observer(on_scroll)
            .add_systems(First, drive_pointer.in_set(PickingSystems::Input))
            .add_systems(
                PostUpdate,
                (place_root, sync_overlay_camera)
                    .after(crate::player::camera::follow_player)
                    .before(TransformSystems::Propagate),
            );
    }
}
