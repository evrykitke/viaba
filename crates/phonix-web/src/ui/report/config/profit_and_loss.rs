//! What was earned and what it cost, between two dates.
//!
//! Five sections, each with its own subtotal, and three results.
//!
//! # Where the three results sit
//!
//! Gross profit, operating profit and the result for the period are not
//! sections and they are not sums of one - each is one section taken from
//! another. The hand-written screen drew them between the sections they fall
//! between; here they are the report footer, together, because a group's
//! subtotal is the group's own arithmetic and a figure that is not would be a
//! number under a word that does not mean it.
//!
//! Every figure the old screen showed is still here and still the same. What
//! changed is that the three results are read together at the foot rather than
//! three times down the page. A band between two groups is what would put them
//! back, and there is a queued item for it.

use app_books::report::{IncomeStatement, ReportGroup};
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

/// The profit and loss.
pub fn profit_and_loss() -> ReportDefinition<IncomeStatement> {
    ReportDefinition::new(
        "profit-and-loss",
        permissions::REPORTS,
        l!("reports.profit_and_loss"),
        ReportKind::List,
    )
    .theme(ReportTheme::Professional)
    .exports(ExportFormat::Pdf)
    .exports(ExportFormat::Xlsx)
    .exports(ExportFormat::Csv)
    .band(
        Band::new(BandKind::ReportHeader)
            .field(Field::bare("span", |report: &IncomeStatement| {
                Cell::text(format!("{} \u{2013} {}", report.from, report.to))
            }))
            .field(Field::bare("currency", |report: &IncomeStatement| {
                Cell::text(l!(
                    "reports.currency_note",
                    currency = report.currency.code()
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
        Grouping::by(|line: &Line| line.section.clone())
            .folding(Folding::Open)
            .totalling("amount", |line: &Line| line.amount),
    ))
    .band(
        Band::new(BandKind::ReportFooter)
            .field(Field::figure(
                "gross_profit",
                l!("reports.gross_profit"),
                |report: &IncomeStatement| Cell::money(report.gross_profit),
            ))
            .field(Field::figure(
                "operating_profit",
                l!("reports.operating_profit"),
                |report: &IncomeStatement| Cell::money(report.operating_profit),
            ))
            .field(Field::figure(
                "net_profit",
                l!("reports.net_profit"),
                |report: &IncomeStatement| Cell::money(report.net_profit),
            )),
    )
}

/// Every line, in the order the sections are read in.
///
/// The order is the statement's own and not alphabetical: revenue, what it
/// cost, what running the place cost, and then the two that sit outside the
/// trade.
fn lines(report: &IncomeStatement) -> Vec<Line> {
    let sections = [
        (l!("reports.section.revenue"), &report.revenue),
        (l!("reports.section.cost_of_sales"), &report.cost_of_sales),
        (
            l!("reports.section.operating_expenses"),
            &report.operating_expenses,
        ),
        (l!("reports.section.other_income"), &report.other_income),
        (l!("reports.section.other_expenses"), &report.other_expenses),
    ];

    sections
        .into_iter()
        .flat_map(|(name, group)| section(name, group))
        .collect()
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
