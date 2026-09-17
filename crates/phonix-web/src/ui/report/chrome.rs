//! What a report is handed before it can draw a document: who the workspace
//! is, and what this kind of document looks like here.
//!
//! The kit is handed both rather than fetching them, the way
//! [`Viewer`](crate::ui::viewer::Viewer) is handed the session: `ui` knows a
//! name, the address of an image and a set of measurements, and nothing about
//! organizations, files or the routes they are served from.
//!
//! Resolved once for the session by the application shell, which is what keeps
//! it off the path of every report and every page of one.

use leptos::prelude::*;
use phonix_core::report::DocumentSettings;

/// What a report heads its pages with: the workspace, as words and a mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letterhead {
    pub name: String,
    /// Where the mark's bytes are. `None` draws the name instead, which is
    /// what a workspace that has not uploaded one gets.
    pub logo_src: Option<String>,
    /// Where the workspace is, as the lines to print, in order. Assembled
    /// before it arrives here, so a document cannot order them differently
    /// from the one beside it.
    pub address: Vec<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub registration_number: Option<String>,
    pub tax_id: Option<String>,
}

/// Held in context by the application shell.
#[derive(Clone, Copy)]
struct HeldLetterhead(Signal<Option<Letterhead>>);

impl Letterhead {
    /// Provide it to every report under this tree.
    pub fn provide(letterhead: Signal<Option<Self>>) {
        provide_context(HeldLetterhead(letterhead));
    }

    /// The letterhead, if one has been provided and has arrived.
    pub fn get() -> Signal<Option<Self>> {
        use_context::<HeldLetterhead>().map_or_else(|| Signal::derive(|| None), |held| held.0)
    }
}

/// What this workspace keeps about each kind of document it issues.
#[derive(Clone, Copy)]
pub struct DocumentStyles(Signal<Vec<DocumentSettings>>);

impl DocumentStyles {
    /// Provide them to every report under this tree.
    pub fn provide(settings: Signal<Vec<DocumentSettings>>) {
        provide_context(Self(settings));
    }

    /// What this workspace keeps for one document type.
    ///
    /// `None` for a type nobody has kept a setting for, and for every report
    /// that is not a document at all - a product list is not something a
    /// tenant has an opinion about the paper of.
    pub fn of(document_type: &str) -> Option<DocumentSettings> {
        use_context::<Self>().and_then(|held| {
            held.0.with(|settings| {
                settings
                    .iter()
                    .find(|kept| kept.document_type == document_type)
                    .cloned()
            })
        })
    }
}
