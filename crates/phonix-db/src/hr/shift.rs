//! `hr.shift_types` — what somebody was expected to work.

use app_hr::shift::{CheckedShiftType, ShiftType, ShiftTypeSummary};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::hr::code_conflict;

const CODE_INDEX: &str = "shift_types_code_key";

fn conflict(err: sqlx::Error, code: &str) -> DbError {
    code_conflict(err, "shift_type", CODE_INDEX, code)
}

pub async fn list<'e, E>(executor: E) -> Result<Vec<ShiftTypeSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT s.id, s.code, s.name, s.starts_at, s.ends_at,
                s.late_grace_minutes, s.is_active,
                (SELECT count(*) FROM hr.current_staff c
                  WHERE c.shift_type_id = s.id) AS headcount
           FROM hr.shift_types s
          ORDER BY s.starts_at, s.name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let grace: i32 = row.try_get("late_grace_minutes")?;

            Ok(ShiftTypeSummary {
                id: row.try_get("id")?,
                code: row.try_get("code")?,
                name: row.try_get("name")?,
                starts_at: row.try_get("starts_at")?,
                ends_at: row.try_get("ends_at")?,
                late_grace_minutes: i64::from(grace),
                is_active: row.try_get("is_active")?,
                headcount: row.try_get("headcount")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The ones a form may offer.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<ShiftType>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, code, name, starts_at, ends_at,
                late_grace_minutes, early_exit_grace_minutes, is_active
           FROM hr.shift_types WHERE is_active ORDER BY starts_at, name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(read).collect()
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<ShiftType>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, code, name, starts_at, ends_at,
                late_grace_minutes, early_exit_grace_minutes, is_active
           FROM hr.shift_types WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read).transpose()
}

/// The shift somebody was on for one date, through the assignment in force
/// then.
///
/// The same resolution `holiday::working_day` does, and for the same reason:
/// punctuality last March has to be read against the shift they were on in
/// March, not the one they are on now.
pub async fn on_date<'e, E>(
    executor: E,
    employee_id: Uuid,
    date: chrono::NaiveDate,
) -> Result<Option<ShiftType>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT s.id, s.code, s.name, s.starts_at, s.ends_at,
                s.late_grace_minutes, s.early_exit_grace_minutes, s.is_active
           FROM hr.assignments a
           JOIN hr.engagements e ON e.id = a.engagement_id
           JOIN hr.shift_types s ON s.id = a.shift_type_id
          WHERE e.employee_id = $1
            AND a.effective_from <= $2
            AND (a.effective_to IS NULL OR a.effective_to >= $2)
          ORDER BY a.effective_from DESC
          LIMIT 1",
    )
    .bind(employee_id)
    .bind(date)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read).transpose()
}

fn read(row: sqlx::postgres::PgRow) -> Result<ShiftType, DbError> {
    let late: i32 = row.try_get("late_grace_minutes").map_err(DbError::Query)?;
    let early: i32 = row
        .try_get("early_exit_grace_minutes")
        .map_err(DbError::Query)?;

    Ok(ShiftType {
        id: row.try_get("id").map_err(DbError::Query)?,
        code: row.try_get("code").map_err(DbError::Query)?,
        name: row.try_get("name").map_err(DbError::Query)?,
        starts_at: row.try_get("starts_at").map_err(DbError::Query)?,
        ends_at: row.try_get("ends_at").map_err(DbError::Query)?,
        late_grace_minutes: i64::from(late),
        early_exit_grace_minutes: i64::from(early),
        is_active: row.try_get("is_active").map_err(DbError::Query)?,
    })
}

/// Write one shift. `None` where the one being edited is no longer there.
pub async fn save<'e, E>(
    executor: E,
    draft: &CheckedShiftType,
    actor: Option<UserId>,
) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    // The columns are INTEGER; the domain carries i64 because that is what
    // `TimeDelta` takes, and the check refuses anything outside a day either
    // way.
    let late = i32::try_from(draft.late_grace_minutes).unwrap_or(i32::MAX);
    let early = i32::try_from(draft.early_exit_grace_minutes).unwrap_or(i32::MAX);

    match draft.id {
        Some(id) => sqlx::query_scalar(
            "UPDATE hr.shift_types
                SET code = $2, name = $3, starts_at = $4, ends_at = $5,
                    late_grace_minutes = $6, early_exit_grace_minutes = $7,
                    is_active = $8, updated_at = now(), updated_by = $9
              WHERE id = $1
              RETURNING id",
        )
        .bind(id)
        .bind(&draft.code)
        .bind(&draft.name)
        .bind(draft.starts_at)
        .bind(draft.ends_at)
        .bind(late)
        .bind(early)
        .bind(draft.is_active)
        .bind(actor)
        .fetch_optional(executor)
        .await
        .map_err(|err| conflict(err, &draft.code)),

        None => sqlx::query_scalar(
            "INSERT INTO hr.shift_types
                 (code, name, starts_at, ends_at, late_grace_minutes,
                  early_exit_grace_minutes, is_active, created_by, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
             RETURNING id",
        )
        .bind(&draft.code)
        .bind(&draft.name)
        .bind(draft.starts_at)
        .bind(draft.ends_at)
        .bind(late)
        .bind(early)
        .bind(draft.is_active)
        .bind(actor)
        .fetch_one(executor)
        .await
        .map(Some)
        .map_err(|err| conflict(err, &draft.code)),
    }
}

/// Every assignment that has ever named this shift. The delete guard.
pub async fn assignment_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM hr.assignments WHERE shift_type_id = $1")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM hr.shift_types WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}
