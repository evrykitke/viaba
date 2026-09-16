//! One customer's account: what they were invoiced, what they have paid, and
//! how long the rest has been owed.

use app_books::report::{CustomerStatement, StatementLine};
use phonix_core::report::{Align, BandKind, ReportKind, ReportTheme};

use crate::i18n::t;
use crate::l;
use crate::ui::report::{Band, Field, ReportDefinition};
use crate::ui::table::Cell;

/// The statement, as a document a customer is handed.
pub fn customer_statement() -> ReportDefinition<CustomerStatement> {
    ReportDefinition::new(
        "customer-statement",
        l!("reports.customer_statement"),
        ReportKind::Document,
    )
    .theme(ReportTheme::Professional)
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
            .field(
                Field::new(
                    "opening",
                    l!("reports.statement.opening"),
                    |statement: &CustomerStatement| {
                        Cell::text(statement.opening.to_display_string())
                    },
                )
                .align(Align::End),
            ),
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
            ),
            Field::new("kind", l!("field.type"), |line: &StatementLine| {
                Cell::text(t(&line.kind.label()))
            }),
            Field::new(
                "due_on",
                l!("reports.column.due"),
                |line: &StatementLine| Cell::maybe(line.due_on.map(|due| due.to_string())),
            ),
            Field::new(
                "amount",
                l!("reports.column.amount"),
                |line: &StatementLine| Cell::text(line.amount.to_display_string()),
            )
            .align(Align::End),
            Field::new(
                "running",
                l!("reports.column.balance"),
                |line: &StatementLine| Cell::text(line.running.to_display_string()),
            )
            .align(Align::End),
        ],
    ))
    .band(
        Band::new(BandKind::ReportFooter)
            .field(bucket(
                "ageing_not_due",
                l!("reports.ageing.not_due"),
                |a| a.not_yet_due.to_display_string(),
            ))
            .field(bucket("ageing_30", l!("reports.ageing.to_30"), |a| {
                a.to_30.to_display_string()
            }))
            .field(bucket("ageing_60", l!("reports.ageing.to_60"), |a| {
                a.to_60.to_display_string()
            }))
            .field(bucket("ageing_90", l!("reports.ageing.to_90"), |a| {
                a.to_90.to_display_string()
            }))
            .field(bucket(
                "ageing_over_90",
                l!("reports.ageing.over_90"),
                |a| a.over_90.to_display_string(),
            ))
            .field(bucket(
                "ageing_on_account",
                l!("reports.ageing.on_account"),
                |a| a.on_account.to_display_string(),
            ))
            .field(total(
                "billed",
                l!("reports.statement.billed"),
                |statement| statement.billed.to_display_string(),
            ))
            .field(total(
                "credited",
                l!("reports.statement.credited"),
                |statement| statement.credited.to_display_string(),
            ))
            .field(total(
                "received",
                l!("reports.statement.received"),
                |statement| statement.received.to_display_string(),
            ))
            .field(total(
                "closing",
                l!("reports.statement.closing"),
                |statement| statement.closing.to_display_string(),
            )),
    )
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

/// One rung of the ageing ladder, on the left of the footer.
fn bucket(
    key: &'static str,
    label: String,
    read: impl Fn(&app_books::report::Ageing) -> String + Send + Sync + 'static,
) -> Field<CustomerStatement> {
    Field::new(key, label, move |statement: &CustomerStatement| {
        Cell::text(read(&statement.ageing))
    })
}

/// One of the figures the statement adds up to, on the right of the footer.
fn total(
    key: &'static str,
    label: String,
    read: impl Fn(&CustomerStatement) -> String + Send + Sync + 'static,
) -> Field<CustomerStatement> {
    Field::new(key, label, move |statement: &CustomerStatement| {
        Cell::text(read(statement))
    })
    .align(Align::End)
}
