//! The `attachments` table: which file is the paperwork for which record.
//!
//! A row here is a link and nothing else. What the file *is* lives in
//! `file_uploads`, which is why every read joins it - an attachment with no
//! name, size or type is a row a screen cannot draw.
//!
//! See migration `0022_attachments.sql` for why the record is addressed by an
//! entity kind and a text id rather than by a foreign key.

use phonix_core::files::attachment::{Attachment, RecordRef};
use phonix_core::identity::UserId;
use sqlx::{AssertSqlSafe, FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::files::{FileRow, prefixed_file_columns};

/// Everything attached to one record, newest first.
pub async fn for_record<'e, E>(executor: E, record: &RecordRef) -> Result<Vec<Attachment>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = AssertSqlSafe(format!(
        "SELECT a.id AS attachment_id, a.title, {columns}, u.display_name AS uploaded_by_name,
                b.display_name AS attached_by_name
           FROM attachments AS a
           JOIN file_uploads AS f ON f.id = a.file_id
           LEFT JOIN users AS u ON u.id = f.uploaded_by
           LEFT JOIN users AS b ON b.id = a.created_by
          WHERE a.entity_type = $1 AND a.entity_id = $2
          ORDER BY a.created_at DESC, a.id DESC",
        columns = prefixed_file_columns("f.")
    ));

    let rows = sqlx::query(statement)
        .bind(&record.entity_type)
        .bind(&record.entity_id)
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            let file = FileRow::from_row(row)?;
            let uploaded_by_name: Option<String> = row.try_get("uploaded_by_name")?;

            Ok(Attachment {
                id: row.try_get("attachment_id")?,
                record: record.clone(),
                file: file.to_summary(uploaded_by_name),
                title: row.try_get("title")?,
                attached_by_name: row.try_get("attached_by_name")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// How many are on this record already.
pub async fn count_for<'e, E>(executor: E, record: &RecordRef) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM attachments WHERE entity_type = $1 AND entity_id = $2")
        .bind(&record.entity_type)
        .bind(&record.entity_id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

/// Link a stored file to a record.
///
/// `None` where the pair is already linked: the unique index is the one that
/// decides, so two people attaching the same scan at the same moment produce
/// one row rather than an error.
pub async fn insert<'e, E>(
    executor: E,
    record: &RecordRef,
    file_id: Uuid,
    title: Option<&str>,
    actor: Option<UserId>,
) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO attachments (entity_type, entity_id, file_id, title, created_by)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (entity_type, entity_id, file_id) DO NOTHING
         RETURNING id",
    )
    .bind(&record.entity_type)
    .bind(&record.entity_id)
    .bind(file_id)
    .bind(title)
    .bind(actor)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)
}

/// One row, with the record it belongs to, so a caller can check the
/// permission against the right thing before removing it.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<(RecordRef, Uuid)>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query("SELECT entity_type, entity_id, file_id FROM attachments WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)?;

    row.map(|row| {
        Ok((
            RecordRef {
                entity_type: row.try_get("entity_type")?,
                entity_id: row.try_get("entity_id")?,
            },
            row.try_get("file_id")?,
        ))
    })
    .transpose()
    .map_err(DbError::Query)
}

/// Unlink one. The file itself stays where it is.
pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM attachments WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// Unlink everything on one record.
///
/// What a service calls in the same transaction that deletes the record. See
/// the migration header: this is the rule that stands in for the foreign key
/// `entity_id` cannot have.
pub async fn delete_for_record<'e, E>(executor: E, record: &RecordRef) -> Result<u64, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM attachments WHERE entity_type = $1 AND entity_id = $2")
        .bind(&record.entity_type)
        .bind(&record.entity_id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected())
}
