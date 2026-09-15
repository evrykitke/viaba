//! What customers have paid, and how much of it was set against something.
//!
//! # Two money columns, and the second is the interesting one
//!
//! The amount is what arrived. What is *on account* - received and allocated to
//! no invoice - is the column somebody scans for, because it is the one that
//! means a conversation: a customer paid a round sum and nobody has said which
//! invoices it clears. It is a subtraction in SQL, so the grid sorts and
//! narrows by it without the browser holding a single allocation.
//!
//! # Paged
//!
//! A receipt is evidence and nothing deletes one, so the list only grows - and
//! faster than the invoice list wherever customers pay in instalments. What
//! follows from [`Source::paged`] is what [`audit`](super::audit) sets out, and
//! the lists it has to agree with are in `phonix_db::books::payment`.

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
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn payments_grid() -> GridConfig<PaymentSummary> {
    GridConfig::new("payments", Source::paged(list_payments))
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
        // The values are the domain's own spellings, because that is what the
        // reader parses them back into. No `matching`: a closure could only
        // narrow the twenty-five rows already fetched.
        .filter(Filter::new(
            "status",
            l!("field.status"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new(PaymentStatus::Posted.as_str(), l!("payments.status.posted")),
                FilterChoice::new(PaymentStatus::Draft.as_str(), l!("payments.status.draft")),
                FilterChoice::new(PaymentStatus::Voided.as_str(), l!("payments.status.voided")),
            ],
        ))
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
            ),
        )
        .date_filter(DateFilter::new("received", l!("payments.received_on")))
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
        Some(number) => view! { <span class="font-mono tabular-nums">{number}</span> }.into_any(),
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

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<PaymentSummary> {
        Owner::new().with(payments_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is `phonix_db::books::payment::SORTABLE`.
    const SERVER_SORTS: &[&str] = &["number", "customer", "received_on", "amount", "on_account"];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "customer", "account", "reference"];

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
    fn both_filters_and_the_span_leave_the_answering_to_the_server() {
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

        // `phonix_db::books::payment::RECEIVED`, written down twice because the
        // two crates do not depend on each other.
        assert_eq!(range.key(), "received");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_offered_is_one_the_reader_parses_back() {
        let grid = grid();
        let status = grid.filters.iter().find(|f| f.key() == "status").unwrap();

        for choice in status.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                PaymentStatus::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }

        // `phonix_db::books::payment::ALLOCATION` and `UNALLOCATED`.
        let allocation = grid
            .filters
            .iter()
            .find(|f| f.key() == "allocation")
            .unwrap();

        assert!(allocation.choices.iter().any(|c| c.value == "unallocated"));
    }
}
