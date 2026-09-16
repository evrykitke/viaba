//! A report with its types erased, which is the form that crosses a crate
//! boundary.
//!
//! # Why this exists
//!
//! A definition is `ReportDefinition<T>` in `phonix-web`, closed over a row
//! type that only that crate knows. The writers live in `phonix-services`,
//! which `phonix-web` depends on rather than the other way round, so a writer
//! can never see a definition. What it can see is this: the bands, their
//! headings, and their rows as text.
//!
//! That is what lets the request path and the exporter call the *same* writer,
//! which is ADR 0008 §9's requirement - two writers that drifted would be a
//! receipt and a statement that disagree about what a CSV looks like.
//!
//! # Text, for now
//!
//! Every cell is a `String`. That is enough for CSV and for a PDF, which draws
//! text either way, and it is not enough for XLSX, which wants numbers as
//! numbers and dates as dates. The answer when that lands is to move the
//! grid's `Cell` into this crate rather than to grow a second one here.

use serde::{Deserialize, Serialize};

use super::{BandKind, PageSetup, ReportTheme};

/// One band, as something that can be written out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedBand {
    pub kind: BandKind,
    /// What the columns are called. Empty for a band drawn once, whose values
    /// carry their own labels.
    pub headings: Vec<String>,
    /// One row of cells per line. A band drawn once has exactly one row.
    pub rows: Vec<Vec<String>>,
}

impl RenderedBand {
    /// A band of rows under headings - a detail band.
    pub fn table(kind: BandKind, headings: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            kind,
            headings,
            rows,
        }
    }

    /// A band drawn once: a letterhead, a total.
    pub fn once(kind: BandKind, cells: Vec<String>) -> Self {
        Self {
            kind,
            headings: Vec::new(),
            rows: vec![cells],
        }
    }

    /// How many columns the widest row of this band has.
    pub fn width(&self) -> usize {
        self.rows
            .iter()
            .map(Vec::len)
            .chain(std::iter::once(self.headings.len()))
            .max()
            .unwrap_or_default()
    }
}

/// A whole report, ready to be written out.
///
/// `PartialEq` and not `Eq`: a page is measured in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rendered {
    /// The definition's own id, which is also the stem an exported file is
    /// named with.
    pub report_id: String,
    pub title: String,
    pub theme: ReportTheme,
    pub page: PageSetup,
    pub bands: Vec<RenderedBand>,
}

impl Rendered {
    /// The band of this kind, if the report drew one.
    pub fn band(&self, kind: BandKind) -> Option<&RenderedBand> {
        self.bands.iter().find(|band| band.kind == kind)
    }

    /// The widest row anywhere in the report.
    ///
    /// What a writer laying the whole thing out in one grid needs: a footer of
    /// four totals under a detail band of six columns is still six columns
    /// wide.
    pub fn width(&self) -> usize {
        self.bands
            .iter()
            .map(RenderedBand::width)
            .max()
            .unwrap_or_default()
    }
}
