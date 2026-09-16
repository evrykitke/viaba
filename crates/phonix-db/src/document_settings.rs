//! The `document_settings` rows: what this workspace decided each kind of
//! document should look like.
//!
//! A document type with no row here is not an error and not an empty setting -
//! it is a type nobody has opened the screen for, and it falls back to
//! [`DocumentSettings::defaults`]. Reads therefore return the defaults rather
//! than an `Option`, so no caller has to remember that.
//!
//! # Codes in, domain types out
//!
//! The look, the paper, the orientation and the logo's placement are TEXT in
//! Postgres and typed values in Rust, resolved in [`FromRow`]. A row holding a
//! word this build does not know is refused here rather than defaulting to
//! something several layers up - the same rule `organization` follows.

use phonix_core::identity::UserId;
use phonix_core::report::{
    Align, DocumentSettings, Logo, LogoPlacement, Orientation, PaperSize, ReportTheme,
};
use sqlx::{AssertSqlSafe, FromRow, PgExecutor, Row};

use crate::error::DbError;

const SELECT: &str = "SELECT document_type, theme, paper, orientation, logo_band, logo_align, \
     logo_height_mm, header_text, footer_text FROM document_settings";

/// The stored row, as the value the rest of the workspace uses.
struct SettingsRow(DocumentSettings);

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for SettingsRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let theme: String = row.try_get("theme")?;
        let paper: String = row.try_get("paper")?;
        let orientation: String = row.try_get("orientation")?;
        let logo_band: Option<String> = row.try_get("logo_band")?;
        let logo_align: Option<String> = row.try_get("logo_align")?;
        let logo_height: Option<f32> = row.try_get("logo_height_mm")?;

        let logo = match (logo_band, logo_align, logo_height) {
            (Some(band), Some(align), Some(height)) => {
                let align = Align::parse(&align).ok_or_else(|| unknown("logo_align"))?;
                let placement =
                    LogoPlacement::parse(&band, align).ok_or_else(|| unknown("logo_band"))?;

                Some(Logo::new(placement).at_height(height))
            }
            // The constraint refuses a partial one, so this is "no mark".
            _ => None,
        };

        Ok(Self(DocumentSettings {
            document_type: row.try_get("document_type")?,
            theme: ReportTheme::parse(&theme).ok_or_else(|| unknown("theme"))?,
            paper: PaperSize::parse(&paper).ok_or_else(|| unknown("paper"))?,
            orientation: Orientation::parse(&orientation).ok_or_else(|| unknown("orientation"))?,
            logo,
            header_text: row.try_get("header_text")?,
            footer_text: row.try_get("footer_text")?,
        }))
    }
}

/// A stored word this build does not know.
fn unknown(column: &'static str) -> sqlx::Error {
    sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        source: format!("`{column}` holds a value this build does not know").into(),
    }
}

/// What this workspace keeps for one document type, or the defaults for it.
pub async fn load<'e, E: PgExecutor<'e>>(
    executor: E,
    document_type: &str,
) -> Result<DocumentSettings, DbError> {
    // Composed from a constant and a literal; the value is bound.
    let statement = AssertSqlSafe(format!("{SELECT} WHERE document_type = $1"));

    let stored: Option<SettingsRow> = sqlx::query_as(statement)
        .bind(document_type)
        .fetch_optional(executor)
        .await?;

    Ok(stored.map_or_else(|| DocumentSettings::defaults(document_type), |row| row.0))
}

