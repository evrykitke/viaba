//! The chart of accounts: reading it, adding to it, and changing one.
//!
//! There is no delete. An account that has been posted to can never be removed
//! - the history would stop naming anything - and one that has not is retired
//! by clearing Active. Same rule the migration states, and the same one
//! `master::party` follows.

use app_books::account::{Account, AccountInput};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::books::account as store;
use phonix_db::error::DbError;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

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

/// One account.
pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Account> {
    caller.require(permissions::ACCOUNTS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("account", msg!("accounts.gone")))
}

/// The editable part of one, for the form to open on.
pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<AccountInput> {
    Ok(AccountInput::from_account(&detail(pool, caller, id).await?))
}

/// Add an account, or change one. `id` absent means add.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: AccountInput,
) -> ServiceResult<Submission<AccountInput>> {
    match draft.id {
        None => create(pool, caller, draft).await,
        Some(id) => update(pool, caller, id, draft).await,
    }
}

async fn create(
    pool: &PgPool,
    caller: &Caller,
    draft: AccountInput,
) -> ServiceResult<Submission<AccountInput>> {
    caller.require(permissions::ACCOUNTS_CREATE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::insert(pool, &checked, caller.user_id()).await {
        Ok(id) => id,
        Err(DbError::CodeExists { code, .. }) => return Ok(number_taken(&code)),
        Err(err) => return Err(err.into()),
    };

    let stored = AccountInput {
        id: Some(id),
        ..checked
    };

    audit::created(
        pool,
        caller,
        Target::new(kinds::ACCOUNT, id)
            .named(&stored.name)
            .fact("number", &stored.number)
            .fact("type", stored.account_type.as_str()),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

async fn update(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    draft: AccountInput,
) -> ServiceResult<Submission<AccountInput>> {
    caller.require(permissions::ACCOUNTS_EDIT)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let before = detail(pool, caller, id).await?;

    let stored = match store::update(pool, id, &checked, caller.user_id()).await {
        Ok(true) => AccountInput {
            id: Some(id),
            ..checked
        },
        Ok(false) => return Ok(Submission::rejected("name", msg!("accounts.gone"))),
        Err(DbError::CodeExists { code, .. }) => return Ok(number_taken(&code)),
        Err(err) => return Err(err.into()),
    };

    audit::updated(
        pool,
        caller,
        Target::new(kinds::ACCOUNT, id).named(&stored.name),
        &AccountInput::from_account(&before),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Reported on the number field, which is the one the person typed.
fn number_taken(number: &str) -> Submission<AccountInput> {
    Submission::rejected("number", msg!("accounts.number_taken", number = number))
}
