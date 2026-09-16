//! What each document should look like out of the box, declared per app in
//! `config/documents/<app_id>.toml`.
//!
//! The same split [`numbering`](crate::numbering) makes, for the same reason.
//! An app owns the *question* - which documents it issues and what they should
//! look like on the first morning - and the tenant owns the *answer*: once the
//! app is installed the rows in `core.document_settings` are theirs, and a
//! redeploy never puts back a look they changed.
//!
//! ```toml
//! # config/documents/books.toml
//!
//! [[document]]
//! doc_type    = "sales_invoice"
//! theme       = "professional"
//! paper       = "a4"
//! orientation = "portrait"
//! logo        = "report_header"
//! logo_align  = "start"
//! ```
//!
//! # The words are not declared here
//!
//! There is no `header_text` or `footer_text`. Those are the tenant's own
//! words - their payment terms, their registration line - and a default here
//! would be this codebase putting sentences in their mouth, in English, on
//! every document they issue. They start empty and the settings screen is
//! where they are written.
//!
//! # A document type here must be one the app issues
//!
//! `config/numbering/` already names the documents a workspace can issue. A
//! setting attached to a type with no series is a setting for a document that
//! does not exist, so it is refused at load rather than installed and
//! forgotten.
//!
//! # Missing is not an error
//!
//! Most apps issue no documents worth printing. [`documents_for`] returns an
//! empty list for an app with no file.

use std::path::{Path, PathBuf};

use config::{Config, File, FileFormat};
use phonix_core::report::{
    Align, DocumentSettings, Logo, LogoPlacement, Orientation, PaperSize, ReportTheme,
};
use serde::Deserialize;

use crate::numbering::{MAX_DOC_TYPE_LEN, Series, SeriesError};

/// The directory under `config/` these files live in.
pub const DIRECTORY: &str = "documents";

/// The tallest mark a declaration may ask for, matching the column's CHECK.
pub const MAX_LOGO_HEIGHT_MM: f32 = 100.0;

/// One app's file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentFile {
    #[serde(default)]
    document: Vec<Document>,
}

/// What one document type looks like before a tenant has said otherwise.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    /// The type this is for, matching a `doc_type` in `config/numbering/`.
    pub doc_type: String,

    /// `modern`, `compact` or `professional`. Defaults to the document look.
    #[serde(default = "professional")]
    pub theme: String,

    /// `a4`, `a5`, `letter` or `legal`.
    #[serde(default = "a4")]
    pub paper: String,

    #[serde(default = "portrait")]
    pub orientation: String,

    /// `report_header` or `page_header`. Absent draws no mark, which a
    /// workspace can still turn on from the settings screen.
    #[serde(default)]
    pub logo: Option<String>,

    #[serde(default = "start")]
    pub logo_align: String,

    #[serde(default = "default_logo_height")]
    pub logo_height_mm: f32,
}

fn professional() -> String {
    ReportTheme::Professional.as_str().to_owned()
}

fn a4() -> String {
    PaperSize::A4.as_str().to_owned()
}

fn portrait() -> String {
    Orientation::Portrait.as_str().to_owned()
}

fn start() -> String {
    Align::Start.as_str().to_owned()
}

const fn default_logo_height() -> f32 {
    Logo::DEFAULT_HEIGHT_MM
}

impl Document {
    /// This declaration as the row a workspace starts with.
    ///
    /// Only called after [`check`] has passed, so the words are known ones.
    fn settings(&self) -> Option<DocumentSettings> {
        let align = Align::parse(&self.logo_align)?;

        Some(DocumentSettings {
            document_type: self.doc_type.clone(),
            theme: ReportTheme::parse(&self.theme)?,
            paper: PaperSize::parse(&self.paper)?,
            orientation: Orientation::parse(&self.orientation)?,
            logo: match self.logo.as_deref() {
                Some(band) => Some(
                    LogoPlacement::parse(band, align)
                        .map(|placement| Logo::new(placement).at_height(self.logo_height_mm))?,
                ),
                None => None,
            },
            header_text: None,
            footer_text: None,
        })
    }
}

/// A marker that changes when any app's declarations here do.
///
/// Compared, never parsed, and part of `apps::schema_fingerprint`. It exists
/// because the boot sweep skips a tenant whose fingerprint already matches: a
/// declaration that is not in the fingerprint never reaches a workspace that
/// already has the app, which is exactly what a new document or a new default look is
/// for.
///
/// Hashes the bytes, so editing a comment in one of these files brings every
/// workspace forward once. That costs an idempotent pass of
/// `ON CONFLICT DO NOTHING` statements and is the cheap side of the trade.
pub fn digest() -> u64 {
    digest_of(crate::workspace_root().join("config").join(DIRECTORY))
}

