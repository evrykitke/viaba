//! The invoices grid.
//!
//! # Why "overdue" is a filter and not a badge
//!
//! Whether a document is overdue depends on today, and this grid renders twice:
//! once on the server and once in the browser at hydration. Near midnight those
//! two have different dates, and a badge that appears on one side and not the
//! other is the fatal kind of hydration mismatch - see `phonix_web::recovery`
//! for what a wasm panic costs.
//!
//! So the due date is drawn plainly and "show me what is late" is one click. It
//! is now answered in SQL against the server's `current_date`, which is the
//! only clock in the building that cannot disagree with itself.
//!
//! # Paged
//!
//! A sales ledger is a list nothing deletes from: last year's invoices are
//! still evidence. So it is a [`Source::paged`], and what follows is what
//! [`audit`](super::audit) sets out - only columns the reader can order by are
//! sortable, only columns it searches are searchable, and the status filter and
//! the span carry a key rather than a closure. The lists live in
//! `phonix_db::books::invoice` and are checked against this file below.
//!
//! # A draft says so rather than showing a blank
//!
//! It has no number, because a number is taken at post and never before. An
//! empty cell would read as missing data; "Draft" reads as what it is.

use app_books::invoice::{InvoiceStatus, InvoiceSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::{InvoiceQuery, list_invoices};
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

/// Everything this workspace has invoiced.
pub fn invoices_grid() -> GridConfig<InvoiceSummary> {
    GridConfig::new(
        "invoices",
        // Unnarrowed, because this screen is every invoice. One customer's
        // ledger would be the same grid handed a query naming them.
        Source::paged(|request| list_invoices(InvoiceQuery::default(), request)),
    )
    .searching(l!("invoices.search"))
    .exports_as("invoices")
    .sorted_by(Sort::descending("issued_on"))
    .min_width("sm:min-w-[52rem]")
    .empty(
        Icon::FileText,
        l!("invoices.empty.title"),
        l!("invoices.empty.detail"),
    )
    .column(
        Column::new("number", l!("invoices.number"), |row: &InvoiceSummary| {
            // Sorts and exports as the number, or as nothing for a draft - so a
            // column sorted by number puts the drafts together rather than
            // scattering them under the word "Draft".
            Cell::maybe(row.number.clone())
        })
        .findable()
        // Without it a row is a name and an amount.
        .pinned()
        .essential()
        .render(|row| number_cell(row).into_any()),
    )
    .column(
        Column::new(
            "party_name",
            l!("invoices.customer"),
            |row: &InvoiceSummary| Cell::text(&row.party_name),
        )
        .findable()
        // The second thing somebody scans for.
        .essential(),
    )
    .column(
        Column::new(
            "issued_on",
            l!("invoices.issued"),
            |row: &InvoiceSummary| Cell::text(row.issued_on.to_string()),
        )
        .sortable()
        .class("whitespace-nowrap tabular-nums text-content-muted"),
    )
    .column(
        Column::new("due_on", l!("invoices.due"), |row: &InvoiceSummary| {
            Cell::maybe(row.due_on.map(|due| due.to_string()))
        })
        .sortable()
        .class("whitespace-nowrap tabular-nums text-content-muted"),
    )
    .column(
        Column::new("status", l!("field.status"), |row: &InvoiceSummary| {
            Cell::text(row.status.as_str())
        })
        .sortable()
        .essential()
        .render(|row| status_cell(row.status).into_any()),
    )
    .column(
        Column::new("net", l!("invoices.net"), |row: &InvoiceSummary| {
            // The scaled integer, so sorting is numeric: "9.00" and "10.00"
            // sort the wrong way round as strings, and a list of money sorted
            // like that is one nobody trusts.
            Cell::number(row.net.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .hidden()
        .render(|row| amount_cell(&row.net).into_any()),
    )
    .column(
        Column::new("tax", l!("invoices.tax"), |row: &InvoiceSummary| {
            Cell::number(row.tax.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .hidden()
        .render(|row| amount_cell(&row.tax).into_any()),
    )
    .column(
        Column::new("gross", l!("invoices.total"), |row: &InvoiceSummary| {
            Cell::number(row.gross.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .render(|row| amount_cell(&row.gross).into_any()),
    )
    .column(
        Column::new(
            "line_count",
            l!("invoices.lines"),
            |row: &InvoiceSummary| Cell::number(row.line_count as f64),
        )
        .sortable()
        .align(Align::End)
        .hidden(),
    )
    // No `matching`: the reader answers these, and `overdue` is the reason to
    // be glad of it - it is a question about today, and the server's today is
    // the same one on both renders of this page.
    .filter(Filter::new(
        "status",
        l!("field.status"),
        vec![
            FilterChoice::all(l!("common.all")),
            FilterChoice::new(InvoiceStatus::Draft.as_str(), l!("books.status.draft")),
            FilterChoice::new(InvoiceStatus::Posted.as_str(), l!("books.status.posted")),
            FilterChoice::new(InvoiceStatus::Voided.as_str(), l!("books.status.voided")),
            FilterChoice::new("overdue", l!("invoices.overdue")),
        ],
    ))
    .date_filter(DateFilter::new("issued", l!("invoices.issued")))
    .toolbar(
        ToolbarAction::link(l!("invoices.new"), Icon::Plus, "/sales/invoices/new")
            .require(permissions::INVOICES_CREATE)
            .primary(),
    )
    .action(
        RowAction::link(l!("common.open"), Icon::Eye, |row: &InvoiceSummary| {
            format!("/sales/invoices/{}", row.id)
        })
        .require(permissions::INVOICES),
    )
    .action(
        RowAction::link(l!("common.edit"), Icon::Pencil, |row: &InvoiceSummary| {
            format!("/sales/invoices/{}", row.id)
        })
        // Only a draft can be edited. Offering it on a posted document would be
        // offering a button that only ever produces a refusal.
        .when(|row: &InvoiceSummary| row.status.is_editable())
        .require(permissions::INVOICES_EDIT),
    )
}

/// The number, or the word for a draft.
fn number_cell(row: &InvoiceSummary) -> impl IntoView {
    let number = row.number.clone();

    view! {
        {match number {
            Some(number) => {
                view! { <span class="font-medium tabular-nums text-content">{number}</span> }
                    .into_any()
            }
            // Never the number it is *going* to get: promising before the post
            // promises something that may not be kept.
            None => {
                view! {
                    <span class="text-xs italic text-content-subtle">
                        {l!("books.status.draft")}
                    </span>
                }
                    .into_any()
            }
        }}
    }
}

fn status_cell(status: InvoiceStatus) -> impl IntoView {
    let label = crate::i18n::t(&status.label());
    let tone = match status {
        InvoiceStatus::Draft => Tone::Neutral,
        InvoiceStatus::Posted => Tone::Success,
        // Not danger: withdrawing a document is an ordinary correction, and
        // painting it red every time somebody opens the list is noise.
        InvoiceStatus::Voided => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

/// An amount with its currency, right-aligned and monospaced so the columns
/// line up under each other.
fn amount_cell(amount: &phonix_core::money::Money) -> impl IntoView {
    let text = amount.to_display_string();
    let code = amount.currency().code().to_owned();

    view! {
        <span class="whitespace-nowrap tabular-nums">
            <span class="text-2xs text-content-subtle">{code}</span>
            " "
            <span class="text-content">{text}</span>
        </span>
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use phonix_core::locale::Currency;
    use phonix_core::money::Money;
    use uuid::Uuid;

    use super::*;

    fn grid() -> GridConfig<InvoiceSummary> {
        Owner::new().with(invoices_grid)
    }

    fn usd() -> Currency {
        Currency::parse("USD").unwrap()
    }

    fn invoice(number: Option<&str>, status: InvoiceStatus, gross: &str) -> InvoiceSummary {
        InvoiceSummary {
            id: Uuid::nil(),
            number: number.map(str::to_owned),
            status,
            party_id: Uuid::nil(),
            party_name: "Acme".to_owned(),
            issued_on: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            due_on: NaiveDate::from_ymd_opt(2026, 7, 1),
            currency: usd(),
            net: Money::parse(usd(), gross).unwrap(),
            tax: Money::zero(usd()),
            gross: Money::parse(usd(), gross).unwrap(),
            line_count: 1,
        }
    }

    #[test]
    fn a_phone_keeps_the_number_the_customer_and_the_state() {
        let grid = grid();
        let essential: Vec<&str> = grid
            .columns
            .iter()
            .filter(|c| c.essential)
            .map(|c| c.field())
            .collect();

        assert_eq!(essential, vec!["number", "party_name", "status"]);
    }

    #[test]
    fn money_columns_sort_numerically_rather_than_by_their_printed_text() {
        // "9.00" and "10.00" sort the wrong way round as strings.
        let grid = grid();
        let total = grid.columns.iter().find(|c| c.field() == "gross").unwrap();

        let small = total.value(&invoice(None, InvoiceStatus::Draft, "9.00"));
        let large = total.value(&invoice(None, InvoiceStatus::Draft, "10.00"));
        assert_eq!(small.compare(&large), std::cmp::Ordering::Less);
    }

    #[test]
    fn overdue_is_not_the_filter_the_grid_opens_on() {
        // Not for safety any more - the server answers it against its own
        // `current_date`, and there is no second clock to disagree with. It is
        // that a list of invoices opens as a list of invoices.
        let grid = grid();
        let filter = grid.filters.iter().find(|f| f.key() == "status").unwrap();

        assert_ne!(filter.default_value(), "overdue");
        assert!(
            grid.initial_request().filter("status").is_none(),
            "the grid must not open on a date-dependent filter",
        );
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is `phonix_db::books::invoice::SORTABLE`.
    const SERVER_SORTS: &[&str] = &[
        "number",
        "party_name",
        "issued_on",
        "due_on",
        "status",
        "net",
        "tax",
        "gross",
        "line_count",
    ];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "party_name"];

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

        assert_eq!(sort, Sort::descending("issued_on"));
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

        // `phonix_db::books::invoice::ISSUED`, written down twice because the
        // two crates do not depend on each other.
        assert_eq!(range.key(), "issued");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_offered_is_a_state_or_the_one_question_that_is_not() {
        let grid = grid();
        let status = grid.filters.iter().find(|f| f.key() == "status").unwrap();

        for choice in status.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                choice.value == "overdue" || InvoiceStatus::parse(choice.value).is_some(),
                "{} is offered and the reader ignores it",
                choice.value,
            );
        }

        // `phonix_db::books::invoice::OVERDUE`. The one value that is a
        // question about today rather than a column.
        assert!(status.choices.iter().any(|c| c.value == "overdue"));
    }

    #[test]
    fn only_a_draft_is_offered_an_edit_button() {
        // A posted invoice cannot be edited, so the button could only ever
        // produce a refusal.
        let grid = grid();
        let edit = grid.actions.iter().find(|a| a.label == "Edit").unwrap();

        assert!(edit.applies_to(&invoice(None, InvoiceStatus::Draft, "10.00")));
        assert!(!edit.applies_to(&invoice(
            Some("INV-2026-00001"),
            InvoiceStatus::Posted,
            "10.00"
        )));
    }

    #[test]
    fn there_is_no_delete_and_no_post_on_a_row() {
        // Both are decisions with consequences - one destroys a draft, the
        // other takes a number nobody can hand back. They belong on the
        // document, where the whole thing is in front of the reader, not in a
        // menu at the end of a row.
        let grid = grid();

        for label in ["Delete", "Post", "Void"] {
            assert!(
                !grid.actions.iter().any(|a| a.label == label),
                "{label} must not be a row action",
            );
        }
    }

    #[test]
    fn every_action_names_a_permission() {
        let grid = grid();

        for action in &grid.actions {
            assert!(action.permission.is_some(), "{} is ungated", action.label);
        }
    }
}
