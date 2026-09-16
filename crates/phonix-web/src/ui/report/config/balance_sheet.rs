//! What is owned and what is owed, at one date.
//!
//! The first statement with nesting in it: three sections, the lines inside
//! them, a subtotal on each, and two totals that have to agree. Nothing here
//! is a special case either - a section is a group, a section total is the
//! group's subtotal, and the two that have to agree are the report footer.
//!
//! # The two earnings lines are equity
//!
//! `brought_forward` and `result_for_year` are not accounts and do not come
//! out of the query as lines, but they sit in equity and they are part of what
//! funds the assets - so they are rows of the equity section here, which is
//! what makes the section's own subtotal the number the reader expects.

use app_books::report::{BalanceSheet, ReportGroup};
use phonix_core::money::Money;
use phonix_core::permissions;
use phonix_core::report::{BandKind, ExportFormat, ReportKind, ReportTheme};

use crate::l;
use crate::ui::report::{Band, Field, Folding, Grouping, ReportDefinition};
use crate::ui::table::Cell;

/// One line of the statement, under the section it belongs to.
#[derive(Clone)]
pub struct Line {
    section: String,
    label: String,
    amount: Money,
}

/// The balance sheet.
pub fn balance_sheet() -> ReportDefinition<BalanceSheet> {
    ReportDefinition::new(
        "balance-sheet",
        permissions::REPORTS,
        l!("reports.balance_sheet"),
        ReportKind::List,
    )
    // What a customer or a bank is handed: wider margins, a rule under the
    // letterhead, and the totals given weight.
    .theme(ReportTheme::Professional)
    .exports(ExportFormat::Pdf)
    .exports(ExportFormat::Xlsx)
    .exports(ExportFormat::Csv)
    .band(
        Band::new(BandKind::ReportHeader)
            .field(Field::bare("as_at", |report: &BalanceSheet| {
                Cell::text(format!("{} {}", l!("reports.as_at"), report.as_at))
            }))
            .field(Field::bare("currency", |report: &BalanceSheet| {
                Cell::text(l!(
                    "reports.currency_note",
                    currency = report.currency.code()
                ))
            }))
            .field(Field::bare("note", |report: &BalanceSheet| {
                Cell::text(l!(
                    "reports.balance_sheet.note",
                    opened = report.year_opened.to_string()
                ))
            })),
    )
    .band(Band::grouped(
        lines,
        vec![
            Field::new("label", l!("reports.column.account"), |line: &Line| {
                Cell::text(&line.label)
            }),
            Field::figure("amount", l!("reports.column.amount"), |line: &Line| {
                Cell::money(line.amount)
            }),
        ],
        // Opens open: a balance sheet is read whole, and the sections are how
        // it is arranged rather than what somebody came for.
        Grouping::by(|line: &Line| line.section.clone())
            .folding(Folding::Open)
            .totalling("amount", |line: &Line| line.amount),
    ))
    .band(
        Band::new(BandKind::ReportFooter)
            .field(Field::figure(
                "total_assets",
                l!("reports.total_assets"),
                |report: &BalanceSheet| Cell::money(report.total_assets),
            ))
            .field(Field::figure(
                "total_funding",
                l!("reports.total_funding"),
                |report: &BalanceSheet| Cell::money(report.total_funding),
            ))
            // The two above are what has to agree, and `is_balanced` is still
            // what decides whether they do.
            .field(Field::bare("balanced", |report: &BalanceSheet| {
                Cell::text(if report.is_balanced() {
                    l!("reports.balanced")
                } else {
                    l!("reports.not_balanced")
                })
            })),
    )
}

/// Every line of the statement, in the order the sections are read in.
fn lines(report: &BalanceSheet) -> Vec<Line> {
    let mut lines = section(l!("reports.section.assets"), &report.assets);

    lines.extend(section(
        l!("reports.section.liabilities"),
        &report.liabilities,
    ));

    let equity = l!("reports.section.equity");
    lines.extend(section(equity.clone(), &report.equity));
    lines.push(Line {
        section: equity.clone(),
        label: l!("reports.brought_forward"),
        amount: report.brought_forward,
    });
    lines.push(Line {
        section: equity,
        label: l!("reports.result_for_year"),
        amount: report.result_for_year,
    });

    lines
}

/// One section's accounts, as lines.
fn section(name: String, group: &ReportGroup) -> Vec<Line> {
    group
        .lines
        .iter()
        .map(|line| Line {
            section: name.clone(),
            label: format!("{} {}", line.number, line.name),
            amount: line.amount,
        })
        .collect()
}
