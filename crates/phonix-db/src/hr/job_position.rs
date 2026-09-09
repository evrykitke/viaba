//! `hr.job_positions` — the roles the organization is made of.
//!
//! # `filled` is counted, never stored
//!
//! How many people hold a role is arithmetic over the open assignments, and a
//! stored count is a second fact about the same thing that stops agreeing the
//! first time somebody is moved. It is what makes a vacancy visible, so it has
//! to be right.

use app_hr::job_position::{CheckedJobPosition, JobPosition, JobPositionSummary};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::hr::code_conflict;

const CODE_INDEX: &str = "job_positions_code_key";

fn conflict(err: sqlx::Error, code: &str) -> DbError {
    code_conflict(err, "job_position", CODE_INDEX, code)
}

/// Every role, with how many people currently hold it.
pub async fn list<'e, E>(executor: E) -> Result<Vec<JobPositionSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT j.id, j.code, j.title, j.department_id, j.is_active,
                d.name AS department_name,
                (SELECT count(*)
                   FROM hr.current_staff s
                  WHERE s.job_position_id = j.id) AS filled
           FROM hr.job_positions j
           LEFT JOIN hr.departments d ON d.id = j.department_id
          ORDER BY j.title",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok(JobPositionSummary {
                id: row.try_get("id")?,
                code: row.try_get("code")?,
                title: row.try_get("title")?,
                department_id: row.try_get("department_id")?,
                department_name: row.try_get("department_name")?,
                is_active: row.try_get("is_active")?,
                filled: row.try_get("filled")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The ones a form may offer. Active only: a retired role is history, not a
/// choice.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<JobPosition>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, code, title, department_id, description, is_active
           FROM hr.job_positions
          WHERE is_active
          ORDER BY title",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(read).collect()
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<JobPosition>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, code, title, department_id, description, is_active
           FROM hr.job_positions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read).transpose()
}

fn read(row: sqlx::postgres::PgRow) -> Result<JobPosition, DbError> {
    Ok(JobPosition {
        id: row.try_get("id").map_err(DbError::Query)?,
        code: row.try_get("code").map_err(DbError::Query)?,
        title: row.try_get("title").map_err(DbError::Query)?,
        department_id: row.try_get("department_id").map_err(DbError::Query)?,
        description: row.try_get("description").map_err(DbError::Query)?,
        is_active: row.try_get("is_active").map_err(DbError::Query)?,
    })
}

pub async fn insert<'e, E>(
    executor: E,
    draft: &CheckedJobPosition,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO hr.job_positions
             (code, title, department_id, description, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $6)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.title)
    .bind(draft.department_id)
    .bind(draft.description.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| conflict(err, &draft.code))
}

pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &CheckedJobPosition,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE hr.job_positions
            SET code = $2, title = $3, department_id = $4, description = $5,
                is_active = $6, updated_at = now(), updated_by = $7
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.title)
    .bind(draft.department_id)
    .bind(draft.description.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| conflict(err, &draft.code))?;

    Ok(done.rows_affected() > 0)
}

/// How many people have EVER been assigned to this role, open or closed.
///
/// The delete guard. Not `current_staff`: a role nobody holds today but three
/// people held last year is still cited by their assignment history, and
/// deleting it would be `ON DELETE RESTRICT` refusing at the last moment - or
/// worse, succeeding and taking the history with it.
pub async fn assignment_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM hr.assignments WHERE job_position_id = $1")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM hr.job_positions WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}