/// The same, for an explicit directory, so a test can point somewhere else.
pub fn digest_of(dir: impl AsRef<Path>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    let Ok(entries) = std::fs::read_dir(dir) else {
        // No directory is no declarations, which is a stable answer rather
        // than a failure - most deployments of most apps declare none.
        return 0;
    };

    let mut files: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "toml"))
        .collect();

    // Sorted, because a directory listing is in whatever order the filesystem
    // hands back and a fingerprint that moved with it would re-migrate every
    // workspace at random.
    files.sort();

    for path in files {
        path.file_name().hash(&mut hasher);

        if let Ok(bytes) = std::fs::read(&path) {
            bytes.hash(&mut hasher);
        }
    }

    hasher.finish()
}

/// Read one app's document defaults from the workspace's own `config`.
pub fn documents_for(app_id: &str) -> Result<Vec<DocumentSettings>, DocumentError> {
    let series = crate::numbering::series_for(app_id).map_err(DocumentError::Numbering)?;

    documents_from(
        crate::workspace_root().join("config").join(DIRECTORY),
        app_id,
        &series,
    )
}

/// Read one app's document defaults from an explicit directory.
///
/// Separated from [`documents_for`] so tests can point at a fixture directory,
/// the same way [`crate::load_from`] is separated from [`crate::load`].
pub fn documents_from(
    dir: impl AsRef<Path>,
    app_id: &str,
    series: &[Series],
) -> Result<Vec<DocumentSettings>, DocumentError> {
    let path = dir.as_ref().join(format!("{app_id}.toml"));
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let file: DocumentFile = Config::builder()
        .add_source(File::from(path.clone()).format(FileFormat::Toml))
        .build()
        .map_err(|source| DocumentError::Read {
            path: path.clone(),
            source: Box::new(source),
        })?
        .try_deserialize()
        .map_err(|source| DocumentError::Read {
            path: path.clone(),
            source: Box::new(source),
        })?;

    check(&file.document, series, &path)?;

    Ok(file
        .document
        .iter()
        .filter_map(Document::settings)
        .collect())
}

/// Everything that has to be true before a file is worth installing.
fn check(documents: &[Document], series: &[Series], path: &Path) -> Result<(), DocumentError> {
    let mut seen: Vec<&str> = Vec::new();

    for entry in documents {
        let doc_type = entry.doc_type.as_str();

        if doc_type.len() > MAX_DOC_TYPE_LEN
            || !series.iter().any(|issued| issued.doc_type == doc_type)
        {
            return Err(DocumentError::UnknownDocType {
                path: path.to_path_buf(),
                doc_type: doc_type.to_owned(),
            });
        }

        for (field, value, known) in [
            (
                "theme",
                entry.theme.as_str(),
                ReportTheme::parse(&entry.theme).is_some(),
            ),
            (
                "paper",
                entry.paper.as_str(),
                PaperSize::parse(&entry.paper).is_some(),
            ),
            (
                "orientation",
                entry.orientation.as_str(),
                Orientation::parse(&entry.orientation).is_some(),
            ),
            (
                "logo_align",
                entry.logo_align.as_str(),
                Align::parse(&entry.logo_align).is_some(),
            ),
        ] {
            if !known {
                return Err(DocumentError::Unknown {
                    path: path.to_path_buf(),
                    doc_type: doc_type.to_owned(),
                    field,
                    value: value.to_owned(),
                });
            }
        }

        if let Some(band) = &entry.logo
            && LogoPlacement::parse(band, Align::Start).is_none()
        {
            return Err(DocumentError::Unknown {
                path: path.to_path_buf(),
                doc_type: doc_type.to_owned(),
                field: "logo",
                value: band.clone(),
            });
        }

        if entry.logo_height_mm <= 0.0 || entry.logo_height_mm > MAX_LOGO_HEIGHT_MM {
            return Err(DocumentError::LogoHeight {
                path: path.to_path_buf(),
                doc_type: doc_type.to_owned(),
                height: entry.logo_height_mm,
            });
        }

        // Two entries for one type would race to install, and the loser would
        // be dropped silently by `ON CONFLICT DO NOTHING`.
        if seen.contains(&doc_type) {
            return Err(DocumentError::Duplicate {
                path: path.to_path_buf(),
                doc_type: doc_type.to_owned(),
            });
        }
        seen.push(doc_type);
    }

    Ok(())
}

