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

use phonix_core::report::{ExportRequest, NewExport};
use phonix_db::report_exports as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

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
