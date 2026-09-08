//! What suppliers charged, and how well it matched.
//!
//! The supplier's own invoice number is a findable column, because that is what
//! somebody has in front of them when the supplier rings.

use app_inventory::bill::{BillState, BillSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_bills;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn bills_grid() -> GridConfig<BillSummary> {
    GridConfig::new("bills", Source::in_memory(list_bills))
        .searching(l!("bills.search"))
        .exports_as("supplier-bills")
        .sorted_by(Sort::descending("bill_date"))
        .min_width("sm:min-w-[54rem]")
        .empty(
            Icon::Receipt,
            l!("bills.empty.title"),
            l!("bills.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &BillSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new("supplier", l!("purchase_orders.supplier"), |row: &BillSummary| {
                Cell::text(&row.supplier_name)
            })
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new("reference", l!("bills.reference"), |row: &BillSummary| {
                Cell::text(&row.supplier_reference)
            })
            .searchable()
            .findable()
            .essential()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new("bill_date", l!("bills.dated"), |row: &BillSummary| {
                Cell::text(row.bill_date.to_string())
            })
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new("due_on", l!("bills.due"), |row: &BillSummary| {
                Cell::text(row.due_on.map(|due| due.to_string()).unwrap_or_default())
            })
            .sortable()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("order", l!("receipts.order"), |row: &BillSummary| {
                Cell::text(row.order_number.clone().unwrap_or_default())
            })
            .findable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &BillSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new("net", l!("bills.net"), |row: &BillSummary| {
                Cell::number(row.net.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = row.net.to_display_string();
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new("variance", l!("bills.variance"), |row: &BillSummary| {
                Cell::number(row.variance.scaled() as f64)
            })
            .sortable()
            .align(Align::End)
            .render(|row| variance_cell(row).into_any()),
        )
        .column(
            Column::new("lines", l!("bills.lines"), |row: &BillSummary| {
                Cell::number(row.line_count as f64)
            })
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::new("draft", l!("bills.state.draft")),
                    FilterChoice::new("posted", l!("bills.state.posted")),
                    FilterChoice::new("cancelled", l!("bills.state.cancelled")),
                ],
            )
            .matching(|row: &BillSummary, wanted| match wanted {
                    "draft" => matches!(row.state, BillState::Draft),
                    "posted" => matches!(row.state, BillState::Posted),
                    "cancelled" => matches!(row.state, BillState::Cancelled),
                    _ => true,
                }),
        )
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/bills/new")
                .require(permissions::BILLS_CREATE)
                .primary(),
        )
        .toolbar(
            ToolbarAction::link(
                l!("bills.unbilled.title"),
                Icon::Clock,
                "/inventory/unbilled",
            )
            .require(permissions::BILLS),
        )
        .action(
            RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &BillSummary| {
                format!("/inventory/bills/{}", row.id)
            })
            .require(permissions::BILLS),
        )
}

fn number_cell(row: &BillSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("bills.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    let overridden = row.was_overridden;

    view! {
        <span class="font-mono tabular-nums">
            {number}
            // A posted bill whose match was forced says so in the list, not
            // only on the document.
            {overridden
                .then(|| {
                    view! {
                        <span class="ml-1 text-2xs not-italic text-warning" title=l!("bills.match_note")>
                            "!"
                        </span>
                    }
                })}
        </span>
    }
    .into_any()
}

/// Zero shows as nothing rather than as `0.00`: a column of zeroes with three
/// real numbers in it hides the three.
fn variance_cell(row: &BillSummary) -> impl IntoView {
    if row.variance.is_zero() {
        return view! { <span class="text-content-subtle">"—"</span> }.into_any();
    }

    let text = row.variance.to_display_string();
    let tone = if row.variance.is_negative() {
        "tabular-nums text-success"
    } else {
        "tabular-nums text-warning"
    };

    view! { <span class=tone>{text}</span> }.into_any()
}

fn state_label(state: BillState) -> String {
    crate::i18n::t(&state.label())
}

fn state_tone(state: BillState) -> Tone {
    match state {
        BillState::Draft => Tone::Neutral,
        BillState::Posted => Tone::Success,
        BillState::Cancelled => Tone::Warning,
    }
}
