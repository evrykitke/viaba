//! `hr.employees`, `hr.engagements` and `hr.assignments`.
//!
//! # Three tables, and the chain between them
//!
//! An employee has engagements, newest first; an engagement has assignments,
//! newest first. Reading one person is three queries rather than one join,
//! because a join across two one-to-many edges multiplies the rows and the
//! de-duplication costs more than the round trip on a record this size.
//!
//! # Closing before opening
//!
//! Both `engagements_one_open_per_employee` and
//! `assignments_one_open_per_engagement` are partial unique indexes, so the
//! database refuses a second open row. [`close_open_assignment`] and
//! [`end_engagement`] exist so the service can close the old one first, in the
//! same transaction. That is the "rules live in statements" rule of ADR 0006
//! applied to a date chain: no trigger maintains it, a transaction does.
//!
//! # `is_employed` is a query
//!
//! There is no flag. `current_staff` is the view that answers it, and the store
//! reads that rather than reconstructing the join per screen.

use app_hr::employee::{
    Assignment, CheckedAssignment, CheckedEmployee, CheckedLeaving, Employee, EmployeeSummary,
    EmploymentType, EndReason, Engagement,
};
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::hr::code_conflict;

const CODE_INDEX: &str = "employees_code_key";
const NATIONAL_ID_INDEX: &str = "employees_national_id_key";

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("employees.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_type(raw: &str) -> Result<EmploymentType, sqlx::Error> {
    EmploymentType::parse(raw).ok_or_else(|| unknown("employment_type", raw))
}

fn read_reason(raw: &str) -> Result<EndReason, sqlx::Error> {
    EndReason::parse(raw).ok_or_else(|| unknown("end_reason", raw))
}

