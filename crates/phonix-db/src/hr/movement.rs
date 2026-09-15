//! `hr.movements` — the document behind a promotion, a transfer or an exit.

use app_hr::employee::EndReason;
use app_hr::movement::{CheckedMovement, Movement, MovementKind, MovementStatus, MovementSummary};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// What every read selects, so the row reader can be one function.
const COLUMNS: &str = "m.id, m.number, m.kind, m.status, m.employee_id,
                m.effective_on, m.department_id, m.job_position_id,
                m.work_location_id, m.manager_id, m.holiday_list_id,
                m.shift_type_id, m.end_reason, m.reason, m.assignment_id,
                m.confirmed_at,
                COALESCE(NULLIF(btrim(e.preferred_name), ''), e.given_name)
                    || ' ' || e.family_name AS employee_name";

fn read_kind(raw: &str) -> Result<MovementKind, sqlx::Error> {
    MovementKind::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("movements.kind holds '{raw}', which this build does not know").into(),
        )
    })
}

fn read_status(raw: &str) -> Result<MovementStatus, sqlx::Error> {
    MovementStatus::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("movements.status holds '{raw}', which this build does not know").into(),
        )
    })
}

/// Refused rather than defaulted, like every other stored vocabulary here: a
/// reason this build cannot read would land on a leaver's record as something
/// they did not do.
fn read_end_reason(raw: Option<String>) -> Result<Option<EndReason>, sqlx::Error> {
    match raw {
        None => Ok(None),
        Some(raw) => EndReason::parse(&raw).map(Some).ok_or_else(|| {
            sqlx::Error::Decode(
                format!("movements.end_reason holds '{raw}', which this build does not know")
                    .into(),
            )
        }),
    }
}

/// Everything that has happened to one person, newest first.
///
/// Unpaged: a personnel file is a handful of documents over a career, and one
/// that is not has a different problem.
pub async fn for_employee<'e, E>(
    executor: E,
    employee_id: Uuid,
) -> Result<Vec<MovementSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    summaries(executor, "WHERE m.employee_id = $1", Some(employee_id)).await
}

/// Every movement, newest first. What the list screen reads.
pub async fn list<'e, E>(executor: E) -> Result<Vec<MovementSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    summaries(executor, "", None).await
}

async fn summaries<'e, E>(
    executor: E,
    filter: &str,
    employee_id: Option<Uuid>,
) -> Result<Vec<MovementSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    // `AssertSqlSafe` because `filter` is one of two constants in this file.
    // Nothing from a browser reaches the text of the statement.
    let statement = sqlx::AssertSqlSafe(format!(
        "SELECT m.id, m.number, m.kind, m.status, m.employee_id,
                m.effective_on, m.end_reason, e.code AS employee_code,
                COALESCE(NULLIF(btrim(e.preferred_name), ''), e.given_name)
                    || ' ' || e.family_name AS employee_name
           FROM hr.movements m
           JOIN hr.employees e ON e.id = m.employee_id
          {filter}
          ORDER BY m.effective_on DESC, m.created_at DESC"
    ));

    let rows = match employee_id {
        Some(id) => sqlx::query(statement).bind(id).fetch_all(executor).await,
        None => sqlx::query(statement).fetch_all(executor).await,
    }
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let kind: String = row.try_get("kind")?;
            let status: String = row.try_get("status")?;
            let end_reason: Option<String> = row.try_get("end_reason")?;

            Ok(MovementSummary {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                kind: read_kind(&kind)?,
                status: read_status(&status)?,
                employee_id: row.try_get("employee_id")?,
                employee_code: row.try_get("employee_code")?,
                employee_name: row.try_get("employee_name")?,
                effective_on: row.try_get("effective_on")?,
                end_reason: read_end_reason(end_reason)?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Movement>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS}
           FROM hr.movements m
           JOIN hr.employees e ON e.id = m.employee_id
          WHERE m.id = $1"
    ));

    let row = sqlx::query(statement)
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)?;

    row.map(read).transpose()
}

