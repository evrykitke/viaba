//! Who the workspace is, for the top of a document.
//!
//! The kit is handed this rather than fetching it, the way
//! [`Viewer`](crate::ui::viewer::Viewer) is handed the session: `ui` knows a
//! name and the address of an image, and nothing about organizations, files or
//! the routes they are served from.

use leptos::prelude::*;

/// The name and the mark a report heads its pages with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letterhead {
    pub name: String,
    /// Where the mark's bytes are. `None` draws the name instead, which is
    /// what a workspace that has not uploaded one gets.
    pub logo_src: Option<String>,
}

/// Held in context by the application shell.
#[derive(Clone, Copy)]
struct Held(Signal<Option<Letterhead>>);

impl Letterhead {
    /// Provide it to every report under this tree.
    pub fn provide(letterhead: Signal<Option<Self>>) {
        provide_context(Held(letterhead));
    }

    /// The letterhead, if one has been provided and has arrived.
    pub fn get() -> Signal<Option<Self>> {
        use_context::<Held>().map_or_else(|| Signal::derive(|| None), |held| held.0)
    }
}