/// Every type this workspace has kept a setting for, by name.
///
/// Unpaged on purpose: a workspace issues as many document types as its apps
/// declare in `config/numbering/`, which is a list of a dozen and not a list
/// that grows with its data.
pub async fn list<'e, E: PgExecutor<'e>>(executor: E) -> Result<Vec<DocumentSettings>, DbError> {
    let statement = AssertSqlSafe(format!("{SELECT} ORDER BY document_type"));

    let rows: Vec<SettingsRow> = sqlx::query_as(statement).fetch_all(executor).await?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// Put an app's declared defaults in, without touching what a tenant changed.
///
/// `ON CONFLICT DO NOTHING`, and it runs on every migration pass rather than
/// only the first: an upgrade that adds a document type has to reach the
/// workspaces that already have the app. Returns how many rows were new.
///
/// **Fully qualified**, unlike every read in this module, because this one
/// runs on a connection whose search path is the *app's* schema - the same
/// reason `apps::record_installed` spells out `core.installed_apps`. Written
/// unqualified it does not resolve, and the failure is not local: it fails the
/// whole tenant's migration, on every boot, after the apps before it have
/// already been dealt with.
pub async fn install_from_config<'e, E: PgExecutor<'e>>(
    executor: E,
    documents: &[DocumentSettings],
) -> Result<u64, DbError> {
    if documents.is_empty() {
        return Ok(0);
    }

    let types: Vec<&str> = documents
        .iter()
        .map(|settings| settings.document_type.as_str())
        .collect();
    let themes: Vec<&str> = documents
        .iter()
        .map(|settings| settings.theme.as_str())
        .collect();
    let papers: Vec<&str> = documents
        .iter()
        .map(|settings| settings.paper.as_str())
        .collect();
    let orientations: Vec<&str> = documents
        .iter()
        .map(|settings| settings.orientation.as_str())
        .collect();
    let bands: Vec<Option<&str>> = documents
        .iter()
        .map(|settings| settings.logo.map(|logo| logo.placement.band_str()))
        .collect();
    let aligns: Vec<Option<&str>> = documents
        .iter()
        .map(|settings| settings.logo.map(|logo| logo.placement.align().as_str()))
        .collect();
    let heights: Vec<Option<f32>> = documents
        .iter()
        .map(|settings| settings.logo.map(|logo| logo.height_mm))
        .collect();

    let result = sqlx::query(
        "INSERT INTO core.document_settings (document_type, theme, paper, orientation, \
              logo_band, logo_align, logo_height_mm) \
         SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], \
              $6::text[], $7::real[]) \
         ON CONFLICT (document_type) DO NOTHING",
    )
    .bind(&types)
    .bind(&themes)
    .bind(&papers)
    .bind(&orientations)
    .bind(&bands)
    .bind(&aligns)
    .bind(&heights)
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}

/// Write what an administrator chose, replacing whatever was there.
pub async fn save<'e, E: PgExecutor<'e>>(
    executor: E,
    settings: &DocumentSettings,
    updated_by: Option<UserId>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO document_settings (document_type, theme, paper, orientation, logo_band, \
              logo_align, logo_height_mm, header_text, footer_text, updated_at, updated_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), $10) \
         ON CONFLICT (document_type) DO UPDATE \
            SET theme = EXCLUDED.theme, \
                paper = EXCLUDED.paper, \
                orientation = EXCLUDED.orientation, \
                logo_band = EXCLUDED.logo_band, \
                logo_align = EXCLUDED.logo_align, \
                logo_height_mm = EXCLUDED.logo_height_mm, \
                header_text = EXCLUDED.header_text, \
                footer_text = EXCLUDED.footer_text, \
                updated_at = now(), \
                updated_by = EXCLUDED.updated_by",
    )
    .bind(&settings.document_type)
    .bind(settings.theme.as_str())
    .bind(settings.paper.as_str())
    .bind(settings.orientation.as_str())
    .bind(settings.logo.map(|logo| logo.placement.band_str()))
    .bind(settings.logo.map(|logo| logo.placement.align().as_str()))
    .bind(settings.logo.map(|logo| logo.height_mm))
    .bind(settings.header_text.as_deref())
    .bind(settings.footer_text.as_deref())
    .bind(updated_by)
    .execute(executor)
    .await?;

    Ok(())
}
