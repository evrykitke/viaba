//! What a report can be written out as.

use serde::{Deserialize, Serialize};

/// A format an export of a report is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Xlsx,
    Pdf,
}

impl ExportFormat {
    /// Every format, in the order an export menu should offer them.
    pub const ALL: &'static [Self] = &[Self::Pdf, Self::Xlsx, Self::Csv];

    /// What the menu calls it. Not an i18n key: a format is a proper noun and
    /// is the same word in every catalogue.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Xlsx => "XLSX",
            Self::Pdf => "PDF",
        }
    }

    /// The stored value, and the extension the written file carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
            Self::Pdf => "pdf",
        }
    }

    /// The type the stored file is served as.
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Pdf => "application/pdf",
        }
    }
}
