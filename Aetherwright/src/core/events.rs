//! Cross-subsystem messages. Subsystems talk through these instead of
//! reaching into each other's components.

use bevy::prelude::*;

/// A short message for the player (shown in the HUD notification feed).
#[derive(Message, Debug, Clone)]
pub struct Notify {
    pub text: String,
    pub kind: NotifyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    Info,
    Success,
    Warning,
}

impl Notify {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: NotifyKind::Info,
        }
    }
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: NotifyKind::Success,
        }
    }
    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: NotifyKind::Warning,
        }
    }
}

pub struct EventsPlugin;

impl Plugin for EventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Notify>();
    }
}
