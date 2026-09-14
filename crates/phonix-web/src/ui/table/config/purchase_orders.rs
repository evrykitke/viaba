//! What has been committed to, and how much of it has arrived.
//!
//! # Two states, side by side
//!
//! An order's own state - draft, sent, confirmed - is what somebody did to it.
//! Its *receipt* state is what the supplier did, and it is worked out from the
//! lines rather than stored. Showing both is the point of this screen: a
//! confirmed order that is nothing-received is a chase, and a confirmed order
//! that is fully-received is a bill waiting to be matched.
//!
//! # Paged
//!
//! Nothing deletes from this list - a cancelled order is still what was
//! cancelled - so it grows for as long as the workspace trades. It is a
//! [`Source::paged`], and what follows is what [`audit`](super::audit) sets
//! out: only columns the reader can order by are sortable, only columns it
//! searches are searchable, and the filters and the span carry a key rather
//! than a closure. The lists live in `phonix_db::inventory::purchase` and are
//! checked against this file below.

use app_inventory::purchase::{OrderState, OrderSummary, ReceiptState};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_purchase_orders;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn purchase_orders_grid() -> GridConfig<OrderSummary> {
    GridConfig::new("purchase-orders", Source::paged(list_purchase_orders))
        .searching(l!("purchase_orders.search"))
        .exports_as("purchase-orders")
        .sorted_by(Sort::descending("order_date"))
        .min_width("sm:min-w-[54rem]")
        .empty(
            Icon::ScrollText,
            l!("purchase_orders.empty.title"),
            l!("purchase_orders.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &OrderSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .class("font-mono tabular-nums")
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "supplier",
                l!("purchase_orders.supplier"),
                |row: &OrderSummary| Cell::text(&row.supplier_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "order_date",
                l!("purchase_orders.ordered"),
                |row: &OrderSummary| Cell::text(row.order_date.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "expected_on",
                l!("purchase_orders.expected"),
                |row: &OrderSummary| {
                    Cell::text(
                        row.expected_on
                            .map(|date| date.to_string())
                            .unwrap_or_default(),
                    )
                },
            )
            .sortable()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &OrderSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            // What the supplier did, as opposed to what we did.
            Column::new(
                "received",
                l!("purchase_orders.received"),
                |row: &OrderSummary| Cell::text(received_label(row.receipt_state)),
            )
            .essential()
            .render(|row| {
                view! {
                    <Badge
                        label=received_label(row.receipt_state)
                        tone=received_tone(row.receipt_state)
                    />
                }
                .into_any()
            }),
        )
        .column(
            Column::new("net", l!("purchase_orders.net"), |row: &OrderSummary| {
                Cell::number(row.net.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = format!("{} {}", row.net.to_display_string(), row.currency);
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new(
                "warehouse",
                l!("nav.warehouses"),
                |row: &OrderSummary| Cell::text(&row.warehouse_name),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "line_count",
                l!("purchase_orders.lines"),
                |row: &OrderSummary| Cell::number(row.line_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        // The values are groups rather than states - a draft and one that has
        // been sent are the same question to somebody scanning - and the
        // grouping is `OrderState::group`, so the reader turns each back into
        // the states it covers rather than the two lists being written out
        // twice. No `matching`: a closure could only narrow the rows already
        // fetched.
        .filter(Filter::new(
            "state",
            l!("field.status"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("open", l!("purchase_orders.state.confirmed")),
                FilterChoice::new("draft", l!("purchase_orders.state.draft")),
                FilterChoice::new("done", l!("purchase_orders.state.done")),
                FilterChoice::new("cancelled", l!("purchase_orders.state.cancelled")),
            ],
        ))
        .filter(Filter::new(
            "received",
            l!("purchase_orders.received"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("outstanding", l!("purchase_orders.received.partly")),
                FilterChoice::new("complete", l!("purchase_orders.received.everything")),
            ],
        ))
        .date_filter(DateFilter::new("ordered", l!("purchase_orders.ordered")))
        .toolbar(
            ToolbarAction::link(
                l!("purchase_orders.new"),
                Icon::Plus,
                "/inventory/orders/new",
            )
            .require(permissions::PURCHASE_ORDERS_CREATE)
            .primary(),
        )
        .action(
            RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &OrderSummary| {
                format!("/inventory/orders/{}", row.id)
            })
            .require(permissions::PURCHASE_ORDERS),
        )
}

/// The number, or what a draft is recognisable by before it has one.
fn number_cell(row: &OrderSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">
                {l!("purchase_orders.state.draft")}
            </span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

fn state_label(state: OrderState) -> String {
    match state {
        OrderState::Draft => l!("purchase_orders.state.draft"),
        OrderState::Sent => l!("purchase_orders.state.sent"),
        OrderState::Confirmed => l!("purchase_orders.state.confirmed"),
        OrderState::Done => l!("purchase_orders.state.done"),
        OrderState::Cancelled => l!("purchase_orders.state.cancelled"),
    }
}

const fn state_tone(state: OrderState) -> Tone {
    match state {
        OrderState::Confirmed => Tone::Brand,
        OrderState::Done => Tone::Success,
        OrderState::Cancelled => Tone::Danger,
        OrderState::Draft | OrderState::Sent => Tone::Neutral,
    }
}

fn received_label(state: ReceiptState) -> String {
    match state {
        ReceiptState::Nothing => l!("purchase_orders.received.nothing"),
        ReceiptState::Partly => l!("purchase_orders.received.partly"),
        ReceiptState::Everything => l!("purchase_orders.received.everything"),
        ReceiptState::Over => l!("purchase_orders.received.over"),
    }
}

/// An over-receipt is a warning rather than a success: it is not wrong, and
/// somebody should look at it.
const fn received_tone(state: ReceiptState) -> Tone {
    match state {
        ReceiptState::Nothing => Tone::Neutral,
        ReceiptState::Partly => Tone::Brand,
        ReceiptState::Everything => Tone::Success,
        ReceiptState::Over => Tone::Warning,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<OrderSummary> {
        Owner::new().with(purchase_orders_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is
    /// `phonix_db::inventory::purchase::SORTABLE`.
    const SERVER_SORTS: &[&str] = &[
        "number",
        "supplier",
        "order_date",
        "expected_on",
        "net",
        "line_count",
    ];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "supplier", "warehouse"];

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

        assert_eq!(sort, Sort::descending("order_date"));
        assert!(SERVER_SORTS.contains(&sort.field.as_str()));
    }

    #[test]
    fn the_filters_and_the_span_leave_the_answering_to_the_server() {
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

        // `phonix_db::inventory::purchase::ORDERED`.
        assert_eq!(range.key(), "ordered");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_group_offered_covers_at_least_one_state() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                !OrderState::in_group(choice.value).is_empty(),
                "{} is offered and covers nothing",
                choice.value,
            );
        }
    }

    #[test]
    fn every_state_is_offered_under_some_group() {
        // A state in no group is an order nobody can filter for, and it would
        // be the cancelled ones - the list somebody goes looking for.
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for state in OrderState::ALL {
            assert!(
                states.choices.iter().any(|c| c.value == state.group()),
                "{} is in no group the grid offers",
                state.as_str(),
            );
        }
    }
}
