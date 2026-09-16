//! The documents this workspace issues, and what each one looks like.
//!
//! In memory: the list is as long as the document types the installed apps
//! declare in `config/documents/`, which is a dozen and does not grow with the
//! workspace's data.

use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::report::DocumentSettings;

use super::GridConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::admin_fns::document_settings;
use crate::ui::table::{Cell, Column, RowAction, Source};

/// Every document type, with the choices made for it.
///
/// `open` is the row action, because the editor is a panel beside the grid
/// rather than a page of its own - the same arrangement the numbering tab has.
pub fn document_settings_grid(open: Callback<DocumentSettings>) -> GridConfig<DocumentSettings> {
    GridConfig::new("document-settings", Source::in_memory(document_settings))
        .searching(l!("documents.doc_type"))
        .min_width("sm:min-w-[40rem]")
        .empty(
            Icon::FileText,
            l!("documents.empty.title"),
            l!("documents.empty.detail"),
        )
        .column(
            Column::new(
                "document_type",
                l!("documents.doc_type"),
                |row: &DocumentSettings| Cell::text(&row.document_type),
            )
            .findable()
            .pinned()
            .essential()
            .render(|row| document_cell(row).into_any()),
        )
        .column(
            Column::new("theme", l!("documents.look"), |row: &DocumentSettings| {
                Cell::text(crate::i18n::t(&row.theme.label()))
            })
            .sortable()
            .essential(),
        )
        .column(
            Column::new("paper", l!("documents.paper"), |row: &DocumentSettings| {
                Cell::text(format!(
                    "{} \u{b7} {}",
                    crate::i18n::t(&row.paper.label()),
                    crate::i18n::t(&row.orientation.label()),
                ))
            })
            .sortable(),
        )
        .column(Column::new(
            "logo",
            l!("documents.logo"),
            |row: &DocumentSettings| {
                Cell::text(match row.logo.map(|logo| logo.placement.band_str()) {
                    Some("page_header") => l!("documents.logo.page_header"),
                    Some(_) => l!("documents.logo.report_header"),
                    None => l!("documents.logo.none"),
                })
            },
        ))
        .action(
            RowAction::run(
                l!("common.edit"),
                Icon::Pencil,
                move |row: DocumentSettings, _| open.run(row),
            )
            .require(permissions::SETTINGS),
        )
}

/// The type, as a person reads it: `sales_invoice` is a column name.
fn document_cell(row: &DocumentSettings) -> impl IntoView {
    let name = row.document_type.replace('_', " ");

    view! { <span class="font-medium capitalize text-content">{name}</span> }
}