/// Why a document file was refused. Every one stops the process at start-up.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: Box<config::ConfigError>,
    },
    /// The app's own numbering file could not be read, so there is nothing to
    /// check these against.
    #[error(transparent)]
    Numbering(SeriesError),
    #[error(
        "{path}: '{doc_type}' is not a document this app issues; declare a series for it first"
    )]
    UnknownDocType { path: PathBuf, doc_type: String },
    #[error("{path}: '{doc_type}' has {field} '{value}', which this build does not know")]
    Unknown {
        path: PathBuf,
        doc_type: String,
        field: &'static str,
        value: String,
    },
    #[error("{path}: '{doc_type}' draws its mark {height}mm tall; that is not a height")]
    LogoHeight {
        path: PathBuf,
        doc_type: String,
        height: f32,
    },
    #[error("{path}: '{doc_type}' is declared twice")]
    Duplicate { path: PathBuf, doc_type: String },
}

#[cfg(test)]
mod tests {
    use std::fs;

    use phonix_core::numbering::Pattern;

    use super::*;

    fn issues(doc_types: &[&str]) -> Vec<Series> {
        doc_types
            .iter()
            .map(|doc_type| Series {
                doc_type: (*doc_type).to_owned(),
                label: None,
                mask: Pattern::parse("X-#####").expect("a pattern"),
                reset: phonix_core::numbering::ResetPeriod::Never,
                start_at: 1,
                scope: String::new(),
            })
            .collect()
    }

    fn load(
        name: &str,
        body: &str,
        series: &[Series],
    ) -> Result<Vec<DocumentSettings>, DocumentError> {
        let dir = std::env::temp_dir().join(format!(
            "phonix-documents-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&dir).expect("scratch directory");
        fs::write(dir.join("books.toml"), body).expect("write fixture");
        let result = documents_from(&dir, "books", series);
        let _ = fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn the_digest_moves_when_a_declaration_does() {
        let dir = std::env::temp_dir().join(format!(
            "phonix-documents-digest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&dir).expect("scratch directory");

        fs::write(dir.join("books.toml"), "[[document]]\ndoc_type = \"a\"\n").expect("write");
        let before = digest_of(&dir);

        fs::write(dir.join("books.toml"), "[[document]]\ndoc_type = \"b\"\n").expect("write");
        let after = digest_of(&dir);

        // The whole point: a workspace already at the current schema is only
        // brought forward when this number changes.
        assert_ne!(before, after);
        assert_eq!(after, digest_of(&dir), "the same files hash the same way");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_that_is_not_there_hashes_to_nothing() {
        assert_eq!(
            digest_of(std::env::temp_dir().join("phonix-documents-no-such-dir")),
            0
        );
    }

    #[test]
    fn an_app_with_no_file_declares_nothing() {
        let documents = documents_from(
            std::env::temp_dir().join("phonix-documents-absent"),
            "books",
            &issues(&["sales_invoice"]),
        )
        .expect("a missing file is not an error");

        assert!(documents.is_empty());
    }

    #[test]
    fn a_declaration_becomes_the_row_a_workspace_starts_with() {
        let documents = load(
            "good",
            "[[document]]\ndoc_type = \"sales_invoice\"\ntheme = \"compact\"\n\
             paper = \"letter\"\nlogo = \"page_header\"\nlogo_align = \"end\"\n",
            &issues(&["sales_invoice"]),
        )
        .expect("a valid file");

        let settings = documents.first().expect("one document");
        assert_eq!(settings.theme, ReportTheme::Compact);
        assert_eq!(settings.paper, PaperSize::Letter);
        assert_eq!(
            settings.logo.map(|logo| logo.placement),
            Some(LogoPlacement::PageHeader(Align::End))
        );
        // The words are the tenant's, and a default never writes them.
        assert!(settings.header_text.is_none());
        assert!(settings.footer_text.is_none());
    }

    #[test]
    fn a_setting_for_a_document_the_app_does_not_issue_is_refused() {
        let error = load(
            "unissued",
            "[[document]]\ndoc_type = \"delivery_note\"\n",
            &issues(&["sales_invoice"]),
        )
        .expect_err("a type with no series");

        assert!(matches!(error, DocumentError::UnknownDocType { .. }));
    }

    #[test]
    fn a_look_this_build_does_not_know_is_refused() {
        let error = load(
            "sleek",
            "[[document]]\ndoc_type = \"sales_invoice\"\ntheme = \"sleek\"\n",
            &issues(&["sales_invoice"]),
        )
        .expect_err("an unknown look");

        assert!(matches!(
            error,
            DocumentError::Unknown { field: "theme", .. }
        ));
    }
}
