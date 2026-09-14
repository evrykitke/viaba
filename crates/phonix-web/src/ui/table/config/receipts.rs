//! What arrived, from whom, and what it put on the balance sheet.
//!
//! # The delivery note is the column somebody searches
//!
//! Not the receipt number. When a supplier rings about a shortage they quote
//! their own paperwork, and a screen that could only be searched by our number
//! makes the person on the phone read out something the caller does not have.

use app_inventory::receipt::{ReceiptState, ReceiptSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_receipts;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn receipts_grid() -> GridConfig<ReceiptSummary> {
    GridConfig::new("receipts", Source::paged(list_receipts))
        .searching(l!("receipts.search"))
        .exports_as("goods-receipts")
        .sorted_by(Sort::descending("received_on"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Package,
            l!("receipts.empty.title"),
            l!("receipts.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &ReceiptSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "supplier",
                l!("purchase_orders.supplier"),
                |row: &ReceiptSummary| Cell::text(&row.supplier_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "received_on",
                l!("receipts.received_on"),
                |row: &ReceiptSummary| Cell::text(row.received_on.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "delivery_note",
                l!("receipts.delivery_note"),
                |row: &ReceiptSummary| {
                    Cell::text(row.delivery_note.clone().unwrap_or_default())
                },
            )
            .searchable()
            .essential()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new("order", l!("receipts.order"), |row: &ReceiptSummary| {
                Cell::text(row.order_number.clone().unwrap_or_default())
            })
            .findable()
            .render(|row| order_cell(row).into_any()),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &ReceiptSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new("value", l!("receipts.value"), |row: &ReceiptSummary| {
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
                |row: &ReceiptSummary| Cell::text(&row.warehouse_name),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "line_count",
                l!("purchase_orders.lines"),
                |row: &ReceiptSummary| Cell::number(row.line_count as f64),
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
                    FilterChoice::new("done", l!("receipts.state.done")),
                    FilterChoice::new("draft", l!("receipts.state.draft")),
                    FilterChoice::new("cancelled", l!("receipts.state.cancelled")),
                ],
            ),
        )
        .date_filter(DateFilter::new("received", l!("receipts.received_on")))
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/receipts/new")
                .require(permissions::RECEIPTS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &ReceiptSummary| format!("/inventory/receipts/{}", row.id),
            )
            .require(permissions::RECEIPTS),
        )
}

fn number_cell(row: &ReceiptSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("receipts.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

/// The order behind it, or a note that there was none - which is ordinary, not
/// a gap: samples and customer returns arrive without one.
fn order_cell(row: &ReceiptSummary) -> impl IntoView {
    match row.order_number.clone() {
        Some(number) => {
            view! { <span class="font-mono text-xs tabular-nums">{number}</span> }.into_any()
        }
        None => view! {
            <span class="text-xs text-content-muted">{l!("common.none")}</span>
        }
        .into_any(),
    }
}

fn state_label(state: ReceiptState) -> String {
    match state {
        ReceiptState::Draft => l!("receipts.state.draft"),
        ReceiptState::Done => l!("receipts.state.done"),
        ReceiptState::Cancelled => l!("receipts.state.cancelled"),
    }
}

const fn state_tone(state: ReceiptState) -> Tone {
    match state {
        ReceiptState::Draft => Tone::Neutral,
        ReceiptState::Done => Tone::Success,
        ReceiptState::Cancelled => Tone::Danger,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<ReceiptSummary> {
        Owner::new().with(receipts_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is
    /// `phonix_db::inventory::receipt::SORTABLE`.
    const SERVER_SORTS: &[&str] = &["number", "supplier", "received_on", "order", "value", "line_count"];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "supplier", "delivery_note", "order", "warehouse"];

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

        assert_eq!(sort, Sort::descending("received_on"));
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

        // `phonix_db::inventory::receipt::RECEIVED`, written down twice because the
        // two crates do not depend on each other.
        assert_eq!(range.key(), "received");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_offered_is_one_the_reader_parses_back() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                ReceiptState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
