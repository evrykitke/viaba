//! The `report_exports` rows: what somebody asked to be written out.
//!
//! This module raises a request and reads one back. Claiming one, writing the
//! outcome and storing the bytes belong to the worker, and live with it.
//!
//! # Codes in, domain types out
//!
//! The format and the state are TEXT in Postgres and typed values in Rust,
//! resolved in [`FromRow`]. A row holding a word this build does not know is
//! refused here rather than defaulting to something several layers up.

use phonix_core::identity::UserId;
use phonix_core::report::{ExportFormat, ExportRequest, ExportState, NewExport};
use sqlx::{FromRow, PgExecutor, PgPool, Row};
use uuid::Uuid;

use crate::error::DbError;

const SELECT: &str = "SELECT id, report_id, parameters, format, state, requested_by, \
     requested_at, file_id, failure FROM report_exports";

/// Longest failure the column keeps, matching its CHECK.
const MAX_FAILURE_LEN: usize = 500;

/// The same columns as [`SELECT`], for a statement that returns from an alias.
const COLUMNS_PREFIXED: &str = "e.id, e.report_id, e.parameters, e.format, e.state,      e.requested_by, e.requested_at, e.file_id, e.failure";

struct RequestRow(ExportRequest);

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RequestRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let format: String = row.try_get("format")?;
        let state: String = row.try_get("state")?;

        Ok(Self(ExportRequest {
            id: row.try_get("id")?,
            report_id: row.try_get("report_id")?,
            parameters: row.try_get("parameters")?,
            format: parse_format(&format).ok_or_else(|| unknown("format"))?,
            state: ExportState::parse(&state).ok_or_else(|| unknown("state"))?,
            requested_by: row.try_get("requested_by")?,
            requested_at: row.try_get("requested_at")?,
            file_id: row.try_get("file_id")?,
            failure: row.try_get("failure")?,
        }))
    }
}

/// The format a stored value names.
///
/// `ExportFormat` has no `parse` of its own: the only thing that ever reads
/// one back is this row, and a second spelling of the same four-way match is
/// a second chance to disagree with the column's CHECK.
fn parse_format(raw: &str) -> Option<ExportFormat> {
    ExportFormat::ALL
        .iter()
        .copied()
        .find(|format| format.as_str() == raw)
}

fn unknown(column: &'static str) -> sqlx::Error {
    sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        source: format!("`{column}` holds a value this build does not know").into(),
    }
}

/// Raise a request, and hand back the row it became.
pub async fn raise<'e, E: PgExecutor<'e>>(
    executor: E,
    asked: &NewExport,
    requested_by: UserId,
) -> Result<ExportRequest, DbError> {
    let statement = sqlx::AssertSqlSafe(format!(
        "WITH raised AS (
             INSERT INTO report_exports (report_id, parameters, format, requested_by)
             VALUES ($1, $2, $3, $4)
             RETURNING *
         )
         {SELECT_FROM_RAISED}",
        SELECT_FROM_RAISED = SELECT.replace("FROM report_exports", "FROM raised"),
    ));

    let row: RequestRow = sqlx::query_as(statement)
        .bind(&asked.report_id)
        .bind(&asked.parameters)
        .bind(asked.format.as_str())
        .bind(requested_by)
        .fetch_one(executor)
        .await?;

    Ok(row.0)
}

/// One request, as the screen waiting on it reads it.
pub async fn load<'e, E: PgExecutor<'e>>(
    executor: E,
    id: Uuid,
) -> Result<Option<ExportRequest>, DbError> {
    let statement = sqlx::AssertSqlSafe(format!("{SELECT} WHERE id = $1"));

    let row: Option<RequestRow> = sqlx::query_as(statement)
        .bind(id)
        .fetch_optional(executor)
        .await?;

    Ok(row.map(|row| row.0))
}

