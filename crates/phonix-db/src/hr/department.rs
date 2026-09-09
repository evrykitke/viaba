//! `hr.departments`.
//!
//! [`list`] reads every row and the caller filters, unlike the parties grid.
//! Depth follows from the parent chain, so a filtered query returns rows whose
//! parents are missing; `app_hr::in_tree_order` arranges them, in code the
//! browser has too. The table is a few hundred rows at its largest.
//!
//! `updated_at` is set at the call site. There is no trigger — see ADR 0001.

use app_hr::department::{Department, DepartmentInput, DepartmentSummary};
use phonix_core::identity::UserId;
use sqlx::{FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// Matched by name, not message text: Postgres localises the message.
const CODE_INDEX: &str = "departments_code_key";

/// Turn the unique-index violation into something a form can render. Reached
/// by a typed code that is taken, and by an allocator whose `start_at` was
/// edited backwards.
fn as_code_conflict(err: sqlx::Error, code: &str) -> DbError {
    super::code_conflict(err, "department", CODE_INDEX, code)
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Department> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self(Department {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            parent_id: row.try_get("parent_id")?,
            is_cost_centre: row.try_get("is_cost_centre")?,
            manager_user_id: row.try_get("manager_user_id")?,
            is_active: row.try_get("is_active")?,
        }))
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<DepartmentSummary> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self(DepartmentSummary {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            parent_id: row.try_get("parent_id")?,
            is_cost_centre: row.try_get("is_cost_centre")?,
            is_active: row.try_get("is_active")?,
            // Filled by `app_hr::in_tree_order`.
            depth: 0,
            manager_name: row.try_get("manager_name")?,
            child_count: row.try_get("child_count")?,
        }))
    }
}

/// A newtype so `FromRow` can live on types this crate does not own — `app-hr`
/// compiles to wasm and has no sqlx.
struct RowOf<T>(T);

/// Every department, arranged into the tree. Inactive rows included: hiding
/// them would hide the division that live cost centres sit under.
pub async fn list<'e, E>(executor: E) -> Result<Vec<DepartmentSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<DepartmentSummary>>(
        // LEFT: a department with no manager is ordinary.
        "SELECT d.id, d.code, d.name, d.parent_id, d.is_cost_centre, d.is_active,
                u.display_name AS manager_name,
                (SELECT count(*) FROM hr.departments c WHERE c.parent_id = d.id)
                    AS child_count
           FROM hr.departments d
           LEFT JOIN core.users u ON u.id = d.manager_user_id
          ORDER BY lower(d.name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    // The SQL sort decides sibling order; `in_tree_order` is stable over it.
    Ok(app_hr::in_tree_order(
        rows.into_iter().map(|row| row.0).collect(),
    ))
}

/// The active cost centres, for the port. A separate query rather than a
/// filter over [`list`]: it is called while somebody is posting, and should hit
/// the partial index rather than read the whole table.
pub async fn cost_centres<'e, E>(executor: E) -> Result<Vec<Department>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<Department>>(
        "SELECT id, code, name, parent_id, is_cost_centre, manager_user_id, is_active
           FROM hr.departments
          WHERE is_cost_centre AND is_active
          ORDER BY lower(name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// One department, whatever state it is in. The port's `resolve` is built on
/// this and has to answer for retired rows.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Department>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Department>>(
        "SELECT id, code, name, parent_id, is_cost_centre, manager_user_id, is_active
           FROM hr.departments
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// Create a department. `draft.code` is already allocated by the service, in
/// the same transaction, so a rolled-back insert returns the number.
pub async fn insert<'e, E>(
    executor: E,
    draft: &DepartmentInput,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO hr.departments
             (code, name, parent_id, is_cost_centre, manager_user_id, is_active,
              created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.parent_id)
    .bind(draft.is_cost_centre)
    .bind(draft.manager_user_id)
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| as_code_conflict(err, &draft.code))
}

/// Change one. Answers whether a row was there to change.
pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &DepartmentInput,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        // `updated_at` is written here, not by a trigger. See ADR 0001.
        "UPDATE hr.departments
            SET code            = $2,
                name            = $3,
                parent_id       = $4,
                is_cost_centre  = $5,
                manager_user_id = $6,
                is_active       = $7,
                updated_at      = now(),
                updated_by      = $8
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.parent_id)
    .bind(draft.is_cost_centre)
    .bind(draft.manager_user_id)
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_code_conflict(err, &draft.code))?;

    Ok(result.rows_affected() > 0)
}

/// Remove one. Answers whether a row was there to remove.
///
/// It cannot check whether anything has been charged here — no app holds a
/// foreign key into `hr`, which is what makes the schema droppable. The service
/// refuses any cost centre for that reason and offers deactivation instead.
/// Children are `ON DELETE RESTRICT`, so Postgres refuses those itself.
pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM hr.departments WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// The ids of every department beneath this one, at any depth — what the
/// re-parent cycle check needs. `UNION`, not `UNION ALL`, so already-cyclic
/// data terminates.
pub async fn descendants_of<'e, E>(executor: E, id: Uuid) -> Result<Vec<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "WITH RECURSIVE below AS (
             SELECT id FROM hr.departments WHERE parent_id = $1
             UNION
             SELECT d.id FROM hr.departments d JOIN below b ON d.parent_id = b.id
         )
         SELECT id FROM below",
    )
    .bind(id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)
}

/// How deep this department sits, counting itself as one. `None` for an id
/// that is not there.
pub async fn depth_of<'e, E>(executor: E, id: Uuid) -> Result<Option<i64>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "WITH RECURSIVE up AS (
             SELECT id, parent_id, 1 AS level FROM hr.departments WHERE id = $1
             UNION ALL
             SELECT d.id, d.parent_id, up.level + 1
               FROM hr.departments d JOIN up ON d.id = up.parent_id
              -- Stops a cyclic parent chain looping for ever.
              WHERE up.level < 64
         )
         SELECT max(level) FROM up",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)
    .map(Option::flatten)
}

/// Who could be named as a manager: id and display name, nothing else.
///
/// Active accounts only. A department managed by somebody who has left keeps
/// its stored id; what would be wrong is offering that person today.
pub async fn manager_candidates<'e, E>(executor: E) -> Result<Vec<(UserId, String)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, display_name
           FROM core.users
          WHERE deleted_at IS NULL AND status = 'active'
          ORDER BY lower(display_name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("display_name")?)))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// How many departments there are, and how many are chargeable. One round trip
/// because the home page shows them side by side.
pub async fn counts<'e, E>(executor: E) -> Result<(i64, i64), DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE is_cost_centre AND is_active) AS chargeable
           FROM hr.departments",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Ok((row.try_get("total")?, row.try_get("chargeable")?))
}
