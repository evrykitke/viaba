//! A document that is not about anything, for comparing looks against.
//!
//! The settings screen previews here rather than against a real document, for
//! the reason the numbering tab previews a format against a sample counter: a
//! preview drawn from real data promises something the save may not keep, and
//! a look is easier to judge on a page whose figures nobody is reading.
//!
//! Nothing here reads the clock. The dates are written down, which is also
//! what makes the sample identical on the server and in the browser.

use phonix_core::permissions;
use phonix_core::report::{BandKind, DocumentSettings, ReportKind};

use crate::l;
use crate::ui::report::{Band, Field, ReportDefinition};
use crate::ui::table::Cell;

/// One line of the made-up document.
#[derive(Clone)]
pub struct SampleLine {
    pub description: String,
    pub dated_on: &'static str,
    pub amount: &'static str,
}

/// The made-up document itself.
#[derive(Clone)]
pub struct Sample {
    pub customer: String,
    pub lines: Vec<SampleLine>,
    pub total: &'static str,
}

impl Default for Sample {
    fn default() -> Self {
        let line = l!("documents.sample.line");

        Self {
            customer: l!("documents.sample.customer"),
            lines: (1..=3)
                .map(|number| SampleLine {
                    description: format!("{line} {number}"),
                    dated_on: "2026-01-31",
                    amount: "1,250.00",
                })
                .collect(),
            total: "3,750.00",
        }
    }
}

/// The sample, drawn in the settings an administrator is choosing.
///
/// Takes the settings rather than reading them back, so the page redraws as
/// the choice changes rather than after a save.
pub fn sample(settings: &DocumentSettings) -> ReportDefinition<Sample> {
    let mut definition = ReportDefinition::new(
        "document-sample",
        permissions::SETTINGS,
        l!("documents.preview"),
        ReportKind::Document,
    )
    .theme(settings.theme)
    .page(settings.page());

    if let Some(logo) = settings.logo {
        definition = definition.logo(logo);
    }

    definition
        .band(header(settings))
        .band(Band::lines(
            |sample: &Sample| sample.lines.clone(),
            vec![
                Field::new(
                    "description",
                    l!("reports.column.document"),
                    |line: &SampleLine| Cell::text(line.description.clone()),
                ),
                Field::new(
                    "dated_on",
                    l!("reports.column.date"),
                    |line: &SampleLine| Cell::text(line.dated_on),
                ),
                Field::figure(
                    "amount",
                    l!("reports.column.amount"),
                    |line: &SampleLine| Cell::text(line.amount),
                ),
            ],
        ))
        .band(footer(settings))
}

fn header(settings: &DocumentSettings) -> Band<Sample> {
    let band = Band::new(BandKind::ReportHeader)
        .field(Field::bare("customer", |sample: &Sample| {
            Cell::text(sample.customer.clone())
        }));

    match settings.header_text.clone() {
        Some(text) => band.field(Field::text("header_text", text)),
        None => band,
    }
}

fn footer(settings: &DocumentSettings) -> Band<Sample> {
    let band = Band::new(BandKind::ReportFooter).field(Field::figure(
        "total",
        l!("invoices.total"),
        |sample: &Sample| Cell::text(sample.total),
    ));

    match settings.footer_text.clone() {
        Some(text) => band.field(Field::text("footer_text", text)),
        None => band,
    }
}