/// Take the oldest exports still outstanding, and mark them running.
///
/// `SKIP LOCKED`, like the upload verifier's claim and for the same reason: a
/// second worker that arrives mid-claim takes the next row rather than waiting
/// for this one. A `running` row older than the timeout is claimed again -
/// that is the whole of the recovery story for a process that died mid-job.
pub async fn claim_batch(
    pool: &PgPool,
    limit: usize,
    claim_timeout_secs: u64,
) -> Result<Vec<ExportRequest>, DbError> {
    let statement = sqlx::AssertSqlSafe(format!(
        "WITH claimed AS (
             SELECT id
               FROM report_exports
              WHERE state = 'requested'
                 OR (state = 'running'
                     AND claimed_at < now() - make_interval(secs => $1::double precision))
              ORDER BY requested_at
              LIMIT $2
                FOR UPDATE SKIP LOCKED
         )
         UPDATE report_exports AS e
            SET state = 'running', claimed_at = now()
           FROM claimed
          WHERE e.id = claimed.id
      RETURNING {RETURNING}",
        RETURNING = COLUMNS_PREFIXED,
    ));

    let rows: Vec<RequestRow> = sqlx::query_as(statement)
        .bind(claim_timeout_secs as f64)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// Claim one export by id, for the dispatch that runs straight after a raise.
///
/// `Ok(None)` means somebody else got there first, which is an ordinary
/// outcome rather than an error - the same race `files::claim_one` runs on
/// purpose.
pub async fn claim_one(
    pool: &PgPool,
    id: Uuid,
    claim_timeout_secs: u64,
) -> Result<Option<ExportRequest>, DbError> {
    let statement = sqlx::AssertSqlSafe(format!(
        "WITH claimed AS (
             SELECT id
               FROM report_exports
              WHERE id = $1
                AND (state = 'requested'
                     OR (state = 'running'
                         AND claimed_at < now() - make_interval(secs => $2::double precision)))
                FOR UPDATE SKIP LOCKED
         )
         UPDATE report_exports AS e
            SET state = 'running', claimed_at = now()
           FROM claimed
          WHERE e.id = claimed.id
      RETURNING {RETURNING}",
        RETURNING = COLUMNS_PREFIXED,
    ));

    let row: Option<RequestRow> = sqlx::query_as(statement)
        .bind(id)
        .bind(claim_timeout_secs as f64)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|row| row.0))
}

/// Mark a request finished, with the file it produced.
pub async fn mark_ready<'e, E: PgExecutor<'e>>(
    executor: E,
    id: Uuid,
    file_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE report_exports
            SET state = 'ready', file_id = $2, failure = NULL, finished_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(file_id)
    .execute(executor)
    .await?;

    Ok(())
}

/// Mark a request finished, with the reason it produced nothing.
///
/// The reason is cut to what the column holds. A screen wants a sentence, and
/// a longer one would be refused by the constraint and lose the whole update -
/// so the row would say `running` for ever about work that had stopped.
pub async fn mark_failed<'e, E: PgExecutor<'e>>(
    executor: E,
    id: Uuid,
    reason: &str,
) -> Result<(), DbError> {
    let reason: String = reason.chars().take(MAX_FAILURE_LEN).collect();

    sqlx::query(
        "UPDATE report_exports
            SET state = 'failed', failure = $2, finished_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(reason)
    .execute(executor)
    .await?;

    Ok(())
}

/// What one person has asked for lately, newest first.
///
/// Bounded by `limit` on purpose: this is a list that grows with use, and
/// nothing wants all of it.
pub async fn recent_for<'e, E: PgExecutor<'e>>(
    executor: E,
    requested_by: UserId,
    limit: i64,
) -> Result<Vec<ExportRequest>, DbError> {
    let statement = sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE requested_by = $1 ORDER BY requested_at DESC LIMIT $2"
    ));

    let rows: Vec<RequestRow> = sqlx::query_as(statement)
        .bind(requested_by)
        .bind(limit)
        .fetch_all(executor)
        .await?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}
