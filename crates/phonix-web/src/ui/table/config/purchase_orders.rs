//! What has been committed to, and how much of it has arrived.
//!
//! # Two states, side by side
//!
//! An order's own state - draft, sent, confirmed - is what somebody did to it.
//! Its *receipt* state is what the supplier did, and it is worked out from the
//! lines rather than stored. Showing both is the point of this screen: a
//! confirmed order that is nothing-received is a chase, and a confirmed order
//! that is fully-received is a bill waiting to be matched.

use app_inventory::purchase::{OrderState, OrderSummary, ReceiptState};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_purchase_orders;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn purchase_orders_grid() -> GridConfig<OrderSummary> {
    GridConfig::new("purchase-orders", Source::in_memory(list_purchase_orders))
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
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("open", l!("purchase_orders.state.confirmed")),
                    FilterChoice::new("draft", l!("purchase_orders.state.draft")),
                    FilterChoice::new("done", l!("purchase_orders.state.done")),
                    FilterChoice::new("cancelled", l!("purchase_orders.state.cancelled")),
                ],
            )
            .matching(|row: &OrderSummary, wanted| match wanted {
                "open" => matches!(row.state, OrderState::Confirmed),
                "draft" => matches!(row.state, OrderState::Draft | OrderState::Sent),
                "done" => matches!(row.state, OrderState::Done),
                "cancelled" => matches!(row.state, OrderState::Cancelled),
                _ => true,
            }),
        )
        .filter(
            Filter::new(
                "received",
                l!("purchase_orders.received"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("outstanding", l!("purchase_orders.received.partly")),
                    FilterChoice::new("complete", l!("purchase_orders.received.everything")),
                ],
            )
            .matching(|row: &OrderSummary, wanted| match wanted {
                "outstanding" => matches!(
                    row.receipt_state,
                    ReceiptState::Nothing | ReceiptState::Partly
                ),
                "complete" => matches!(
                    row.receipt_state,
                    ReceiptState::Everything | ReceiptState::Over
                ),
                _ => true,
            }),
        )
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
