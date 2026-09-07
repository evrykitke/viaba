//! `books.account_roles`: which account a sub-ledger's posting lands on.
//!
//! Account determination. A goods receipt names a role and this says what that
//! role means in this workspace, so the chart stays renumberable - see
//! `migrations/apps/books/0004_account_roles.sql`.

use phonix_core::identity::UserId;
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
