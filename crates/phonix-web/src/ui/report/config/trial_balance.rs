//! Every account, both columns, and the proof they agree.
//!
//! The simplest of the four statements, which is why it is the first onto the
//! engine: if a statement cannot be said without a special case, it shows
//! here. Nothing in it is special - six columns, a total row, and a sentence
//! that depends on whether two of those totals match.

use app_books::report::{TrialBalance, TrialBalanceRow};
use phonix_core::permissions;
use phonix_core::report::{BandKind, ExportFormat, ReportKind, ReportTheme};

use crate::l;
use crate::ui::report::{Band, Field, ReportDefinition};
use crate::ui::table::Cell;

/// The trial balance.
pub fn trial_balance() -> ReportDefinition<TrialBalance> {
    ReportDefinition::new(
        "trial-balance",
        permissions::REPORTS,
        l!("reports.trial_balance"),
        ReportKind::List,
    )
    // The dense look: it is read down a column of accounts, and how many
    // reach a page is the thing that matters about it.
    .theme(ReportTheme::Compact)
    .exports(ExportFormat::Pdf)
    .exports(ExportFormat::Xlsx)
    .exports(ExportFormat::Csv)
    .band(
        Band::new(BandKind::ReportHeader)
            .field(Field::bare("span", |report: &TrialBalance| {
                Cell::text(format!("{} \u{2013} {}", report.from, report.to))
            }))
            .field(Field::bare("currency", |report: &TrialBalance| {
                Cell::text(l!(
                    "reports.currency_note",
                    currency = report.currency.code()
                ))
            })),
    )
    .band(Band::new(BandKind::PageHeader).field(Field::text("name", l!("reports.trial_balance"))))
    .band(Band::lines(
        |report: &TrialBalance| report.rows.clone(),
        vec![
            // The number and the name in one column, the way the hand-written
            // screen had them: two columns for one identifier is a column of
            // whitespace down the middle of a statement.
            Field::new(
                "account",
                l!("reports.column.account"),
                |row: &TrialBalanceRow| Cell::text(format!("{} {}", row.number, row.name)),
            ),
            Field::figure(
                "opening",
                l!("reports.column.opening"),
                |row: &TrialBalanceRow| Cell::money(row.opening),
            ),
            Field::figure(
                "debits",
                l!("reports.column.debit"),
                |row: &TrialBalanceRow| Cell::money(row.debits),
            ),
            Field::figure(
                "credits",
                l!("reports.column.credit"),
                |row: &TrialBalanceRow| Cell::money(row.credits),
            ),
            Field::figure(
                "closing_debit",
                l!("reports.column.closing_debit"),
                |row: &TrialBalanceRow| Cell::money(row.closing_debit),
            ),
            Field::figure(
                "closing_credit",
                l!("reports.column.closing_credit"),
                |row: &TrialBalanceRow| Cell::money(row.closing_credit),
            ),
        ],
    ))
    .band(
        Band::new(BandKind::ReportFooter)
            .field(Field::bare("label", |_: &TrialBalance| {
                Cell::text(l!("reports.total"))
            }))
            .field(Field::figure(
                "debits",
                l!("reports.column.debit"),
                |report: &TrialBalance| Cell::money(report.debits),
            ))
            .field(Field::figure(
                "credits",
                l!("reports.column.credit"),
                |report: &TrialBalance| Cell::money(report.credits),
            ))
            .field(Field::figure(
                "closing_debit",
                l!("reports.column.closing_debit"),
                |report: &TrialBalance| Cell::money(report.closing_debits),
            ))
            .field(Field::figure(
                "closing_credit",
                l!("reports.column.closing_credit"),
                |report: &TrialBalance| Cell::money(report.closing_credits),
            ))
            // `is_balanced` still decides what this says. The statement is the
            // proof that the two columns agree, so a statement that does not
            // say whether they do is missing the point of itself.
            .field(Field::bare("balanced", |report: &TrialBalance| {
                Cell::text(if report.is_balanced() {
                    l!("reports.balanced")
                } else {
                    l!("reports.not_balanced")
                })
            })),
    )
}
