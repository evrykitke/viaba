//! Viewer context for permission-gated UI components.
//!
//! The application shell provides it once; components without a viewer hide
//! gated controls.

use leptos::prelude::*;
use phonix_core::identity::AuthUser;

/// The signed-in account, for anything in the kit that gates on permissions.
#[derive(Clone, Copy)]
pub struct Viewer(pub Signal<Option<AuthUser>>);

impl Viewer {
    /// Provides the viewer to descendant components.
    pub fn provide(user: Signal<Option<AuthUser>>) {
        provide_context(Self(user));
    }

    /// Returns the viewer, if one has been provided.
    pub fn get() -> Signal<Option<AuthUser>> {
        use_context::<Self>().map_or_else(|| Signal::derive(|| None), |viewer| viewer.0)
    }
}
