//! `inventory.account_mappings`: which account an item's postings land on.
//!
//! # `account_id` is a bare id, and that is the whole point
//!
//! No foreign key into `books.accounts`, exactly as `books.invoices` carries a
//! `master.parties` id without one. Nothing here joins to that table, the
//! `Ledger` port verifies the id when a posting arrives, and dropping the
//! `books` schema leaves rows that resolve to nothing rather than a database
//! that will not drop. See ADR 0001.
//!
//! `account_number` and `account_name` are a snapshot so a mapping screen draws
//! without asking Books for a row per line. Refreshed when the mapping is
//! edited, and never trusted for anything but display.

use app_inventory::accounts::{AccountOverrides, AccountRef};
use phonix_core::identity::UserId;
use phonix_ports::ledger::AccountRole;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// Which table an `owner_id` is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Category,
    Item,
}

impl Owner {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Item => "item",
        }
    }
}

/// What one owner has overridden. Empty for almost every row, which is what
/// the default is: nothing set, and every posting falls through to the role.
pub async fn for_owner<'e, E>(
    executor: E,
    owner: Owner,
    owner_id: Uuid,
) -> Result<AccountOverrides, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT role, account_id, account_number, account_name
           FROM inventory.account_mappings
          WHERE owner_kind = $1 AND owner_id = $2",
    )
    .bind(owner.as_str())
    .bind(owner_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let mut overrides = AccountOverrides::default();

    for row in &rows {
        let role: String = row.try_get("role").map_err(DbError::Query)?;
        let chosen = AccountRef {
            account_id: row.try_get("account_id").map_err(DbError::Query)?,
            number: row.try_get("account_number").map_err(DbError::Query)?,
            name: row.try_get("account_name").map_err(DbError::Query)?,
        };

        // A role this build does not know is a row written by a newer
        // deployment. Skipped rather than refused: an unrecognised override
        // falls through to the default, which is the safe direction.
        match AccountRole::parse(&role) {
            Some(AccountRole::Inventory) => overrides.stock_valuation = Some(chosen),
            Some(AccountRole::GoodsReceivedNotInvoiced) => overrides.stock_input = Some(chosen),
            Some(AccountRole::GoodsDeliveredNotInvoiced) => overrides.stock_output = Some(chosen),
            Some(AccountRole::PurchasePriceVariance) => overrides.price_difference = Some(chosen),
            Some(AccountRole::Revenue) => overrides.revenue = Some(chosen),
            Some(AccountRole::CostOfSales) => overrides.cost_of_sales = Some(chosen),
            _ => {}
        }
    }

    Ok(overrides)
}

/// Point a role at an account for this owner, or move it.
pub async fn set<'e, E>(
    executor: E,
    owner: Owner,
    owner_id: Uuid,
    role: AccountRole,
    chosen: &AccountRef,
    actor: Option<UserId>,
) -> Result<(), DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query(
        "INSERT INTO inventory.account_mappings
             (owner_kind, owner_id, role, account_id, account_number, account_name, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (owner_kind, owner_id, role) DO UPDATE
            SET account_id     = EXCLUDED.account_id,
                account_number = EXCLUDED.account_number,
                account_name   = EXCLUDED.account_name,
                updated_at     = now(),
                updated_by     = EXCLUDED.updated_by",
    )
    .bind(owner.as_str())
    .bind(owner_id)
    .bind(role.as_str())
    .bind(chosen.account_id)
    .bind(&chosen.number)
    .bind(&chosen.name)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Stop overriding a role: it falls back to the category's, then to the role's
/// own default in `books.account_roles`.
pub async fn clear<'e, E>(
    executor: E,
    owner: Owner,
    owner_id: Uuid,
    role: AccountRole,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "DELETE FROM inventory.account_mappings
          WHERE owner_kind = $1 AND owner_id = $2 AND role = $3",
    )
    .bind(owner.as_str())
    .bind(owner_id)
    .bind(role.as_str())
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// Every mapping pointing at one account, asked before retiring it.
pub async fn using_account<'e, E>(executor: E, account_id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM inventory.account_mappings WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}
