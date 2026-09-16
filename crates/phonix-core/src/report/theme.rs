//! The three looks a report can be drawn in, as the numbers behind them.
//!
//! Type sizes are points, the unit type is specified in. Everything else is
//! millimetres, like the rest of this module.

use serde::{Deserialize, Serialize};

use super::{BandKind, Margins};

/// A look, chosen rather than authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportTheme {
    /// The application's own look: generous spacing, hairline rules, colour
    /// for emphasis.
    #[default]
    Modern,
    /// Dense rows, small type and full gridlines. The look to choose when how
    /// many rows reach a page is what matters.
    Compact,
    /// For something a customer is handed: a strong rule under the letterhead,
    /// wide margins, and weight on the totals.
    Professional,
}

impl ReportTheme {
    /// Every look, in the order a settings screen should offer them.
    pub const ALL: &'static [Self] = &[Self::Modern, Self::Compact, Self::Professional];

    /// The stored value, matching the column's CHECK constraint.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Compact => "compact",
            Self::Professional => "professional",
        }
    }

    /// What the look measures out to.
    pub const fn metrics(self) -> Metrics {
        match self {
            Self::Modern => MODERN,
            Self::Compact => COMPACT,
            Self::Professional => PROFESSIONAL,
        }
    }
}

/// A look resolved to the numbers the screen and the PDF writer draw with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub type_scale: TypeScale,
    pub bands: BandHeights,
    pub padding: Padding,
    pub rules: Rules,
    /// The page margins this look asks for, unless a document setting says
    /// otherwise.
    pub margins: Margins,
    pub colour: Colour,
}

/// Type sizes, in points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TypeScale {
    /// The report's own title, in the report header.
    pub title_pt: f32,
    /// A column heading or a group header.
    pub heading_pt: f32,
    /// A detail row.
    pub body_pt: f32,
    /// A subtotal or a total.
    pub total_pt: f32,
    /// The page header and footer: a page number, a date, a workspace name.
    pub caption_pt: f32,
}

/// How tall each band is drawn, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BandHeights {
    pub report_header: f32,
    pub page_header: f32,
    pub group_header: f32,
    pub detail: f32,
    pub group_footer: f32,
    pub report_footer: f32,
    pub page_footer: f32,
}

impl BandHeights {
    /// The height of one band kind.
    pub const fn of(&self, kind: BandKind) -> f32 {
        match kind {
            BandKind::ReportHeader => self.report_header,
            BandKind::PageHeader => self.page_header,
            BandKind::GroupHeader => self.group_header,
            BandKind::Detail => self.detail,
            BandKind::GroupFooter => self.group_footer,
            BandKind::ReportFooter => self.report_footer,
            BandKind::PageFooter => self.page_footer,
        }
    }
}

/// The space inside a cell, around its content, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Padding {
    pub horizontal: f32,
    pub vertical: f32,
}

/// Rule weights in millimetres, one per place a rule can be drawn.
///
/// Zero is how a look says it draws no rule there: Modern separates its
/// columns by alignment alone, and Compact rules every edge of every cell.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    pub under_letterhead: f32,
    pub under_headings: f32,
    pub between_rows: f32,
    pub between_columns: f32,
    pub above_total: f32,
}

/// How far colour is allowed to reach in a look, not which colour it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
    /// Ink only. Everything a colour would have said is said by weight.
    None,
    /// The letterhead rule and the totals.
    Restrained,
    /// Headings and totals as well.
    #[default]
    Emphasis,
}

const MODERN: Metrics = Metrics {
    type_scale: TypeScale {
        title_pt: 16.0,
        heading_pt: 10.0,
        body_pt: 9.5,
        total_pt: 10.0,
        caption_pt: 7.5,
    },
    bands: BandHeights {
        report_header: 30.0,
        page_header: 12.0,
        group_header: 9.0,
        detail: 7.0,
        group_footer: 8.0,
        report_footer: 12.0,
        page_footer: 10.0,
    },
    padding: Padding {
        horizontal: 2.5,
        vertical: 1.8,
    },
    rules: Rules {
        under_letterhead: 0.3,
        under_headings: 0.2,
        between_rows: 0.1,
        between_columns: 0.0,
        above_total: 0.2,
    },
    margins: Margins::DEFAULT,
    colour: Colour::Emphasis,
};

const COMPACT: Metrics = Metrics {
    type_scale: TypeScale {
        title_pt: 12.0,
        heading_pt: 8.0,
        body_pt: 7.5,
        total_pt: 8.0,
        caption_pt: 6.5,
    },
    bands: BandHeights {
        report_header: 18.0,
        page_header: 8.0,
        group_header: 6.0,
        detail: 4.5,
        group_footer: 5.5,
        report_footer: 8.0,
        page_footer: 7.0,
    },
    padding: Padding {
        horizontal: 1.0,
        vertical: 0.6,
    },
    rules: Rules {
        under_letterhead: 0.2,
        under_headings: 0.2,
        between_rows: 0.15,
        between_columns: 0.15,
        above_total: 0.2,
    },
    margins: Margins::uniform(10.0),
    colour: Colour::None,
};

const PROFESSIONAL: Metrics = Metrics {
    type_scale: TypeScale {
        title_pt: 18.0,
        heading_pt: 10.0,
        body_pt: 9.5,
        total_pt: 11.0,
        caption_pt: 8.0,
    },
    bands: BandHeights {
        report_header: 38.0,
        page_header: 12.0,
        group_header: 10.0,
        detail: 7.5,
        group_footer: 9.0,
        report_footer: 14.0,
        page_footer: 10.0,
    },
    padding: Padding {
        horizontal: 3.0,
        vertical: 2.0,
    },
    rules: Rules {
        under_letterhead: 0.8,
        under_headings: 0.25,
        between_rows: 0.0,
        between_columns: 0.0,
        above_total: 0.4,
    },
    margins: Margins {
        top: 25.0,
        right: 22.0,
        bottom: 25.0,
        left: 22.0,
    },
    colour: Colour::Restrained,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_band_has_a_height_in_every_look() {
        for theme in ReportTheme::ALL {
            let metrics = theme.metrics();

            for kind in BandKind::ALL {
                assert!(
                    metrics.bands.of(*kind) > 0.0,
                    "{theme:?} draws {kind:?} with no height"
                );
            }
        }
    }

    #[test]
    fn compact_is_the_dense_one() {
        let compact = ReportTheme::Compact.metrics();
        let modern = ReportTheme::Modern.metrics();

        assert!(compact.bands.detail < modern.bands.detail);
        assert!(compact.padding.vertical < modern.padding.vertical);
        assert!(compact.type_scale.body_pt < modern.type_scale.body_pt);
    }

    #[test]
    fn compact_rules_its_columns_and_the_others_do_not() {
        assert!(ReportTheme::Compact.metrics().rules.between_columns > 0.0);
        assert_eq!(ReportTheme::Modern.metrics().rules.between_columns, 0.0);
        assert_eq!(
            ReportTheme::Professional.metrics().rules.between_columns,
            0.0
        );
    }
}
