//! `hr.holiday_lists` and `hr.holidays` — the days nobody is expected to work.

use app_hr::holiday::{CheckedHolidayList, Holiday, HolidayList, HolidayListSummary, WorkingDay};
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::hr::code_conflict;

const CODE_INDEX: &str = "holiday_lists_code_key";

fn conflict(err: sqlx::Error, code: &str) -> DbError {
    code_conflict(err, "holiday_list", CODE_INDEX, code)
}

pub async fn list<'e, E>(executor: E) -> Result<Vec<HolidayListSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.code, l.name, l.valid_from, l.valid_to, l.is_active,
                (SELECT count(*) FROM hr.holidays h
                  WHERE h.holiday_list_id = l.id) AS holiday_count,
                (SELECT count(*) FROM hr.current_staff s
                  WHERE s.holiday_list_id = l.id) AS headcount
           FROM hr.holiday_lists l
          ORDER BY l.valid_from DESC, l.name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok(HolidayListSummary {
                id: row.try_get("id")?,
                code: row.try_get("code")?,
                name: row.try_get("name")?,
                valid_from: row.try_get("valid_from")?,
                valid_to: row.try_get("valid_to")?,
                is_active: row.try_get("is_active")?,
                holiday_count: row.try_get("holiday_count")?,
                headcount: row.try_get("headcount")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The ones a form may offer.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<(Uuid, String, String)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, code, name FROM hr.holiday_lists WHERE is_active
          ORDER BY valid_from DESC, name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("id")?,
                row.try_get("code")?,
                row.try_get("name")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// One calendar with its days.
///
/// Two statements rather than a join, for the reason `books::invoice::find`
/// takes three: a list with no days is still a list, and a left join would make
/// that case a row of nulls to unpick.
pub async fn find(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<HolidayList>, DbError> {
    let Some(row) = sqlx::query(
        "SELECT id, code, name, valid_from, valid_to, is_active
           FROM hr.holiday_lists WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    Ok(Some(HolidayList {
        id: row.try_get("id").map_err(DbError::Query)?,
        code: row.try_get("code").map_err(DbError::Query)?,
        name: row.try_get("name").map_err(DbError::Query)?,
        valid_from: row.try_get("valid_from").map_err(DbError::Query)?,
        valid_to: row.try_get("valid_to").map_err(DbError::Query)?,
        is_active: row.try_get("is_active").map_err(DbError::Query)?,
        holidays: days_of(pool, id).await?,
    }))
}

/// The days on one list, earliest first.
///
/// Unpaged on purpose: a list covers at most five years, and a calendar with
/// every weekend generated into it tops out near two thousand rows.
async fn days_of<'e, E>(executor: E, list_id: Uuid) -> Result<Vec<Holiday>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, observed_on, name, is_weekly_off
           FROM hr.holidays WHERE holiday_list_id = $1
          ORDER BY observed_on",
    )
    .bind(list_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok(Holiday {
                id: row.try_get("id")?,
                observed_on: row.try_get("observed_on")?,
                name: row.try_get("name")?,
                is_weekly_off: row.try_get("is_weekly_off")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Write a calendar and its days, in one transaction.
///
/// The days are deleted and written again, which is the same shape
/// `books::invoice::save_draft` uses and for the same reason: the screen
/// submits the whole calendar, and reconciling an edited day list in SQL is how
/// a date ends up attached to the wrong name.
///
/// `None` where the calendar being edited is no longer there - somebody else
/// deleted it while this screen was open. The caller decides what to say.
pub async fn save(
    pool: &sqlx::PgPool,
    draft: &CheckedHolidayList,
    actor: Option<UserId>,
) -> Result<Option<Uuid>, DbError> {
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match draft.id {
        Some(id) => {
            let done = sqlx::query(
                "UPDATE hr.holiday_lists
                    SET code = $2, name = $3, valid_from = $4, valid_to = $5,
                        is_active = $6, updated_at = now(), updated_by = $7
                  WHERE id = $1",
            )
            .bind(id)
            .bind(&draft.code)
            .bind(&draft.name)
            .bind(draft.valid_from)
            .bind(draft.valid_to)
            .bind(draft.is_active)
            .bind(actor)
            .execute(&mut *tx)
            .await
            .map_err(|err| conflict(err, &draft.code))?;

            if done.rows_affected() == 0 {
                return Ok(None);
            }

            sqlx::query("DELETE FROM hr.holidays WHERE holiday_list_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(DbError::Query)?;

            id
        }
        None => sqlx::query_scalar(
            "INSERT INTO hr.holiday_lists
                 (code, name, valid_from, valid_to, is_active, created_by, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $6)
             RETURNING id",
        )
        .bind(&draft.code)
        .bind(&draft.name)
        .bind(draft.valid_from)
        .bind(draft.valid_to)
        .bind(draft.is_active)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| conflict(err, &draft.code))?,
    };

    for day in &draft.holidays {
        sqlx::query(
            "INSERT INTO hr.holidays
                 (holiday_list_id, observed_on, name, is_weekly_off, created_by)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(day.observed_on)
        .bind(&day.name)
        .bind(day.is_weekly_off)
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;
    }

    tx.commit().await.map_err(DbError::Query)?;

    Ok(Some(id))
}

/// Every assignment that has ever named this calendar. The delete guard.
pub async fn assignment_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM hr.assignments WHERE holiday_list_id = $1")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM hr.holiday_lists WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}

/// What the calendar says about every date in a span, earliest first.
///
/// [`working_day`] once per date would be thirty-one statements for a month,
/// and it would read the assignment chain thirty-one times to get the same
/// answer. This walks `generate_series` and resolves the assignment per day in
/// one pass, which matters because the assignment *can* change inside the span:
/// somebody who moved office on the fifteenth is on two calendars that month.
///
/// Bounded by the span the caller passes. The service is what decides how wide
/// a span a screen may ask for.
pub async fn working_days<'e, E>(
    executor: E,
    employee_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<(NaiveDate, WorkingDay)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT d.day::date AS on_date,
                l.id IS NOT NULL AS covered,
                h.name,
                h.is_weekly_off
           FROM generate_series($2::date, $3::date, interval '1 day') AS d(day)
           LEFT JOIN LATERAL (
               SELECT l.id
                 FROM hr.assignments a
                 JOIN hr.engagements e ON e.id = a.engagement_id
                 JOIN hr.holiday_lists l ON l.id = a.holiday_list_id
                WHERE e.employee_id = $1
                  AND a.effective_from <= d.day::date
                  AND (a.effective_to IS NULL OR a.effective_to >= d.day::date)
                  AND d.day::date BETWEEN l.valid_from AND l.valid_to
                ORDER BY a.effective_from DESC
                LIMIT 1
           ) l ON TRUE
           LEFT JOIN hr.holidays h
                  ON h.holiday_list_id = l.id AND h.observed_on = d.day::date
          ORDER BY d.day",
    )
    .bind(employee_id)
    .bind(from)
    .bind(to)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let on_date: NaiveDate = row.try_get("on_date")?;
            let covered: bool = row.try_get("covered")?;
            let name: Option<String> = row.try_get("name")?;

            let answer = match (covered, name) {
                (false, _) => WorkingDay::NotCovered,
                (true, Some(name)) => WorkingDay::Off {
                    name,
                    is_weekly_off: row.try_get("is_weekly_off")?,
                },
                (true, None) => WorkingDay::Working,
            };

            Ok((on_date, answer))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Whether one employee is expected in on one date.
///
/// Resolves the calendar through the assignment in force *on that date*, not
/// the current one: a person who moved office in June has last March read
/// against the calendar they were on in March. That is the whole reason the
/// column sits on `assignments` rather than on `employees`.
///
/// [`WorkingDay::NotCovered`] where nobody has said which calendar applies, or
/// where the one that does has run out. Both are "the calendar cannot say",
/// which is deliberately not the same answer as "yes, they work that day".
pub async fn working_day<'e, E>(
    executor: E,
    employee_id: Uuid,
    date: NaiveDate,
) -> Result<WorkingDay, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT h.name, h.is_weekly_off
           FROM hr.assignments a
           JOIN hr.engagements e ON e.id = a.engagement_id
           JOIN hr.holiday_lists l ON l.id = a.holiday_list_id
           LEFT JOIN hr.holidays h
                  ON h.holiday_list_id = l.id AND h.observed_on = $2
          WHERE e.employee_id = $1
            AND a.effective_from <= $2
            AND (a.effective_to IS NULL OR a.effective_to >= $2)
            AND $2 BETWEEN l.valid_from AND l.valid_to
          ORDER BY a.effective_from DESC
          LIMIT 1",
    )
    .bind(employee_id)
    .bind(date)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    let Some(row) = row else {
        return Ok(WorkingDay::NotCovered);
    };

    // The row exists because an assignment and a covering calendar do. The
    // holiday half is the LEFT JOIN, so a null name is a day nobody named.
    let name: Option<String> = row.try_get("name").map_err(DbError::Query)?;

    Ok(match name {
        Some(name) => WorkingDay::Off {
            name,
            is_weekly_off: row.try_get("is_weekly_off").map_err(DbError::Query)?,
        },
        None => WorkingDay::Working,
    })
}
