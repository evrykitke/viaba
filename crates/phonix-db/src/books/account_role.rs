//! `books.account_roles`: which account a sub-ledger's posting lands on.
//!
//! Account determination. A goods receipt names a role and this says what that
//! role means in this workspace, so the chart stays renumberable - see
//! `migrations/apps/books/0004_account_roles.sql`.

use app_books::account::DefaultChart;
use phonix_core::identity::UserId;
use phonix_ports::ledger::AccountRole;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// One mapping, as a screen reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleMapping {
    pub role: String,
    pub account_id: Uuid,
    pub account_number: String,
    pub account_name: String,
}

/// The account a role means, or `None` where nobody has chosen one.
pub async fn account_for<'e, E>(executor: E, role: &str) -> Result<Option<Uuid>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT account_id FROM books.account_roles WHERE role = $1")
        .bind(role)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)
}

/// Point every role the default chart declares at the account it names.
///
/// # Why this is not the migration's job any more
///
/// `0004_account_roles.sql` seeded these, and on a workspace provisioned after
/// it was written it seeded nothing: migrations run first and the chart is
/// installed afterwards, so the accounts those statements selected from did not
/// exist yet. The mapping was empty for every new workspace, and the first
/// goods receipt failed with an unmapped role.
///
/// So it belongs beside the chart it is derived from - which also means a role
/// added by a later release reaches a workspace that already has Books, the
/// same way a new account does.
///
/// `ON CONFLICT DO NOTHING`, so a workspace that has remapped a role keeps its
/// choice. A role naming an account the workspace has since deleted or retired
/// installs nothing rather than failing: the port answers `UnmappedRole` for it
/// and a screen shows a sentence.
pub async fn install_defaults<'e, E>(executor: E, chart: &DefaultChart) -> Result<u64, DbError>
where
    E: PgExecutor<'e>,
{
    if chart.role.is_empty() {
        return Ok(0);
    }

    let mut roles: Vec<&str> = Vec::with_capacity(chart.role.len());
    let mut numbers: Vec<&str> = Vec::with_capacity(chart.role.len());

    for mapping in &chart.role {
        let role = mapping.role.trim();

        // The closed set is checked here rather than in `app-books`, which is
        // compiled for the browser and may not name a port. A role this build
        // does not know would install a row nothing ever asks for.
        if AccountRole::parse(role).is_none() {
            return Err(DbError::CorruptCatalogRow {
                slug: "books".to_owned(),
                reason: format!("default chart maps unknown account role '{role}'"),
            });
        }

        roles.push(role);
        numbers.push(mapping.number.trim());
    }

    let result = sqlx::query(
        "INSERT INTO books.account_roles (role, account_id)
              SELECT m.role, a.id
                FROM unnest($1::text[], $2::text[]) AS m(role, number)
                JOIN books.accounts a ON a.number = m.number AND a.is_active
         ON CONFLICT DO NOTHING",
    )
    .bind(&roles)
    .bind(&numbers)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(result.rows_affected())
}

/// Every mapping, with the account each one names.
pub async fn list<'e, E>(executor: E) -> Result<Vec<RoleMapping>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT r.role, r.account_id, a.number AS account_number, a.name AS account_name
           FROM books.account_roles r
           JOIN books.accounts a ON a.id = r.account_id
          ORDER BY r.role",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok(RoleMapping {
                role: row.try_get("role").map_err(DbError::Query)?,
                account_id: row.try_get("account_id").map_err(DbError::Query)?,
                account_number: row.try_get("account_number").map_err(DbError::Query)?,
                account_name: row.try_get("account_name").map_err(DbError::Query)?,
            })
        })
        .collect()
}

/// Unmap a role. `false` where it was not mapped in the first place.
///
/// A deliberate act, not a tidy-up: the role stops meaning anything here, and
/// the next posting that needs it is refused by name. That is sometimes the
/// right answer - a workspace that does not track goods delivered and not
/// invoiced should be told so at the moment it matters, rather than have the
/// accrual land in whichever account was nearest.
pub async fn clear<'e, E>(executor: E, role: &str) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let affected = sqlx::query("DELETE FROM books.account_roles WHERE role = $1")
        .bind(role)
        .execute(executor)
        .await
        .map_err(DbError::Query)?
        .rows_affected();

    Ok(affected > 0)
}

/// Point a role at an account, or move it.
pub async fn set<'e, E>(
    executor: E,
    role: &str,
    account_id: Uuid,
    actor: Option<UserId>,
) -> Result<(), DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query(
        "INSERT INTO books.account_roles (role, account_id, updated_by)
         VALUES ($1, $2, $3)
         ON CONFLICT (role) DO UPDATE
            SET account_id = EXCLUDED.account_id,
                updated_at = now(),
                updated_by = EXCLUDED.updated_by",
    )
    .bind(role)
    .bind(account_id)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}
