//! One customer's account: what they were invoiced, what they have paid, and
//! how long the rest has been owed.

use app_books::report::{CustomerStatement, EntryKind, StatementLine};
use phonix_core::money::Money;
use phonix_core::permissions;
use phonix_core::report::{
    Align, BandKind, ExportFormat, Logo, LogoPlacement, ReportKind, ReportTheme,
};

use crate::i18n::t;
use crate::l;
use crate::ui::report::{Band, Field, ReportDefinition};
use crate::ui::table::Cell;

/// The statement, as a document a customer is handed.
pub fn customer_statement() -> ReportDefinition<CustomerStatement> {
    ReportDefinition::new(
        "customer-statement",
        permissions::REPORTS,
        l!("reports.customer_statement"),
        ReportKind::Document,
    )
    // The workspace's own `statement` document, so its settings apply. It is
    // declared `numbered = false`: this is the one document here that carries
    // no number.
    .document_type("statement")
    .theme(ReportTheme::Professional)
    .exports(ExportFormat::Pdf)
    .exports(ExportFormat::Xlsx)
    .exports(ExportFormat::Csv)
    .logo(Logo::new(LogoPlacement::ReportHeader(Align::Start)))
    .band(
        Band::new(BandKind::ReportHeader)
            .field(Field::bare("party", |statement: &CustomerStatement| {
                Cell::text(format!(
                    "{} \u{b7} {}",
                    statement.party_code, statement.party_name
                ))
            }))
            .field(Field::bare("span", |statement: &CustomerStatement| {
                Cell::text(format!("{} \u{2013} {}", statement.from, statement.to))
            }))
            .field(Field::bare("currency", |statement: &CustomerStatement| {
                Cell::text(l!(
                    "reports.currency_note",
                    currency = statement.currency.code()
                ))
            }))
            .field(Field::figure(
                "opening",
                l!("reports.statement.opening"),
                |statement: &CustomerStatement| Cell::money(statement.opening),
            )),
    )
    .band(Band::lines(
        |statement: &CustomerStatement| statement.lines.clone(),
        vec![
            Field::new(
                "dated_on",
                l!("reports.column.date"),
                |line: &StatementLine| Cell::text(line.dated_on.to_string()),
            ),
            Field::new(
                "number",
                l!("reports.column.document"),
                |line: &StatementLine| Cell::text(document(line)),
            )
            .link(screen),
            Field::new("kind", l!("field.type"), |line: &StatementLine| {
                Cell::text(t(&line.kind.label()))
            }),
            Field::new(
                "due_on",
                l!("reports.column.due"),
                |line: &StatementLine| Cell::maybe(line.due_on.map(|due| due.to_string())),
            ),
            Field::figure(
                "amount",
                l!("reports.column.amount"),
                |line: &StatementLine| Cell::money(line.amount),
            ),
            Field::figure(
                "running",
                l!("reports.column.balance"),
                |line: &StatementLine| Cell::money(line.running),
            ),
        ],
    ))
    .band(
        Band::new(BandKind::ReportFooter)
            .field(bucket(
                "ageing_not_due",
                l!("reports.ageing.not_due"),
                |a| a.not_yet_due,
            ))
            .field(bucket("ageing_30", l!("reports.ageing.to_30"), |a| a.to_30))
            .field(bucket("ageing_60", l!("reports.ageing.to_60"), |a| a.to_60))
            .field(bucket("ageing_90", l!("reports.ageing.to_90"), |a| a.to_90))
            .field(bucket(
                "ageing_over_90",
                l!("reports.ageing.over_90"),
                |a| a.over_90,
            ))
            .field(bucket(
                "ageing_on_account",
                l!("reports.ageing.on_account"),
                |a| a.on_account,
            ))
            .field(total(
                "billed",
                l!("reports.statement.billed"),
                |statement| statement.billed,
            ))
            .field(total(
                "credited",
                l!("reports.statement.credited"),
                |statement| statement.credited,
            ))
            .field(total(
                "received",
                l!("reports.statement.received"),
                |statement| statement.received,
            ))
            .field(total(
                "closing",
                l!("reports.statement.closing"),
                |statement| statement.closing,
            )),
    )
}

/// Where a line's document is read. `None` for a kind with no screen of its
/// own, which draws the number as ordinary text rather than as a dead link.
fn screen(line: &StatementLine) -> Option<String> {
    match line.kind {
        // A credit note is an invoice row, and opens at the invoice's address.
        EntryKind::Invoice | EntryKind::CreditNote => {
            Some(format!("/selling/invoices/{}", line.id))
        }
        EntryKind::Payment => Some(format!("/selling/payments/{}", line.id)),
    }
}

/// The document number, with what the document itself says where that is not
/// what it is worth in the books.
fn document(line: &StatementLine) -> String {
    if line.document.currency() == line.amount.currency() {
        line.number.clone()
    } else {
        format!("{} ({})", line.number, line.document)
    }
}

/// One rung of the ageing ladder. A figure, and on the left of the footer
/// rather than beside the totals - which is the one place a figure says where
/// it goes.
fn bucket(
    key: &'static str,
    label: String,
    read: impl Fn(&app_books::report::Ageing) -> Money + Send + Sync + 'static,
) -> Field<CustomerStatement> {
    Field::figure(key, label, move |statement: &CustomerStatement| {
        Cell::money(read(&statement.ageing))
    })
    .align(Align::Start)
}

/// One of the figures the statement adds up to, on the right of the footer.
fn total(
    key: &'static str,
    label: String,
    read: impl Fn(&CustomerStatement) -> Money + Send + Sync + 'static,
) -> Field<CustomerStatement> {
    Field::figure(key, label, move |statement: &CustomerStatement| {
        Cell::money(read(statement))
    })
}