/// A code clash and a national-identifier clash arrive the same way and mean
/// different things, so they are told apart by which index complained.
fn conflict(err: sqlx::Error, code: &str) -> DbError {
    if let sqlx::Error::Database(db) = &err {
        if db.constraint() == Some(NATIONAL_ID_INDEX) {
            return DbError::CodeExists {
                entity: "employee_national_id",
                code: code.to_owned(),
            };
        }
    }

    code_conflict(err, "employee", CODE_INDEX, code)
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Everybody, current and former, one row each.
///
/// A `LEFT JOIN` onto `current_staff` rather than a read of it: somebody who
/// has left has no row there, and a list that dropped them would be a staff
/// list nobody could look a leaver up in.
pub async fn list<'e, E>(executor: E) -> Result<Vec<EmployeeSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT e.id, e.code, e.given_name, e.family_name, e.preferred_name,
                e.work_email, (e.user_id IS NOT NULL) AS has_login,
                s.started_on, s.employment_type,
                s.department_name, s.job_title, s.work_location_name,
                s.manager_given_name, s.manager_family_name
           FROM hr.employees e
           LEFT JOIN hr.current_staff s ON s.employee_id = e.id
          ORDER BY lower(e.family_name), lower(e.given_name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let employment_type: Option<String> = row.try_get("employment_type")?;
            let manager_given: Option<String> = row.try_get("manager_given_name")?;
            let manager_family: Option<String> = row.try_get("manager_family_name")?;

            Ok(EmployeeSummary {
                id: row.try_get("id")?,
                code: row.try_get("code")?,
                given_name: row.try_get("given_name")?,
                family_name: row.try_get("family_name")?,
                preferred_name: row.try_get("preferred_name")?,
                work_email: row.try_get("work_email")?,
                has_login: row.try_get("has_login")?,
                started_on: row.try_get("started_on")?,
                employment_type: employment_type.as_deref().map(read_type).transpose()?,
                department_name: row.try_get("department_name")?,
                job_title: row.try_get("job_title")?,
                manager_name: manager_given
                    .zip(manager_family)
                    .map(|(given, family)| format!("{given} {family}")),
                work_location_name: row.try_get("work_location_name")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The people a picker may offer as a manager: everybody currently employed.
///
/// Employees rather than users, for the reason the column is an employee id:
/// most managers never sign in.
pub async fn employed<'e, E>(executor: E) -> Result<Vec<(Uuid, String, String)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT employee_id, code,
                COALESCE(NULLIF(btrim(preferred_name), ''), given_name) || ' ' || family_name
                    AS display_name
           FROM hr.current_staff
          ORDER BY lower(family_name), lower(given_name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("employee_id")?,
                row.try_get("code")?,
                row.try_get("display_name")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Employee>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT id, code, given_name, family_name, preferred_name, work_email,
                work_phone, user_id, date_of_birth, national_id, note
           FROM hr.employees WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    Ok(Some(Employee {
        id,
        code: row.try_get("code").map_err(DbError::Query)?,
        given_name: row.try_get("given_name").map_err(DbError::Query)?,
        family_name: row.try_get("family_name").map_err(DbError::Query)?,
        preferred_name: row.try_get("preferred_name").map_err(DbError::Query)?,
        work_email: row.try_get("work_email").map_err(DbError::Query)?,
        work_phone: row.try_get("work_phone").map_err(DbError::Query)?,
        user_id: row.try_get("user_id").map_err(DbError::Query)?,
        date_of_birth: row.try_get("date_of_birth").map_err(DbError::Query)?,
        national_id: row.try_get("national_id").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        engagements: engagements_of(executor, id).await?,
    }))
}

/// Who holds this login, if anybody. What a session uses to find the person
/// behind the account.
pub async fn find_by_user<'e, E>(executor: E, user_id: UserId) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT id FROM hr.employees WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)
}

/// Every period of employment, newest first, each with its assignments.
pub async fn engagements_of<'e, E>(
    executor: E,
    employee_id: Uuid,
) -> Result<Vec<Engagement>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let rows = sqlx::query(
        "SELECT id, employee_id, started_on, ended_on, end_reason, end_note,
                employment_type, expected_end_on, note
           FROM hr.engagements
          WHERE employee_id = $1
          ORDER BY started_on DESC, created_at DESC",
    )
    .bind(employee_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let mut engagements = Vec::with_capacity(rows.len());

    for row in rows {
        let id: Uuid = row.try_get("id").map_err(DbError::Query)?;
        let employment_type: String = row.try_get("employment_type").map_err(DbError::Query)?;
        let end_reason: Option<String> = row.try_get("end_reason").map_err(DbError::Query)?;

        engagements.push(Engagement {
            id,
            employee_id: row.try_get("employee_id").map_err(DbError::Query)?,
            started_on: row.try_get("started_on").map_err(DbError::Query)?,
            ended_on: row.try_get("ended_on").map_err(DbError::Query)?,
            end_reason: end_reason
                .as_deref()
                .map(read_reason)
                .transpose()
                .map_err(DbError::Query)?,
            end_note: row.try_get("end_note").map_err(DbError::Query)?,
            employment_type: read_type(&employment_type).map_err(DbError::Query)?,
            expected_end_on: row.try_get("expected_end_on").map_err(DbError::Query)?,
            note: row.try_get("note").map_err(DbError::Query)?,
            assignments: assignments_of(executor, id).await?,
        });
    }

    Ok(engagements)
}

/// One engagement's assignments, newest first, with every reference resolved
/// for display.
pub async fn assignments_of<'e, E>(
    executor: E,
    engagement_id: Uuid,
) -> Result<Vec<Assignment>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT a.id, a.engagement_id, a.effective_from, a.effective_to,
                a.department_id, a.job_position_id, a.work_location_id,
                a.manager_id, a.reason,
                d.name AS department_name,
                j.title AS job_title,
                w.name AS work_location_name,
                COALESCE(NULLIF(btrim(m.preferred_name), ''), m.given_name)
                    || ' ' || m.family_name AS manager_name
           FROM hr.assignments a
           LEFT JOIN hr.departments d ON d.id = a.department_id
           LEFT JOIN hr.job_positions j ON j.id = a.job_position_id
           LEFT JOIN hr.work_locations w ON w.id = a.work_location_id
           LEFT JOIN hr.employees m ON m.id = a.manager_id
          WHERE a.engagement_id = $1
          ORDER BY a.effective_from DESC, a.created_at DESC",
    )
    .bind(engagement_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok(Assignment {
                id: row.try_get("id")?,
                engagement_id: row.try_get("engagement_id")?,
                effective_from: row.try_get("effective_from")?,
                effective_to: row.try_get("effective_to")?,
                department_id: row.try_get("department_id")?,
                department_name: row.try_get("department_name")?,
                job_position_id: row.try_get("job_position_id")?,
                job_title: row.try_get("job_title")?,
                work_location_id: row.try_get("work_location_id")?,
                work_location_name: row.try_get("work_location_name")?,
                manager_id: row.try_get("manager_id")?,
                manager_name: row.try_get("manager_name")?,
                reason: row.try_get("reason")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The engagement somebody is in now, if any. What every act on a person needs
/// before it can do anything.
pub async fn open_engagement<'e, E>(
    executor: E,
    employee_id: Uuid,
) -> Result<Option<(Uuid, NaiveDate)>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, started_on FROM hr.engagements
          WHERE employee_id = $1 AND ended_on IS NULL",
    )
    .bind(employee_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(|row| Ok((row.try_get("id")?, row.try_get("started_on")?)))
        .transpose()
        .map_err(DbError::Query)
}

/// Who somebody reports to right now, for the cycle walk.
pub async fn current_manager<'e, E>(
    executor: E,
    employee_id: Uuid,
) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "SELECT a.manager_id
           FROM hr.assignments a
           JOIN hr.engagements g ON g.id = a.engagement_id
          WHERE g.employee_id = $1
            AND g.ended_on IS NULL
            AND a.effective_to IS NULL",
    )
    .bind(employee_id)
    .fetch_optional(executor)
    .await
    .map(Option::flatten)
    .map_err(DbError::Query)
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

