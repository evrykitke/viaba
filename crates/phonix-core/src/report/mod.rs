//! What a report is made of, and the page it is printed on.
//!
//! The screen renderer in `phonix-web` and the PDF writer in `phonix-services`
//! both measure a report from here, so neither can hold its own idea of where a
//! margin is. See `docs/adr/0008-reporting.md`.
//!
//! Lengths are millimetres throughout.

mod band;
mod export;
mod page;
mod theme;

pub use band::{Align, BandKind, ReportKind};
pub use export::ExportFormat;
pub use page::{Logo, LogoPlacement, Margins, Orientation, PageSetup, PaperSize};
pub use theme::{BandHeights, Colour, Metrics, Padding, ReportTheme, Rules, TypeScale, Typeface};
