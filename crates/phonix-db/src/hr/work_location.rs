//! `hr.work_locations` — where people work.

use app_hr::work_location::{
    CheckedWorkLocation, LocationKind, WorkLocation, WorkLocationSummary,
};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::hr::code_conflict;

const CODE_INDEX: &str = "work_locations_code_key";

fn conflict(err: sqlx::Error, code: &str) -> DbError {
    code_conflict(err, "work_location", CODE_INDEX, code)
}

fn read_kind(raw: &str) -> Result<LocationKind, sqlx::Error> {
    LocationKind::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("work_locations.kind holds '{raw}', which this build does not know").into(),
        )
    })
}

pub async fn list<'e, E>(executor: E) -> Result<Vec<WorkLocationSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT w.id, w.code, w.name, w.kind, w.address, w.is_active,
                (SELECT count(*)
                   FROM hr.current_staff s
                  WHERE s.work_location_id = w.id) AS headcount
           FROM hr.work_locations w
          ORDER BY w.name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let kind: String = row.try_get("kind")?;

            Ok(WorkLocationSummary {
                id: row.try_get("id")?,
                code: row.try_get("code")?,
                name: row.try_get("name")?,
                kind: read_kind(&kind)?,
                address: row.try_get("address")?,
                is_active: row.try_get("is_active")?,
                headcount: row.try_get("headcount")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The ones a form may offer.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<WorkLocation>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, code, name, kind, address, is_active
           FROM hr.work_locations WHERE is_active ORDER BY name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(read).collect()
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<WorkLocation>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, code, name, kind, address, is_active
           FROM hr.work_locations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read).transpose()
}

fn read(row: sqlx::postgres::PgRow) -> Result<WorkLocation, DbError> {
    let kind: String = row.try_get("kind").map_err(DbError::Query)?;

    Ok(WorkLocation {
        id: row.try_get("id").map_err(DbError::Query)?,
        code: row.try_get("code").map_err(DbError::Query)?,
        name: row.try_get("name").map_err(DbError::Query)?,
        kind: read_kind(&kind).map_err(DbError::Query)?,
        address: row.try_get("address").map_err(DbError::Query)?,
        is_active: row.try_get("is_active").map_err(DbError::Query)?,
    })
}

pub async fn insert<'e, E>(
    executor: E,
    draft: &CheckedWorkLocation,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO hr.work_locations
             (code, name, kind, address, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $6)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.kind.as_str())
    .bind(draft.address.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| conflict(err, &draft.code))
}

pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &CheckedWorkLocation,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE hr.work_locations
            SET code = $2, name = $3, kind = $4, address = $5, is_active = $6,
                updated_at = now(), updated_by = $7
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.kind.as_str())
    .bind(draft.address.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| conflict(err, &draft.code))?;

    Ok(done.rows_affected() > 0)
}

/// Every assignment that has ever named this place. The delete guard - see
/// [`super::job_position::assignment_count`] for why it is not `current_staff`.
pub async fn assignment_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM hr.assignments WHERE work_location_id = $1")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM hr.work_locations WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}
