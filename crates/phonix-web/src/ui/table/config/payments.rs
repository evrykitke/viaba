//! What customers have paid, and how much of it was set against something.
//!
//! # Two money columns, and the second is the interesting one
//!
//! The amount is what arrived. What is *on account* - received and allocated to
//! no invoice - is the column somebody scans for, because it is the one that
//! means a conversation: a customer paid a round sum and nobody has said which
//! invoices it clears.

use app_books::payment::{PaymentStatus, PaymentSummary};
use leptos::prelude::*;
use phonix_core::money::Money;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::list_payments;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn payments_grid() -> GridConfig<PaymentSummary> {
    GridConfig::new("payments", Source::in_memory(list_payments))
        .searching(l!("payments.search"))
        .exports_as("payments")
        .sorted_by(Sort::descending("received_on"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Receipt,
            l!("payments.empty.title"),
            l!("payments.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &PaymentSummary| {
                Cell::text(row.number.clone().unwrap_or_default())
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "customer",
                l!("payments.customer"),
                |row: &PaymentSummary| Cell::text(&row.party_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "received_on",
                l!("payments.received_on"),
                |row: &PaymentSummary| Cell::text(row.received_on.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new("account", l!("payments.account"), |row: &PaymentSummary| {
                Cell::text(&row.account_name)
            })
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "reference",
                l!("payments.reference"),
                |row: &PaymentSummary| Cell::text(row.reference.clone().unwrap_or_default()),
            )
            .searchable()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new("status", l!("field.status"), |row: &PaymentSummary| {
                Cell::text(status_label(row.status))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=status_label(row.status) tone=status_tone(row.status) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new("amount", l!("payments.amount"), |row: &PaymentSummary| {
                Cell::number(row.amount.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = format!("{} {}", row.amount.to_display_string(), row.currency.code());
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new(
                "on_account",
                l!("payments.on_account"),
                |row: &PaymentSummary| {
                    Cell::number(on_account(row).map_or(0.0, |left| left.scaled() as f64))
                },
            )
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| on_account_cell(row).into_any()),
        )
        .filter(
            Filter::new(
                "status",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("posted", l!("payments.status.posted")),
                    FilterChoice::new("draft", l!("payments.status.draft")),
                    FilterChoice::new("voided", l!("payments.status.voided")),
                ],
            )
            .matching(|row: &PaymentSummary, wanted| match wanted {
                "posted" => matches!(row.status, PaymentStatus::Posted),
                "draft" => matches!(row.status, PaymentStatus::Draft),
                "voided" => matches!(row.status, PaymentStatus::Voided),
                _ => true,
            }),
        )
        .filter(
            // The question this screen is opened for: whose money is sitting
            // against nothing.
            Filter::new(
                "allocation",
                l!("payments.on_account"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("unallocated", l!("payments.on_account")),
                ],
            )
            .matching(|row: &PaymentSummary, wanted| match wanted {
                "unallocated" => on_account(row).is_some_and(|left| !left.is_zero()),
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("payments.new"), Icon::Plus, "/sales/payments/new")
                .require(permissions::PAYMENTS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &PaymentSummary| format!("/sales/payments/{}", row.id),
            )
            .require(permissions::PAYMENTS),
        )
}

/// Received and set against nothing. `None` only where the two amounts are in
/// different currencies, which the schema does not allow.
fn on_account(row: &PaymentSummary) -> Option<Money> {
    row.on_account().ok()
}

fn number_cell(row: &PaymentSummary) -> impl IntoView {
    match row.number.clone() {
        Some(number) => {
            view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
        }
        None => view! {
            <span class="text-xs italic text-content-muted">{l!("payments.status.draft")}</span>
        }
        .into_any(),
    }
}

/// Drawn only where there is some. A nought in this column on every fully
/// allocated payment is a column of noughts nobody reads.
fn on_account_cell(row: &PaymentSummary) -> impl IntoView {
    match on_account(row) {
        Some(left) if !left.is_zero() => {
            let text = left.to_display_string();
            view! { <span class="tabular-nums font-medium text-content">{text}</span> }.into_any()
        }
        _ => ().into_any(),
    }
}

fn status_label(status: PaymentStatus) -> String {
    match status {
        PaymentStatus::Draft => l!("payments.status.draft"),
        PaymentStatus::Posted => l!("payments.status.posted"),
        PaymentStatus::Voided => l!("payments.status.voided"),
    }
}

const fn status_tone(status: PaymentStatus) -> Tone {
    match status {
        PaymentStatus::Draft => Tone::Neutral,
        PaymentStatus::Posted => Tone::Success,
        PaymentStatus::Voided => Tone::Danger,
    }
}
