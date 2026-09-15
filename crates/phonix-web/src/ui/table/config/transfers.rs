//! Stock moved between the workspace's own places.
//!
//! **On the road** is the column that earns its place: it is what left and has
//! not arrived, and a non-zero figure on a journey despatched three weeks ago
//! is the thing somebody needs to see without opening anything.

use app_inventory::transfer::{TransferState, TransferSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_transfers;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn transfers_grid() -> GridConfig<TransferSummary> {
    GridConfig::new("transfers", Source::paged(list_transfers))
        .searching(l!("transfers.search"))
        .exports_as("stock-transfers")
        .sorted_by(Sort::descending("planned_on"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Truck,
            l!("transfers.empty.title"),
            l!("transfers.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &TransferSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new("from", l!("transfers.from"), |row: &TransferSummary| {
                Cell::text(&row.from_path)
            })
            .searchable()
            .sortable()
            .essential()
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("to", l!("transfers.to"), |row: &TransferSummary| {
                Cell::text(&row.to_path)
            })
            .searchable()
            .sortable()
            .essential()
            .class("font-mono text-xs"),
        )
        .column(
            Column::new(
                "planned_on",
                l!("transfers.planned"),
                |row: &TransferSummary| Cell::text(row.planned_on.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "despatched_on",
                l!("transfers.despatched"),
                |row: &TransferSummary| {
                    Cell::text(
                        row.despatched_on
                            .map(|on| on.to_string())
                            .unwrap_or_default(),
                    )
                },
            )
            .sortable()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &TransferSummary| {
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
                "in_transit",
                l!("transfers.on_the_road"),
                |row: &TransferSummary| Cell::number(row.in_transit.scaled() as f64),
            )
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| on_the_road_cell(row).into_any()),
        )
        .column(
            Column::new(
                "reference",
                l!("transfers.reference"),
                |row: &TransferSummary| Cell::text(row.reference.clone().unwrap_or_default()),
            )
            .searchable()
            .findable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("lines", l!("transfers.lines"), |row: &TransferSummary| {
                Cell::number(row.line_count as f64)
            })
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        // Filtering is handled by the paged source.
        .filter(Filter::new(
            "state",
            l!("field.status"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("draft", l!("transfers.state.draft")),
                FilterChoice::new("in_transit", l!("transfers.state.in_transit")),
                FilterChoice::new("done", l!("transfers.state.done")),
                FilterChoice::new("cancelled", l!("transfers.state.cancelled")),
            ],
        ))
        .date_filter(DateFilter::new("planned", l!("transfers.planned")))
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/transfers/new")
                .require(permissions::TRANSFERS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &TransferSummary| format!("/inventory/transfers/{}", row.id),
            )
            .require(permissions::TRANSFERS),
        )
}

fn number_cell(row: &TransferSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("transfers.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

/// Zero shows as a dash: a column of zeroes with two real numbers in it hides
/// the two, and those two are the whole reason the column exists.
fn on_the_road_cell(row: &TransferSummary) -> impl IntoView {
    if !row.in_transit.is_positive() {
        return view! { <span class="text-content-subtle">"—"</span> }.into_any();
    }

    let text = row.in_transit.to_display_string();
    view! { <span class="tabular-nums text-warning">{text}</span> }.into_any()
}

fn state_label(state: TransferState) -> String {
    crate::i18n::t(&state.label())
}

fn state_tone(state: TransferState) -> Tone {
    match state {
        TransferState::Draft => Tone::Neutral,
        TransferState::InTransit => Tone::Warning,
        TransferState::Done => Tone::Success,
        TransferState::Cancelled => Tone::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<TransferSummary> {
        Owner::new().with(transfers_grid)
    }

    /// Mirrors the database sort fields without adding a crate dependency.
    const SERVER_SORTS: &[&str] = &[
        "number",
        "from",
        "to",
        "planned_on",
        "despatched_on",
        "in_transit",
        "reference",
    ];

    /// Mirrors the database search fields.
    const SERVER_SEARCHES: &[&str] = &["number", "from", "to", "reference"];

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

        assert_eq!(sort, Sort::descending("planned_on"));
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
        assert_eq!(range.key(), "planned");
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
                TransferState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
