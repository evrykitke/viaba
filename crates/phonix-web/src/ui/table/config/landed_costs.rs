//! Freight, duty and handling, and how much of each stayed on the shelf.
//!
//! Capitalised is its own column beside the total, because the difference
//! between them is the part that went to cost of sales - the number somebody
//! asks about when a month's margin moves and nobody knows why.

use app_inventory::landed_cost::{LandedCostState, LandedCostSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_landed_costs;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn landed_costs_grid() -> GridConfig<LandedCostSummary> {
    GridConfig::new("landed_costs", Source::paged(list_landed_costs))
        .searching(l!("landed_costs.search"))
        .exports_as("landed-costs")
        .sorted_by(Sort::descending("cost_date"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Truck,
            l!("landed_costs.empty.title"),
            l!("landed_costs.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &LandedCostSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "receipt",
                l!("landed_costs.receipt"),
                |row: &LandedCostSummary| Cell::text(&row.receipt_number),
            )
            .searchable()
            .findable()
            .essential()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "supplier",
                l!("purchase_orders.supplier"),
                |row: &LandedCostSummary| Cell::text(&row.supplier_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "cost_date",
                l!("landed_costs.dated"),
                |row: &LandedCostSummary| Cell::text(row.cost_date.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &LandedCostSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new("total", l!("landed_costs.total"), |row: &LandedCostSummary| {
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
                "capitalised",
                l!("landed_costs.capitalised"),
                |row: &LandedCostSummary| Cell::number(row.capitalised.scaled() as f64),
            )
            .sortable()
            .align(Align::End)
            .render(|row| {
                let text = row.capitalised.to_display_string();
                view! { <span class="tabular-nums text-content-muted">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new(
                "charges",
                l!("landed_costs.charges"),
                |row: &LandedCostSummary| Cell::number(row.charge_count as f64),
            )
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        // Filtering is handled by the paged source.
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("draft", l!("landed_costs.state.draft")),
                    FilterChoice::new("done", l!("landed_costs.state.done")),
                    FilterChoice::new("cancelled", l!("landed_costs.state.cancelled")),
                ],
            ),
        )
        .date_filter(DateFilter::new("costed", l!("landed_costs.dated")))
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/landed-costs/new")
                .require(permissions::LANDED_COSTS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &LandedCostSummary| format!("/inventory/landed-costs/{}", row.id),
            )
            .require(permissions::LANDED_COSTS),
        )
}

fn number_cell(row: &LandedCostSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("landed_costs.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

fn state_label(state: LandedCostState) -> String {
    crate::i18n::t(&state.label())
}

fn state_tone(state: LandedCostState) -> Tone {
    match state {
        LandedCostState::Draft => Tone::Neutral,
        LandedCostState::Done => Tone::Success,
        LandedCostState::Cancelled => Tone::Warning,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<LandedCostSummary> {
        Owner::new().with(landed_costs_grid)
    }

    /// Mirrors the database sort fields without adding a crate dependency.
    const SERVER_SORTS: &[&str] = &["number", "receipt", "supplier", "cost_date", "total", "capitalised"];

    /// Mirrors the database search fields.
    const SERVER_SEARCHES: &[&str] = &["number", "receipt", "supplier"];

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

        assert_eq!(sort, Sort::descending("cost_date"));
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
        assert_eq!(range.key(), "costed");
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
                LandedCostState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
