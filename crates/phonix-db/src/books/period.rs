//! `books.periods`: the accounting calendar.
//!
//! Opening a year is `ON CONFLICT DO NOTHING` on the label, so running it twice
//! is not twelve duplicate months and a workspace that already opened half a
//! year gets the other half.

use app_books::period::{NewPeriod, Period};
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use sqlx::{FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// Every period, oldest first.
pub async fn list<'e, E>(executor: E) -> Result<Vec<Period>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Period>>(
        "SELECT id, label, starts_on, ends_on, is_closed, closed_at
           FROM books.periods
          ORDER BY starts_on",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?
    .into_iter()
    .map(|row| row.0)
    .collect())
}

/// The period a date falls in, if the calendar has been opened that far.
pub async fn covering<'e, E>(executor: E, date: NaiveDate) -> Result<Option<Period>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Period>>(
        "SELECT id, label, starts_on, ends_on, is_closed, closed_at
           FROM books.periods
          WHERE $1 BETWEEN starts_on AND ends_on",
    )
    .bind(date)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// How many periods there are, and how many are still open.
pub async fn counts<'e, E>(executor: E) -> Result<(i64, i64), DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE NOT is_closed) AS open
           FROM books.periods",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Ok((
        row.try_get("total").map_err(DbError::Query)?,
        row.try_get("open").map_err(DbError::Query)?,
    ))
}

/// Open a financial year. Returns how many periods were actually created.
///
/// One statement rather than twelve round trips, and idempotent: a workspace
/// that opened 2026 already gets zero back rather than a duplicate-key error.
pub async fn open_year<'e, E>(executor: E, periods: &[NewPeriod]) -> Result<u64, DbError>
where
    E: PgExecutor<'e>,
{
    if periods.is_empty() {
        return Ok(0);
    }

    let labels: Vec<&str> = periods.iter().map(|it| it.label.as_str()).collect();
    let starts: Vec<NaiveDate> = periods.iter().map(|it| it.starts_on).collect();
    let ends: Vec<NaiveDate> = periods.iter().map(|it| it.ends_on).collect();

    Ok(sqlx::query(
        "INSERT INTO books.periods (label, starts_on, ends_on)
         SELECT * FROM unnest($1::text[], $2::date[], $3::date[])
         ON CONFLICT DO NOTHING",
    )
    .bind(&labels)
    .bind(&starts)
    .bind(&ends)
    .execute(executor)
    .await
    .map_err(DbError::Query)?
    .rows_affected())
}

/// Close a period or reopen it. Answers whether a row was there to change.
pub async fn set_closed<'e, E>(
    executor: E,
    id: Uuid,
    closed: bool,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE books.periods
            SET is_closed = $2,
                closed_at = CASE WHEN $2 THEN now() ELSE NULL END,
                closed_by = CASE WHEN $2 THEN $3 ELSE NULL END
          WHERE id = $1",
    )
    .bind(id)
    .bind(closed)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// A newtype so the `FromRow` impl does not have to live in `app-books`, which
/// knows nothing of sqlx.
struct RowOf<T>(T);

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Period> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self(Period {
            id: row.try_get("id")?,
            label: row.try_get("label")?,
            starts_on: row.try_get("starts_on")?,
            ends_on: row.try_get("ends_on")?,
            is_closed: row.try_get("is_closed")?,
            closed_at: row.try_get("closed_at")?,
        }))
    }
}
