//! What a tenant keeps about one kind of document.
//!
//! A bounded set of answers and never a layout: the paper, the look, whether
//! the mark is drawn and where, and the tenant's own words at the head and the
//! foot. Anything that would move a band, add a column or bind a field belongs
//! to the definition, which is code. See `docs/adr/0008-reporting.md` §6.

use serde::{Deserialize, Serialize};

use super::{Logo, Orientation, PageSetup, PaperSize, ReportTheme};
use crate::identity::validation::FieldError;
use crate::msg;
use crate::organization::Letterhead;

/// Longest header or footer a tenant may keep, in characters.
pub const MAX_DOCUMENT_TEXT_LEN: usize = 500;

/// Everything a report needs before it can draw a document: who the
/// workspace is, and what each kind of document it issues looks like.
///
/// One value because it is one question, asked once for a session. The screen
/// and the PDF writer resolve it the same way, which is what stops a printed
/// document and the one on screen disagreeing about a footer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentChrome {
    pub letterhead: Letterhead,
    pub settings: Vec<DocumentSettings>,
}

impl DocumentChrome {
    /// What this workspace keeps for one document type, if it keeps anything.
    pub fn of(&self, document_type: &str) -> Option<&DocumentSettings> {
        self.settings
            .iter()
            .find(|settings| settings.document_type == document_type)
    }
}

/// The answers this workspace gives for one document type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    /// Which document these are for - `invoice`, `receipt`. The vocabulary is
    /// the one `config/numbering/` already names, not an enum here.
    pub document_type: String,
    pub theme: ReportTheme,
    pub paper: PaperSize,
    pub orientation: Orientation,
    /// `None` draws no mark at all, which is a setting rather than an absence.
    pub logo: Option<Logo>,
    /// The tenant's own words. Not an i18n key, and never looked up in a
    /// catalogue: a workspace writes its own payment terms in its own
    /// language.
    pub header_text: Option<String>,
    pub footer_text: Option<String>,
}

impl DocumentSettings {
    /// What a document type with no row of its own gets.
    ///
    /// The document look on A4, with the mark in the letterhead - which is
    /// what a workspace that has never opened the settings screen should see
    /// on an invoice rather than an unstyled page.
    pub fn defaults(document_type: impl Into<String>) -> Self {
        Self {
            document_type: document_type.into(),
            theme: ReportTheme::Professional,
            paper: PaperSize::A4,
            orientation: Orientation::Portrait,
            logo: Some(Logo::new(super::LogoPlacement::ReportHeader(
                super::Align::Start,
            ))),
            header_text: None,
            footer_text: None,
        }
    }

    /// The page these describe: the paper they name, with the margins the look
    /// asks for.
    pub const fn page(&self) -> PageSetup {
        PageSetup {
            paper: self.paper,
            orientation: self.orientation,
            margins: self.theme.metrics().margins,
        }
    }

    /// What a screen must be told before this can be stored.
    ///
    /// Checked here rather than left to the column so the form can say which
    /// box is wrong: a CHECK constraint refuses the whole row and arrives as a
    /// constraint name nobody outside this codebase can read.
    pub fn validate(&self) -> Vec<FieldError> {
        let mut errors = Vec::new();

        if self.document_type.trim().is_empty() {
            errors.push(FieldError::new(
                "document_type",
                msg!("documents.error.type_required"),
            ));
        }

        for (field, text) in [
            ("header_text", self.header_text.as_deref()),
            ("footer_text", self.footer_text.as_deref()),
        ] {
            if text.is_some_and(|text| text.chars().count() > MAX_DOCUMENT_TEXT_LEN) {
                errors.push(FieldError::new(
                    field,
                    msg!("documents.error.text_too_long", max = MAX_DOCUMENT_TEXT_LEN),
                ));
            }
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_with_no_row_still_has_a_page() {
        let settings = DocumentSettings::defaults("invoice");

        assert_eq!(settings.page().paper, PaperSize::A4);
        assert_eq!(
            settings.page().margins,
            ReportTheme::Professional.metrics().margins
        );
    }

    #[test]
    fn text_longer_than_the_column_is_refused() {
        let mut settings = DocumentSettings::defaults("invoice");
        settings.footer_text = Some("x".repeat(MAX_DOCUMENT_TEXT_LEN + 1));

        let errors = settings.validate();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "footer_text");
    }
}