fn read(row: sqlx::postgres::PgRow) -> Result<Movement, DbError> {
    let kind: String = row.try_get("kind").map_err(DbError::Query)?;
    let status: String = row.try_get("status").map_err(DbError::Query)?;
    let end_reason: Option<String> = row.try_get("end_reason").map_err(DbError::Query)?;

    Ok(Movement {
        id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        kind: read_kind(&kind).map_err(DbError::Query)?,
        status: read_status(&status).map_err(DbError::Query)?,
        employee_id: row.try_get("employee_id").map_err(DbError::Query)?,
        employee_name: row.try_get("employee_name").map_err(DbError::Query)?,
        effective_on: row.try_get("effective_on").map_err(DbError::Query)?,
        department_id: row.try_get("department_id").map_err(DbError::Query)?,
        job_position_id: row.try_get("job_position_id").map_err(DbError::Query)?,
        work_location_id: row.try_get("work_location_id").map_err(DbError::Query)?,
        manager_id: row.try_get("manager_id").map_err(DbError::Query)?,
        holiday_list_id: row.try_get("holiday_list_id").map_err(DbError::Query)?,
        shift_type_id: row.try_get("shift_type_id").map_err(DbError::Query)?,
        end_reason: read_end_reason(end_reason).map_err(DbError::Query)?,
        reason: row.try_get("reason").map_err(DbError::Query)?,
        assignment_id: row.try_get("assignment_id").map_err(DbError::Query)?,
        confirmed_at: row.try_get("confirmed_at").map_err(DbError::Query)?,
    })
}

/// Write a draft. `None` where the one being edited is no longer a draft.
///
/// The `status = 'draft'` in the update is what makes that true of the database
/// rather than only of this codebase - the same guard `books::invoice` puts on
/// rewriting a draft.
pub async fn save_draft<'e, E>(
    executor: E,
    draft: &CheckedMovement,
    actor: Option<UserId>,
) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    match draft.id {
        Some(id) => sqlx::query_scalar(
            "UPDATE hr.movements
                SET kind = $2, employee_id = $3, effective_on = $4,
                    department_id = $5, job_position_id = $6,
                    work_location_id = $7, manager_id = $8,
                    holiday_list_id = $9, shift_type_id = $10,
                    end_reason = $11, reason = $12,
                    updated_at = now(), updated_by = $13
              WHERE id = $1 AND status = 'draft'
              RETURNING id",
        )
        .bind(id)
        .bind(draft.kind.as_str())
        .bind(draft.employee_id)
        .bind(draft.effective_on)
        .bind(draft.department_id)
        .bind(draft.job_position_id)
        .bind(draft.work_location_id)
        .bind(draft.manager_id)
        .bind(draft.holiday_list_id)
        .bind(draft.shift_type_id)
        .bind(draft.end_reason.map(EndReason::as_str))
        .bind(draft.reason.as_deref())
        .bind(actor)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query),

        None => sqlx::query_scalar(
            "INSERT INTO hr.movements
                 (kind, employee_id, effective_on, department_id, job_position_id,
                  work_location_id, manager_id, holiday_list_id, shift_type_id,
                  end_reason, reason, created_by, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
             RETURNING id",
        )
        .bind(draft.kind.as_str())
        .bind(draft.employee_id)
        .bind(draft.effective_on)
        .bind(draft.department_id)
        .bind(draft.job_position_id)
        .bind(draft.work_location_id)
        .bind(draft.manager_id)
        .bind(draft.holiday_list_id)
        .bind(draft.shift_type_id)
        .bind(draft.end_reason.map(EndReason::as_str))
        .bind(draft.reason.as_deref())
        .bind(actor)
        .fetch_one(executor)
        .await
        .map(Some)
        .map_err(DbError::Query),
    }
}

/// Stamp a draft as confirmed, with its number and what it wrote.
///
/// `false` where it was not a draft when this ran - somebody else confirmed it
/// between the read and the write, and the second confirmation must not write a
/// second assignment.
pub async fn confirm<'e, E>(
    executor: E,
    id: Uuid,
    number: &str,
    assignment_id: Option<Uuid>,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE hr.movements
            SET status = 'confirmed', number = $2, assignment_id = $3,
                confirmed_at = now(), confirmed_by = $4,
                updated_at = now(), updated_by = $4
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(assignment_id)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}

/// Withdraw a draft that should not have been raised.
///
/// Cancelled rather than deleted: somebody wrote it and somebody may have seen
/// it, and a document that vanishes is one nobody can ask about.
pub async fn cancel<'e, E>(executor: E, id: Uuid, actor: Option<UserId>) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE hr.movements
            SET status = 'cancelled', updated_at = now(), updated_by = $2
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(id)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}
