//! Every change to every quantity, newest first.
//!
//! # There is no edit and no delete, and that is the screen saying what this is
//!
//! The same statement the journal list makes. A movement that has happened is
//! evidence; the correction for one is a movement the other way, and this grid
//! offers neither a pencil nor a bin because neither exists.
//!
//! # The journal column is the one worth reading
//!
//! It says whether the stock account was told. `Posted` names the journal;
//! `no ledger` is a workspace that never bought the accounting module, and is
//! not a failure; `not needed` is a pallet that crossed an aisle. A row that
//! said nothing here would be the gap ADR 0006 section 6.1 is about.
//!
//! # Paged, for the same reason the audit trail is
//!
//! Nothing deletes from this list - see the section above - so it grows for as
//! long as the workspace trades. It used to be fetched whole with a `LIMIT 500`
//! underneath it, which is not a shorter list but a different one: the five
//! hundred and first movement was simply absent, and neither the grid nor the
//! pager nor the export said so.
//!
//! So it is a [`Source::paged`], and the three things that follow are the ones
//! [`audit`](super::audit) sets out. Only columns the reader can order by are
//! sortable and only columns it searches are searchable - both lists live in
//! `phonix_db::inventory::movement` and are checked against this one below.
//! And the two filters and the span carry a key across the wire rather than a
//! closure, because a closure could only narrow the twenty-five rows already
//! fetched.

use app_inventory::location::MoveKind;
use app_inventory::movement::{JournalOutcome, MoveFilter, MoveState, MoveSummary};
use leptos::prelude::*;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::stock_moves;
use crate::ui::table::{Align, Cell, Column, DateFilter, Filter, FilterChoice, Source};

