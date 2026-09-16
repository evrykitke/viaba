//! What a customer is handed when they have paid.
//!
//! The first report over a single record rather than a span, which is the
//! shape an invoice, a delivery note and a purchase order all take after it.

use app_books::payment::{Allocation, Payment};
use phonix_core::money::Money;
use phonix_core::report::{Align, BandKind, Logo, LogoPlacement, ReportKind, ReportTheme};

use crate::l;
use crate::ui::report::{Band, Field, ReportDefinition};
use crate::ui::table::Cell;

/// One payment, as the document that says it was received.
pub fn receipt() -> ReportDefinition<Payment> {
    ReportDefinition::new("receipt", l!("payments.receipt"), ReportKind::Document)
        .theme(ReportTheme::Professional)
        .logo(Logo::new(LogoPlacement::ReportHeader(Align::Start)))
        // One payment and the invoices it was set against, which is a page.
        // Its export renders in the request rather than through the queue.
        .bounded_by("one payment and its allocations")
        .band(
            Band::new(BandKind::ReportHeader)
                .field(Field::bare("party", |payment: &Payment| {
                    Cell::text(payment.party.label())
                }))
                .field(Field::new(
                    "account",
                    l!("payments.account"),
                    |payment: &Payment| Cell::text(&payment.account_name),
                ))
                .field(
                    Field::new("number", l!("field.number"), |payment: &Payment| {
                        Cell::maybe(payment.number.clone())
                    })
                    .align(Align::End),
                )
                .field(
                    Field::new(
                        "received_on",
                        l!("payments.received_on"),
                        |payment: &Payment| Cell::text(payment.received_on.to_string()),
                    )
                    .align(Align::End),
                ),
        )
        .band(Band::lines(
            |payment: &Payment| payment.allocations.clone(),
            vec![
                Field::new(
                    "invoice_number",
                    l!("reports.column.document"),
                    |line: &Allocation| Cell::maybe(line.invoice_number.clone()),
                )
                .link(|line: &Allocation| Some(format!("/selling/invoices/{}", line.invoice_id))),
                Field::new(
                    "issued_on",
                    l!("reports.column.date"),
                    |line: &Allocation| Cell::text(line.issued_on.to_string()),
                ),
                Field::new("due_on", l!("reports.column.due"), |line: &Allocation| {
                    Cell::maybe(line.due_on.map(|due| due.to_string()))
                }),
                Field::figure("invoiced", l!("invoices.total"), |line: &Allocation| {
                    Cell::text(line.invoiced.to_display_string())
                }),
                Field::figure(
                    "allocated",
                    l!("payments.allocated"),
                    |line: &Allocation| Cell::text(line.amount.to_display_string()),
                ),
            ],
        ))
        .band(
            Band::new(BandKind::ReportFooter)
                .field(Field::figure(
                    "amount",
                    l!("payments.amount"),
                    |payment: &Payment| Cell::text(payment.amount.to_display_string()),
                ))
                .field(Field::figure(
                    "allocated_total",
                    l!("payments.allocated"),
                    |payment: &Payment| money(payment.allocated().ok()),
                ))
                .field(Field::figure(
                    "on_account",
                    l!("payments.on_account"),
                    |payment: &Payment| money(payment.on_account().ok()),
                )),
        )
}

/// A figure the record had to work out, which it cannot do across two
/// currencies - a shape the schema does not allow and a receipt does not draw.
fn money(amount: Option<Money>) -> Cell {
    amount.map_or(Cell::Empty, |amount| Cell::text(amount.to_display_string()))
}
