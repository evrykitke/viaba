//! Asking for an export, and asking after one.
//!
//! # The permission is the report's, and it is resolved here
//!
//! [`raise`] takes the permission the report needs rather than reading one off
//! the request. A request is something a browser sent: a permission taken from
//! it would be a caller naming the gate it is to be let through. The screen
//! that raises an export knows which report it is looking at, and the name
//! travels from there.
//!
//! # Who asked is part of the row
//!
//! A worker has no caller of its own. It renders as whoever asked and
//! re-checks their permission before it draws anything, so a grant withdrawn
//! between the request and the run stops it - see `ExportRequest::may_render`.

use phonix_core::files::{FileCategory, FileId};
use phonix_core::report::{ExportFormat, ExportRequest, NewExport};
use phonix_core::tenant::TenantSlug;
use phonix_db::report_exports as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};
use crate::files::Files;
use crate::files::generated::{self, Generated};

/// Where a written export is kept.
const EXPORTS_BUCKET: &str = "exports";

/// How many of somebody's own exports a screen may ask for at once.
pub const RECENT_LIMIT: i64 = 20;

/// Raise an export, gated on the report's own permission.
pub async fn raise(
    pool: &PgPool,
    caller: &Caller,
    asked: &NewExport,
    required: &str,
) -> ServiceResult<ExportRequest> {
    caller.require(required)?;
    let requested_by = acting_user(caller)?;

    Ok(store::raise(pool, asked, requested_by).await?)
}

/// Write an export's bytes out and mark the request ready.
///
/// Takes no caller. A worker has none, and the permission this export needed
/// was checked when it was raised and re-checked by the worker before it
/// rendered - see `ExportRequest::may_render`. Handing this a `Caller` would
/// suggest there is a third place to check, and three places to check one
/// thing is how one of them ends up not checking.
///
/// The file's id is the request's, so a retry writes the same key rather than
/// leaving two files behind - the reason `files::verify` derives its
/// destination from the row it is working on.
pub async fn finish(
    pool: &PgPool,
    files_ctx: Files<'_>,
    tenant: &TenantSlug,
    request: &ExportRequest,
    bytes: &[u8],
) -> ServiceResult<FileId> {
    let file_name = format!("{}.{}", request.report_id, request.format.as_str());

    let file_id = generated::store(
        pool,
        files_ctx,
        tenant,
        Generated {
            id: request.id,
            bucket: EXPORTS_BUCKET,
            file_name: &file_name,
            content_type: request.format.content_type(),
            category: category_of(request.format),
            bytes,
            made_for: request.requested_by,
        },
    )
    .await?;

    store::mark_ready(pool, request.id, file_id).await?;

    Ok(file_id)
}

/// Mark a request finished with nothing to show for it.
///
/// Separate from letting the error out, because the row is what a screen is
/// waiting on: an export that failed silently leaves somebody watching a
/// spinner for work that stopped.
pub async fn fail(pool: &PgPool, id: Uuid, reason: &str) -> ServiceResult<()> {
    Ok(store::mark_failed(pool, id, reason).await?)
}

/// Which kind of file a format produces, for the stored row.
const fn category_of(format: ExportFormat) -> FileCategory {
    match format {
        ExportFormat::Csv => FileCategory::Text,
        ExportFormat::Xlsx => FileCategory::Spreadsheet,
        ExportFormat::Pdf => FileCategory::Document,
    }
}

/// One request, for the screen waiting on it.
///
/// Only the person who asked may read it back. An export is a report rendered
/// as somebody, and handing the row to anybody else would say how far another
/// person's document had got - and, once it is ready, which file it is in.
pub async fn load(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<ExportRequest> {
    let asking = acting_user(caller)?;

    let request = store::load(pool, id)
        .await?
        .ok_or(ServiceError::NotFound("export"))?;

    if request.requested_by != Some(asking) {
        return Err(ServiceError::NotFound("export"));
    }

    Ok(request)
}

/// What this person has asked for lately.
pub async fn mine(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ExportRequest>> {
    let asking = acting_user(caller)?;

    Ok(store::recent_for(pool, asking, RECENT_LIMIT).await?)
}
