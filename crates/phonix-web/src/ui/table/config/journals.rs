//! The journal list.
//!
//! Newest first, because the question somebody brings to this screen is almost
//! always about something posted recently. The exception - "what is in March" -
//! is the period filter.
//!
//! There is no edit action and no delete action, and that is the screen saying
//! what the ledger is: a posted journal is evidence. The only thing that can be
//! done to one from here is to open it, and the only correction is a reversal,
//! which is raised from the journal's own page where the lines being reversed
//! are in front of the person doing it.

use app_books::journal::JournalSummary;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::{JournalFilter, list_journals};
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// What has been posted.
pub fn journals_grid() -> GridConfig<JournalSummary> {
    GridConfig::new(
        "journals",
        Source::in_memory(|| list_journals(JournalFilter::default())),
    )
    .searching(l!("journals.search"))
    .exports_as("journals")
    .sorted_by(Sort::descending("entry_date"))
    .min_width("sm:min-w-[52rem]")
    .empty(
        Icon::ScrollText,
        l!("journals.empty.title"),
        l!("journals.empty.detail"),
    )
    .column(
        Column::new("number", l!("field.number"), |row: &JournalSummary| {
            Cell::text(&row.number)
        })
        .findable()
        .pinned()
        .essential()
        .class("font-mono tabular-nums"),
    )
    .column(
        Column::new("entry_date", l!("journals.entry_date"), |row: &JournalSummary| {
            Cell::text(row.entry_date.to_string())
        })
        .sortable()
        .essential()
        .class("tabular-nums"),
    )
    .column(
        Column::new("narration", l!("journals.narration"), |row: &JournalSummary| {
            Cell::text(&row.narration)
        })
        .searchable()
        .essential()
        .render(|row| narration_cell(row).into_any()),
    )
    .column(
        Column::new("period", l!("journals.period"), |row: &JournalSummary| {
            Cell::text(&row.period_label)
        })
        .sortable()
        .class("font-mono text-xs tabular-nums text-content-muted"),
    )
    .column(
        Column::new("source", l!("journals.source"), |row: &JournalSummary| {
            Cell::text(&row.source_doc_type)
        })
        .searchable()
        .class("text-xs text-content-muted"),
    )
    .column(
        // One side of it, which is both: they are equal by construction.
        Column::new("total", l!("journals.total"), |row: &JournalSummary| {
            Cell::number(row.total.scaled() as f64)
        })
        .sortable()
        .essential()
        .align(Align::End)
        .render(|row| {
            let text = row.total.to_display_string();
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .column(
        Column::new("line_count", l!("journals.lines"), |row: &JournalSummary| {
            Cell::number(row.line_count as f64)
        })
        .sortable()
        .align(Align::End)
        .class("tabular-nums text-content-muted"),
    )
    .filter(
        Filter::new(
            "kind",
            l!("journals.kind"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("reversal", l!("journals.only_reversals")),
                FilterChoice::new("original", l!("journals.only_originals")),
            ],
        )
        .matching(|row: &JournalSummary, wanted| match wanted {
            "reversal" => row.is_reversal,
            "original" => !row.is_reversal,
            _ => true,
        }),
    )
    .toolbar(
        ToolbarAction::link(l!("journals.new"), Icon::Plus, "/sales/journals/new")
            .require(permissions::JOURNALS_POST)
            .primary(),
    )
    .action(
        RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &JournalSummary| {
            format!("/sales/journals/{}", row.id)
        })
        .require(permissions::JOURNALS),
    )
}

/// The narration, with a badge on the corrections. A reversal read as an
/// ordinary journal is a number counted twice by somebody skimming.
fn narration_cell(row: &JournalSummary) -> impl IntoView {
    let narration = row.narration.clone();
    let is_reversal = row.is_reversal;

    view! {
        <div class="flex min-w-0 items-center gap-2">
            <span class="truncate-fade text-content">{narration}</span>
            {is_reversal
                .then(|| view! { <Badge label=l!("journals.reversal") tone=Tone::Warning /> })}
        </div>
    }
}
