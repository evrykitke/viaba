//! `hr.applicants` — who has applied, against the vacancies that exist.

use app_hr::applicant::{Applicant, ApplicantSummary, CheckedApplicant, Stage};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn read_stage(raw: &str) -> Result<Stage, sqlx::Error> {
    Stage::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("applicants.stage holds '{raw}', which this build does not know").into(),
        )
    })
}

/// Everybody who has applied, newest first.
///
/// Unpaged, and deliberately: a workspace with more applications than a browser
/// can hold has an applicant-tracking system rather than an ERP module, and the
/// day this needs paging is the day it needs a great deal else besides.
pub async fn list<'e, E>(executor: E) -> Result<Vec<ApplicantSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT a.id, a.job_position_id, a.given_name, a.family_name,
                a.email, a.stage, a.applied_on, a.employee_id,
                j.title AS job_title
           FROM hr.applicants a
           JOIN hr.job_positions j ON j.id = a.job_position_id
          ORDER BY a.applied_on DESC, a.created_at DESC",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let stage: String = row.try_get("stage")?;

            Ok(ApplicantSummary {
                id: row.try_get("id")?,
                job_position_id: row.try_get("job_position_id")?,
                job_title: row.try_get("job_title")?,
                given_name: row.try_get("given_name")?,
                family_name: row.try_get("family_name")?,
                email: row.try_get("email")?,
                stage: read_stage(&stage)?,
                applied_on: row.try_get("applied_on")?,
                employee_id: row.try_get("employee_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Applicant>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT a.id, a.job_position_id, a.given_name, a.family_name,
                a.email, a.phone, a.stage, a.source, a.applied_on, a.note,
                a.employee_id, j.title AS job_title
           FROM hr.applicants a
           JOIN hr.job_positions j ON j.id = a.job_position_id
          WHERE a.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read).transpose()
}

fn read(row: sqlx::postgres::PgRow) -> Result<Applicant, DbError> {
    let stage: String = row.try_get("stage").map_err(DbError::Query)?;

    Ok(Applicant {
        id: row.try_get("id").map_err(DbError::Query)?,
        job_position_id: row.try_get("job_position_id").map_err(DbError::Query)?,
        job_title: row.try_get("job_title").map_err(DbError::Query)?,
        given_name: row.try_get("given_name").map_err(DbError::Query)?,
        family_name: row.try_get("family_name").map_err(DbError::Query)?,
        email: row.try_get("email").map_err(DbError::Query)?,
        phone: row.try_get("phone").map_err(DbError::Query)?,
        stage: read_stage(&stage).map_err(DbError::Query)?,
        source: row.try_get("source").map_err(DbError::Query)?,
        applied_on: row.try_get("applied_on").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        employee_id: row.try_get("employee_id").map_err(DbError::Query)?,
    })
}

/// An employee whose work email is this one.
///
/// What [`super::super::hr`]'s hire path asks before creating a person: somebody
/// who left and applied again is one human being with two periods of
/// employment, and the address is the cheapest honest way to notice.
///
/// Matched case-insensitively, which is what the unique index on
/// `employees.work_email` already does.
pub async fn employee_with_email<'e, E>(executor: E, email: &str) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT id FROM hr.employees WHERE lower(work_email) = lower($1)")
        .bind(email)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)
}

/// Write an application. `None` where the one being edited is gone.
pub async fn save<'e, E>(
    executor: E,
    draft: &CheckedApplicant,
    actor: Option<UserId>,
) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    match draft.id {
        // `stage <> 'hired'` in the guard, not only in the domain: a hire
        // opened an engagement, and an edit that moved it back would leave
        // that engagement with nothing claiming it.
        Some(id) => sqlx::query_scalar(
            "UPDATE hr.applicants
                SET job_position_id = $2, given_name = $3, family_name = $4,
                    email = $5, phone = $6, stage = $7, source = $8,
                    applied_on = $9, note = $10,
                    updated_at = now(), updated_by = $11
              WHERE id = $1 AND stage <> 'hired'
              RETURNING id",
        )
        .bind(id)
        .bind(draft.job_position_id)
        .bind(&draft.given_name)
        .bind(&draft.family_name)
        .bind(draft.email.as_deref())
        .bind(draft.phone.as_deref())
        .bind(draft.stage.as_str())
        .bind(draft.source.as_deref())
        .bind(draft.applied_on)
        .bind(draft.note.as_deref())
        .bind(actor)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query),

        None => sqlx::query_scalar(
            "INSERT INTO hr.applicants
                 (job_position_id, given_name, family_name, email, phone,
                  stage, source, applied_on, note, created_by, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
             RETURNING id",
        )
        .bind(draft.job_position_id)
        .bind(&draft.given_name)
        .bind(&draft.family_name)
        .bind(draft.email.as_deref())
        .bind(draft.phone.as_deref())
        .bind(draft.stage.as_str())
        .bind(draft.source.as_deref())
        .bind(draft.applied_on)
        .bind(draft.note.as_deref())
        .bind(actor)
        .fetch_one(executor)
        .await
        .map(Some)
        .map_err(DbError::Query),
    }
}

/// Mark an application hired, pointing at the person they became.
///
/// `false` where it was not open when this ran — somebody else closed it
/// between the read and the write, and the second hire must not claim a person
/// the first already did.
pub async fn mark_hired<'e, E>(
    executor: E,
    id: Uuid,
    employee_id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE hr.applicants
            SET stage = 'hired', employee_id = $2,
                updated_at = now(), updated_by = $3
          WHERE id = $1 AND stage NOT IN ('hired', 'rejected', 'withdrawn')",
    )
    .bind(id)
    .bind(employee_id)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}

pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM hr.applicants WHERE id = $1 AND stage <> 'hired'")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}
