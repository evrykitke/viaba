//! Storing bytes this workspace produced for itself.
//!
//! # Not an upload
//!
//! An upload arrives from outside, lands in quarantine, and is inspected
//! before anybody may have it back - that is what [`verify`](super::verify) is
//! for. Nothing here came from outside: a worker wrote these bytes out of rows
//! the workspace already has, so there is nothing to inspect and nowhere to
//! quarantine from. The object is written straight at its stored key and the
//! row is recorded as `stored`.
//!
//! The rest of the file machinery is unchanged by that. What comes out is an
//! ordinary stored file, which is what lets the download route serve it and
//! the files screen list it without knowing where it came from.

use phonix_core::files::{FileCategory, FileId};
use phonix_core::tenant::TenantSlug;
use phonix_db::files::{self as files_db, GeneratedFile};
use phonix_db::sqlx::PgPool;
use phonix_storage::{NamingContext, StorageKey};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};

use super::Files;

/// Bytes to keep, and what to call them.
pub struct Generated<'a> {
    /// The id the file gets, which is also what its key is derived from.
    ///
    /// Supplied rather than generated so a retry computes the same
    /// destination as the attempt before it - the reason `verify` derives its
    /// key from the row rather than from the clock.
    pub id: Uuid,
    pub bucket: &'a str,
    /// What a browser saves it as: `customer-statement.csv`.
    pub file_name: &'a str,
    pub content_type: &'a str,
    pub category: FileCategory,
    pub bytes: &'a [u8],
    /// Who it was made for.
    pub made_for: Option<Uuid>,
}

/// Write bytes into storage and record the row.
///
/// The object is written first. A row pointing at bytes that are not there
/// would be a download that fails; bytes with no row are a file nobody can
/// reach, which the storage sweep can find and the download route cannot
/// serve.
pub async fn store(
    pool: &PgPool,
    files_ctx: Files<'_>,
    tenant: &TenantSlug,
    file: Generated<'_>,
) -> ServiceResult<FileId> {
    let bucket = phonix_core::files::bucket(file.bucket).ok_or(ServiceError::NotFound("bucket"))?;

    let extension = extension_of(file.file_name);

    let segments = files_ctx.naming.segments(&NamingContext {
        bucket: bucket.name,
        file_id: file.id,
        extension,
        // The digest is not known until the bytes are written, and a key that
        // needed it would have to be computed twice. The id is what makes this
        // deterministic, which is what a retry needs.
        checksum: None,
        at: chrono::Utc::now(),
    });

    let key =
        StorageKey::new(tenant, &segments).map_err(|err| ServiceError::Storage(err.into()))?;

    let mut writer = files_ctx.storage.begin(&key, bucket.max_bytes).await?;

    if let Err(err) = writer.write(file.bytes).await {
        writer.abort().await;
        return Err(err.into());
    }

    let stat = writer.finish().await?;
    let checksum = files_ctx.storage.digest(&key).await?;

    let row = files_db::record_generated(
        pool,
        GeneratedFile {
            id: file.id,
            bucket: bucket.name,
            file_name: file.file_name,
            content_type: file.content_type,
            category: file.category,
            byte_size: stat.byte_size,
            checksum_sha256: &checksum,
            storage_key: key.as_ref(),
            made_for: file.made_for,
        },
    )
    .await?;

    Ok(row.id)
}

/// The extension a name ends in, for the naming strategy.
///
/// Empty rather than a guess: a key with no extension is ugly and a key with
/// the wrong one is a file that opens in the wrong application.
fn extension_of(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension)
}
