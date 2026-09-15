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
//!
//! # Paged, because the ledger only ever grows
//!
//! Every invoice, every payment and every stock movement writes a journal, and
//! nothing deletes one - so of all the lists in this workspace, this is the one
//! that outgrows the browser first. It is a [`Source::paged`] for that reason,
//! and what follows is what [`audit`](super::audit) sets out: only columns the
//! reader can order by are sortable, only columns it searches are searchable,
//! and the filter and the span carry a key across the wire rather than a
//! closure - the lists are in `phonix_db::books::journal` and are checked
//! against this file below.

use app_books::journal::JournalSummary;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::{JournalFilter, list_journals};
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

/// What has been posted.
pub fn journals_grid() -> GridConfig<JournalSummary> {
    GridConfig::new(
        "journals",
        // Unnarrowed, because this screen is the whole ledger. An account's own
        // ledger would be the same grid handed a filter naming it.
        Source::paged(|request| list_journals(JournalFilter::default(), request)),
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
        Column::new(
            "entry_date",
            l!("journals.entry_date"),
            |row: &JournalSummary| Cell::text(row.entry_date.to_string()),
        )
        .sortable()
        .essential()
        .class("tabular-nums"),
    )
    .column(
        Column::new(
            "narration",
            l!("journals.narration"),
            |row: &JournalSummary| Cell::text(&row.narration),
        )
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
        Column::new(
            "line_count",
            l!("journals.lines"),
            |row: &JournalSummary| Cell::number(row.line_count as f64),
        )
        .sortable()
        .align(Align::End)
        .class("tabular-nums text-content-muted"),
    )
    // No `matching`: a closure could only narrow the twenty-five rows already
    // fetched, and "only the corrections" is a question about the ledger.
    .filter(Filter::new(
        "kind",
        l!("journals.kind"),
        vec![
            FilterChoice::all(l!("common.all")),
            FilterChoice::new("reversal", l!("journals.only_reversals")),
            FilterChoice::new("original", l!("journals.only_originals")),
        ],
    ))
    // "What is in March" - which the module header calls the one exception to
    // reading this list newest first, and which used to have no control at all.
    .date_filter(DateFilter::new("entry", l!("journals.entry_date")))
    .toolbar(
        ToolbarAction::link(l!("journals.new"), Icon::Plus, "/accounting/journals/new")
            .require(permissions::JOURNALS_POST)
            .primary(),
    )
    .action(
        RowAction::link(
            l!("common.open"),
            Icon::ArrowRight,
            |row: &JournalSummary| format!("/accounting/journals/{}", row.id),
        )
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

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<JournalSummary> {
        Owner::new().with(journals_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is `phonix_db::books::journal::SORTABLE`.
    const SERVER_SORTS: &[&str] = &["number", "entry_date", "period", "total", "line_count"];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "narration", "source"];

    #[test]
    fn every_sortable_column_is_one_the_server_can_order_by() {
        for column in grid().columns.iter().filter(|column| column.sortable) {
            assert!(
                SERVER_SORTS.contains(&column.field()),
                "{} offers a sort the reader will ignore",
                column.field(),
            );
        }
    }

    #[test]
    fn every_searchable_column_is_one_the_server_looks_inside() {
        for column in grid().columns.iter().filter(|column| column.searchable) {
            assert!(
                SERVER_SEARCHES.contains(&column.field()),
                "{} is offered to the search box and never searched",
                column.field(),
            );
        }
    }

    #[test]
    fn it_opens_newest_first_by_a_column_the_server_can_order_by() {
        let sort = grid().initial_request().sort.expect("an opening order");

        assert_eq!(sort, Sort::descending("entry_date"));
        assert!(SERVER_SORTS.contains(&sort.field.as_str()));
    }

    #[test]
    fn the_filter_and_the_span_leave_the_answering_to_the_server() {
        let grid = grid();

        for filter in &grid.filters {
            assert!(
                !filter.is_local(),
                "{} is answered in the wrong place",
                filter.key()
            );
            assert_eq!(filter.default_value(), "");
        }

        let range = grid.date_filters.first().expect("the grid offers a span");

        // `phonix_db::books::journal::ENTRY`, written down twice because the
        // two crates do not depend on each other.
        assert_eq!(range.key(), "entry");
        assert!(!range.is_local());
    }

    #[test]
    fn the_two_kinds_are_the_two_the_reader_answers() {
        let grid = grid();
        let kinds = grid.filters.iter().find(|f| f.key() == "kind").unwrap();

        let offered: Vec<&str> = kinds
            .choices
            .iter()
            .map(|choice| choice.value)
            .filter(|value| !value.is_empty())
            .collect();

        // Anything else reads as "everything" - see `page`, which matches these
        // two words and treats the rest as unfiltered.
        assert_eq!(offered, ["reversal", "original"]);
    }
}