pub async fn insert(
    conn: &mut PgConnection,
    draft: &CheckedEmployee,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar(
        "INSERT INTO hr.employees
             (code, given_name, family_name, preferred_name, work_email, work_phone,
              date_of_birth, national_id, note, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.given_name)
    .bind(&draft.family_name)
    .bind(draft.preferred_name.as_deref())
    .bind(draft.work_email.as_deref())
    .bind(draft.work_phone.as_deref())
    .bind(draft.date_of_birth)
    .bind(draft.national_id.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(|err| conflict(err, &draft.code))
}

/// The person's own details.
///
/// Deliberately touches nothing dated: an edit here is a correction to who
/// somebody is, and moving them between departments is a different act with its
/// own form. See [`open_assignment`].
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &CheckedEmployee,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE hr.employees
            SET code = $2, given_name = $3, family_name = $4, preferred_name = $5,
                work_email = $6, work_phone = $7, date_of_birth = $8,
                national_id = $9, note = $10, updated_at = now(), updated_by = $11
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.given_name)
    .bind(&draft.family_name)
    .bind(draft.preferred_name.as_deref())
    .bind(draft.work_email.as_deref())
    .bind(draft.work_phone.as_deref())
    .bind(draft.date_of_birth)
    .bind(draft.national_id.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(|err| conflict(err, &draft.code))?;

    Ok(done.rows_affected() > 0)
}

/// Link a login to a person, or clear it.
///
/// Separate from [`update`] because it is a different act with a different
/// permission behind it, and because the unique index means it can fail on its
/// own terms: a login already attached to somebody else is a refusal, not a
/// silent overwrite.
pub async fn set_login(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Option<UserId>,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE hr.employees
            SET user_id = $2, updated_at = now(), updated_by = $3
          WHERE id = $1",
    )
    .bind(id)
    .bind(user_id)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}

pub async fn start_engagement(
    conn: &mut PgConnection,
    employee_id: Uuid,
    started_on: NaiveDate,
    employment_type: EmploymentType,
    expected_end_on: Option<NaiveDate>,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar(
        "INSERT INTO hr.engagements
             (employee_id, started_on, employment_type, expected_end_on,
              created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $5)
         RETURNING id",
    )
    .bind(employee_id)
    .bind(started_on)
    .bind(employment_type.as_str())
    .bind(expected_end_on)
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// End one. `WHERE ended_on IS NULL` in the statement, so recording a leaver
/// twice does not overwrite the first reason with the second.
pub async fn end_engagement(
    conn: &mut PgConnection,
    engagement_id: Uuid,
    leaving: &CheckedLeaving,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE hr.engagements
            SET ended_on = $2, end_reason = $3, end_note = $4,
                updated_at = now(), updated_by = $5
          WHERE id = $1 AND ended_on IS NULL",
    )
    .bind(engagement_id)
    .bind(leaving.ended_on)
    .bind(leaving.reason.as_str())
    .bind(leaving.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Close the open assignment on an engagement, the day before the next starts.
///
/// Called before [`open_assignment`], in the same transaction, because
/// `assignments_one_open_per_engagement` refuses a second open row - which is
/// the index doing its job rather than an obstacle.
///
/// `effective_to` is the day *before* the new one begins, so the two do not
/// both cover the changeover date and a report run on that day does not count
/// the person twice.
pub async fn close_open_assignment(
    conn: &mut PgConnection,
    engagement_id: Uuid,
    ending_on: NaiveDate,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE hr.assignments
            SET effective_to = $2, updated_at = now(), updated_by = $3
          WHERE engagement_id = $1 AND effective_to IS NULL",
    )
    .bind(engagement_id)
    .bind(ending_on)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

pub async fn open_assignment(
    conn: &mut PgConnection,
    engagement_id: Uuid,
    draft: &CheckedAssignment,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar(
        "INSERT INTO hr.assignments
             (engagement_id, effective_from, department_id, job_position_id,
              work_location_id, manager_id, reason, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
         RETURNING id",
    )
    .bind(engagement_id)
    .bind(draft.effective_from)
    .bind(draft.department_id)
    .bind(draft.job_position_id)
    .bind(draft.work_location_id)
    .bind(draft.manager_id)
    .bind(draft.reason.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// How many engagements a person has. The delete guard: somebody who has ever
/// been employed is history, and history is recorded as a leaver rather than
/// removed.
pub async fn engagement_count<'e, E>(executor: E, employee_id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM hr.engagements WHERE employee_id = $1")
        .bind(employee_id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

/// How many people currently report to somebody. Checked before a leaver is
/// recorded, so the screen can say who has to be reassigned.
pub async fn direct_reports<'e, E>(executor: E, employee_id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "SELECT count(*) FROM hr.current_staff WHERE manager_id = $1",
    )
    .bind(employee_id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query("DELETE FROM hr.employees WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() > 0)
}

/// What each department currently costs in people.
pub async fn headcount<'e, E>(executor: E) -> Result<Vec<(Uuid, String, i64)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT department_id, name, headcount FROM hr.current_headcount ORDER BY name",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("department_id")?,
                row.try_get("name")?,
                row.try_get("headcount")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}