pub fn stock_moves_grid() -> GridConfig<MoveSummary> {
    GridConfig::new(
        "stock-moves",
        // Unnarrowed, because this screen is the whole history. A stock card is
        // the same grid handed a filter naming its variant, and nothing else
        // about the configuration would change.
        Source::paged(|request| stock_moves(MoveFilter::default(), request)),
    )
    .searching(l!("moves.search"))
    .exports_as("stock-moves")
    .sorted_by(Sort::descending("moved_on"))
    .min_width("sm:min-w-[60rem]")
    .empty(
        Icon::ArrowRight,
        l!("moves.empty.title"),
        l!("moves.empty.detail"),
    )
    .column(
        Column::new("moved_on", l!("moves.date"), |row: &MoveSummary| {
            Cell::text(row.moved_on.to_string())
        })
        .sortable()
        .pinned()
        .essential()
        .class("tabular-nums"),
    )
    .column(
        Column::new("item", l!("entity.item.singular"), |row: &MoveSummary| {
            Cell::text(&row.item_name)
        })
        .searchable()
        .sortable()
        .essential()
        .render(|row| item_cell(row).into_any()),
    )
    .column(
        Column::new("kind", l!("moves.kind"), |row: &MoveSummary| {
            Cell::text(kind_label(row.kind()))
        })
        .essential()
        .render(|row| {
            view! { <Badge label=kind_label(row.kind()) tone=kind_tone(row.kind()) /> }.into_any()
        }),
    )
    .column(
        Column::new("from", l!("field.from"), |row: &MoveSummary| {
            Cell::text(&row.from_path)
        })
        .searchable()
        .class("font-mono text-xs text-content-muted"),
    )
    .column(
        Column::new("to", l!("moves.to"), |row: &MoveSummary| {
            Cell::text(&row.to_path)
        })
        .searchable()
        .essential()
        .class("font-mono text-xs"),
    )
    .column(
        Column::new("lot", l!("stock.lot"), |row: &MoveSummary| {
            Cell::text(row.lot_number.clone().unwrap_or_default())
        })
        .searchable()
        .class("font-mono text-xs text-content-muted"),
    )
    .column(
        Column::new("quantity", l!("stock.quantity"), |row: &MoveSummary| {
            Cell::number(row.quantity.scaled() as f64)
        })
        .sortable()
        .essential()
        .align(Align::End)
        .render(|row| {
            let text = format!("{} {}", row.quantity.to_display_string(), row.unit_code);
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .column(
        Column::new("value", l!("stock.value"), |row: &MoveSummary| {
            Cell::number(row.value.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .render(|row| {
            let text = row.value.to_display_string();
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .column(
        Column::new("journal", l!("moves.journal"), |row: &MoveSummary| {
            Cell::text(row.journal.number().unwrap_or_default())
        })
        .findable()
        .render(|row| journal_cell(&row.journal).into_any()),
    )
    .column(
        Column::new("reference", l!("moves.reference"), |row: &MoveSummary| {
            Cell::text(row.reference.clone().unwrap_or_default())
        })
        .searchable()
        .class("text-xs text-content-muted"),
    )
    // The values are the domain's own spellings, because that is what the
    // reader parses them back into - and a kind is derived from two location
    // kinds rather than stored, so the `WHERE` asks `MoveKind::ends` which
    // pairs of ends amount to one rather than writing that table twice.
    .filter(Filter::new(
        "kind",
        l!("moves.kind"),
        vec![
            FilterChoice::all(l!("common.all")),
            FilterChoice::new(MoveKind::Receipt.as_str(), l!("moves.kind.receipt")),
            FilterChoice::new(MoveKind::Delivery.as_str(), l!("moves.kind.delivery")),
            FilterChoice::new(MoveKind::Internal.as_str(), l!("moves.kind.internal")),
            FilterChoice::new(MoveKind::Adjustment.as_str(), l!("moves.kind.adjustment")),
        ],
    ))
    .filter(Filter::new(
        "state",
        l!("field.status"),
        vec![
            FilterChoice::all(l!("common.all")),
            FilterChoice::new(MoveState::Done.as_str(), l!("moves.state.done")),
            FilterChoice::new(MoveState::Draft.as_str(), l!("moves.state.draft")),
            FilterChoice::new(MoveState::Cancelled.as_str(), l!("moves.state.cancelled")),
        ],
    ))
    // Without a span, last March is two hundred pages of Next. It is the
    // control a list that only grows cannot be read without, and it has no `at`
    // closure for the reason the two filters have no `matching`.
    .date_filter(DateFilter::new("moved", l!("field.when")))
}

fn item_cell(row: &MoveSummary) -> impl IntoView {
    let name = row.item_name.clone();
    let code = row.variant_code.clone();

    view! {
        <div class="flex min-w-0 flex-col">
            <span class="truncate-fade text-content">{name}</span>
            <span class="font-mono text-xs text-content-muted">{code}</span>
        </div>
    }
}

fn journal_cell(outcome: &JournalOutcome) -> impl IntoView {
    match outcome {
        JournalOutcome::Posted { number, .. } => {
            let number = number.clone();
            view! { <span class="font-mono text-xs tabular-nums">{number}</span> }.into_any()
        }
        // Not a failure, and it must not read as one: the stock moved, and this
        // workspace does not keep books.
        JournalOutcome::NoLedger => {
            view! { <Badge label=l!("moves.journal.no_ledger_short") tone=Tone::Neutral /> }
                .into_any()
        }
        JournalOutcome::NotRequired => {
            view! { <span class="text-xs text-content-muted">{l!("common.none")}</span> }.into_any()
        }
    }
}

fn kind_label(kind: MoveKind) -> String {
    match kind {
        MoveKind::Receipt => l!("moves.kind.receipt"),
        MoveKind::Delivery => l!("moves.kind.delivery"),
        MoveKind::Internal => l!("moves.kind.internal"),
        MoveKind::Adjustment => l!("moves.kind.adjustment"),
        MoveKind::Manufacturing => l!("moves.kind.manufacturing"),
        MoveKind::Neither => l!("common.none"),
    }
}

/// Value in is good news, value out is not, and a rearrangement is neither.
const fn kind_tone(kind: MoveKind) -> Tone {
    match kind {
        MoveKind::Receipt => Tone::Success,
        MoveKind::Delivery => Tone::Brand,
        MoveKind::Adjustment => Tone::Warning,
        MoveKind::Internal | MoveKind::Manufacturing | MoveKind::Neither => Tone::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<MoveSummary> {
        Owner::new().with(stock_moves_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is
    /// `phonix_db::inventory::movement::SORTABLE`.
    const SERVER_SORTS: &[&str] = &["moved_on", "item", "quantity", "value", "journal"];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["item", "from", "to", "lot", "journal", "reference"];

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

        assert_eq!(sort, Sort::descending("moved_on"));
        assert!(SERVER_SORTS.contains(&sort.field.as_str()));
    }

    #[test]
    fn both_filters_leave_the_answering_to_the_server() {
        for filter in &grid().filters {
            assert!(
                !filter.is_local(),
                "{} is answered in the wrong place",
                filter.key()
            );
            assert_eq!(filter.default_value(), "");
        }
    }

    #[test]
    fn the_span_is_answered_by_the_server_and_named_what_the_reader_reads() {
        let grid = grid();
        let range = grid.date_filters.first().expect("the grid offers a span");

        // `phonix_db::inventory::movement::MOVED`, written down twice because
        // the two crates do not depend on each other.
        assert_eq!(range.key(), "moved");
        assert!(!range.is_local());
    }

    #[test]
    fn every_kind_and_state_offered_is_one_the_reader_parses_back() {
        let grid = grid();

        let kinds = grid.filters.iter().find(|f| f.key() == "kind").unwrap();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in kinds.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                MoveKind::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                MoveState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
