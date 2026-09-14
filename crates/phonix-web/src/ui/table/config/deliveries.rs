//! What has gone out, to whom, and what it took off the balance sheet.
//!
//! # The consignment number is the column somebody searches
//!
//! Not our delivery number. When a customer rings about a parcel they quote the
//! carrier's reference, and a screen that could only be searched by our number
//! makes the person on the phone read out something the caller does not have.
//! The mirror of the receipts grid, where the supplier's note number is the
//! searchable one for the same reason.

use app_inventory::delivery::{DeliveryState, DeliverySummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_deliveries;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn deliveries_grid() -> GridConfig<DeliverySummary> {
    GridConfig::new("deliveries", Source::paged(list_deliveries))
        .searching(l!("deliveries.search"))
        .exports_as("deliveries")
        .sorted_by(Sort::descending("despatched_on"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Truck,
            l!("deliveries.empty.title"),
            l!("deliveries.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &DeliverySummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "customer",
                l!("sales_orders.customer"),
                |row: &DeliverySummary| Cell::text(&row.customer_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "despatched_on",
                l!("deliveries.despatched"),
                |row: &DeliverySummary| Cell::text(row.despatched_on.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "carrier_reference",
                l!("deliveries.carrier"),
                |row: &DeliverySummary| {
                    Cell::text(row.carrier_reference.clone().unwrap_or_default())
                },
            )
            .searchable()
            .essential()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new("order", l!("deliveries.order"), |row: &DeliverySummary| {
                Cell::text(row.order_number.clone().unwrap_or_default())
            })
            .findable()
            .render(|row| order_cell(row).into_any()),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &DeliverySummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            // Cost, not price. A despatch posts what the goods were worth to
            // this workspace; what the customer pays is the invoice's column.
            Column::new("value", l!("deliveries.value"), |row: &DeliverySummary| {
                Cell::number(row.value.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = row.value.to_display_string();
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new(
                "warehouse",
                l!("nav.warehouses"),
                |row: &DeliverySummary| Cell::text(&row.warehouse_name),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "line_count",
                l!("deliveries.lines"),
                |row: &DeliverySummary| Cell::number(row.line_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        // No `matching`: a closure could only narrow the rows already
        // fetched, and "only the drafts" is a question about the list.
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("done", l!("deliveries.state.done")),
                    FilterChoice::new("draft", l!("deliveries.state.draft")),
                    FilterChoice::new("cancelled", l!("deliveries.state.cancelled")),
                ],
            ),
        )
        .date_filter(DateFilter::new("despatched", l!("deliveries.despatched")))
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/deliveries/new")
                .require(permissions::DELIVERIES_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &DeliverySummary| format!("/inventory/deliveries/{}", row.id),
            )
            .require(permissions::DELIVERIES),
        )
}

fn number_cell(row: &DeliverySummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("deliveries.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

/// The order behind it, or a note that there was none - which is ordinary, not
/// a gap: a sample and a replacement go out without one.
fn order_cell(row: &DeliverySummary) -> impl IntoView {
    match row.order_number.clone() {
        Some(number) => {
            view! { <span class="font-mono text-xs tabular-nums">{number}</span> }.into_any()
        }
        None => {
            view! { <span class="text-xs text-content-muted">{l!("common.none")}</span> }.into_any()
        }
    }
}

fn state_label(state: DeliveryState) -> String {
    match state {
        DeliveryState::Draft => l!("deliveries.state.draft"),
        DeliveryState::Done => l!("deliveries.state.done"),
        DeliveryState::Cancelled => l!("deliveries.state.cancelled"),
    }
}

const fn state_tone(state: DeliveryState) -> Tone {
    match state {
        DeliveryState::Draft => Tone::Neutral,
        DeliveryState::Done => Tone::Success,
        DeliveryState::Cancelled => Tone::Danger,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<DeliverySummary> {
        Owner::new().with(deliveries_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is
    /// `phonix_db::inventory::delivery::SORTABLE`.
    const SERVER_SORTS: &[&str] = &["number", "customer", "despatched_on", "order", "value", "line_count"];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "customer", "carrier_reference", "order", "warehouse"];

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

        assert_eq!(sort, Sort::descending("despatched_on"));
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

        // `phonix_db::inventory::delivery::DESPATCHED`, written down twice because the
        // two crates do not depend on each other.
        assert_eq!(range.key(), "despatched");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_offered_is_one_the_reader_parses_back() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                DeliveryState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
