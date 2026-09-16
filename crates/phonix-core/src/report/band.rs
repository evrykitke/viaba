//! The horizontal sections a report is drawn in.

use serde::{Deserialize, Serialize};

/// What a report is over: many rows, or one record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    /// Many rows under a repeating header — a product list, a trial balance.
    #[default]
    List,
    /// One record, as somebody is handed it — a receipt, an invoice.
    Document,
}

/// One section of a report, drawn across the width of the content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandKind {
    /// Once, above everything: the letterhead.
    ReportHeader,
    /// The top of every page.
    PageHeader,
    /// Opens a group, and is drawn again when that group continues onto the
    /// next page.
    GroupHeader,
    /// One row of the report's data.
    Detail,
    /// Closes a group; where a subtotal goes.
    GroupFooter,
    /// Once, below everything: the total.
    ReportFooter,
    /// The foot of every page.
    PageFooter,
}

impl BandKind {
    /// Every band kind, top of the report to foot of the page.
    pub const ALL: &'static [Self] = &[
        Self::ReportHeader,
        Self::PageHeader,
        Self::GroupHeader,
        Self::Detail,
        Self::GroupFooter,
        Self::ReportFooter,
        Self::PageFooter,
    ];

    /// Whether the band belongs to the page rather than to the report, and is
    /// therefore drawn on all of them.
    pub const fn repeats_per_page(self) -> bool {
        matches!(self, Self::PageHeader | Self::PageFooter)
    }

    /// Whether the band belongs to a group.
    pub const fn is_group(self) -> bool {
        matches!(self, Self::GroupHeader | Self::GroupFooter)
    }
}

/// Which edge of its box a report's content sits against.
///
/// The grid has its own `Align` in `phonix-web`, a screen type carrying a CSS
/// class. This one is read by the PDF writer as well as by the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}
