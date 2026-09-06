//! Reading the chart of accounts.
//!
//! Read-only in this release. The chart is seeded from
//! `config/defaults/books.toml` when a workspace is provisioned, and editing it
//! belongs with the ledger that posts to it - an account somebody can rename
//! while a journal names it is a report that changes after it was filed.

use app_books::account::Account;
use phonix_core::permissions;
use phonix_db::books::account as store;
use phonix_db::sqlx::PgPool;

use crate::caller::Caller;
use crate::error::ServiceResult;

/// Every account, in number order.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Account>> {
    caller.require(permissions::ACCOUNTS)?;
    Ok(store::list(pool).await?)
}

/// How many accounts there are, and how many are active.
pub async fn counts(pool: &PgPool, caller: &Caller) -> ServiceResult<(i64, i64)> {
    caller.require(permissions::ACCOUNTS)?;
    Ok(store::counts(pool).await?)
}
