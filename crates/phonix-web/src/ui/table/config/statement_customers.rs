//! Who there is to run a statement for.
//!
//! Not the parties grid narrowed to customers: that one is the directory, and
//! it is paged, searchable by role and full of people nobody has invoiced. This
//! is the answer to one question - whose account can be read - and the server
//! answers it whole, because a workspace has as many statement customers as it
//! has customers and the list before this one was a dropdown.

use phonix_core::query::Sort;
use phonix_master::party::PartySummary;

use super::GridConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::statement_customers;
use crate::ui::table::{Cell, Column, RowAction, Source};

/// The customers a statement may be run for.
pub fn statement_customers_grid() -> GridConfig<PartySummary> {
    GridConfig::new(
        "statement-customers",
        Source::in_memory(statement_customers),
    )
    .searching(l!("parties.search"))
    .sorted_by(Sort::ascending("name"))
    .min_width("sm:min-w-[36rem]")
    .empty(
        Icon::Receipt,
        l!("parties.empty.title"),
        l!("reports.statement.no_customers"),
    )
    .column(
        Column::new("name", l!("field.name"), |party: &PartySummary| {
            Cell::text(&party.name)
        })
        .findable()
        .pinned()
        .essential(),
    )
    .column(
        Column::new("code", l!("field.code"), |party: &PartySummary| {
            Cell::text(&party.code)
        })
        .findable()
        .essential(),
    )
    .column(
        Column::new("currency", l!("field.currency"), |party: &PartySummary| {
            Cell::maybe(party.currency.map(|currency| currency.code().to_owned()))
        })
        .sortable(),
    )
    .column(
        Column::new("email", l!("field.email"), |party: &PartySummary| {
            Cell::maybe(party.email.clone())
        })
        .searchable()
        .class("text-xs text-content-muted"),
    )
    .action(
        RowAction::link(l!("common.open"), Icon::Receipt, |party: &PartySummary| {
            format!("/accounting/reports/statement/{}", party.id)
        })
        .on_row_click(),
    )
}
