//! Diegetic holographic interface (concept §10, task §24).
//!
//! The character carries a projector device (`HoloDevice`, any archetype or
//! model can have one). Opening it plays an activation, then projects
//! spatial panels in front of the character: they live in the 3D world,
//! face the camera, have depth and are lit as holograms. Panel contents are
//! regular Bevy UI rendered into textures; the mouse ray is intersected with
//! the panel planes and fed back to Bevy picking as pointer events, so all
//! UI interaction (click, hover, drag) works on the 3D panels.
//!
//! Layering:
//! * `device` — state machine (closed → opening → open → closing), input
//!   focus, cursor/look handling, the device's own animation;
//! * `panels` — render-to-texture panels, 3D placement, pointer driving;
//! * `style` — visual language (thin lines, translucent surfaces, runes);
//! * `sections` — content; each section is a plugin that registers itself
//!   in `HoloSections` and builds its UI when `BuildSection` names it;
//! * `projection` — 3D hologram objects next to the panels (figure, map,
//!   blueprint models);
//! * `engineering` — separate diagnostic panels on machines (not part of the
//!   personal device).

pub mod device;
pub mod engineering;
pub mod panels;
pub mod projection;
pub mod sections;
pub mod style;

use bevy::prelude::*;

/// Registry entry of a device section.
#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub id: &'static str,
    pub title: &'static str,
    /// Short glyph shown on the navigation button.
    pub glyph: &'static str,
    pub order: i32,
}

/// All sections available on the device (extend by registering more).
#[derive(Resource, Default, Debug)]
pub struct HoloSections(pub Vec<SectionInfo>);

impl HoloSections {
    pub fn register(&mut self, info: SectionInfo) {
        if !self.0.iter().any(|s| s.id == info.id) {
            self.0.push(info);
            self.0.sort_by_key(|s| s.order);
        }
    }
}

/// Which section is shown; `rebuild` asks for the content to be rebuilt.
#[derive(Resource, Debug)]
pub struct HoloNav {
    pub current: &'static str,
    pub rebuild: bool,
}

impl Default for HoloNav {
    fn default() -> Self {
        Self { current: "home", rebuild: true }
    }
}

impl HoloNav {
    pub fn go(&mut self, id: &'static str) {
        self.current = id;
        self.rebuild = true;
    }
}

/// Sent when a section must (re)build its UI into the given roots.
#[derive(Message, Debug, Clone, Copy)]
pub struct BuildSection {
    pub id: &'static str,
    /// Root node of the main (centre) panel.
    pub content: Entity,
    /// Root node of the detail (right) panel.
    pub detail: Entity,
    /// 3D anchor for hologram projections of this section.
    pub projection: Entity,
}

/// Run condition: the device is open and showing `id`.
pub fn section_active(id: &'static str) -> impl Fn(Option<Res<HoloNav>>, Res<State<crate::core::states::InputFocus>>) -> bool {
    move |nav, focus| *focus.get() == crate::core::states::InputFocus::Device && nav.is_some_and(|n| n.current == id)
}

/// Run condition: the projection is visible (opening, open or closing).
pub fn device_shown(state: Res<device::DeviceState>) -> bool {
    *state != device::DeviceState::Closed
}

pub struct HoloPlugin;

impl Plugin for HoloPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoloSections>()
            .init_resource::<HoloNav>()
            .add_message::<BuildSection>()
            .add_plugins((
                style::HoloStylePlugin,
                device::DevicePlugin,
                panels::PanelsPlugin,
                projection::ProjectionPlugin,
                sections::SectionsPlugin,
                engineering::EngineeringPlugin,
            ));
    }
}
