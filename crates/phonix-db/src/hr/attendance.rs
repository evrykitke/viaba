//! `hr.attendance` — what was recorded, one row per person per day.

use app_hr::attendance::{
    Attendance, AttendanceSource, AttendanceStatus, AttendanceSummary, CheckedAttendance,
};
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const DAY_INDEX: &str = "attendance_day_key";

/// The unique index on (employee, date), turned into something a form can show.
///
/// Not [`super::code_conflict`]: that one is about a code somebody typed, and
/// this collision is about a day already being recorded.
fn day_taken(err: sqlx::Error) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(DAY_INDEX) => DbError::CodeExists {
            entity: "attendance",
            code: String::new(),
        },
        _ => DbError::Query(err),
    }
}

fn read_status(raw: &str) -> Result<AttendanceStatus, sqlx::Error> {
    AttendanceStatus::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("attendance.status holds '{raw}', which this build does not know").into(),
        )
    })
}

fn read_source(raw: &str) -> Result<AttendanceSource, sqlx::Error> {
    AttendanceSource::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("attendance.source holds '{raw}', which this build does not know").into(),
        )
    })
}

/// One person's records across a span, earliest first.
///
/// Bounded by the span rather than paged: a month of one person is at most
/// thirty-one rows, and the screen that reads it is a timesheet rather than a
/// list. A caller asking for a decade gets a decade, which is why the service
/// is the one that decides how wide a span a screen may ask for.
pub async fn for_employee<'e, E>(
    executor: E,
    employee_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<Attendance>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, employee_id, on_date, status,
                checked_in_at, checked_out_at, source, note
           FROM hr.attendance
          WHERE employee_id = $1 AND on_date BETWEEN $2 AND $3
          ORDER BY on_date",
    )
    .bind(employee_id)
    .bind(from)
    .bind(to)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(read).collect()
}

/// Everybody's records on one date, for the screen that keys a day.
pub async fn on_date<'e, E>(executor: E, date: NaiveDate) -> Result<Vec<AttendanceSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT a.id, a.employee_id, a.on_date, a.status,
                a.checked_in_at, a.checked_out_at, a.source,
                e.code AS employee_code,
                COALESCE(NULLIF(btrim(e.preferred_name), ''), e.given_name)
                    || ' ' || e.family_name AS employee_name
           FROM hr.attendance a
           JOIN hr.employees e ON e.id = a.employee_id
          WHERE a.on_date = $1
          ORDER BY e.family_name, e.given_name",
    )
    .bind(date)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let status: String = row.try_get("status")?;
            let source: String = row.try_get("source")?;

            Ok(AttendanceSummary {
                id: row.try_get("id")?,
                employee_id: row.try_get("employee_id")?,
                employee_code: row.try_get("employee_code")?,
                employee_name: row.try_get("employee_name")?,
                on_date: row.try_get("on_date")?,
                status: read_status(&status)?,
                checked_in_at: row.try_get("checked_in_at")?,
                checked_out_at: row.try_get("checked_out_at")?,
                source: read_source(&source)?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Attendance>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, employee_id, on_date, status,
                checked_in_at, checked_out_at, source, note
           FROM hr.attendance WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read).transpose()
}

fn read(row: sqlx::postgres::PgRow) -> Result<Attendance, DbError> {
    let status: String = row.try_get("status").map_err(DbError::Query)?;
    let source: String = row.try_get("source").map_err(DbError::Query)?;

    Ok(Attendance {
        id: row.try_get("id").map_err(DbError::Query)?,
        employee_id: row.try_get("employee_id").map_err(DbError::Query)?,
        on_date: row.try_get("on_date").map_err(DbError::Query)?,
        status: read_status(&status).map_err(DbError::Query)?,
        checked_in_at: row.try_get("checked_in_at").map_err(DbError::Query)?,
        checked_out_at: row.try_get("checked_out_at").map_err(DbError::Query)?,
        source: read_source(&source).map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
    })
}

/// Write one record.
///
/// `None` where the record being edited is no longer there.
pub async fn save<'e, E>(
    executor: E,
    draft: &CheckedAttendance,
    actor: Option<UserId>,
) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    match draft.id {
        Some(id) => {
            let updated: Option<Uuid> = sqlx::query_scalar(
                "UPDATE hr.attendance
                    SET employee_id = $2, on_date = $3, status = $4,
                        checked_in_at = $5, checked_out_at = $6,
                        source = $7, note = $8,
                        updated_at = now(), updated_by = $9
                  WHERE id = $1
                  RETURNING id",
            )
            .bind(id)
            .bind(draft.employee_id)
            .bind(draft.on_date)
            .bind(draft.status.as_str())
            .bind(draft.checked_in_at)
            .bind(draft.checked_out_at)
            .bind(draft.source.as_str())
            .bind(draft.note.as_deref())
            .bind(actor)
            .fetch_optional(executor)
            .await
            .map_err(day_taken)?;

            Ok(updated)
        }
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO hr.attendance
                     (employee_id, on_date, status, checked_in_at, checked_out_at,
                      source, note, created_by, updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
                 RETURNING id",
            )
            .bind(draft.employee_id)
            .bind(draft.on_date)
            .bind(draft.status.as_str())
            .bind(draft.checked_in_at)
            .bind(draft.checked_out_at)
            .bind(draft.source.as_str())
            .bind(draft.note.as_deref())
            .bind(actor)
            .fetch_one(executor)
            .await
            .map_err(day_taken)?;

            Ok(Some(id))
        }
    }
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM hr.attendance WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}
