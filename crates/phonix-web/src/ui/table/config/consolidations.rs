//! What has been consolidated, and how many orders each one became.
//!
//! # The supplier count is the column that matters
//!
//! A consolidation is an act of aggregation, and the question asked of the list
//! is "what did eleven requests turn into". Three suppliers is three orders, and
//! it is worth seeing before the document is opened as well as after.
//!
//! # A draft with unsourced lines is shown as one
//!
//! `suppliers` below counts distinct suppliers on the lines, so a draft where
//! half the lines have nobody to buy them from reads lower than its line count.
//! That gap is the work still to do, and it is the reason the two columns sit
//! next to each other.

use app_inventory::consolidation::{ConsolidationState, ConsolidationSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_consolidations;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn consolidations_grid() -> GridConfig<ConsolidationSummary> {
    GridConfig::new("consolidations", Source::paged(list_consolidations))
        .searching(l!("consolidations.search"))
        .exports_as("consolidations")
        .sorted_by(Sort::descending("raised_on"))
        .min_width("sm:min-w-[48rem]")
        .empty(
            Icon::Boxes,
            l!("consolidations.empty.title"),
            l!("consolidations.empty.detail"),
        )
        .column(
            Column::new(
                "number",
                l!("field.number"),
                |row: &ConsolidationSummary| Cell::text(&row.number),
            )
            .findable()
            .pinned()
            .essential()
            .class("font-mono tabular-nums")
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "warehouse",
                l!("consolidations.warehouse"),
                |row: &ConsolidationSummary| Cell::text(&row.warehouse_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "raised_on",
                l!("consolidations.raised_on"),
                |row: &ConsolidationSummary| Cell::text(row.raised_on.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "raised_by",
                l!("consolidations.raised_by"),
                |row: &ConsolidationSummary| {
                    Cell::text(row.raised_by_name.clone().unwrap_or_default())
                },
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &ConsolidationSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new(
                "line_count",
                l!("consolidations.lines_count"),
                |row: &ConsolidationSummary| Cell::number(row.line_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new(
                "suppliers",
                l!("consolidations.suppliers"),
                |row: &ConsolidationSummary| Cell::number(row.supplier_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new(
                "orders",
                l!("consolidations.orders"),
                |row: &ConsolidationSummary| Cell::number(row.order_count as f64),
            )
            .sortable()
            .essential()
            .align(Align::End)
            .class("tabular-nums"),
        )
        // Filtering is handled by the paged source.
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    // First, because an unfinished consolidation is the one
                    // thing on this screen that is somebody's outstanding work.
                    FilterChoice::new("draft", l!("consolidations.state.draft")),
                    FilterChoice::new("confirmed", l!("consolidations.state.confirmed")),
                    FilterChoice::new("cancelled", l!("consolidations.state.cancelled")),
                ],
            ),
        )
        .date_filter(DateFilter::new("raised", l!("consolidations.raised_on")))
        .toolbar(
            ToolbarAction::link(
                l!("consolidations.new"),
                Icon::Plus,
                "/inventory/consolidations/new",
            )
            .require(permissions::CONSOLIDATIONS_MANAGE)
            .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &ConsolidationSummary| format!("/inventory/consolidations/{}", row.id),
            )
            .require(permissions::CONSOLIDATIONS),
        )
}

/// The number, or what a draft is recognisable by before it has one.
fn number_cell(row: &ConsolidationSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">
                {l!("consolidations.state.draft")}
            </span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

fn state_label(state: ConsolidationState) -> String {
    match state {
        ConsolidationState::Draft => l!("consolidations.state.draft"),
        ConsolidationState::Confirmed => l!("consolidations.state.confirmed"),
        ConsolidationState::Cancelled => l!("consolidations.state.cancelled"),
    }
}

const fn state_tone(state: ConsolidationState) -> Tone {
    match state {
        ConsolidationState::Draft => Tone::Warning,
        ConsolidationState::Confirmed => Tone::Success,
        ConsolidationState::Cancelled => Tone::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<ConsolidationSummary> {
        Owner::new().with(consolidations_grid)
    }

    /// Mirrors the database sort fields without adding a crate dependency.
    const SERVER_SORTS: &[&str] = &["number", "warehouse", "raised_on", "line_count", "suppliers", "orders"];

    /// Mirrors the database search fields.
    const SERVER_SEARCHES: &[&str] = &["number", "warehouse", "raised_by"];

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

        assert_eq!(sort, Sort::descending("raised_on"));
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
        }

        let range = grid.date_filters.first().expect("the grid offers a span");

        // Mirrors the database date-range key.
        assert_eq!(range.key(), "raised");
        assert!(!range.is_local());
    }

    #[test]
    fn the_grid_opens_on_everything() {
        // The empty choice keeps the initial list unfiltered.
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        assert_eq!(states.default_value(), "");
        assert!(grid.initial_request().filter("state").is_none());
    }

    #[test]
    fn every_state_offered_is_one_the_reader_parses_back() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                ConsolidationState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
