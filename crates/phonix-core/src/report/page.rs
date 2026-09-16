//! The sheet a report is printed on: the paper, which way up, and the margins.

use serde::{Deserialize, Serialize};

use super::{Align, BandKind};

/// A paper size, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperSize {
    #[default]
    A4,
    A5,
    Letter,
    Legal,
}

impl PaperSize {
    /// Every size, in the order a settings screen should offer them.
    pub const ALL: &'static [Self] = &[Self::A4, Self::A5, Self::Letter, Self::Legal];

    /// Width and height in portrait, in millimetres.
    pub const fn dimensions_mm(self) -> (f32, f32) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
        }
    }
}

/// Which way up the sheet is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// The space left around the content, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margins {
    /// Wider at the head and the foot, where a letterhead and a page number
    /// go. What a look does not state for itself.
    pub const DEFAULT: Self = Self {
        top: 20.0,
        right: 15.0,
        bottom: 20.0,
        left: 15.0,
    };

    /// The same margin on all four edges.
    pub const fn uniform(mm: f32) -> Self {
        Self {
            top: mm,
            right: mm,
            bottom: mm,
            left: mm,
        }
    }
}

impl Default for Margins {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The page a report is laid out on.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PageSetup {
    pub paper: PaperSize,
    pub orientation: Orientation,
    pub margins: Margins,
}

impl PageSetup {
    /// A page of this paper, this way up, with the default margins.
    pub const fn new(paper: PaperSize, orientation: Orientation) -> Self {
        Self {
            paper,
            orientation,
            margins: Margins::DEFAULT,
        }
    }

    /// The sheet's own width, with the orientation applied.
    pub const fn width_mm(&self) -> f32 {
        let (width, height) = self.paper.dimensions_mm();

        match self.orientation {
            Orientation::Portrait => width,
            Orientation::Landscape => height,
        }
    }

    /// The sheet's own height, with the orientation applied.
    pub const fn height_mm(&self) -> f32 {
        let (width, height) = self.paper.dimensions_mm();

        match self.orientation {
            Orientation::Portrait => height,
            Orientation::Landscape => width,
        }
    }

    /// How wide a band may be drawn.
    pub const fn content_width_mm(&self) -> f32 {
        self.width_mm() - self.margins.left - self.margins.right
    }

    /// How much height a page has for bands.
    pub const fn content_height_mm(&self) -> f32 {
        self.height_mm() - self.margins.top - self.margins.bottom
    }
}

/// Where the workspace logo is drawn, when it is drawn at all.
///
/// Whether it is drawn is `Option<LogoPlacement>`: a workspace that wants no
/// letterhead has no placement rather than a placement it ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogoPlacement {
    /// Once, in the report header.
    ReportHeader(Align),
    /// On every page, in the page header.
    PageHeader(Align),
}

impl LogoPlacement {
    /// The band the logo is drawn in.
    pub const fn band(self) -> BandKind {
        match self {
            Self::ReportHeader(_) => BandKind::ReportHeader,
            Self::PageHeader(_) => BandKind::PageHeader,
        }
    }

    /// Which edge of that band it sits against.
    pub const fn align(self) -> Align {
        match self {
            Self::ReportHeader(align) | Self::PageHeader(align) => align,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_turns_the_sheet() {
        let portrait = PageSetup::new(PaperSize::A4, Orientation::Portrait);
        let landscape = PageSetup::new(PaperSize::A4, Orientation::Landscape);

        assert_eq!(portrait.width_mm(), landscape.height_mm());
        assert_eq!(portrait.height_mm(), landscape.width_mm());
    }

    #[test]
    fn content_is_the_sheet_less_its_margins() {
        let page = PageSetup {
            paper: PaperSize::A4,
            orientation: Orientation::Portrait,
            margins: Margins::uniform(10.0),
        };

        assert_eq!(page.content_width_mm(), 190.0);
        assert_eq!(page.content_height_mm(), 277.0);
    }
}
