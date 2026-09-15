//! Account determination: which account each kind of posting lands on.
//!
//! The mapping `books.account_roles` holds, as a screen reads and writes it.
//! Every role this build knows comes back, mapped or not - a list of the ones
//! somebody has already chosen would hide exactly the rows worth looking at.
//!
//! # The same rule the item screen applies
//!
//! An account is offered for a role only if the ledger says it carries that
//! role, and one that does not is refused rather than stored. Two hundred
//! accounts, any of which balances, is how revenue ends up in petty cash - and
//! the difference is not discovered by the ledger, because a wrong account
//! balances exactly as well as a right one.
//!
//! # It is the chart's permission, not the ledger's
//!
//! Mapping a role is deciding what the chart means, so it takes the permission
//! that edits the chart. Somebody who may post a journal is not thereby allowed
//! to decide where every future goods receipt lands.

use app_books::account::RoleMapping;
use phonix_core::permissions;
use phonix_db::books::account_role as store;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::{AccountRole, Ledger, LedgerAccount, LedgerError};
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// Every role, in the order the port declares them, with what each one means.
///
/// `AccountRole::ALL` drives it rather than the table. A role nobody has mapped
/// is the row that matters most on this screen, and reading the table alone
/// would leave it out.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<RoleMapping>> {
    caller.require(permissions::ACCOUNTS)?;

    let stored = store::list(pool).await?;

    Ok(AccountRole::ALL
        .iter()
        .copied()
        .map(|role| {
            let found = stored.iter().find(|row| row.role == role.as_str());

            RoleMapping {
                role,
                account_id: found.map(|row| row.account_id),
                number: found
                    .map(|row| row.account_number.clone())
                    .unwrap_or_default(),
                name: found
                    .map(|row| row.account_name.clone())
                    .unwrap_or_default(),
            }
        })
        .collect())
}

/// Every account a role may be pointed at, each carrying the roles it fits.
///
/// The chart, through the port, rather than read out of `books.accounts` here.
/// The fit is the ledger's judgement and this screen is only drawing it; asking
/// it the same way the item screen does is what keeps the two lists identical.
pub async fn chart(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<LedgerAccount>> {
    caller.require(permissions::ACCOUNTS)?;

    let ledger = super::BooksLedger::new(pool.clone(), caller.clone());

    ledger
        .postable_accounts()
        .await
        .map_err(|err| ServiceError::rejected("chart", refusal(err, String::new())))
}

/// Point a role at an account, or stop it meaning anything.
///
/// `None` unmaps it. That is a real choice rather than a way of clearing a
/// mistake: a workspace that does not want an accrual for goods delivered and
/// not invoiced is better told at the moment one is needed than given whichever
/// account happened to be nearest.
pub async fn set(
    pool: &PgPool,
    caller: &Caller,
    role: AccountRole,
    account_id: Option<Uuid>,
) -> ServiceResult<()> {
    caller.require(permissions::ACCOUNTS_EDIT)?;
    acting_user(caller)?;

    // Read first, so the trail records what it was as well as what it became.
    let before = list(pool, caller)
        .await?
        .into_iter()
        .find(|mapping| mapping.role == role);

    let after = match account_id {
        Some(account_id) => {
            let account = check_suits(pool, caller, role, account_id).await?;
            store::set(pool, role.as_str(), account_id, caller.user_id()).await?;

            format!("{} \u{b7} {}", account.number, account.name)
        }
        None => {
            store::clear(pool, role.as_str()).await?;
            String::new()
        }
    };

    audit::updated(
        pool,
        caller,
        Target::new(kinds::ACCOUNT_ROLE, Uuid::nil())
            .named(role.as_str())
            .fact("role", role.as_str())
            .fact("account", &after),
        &before.as_ref().map(RoleMapping::label).unwrap_or_default(),
        &after,
    )
    .await;

    Ok(())
}

/// Whether this account carries this role, and what it is called.
///
/// The same question the item screen asks before storing an override, asked the
/// same way: the ledger judges its own chart, and a screen that decided for
/// itself would be a second opinion to keep in step.
async fn check_suits(
    pool: &PgPool,
    caller: &Caller,
    role: AccountRole,
    account_id: Uuid,
) -> ServiceResult<LedgerAccount> {
    let ledger = super::BooksLedger::new(pool.clone(), caller.clone());

    let account = ledger
        .postable_accounts()
        .await
        .map_err(|err| ServiceError::rejected("account_id", refusal(err, String::new())))?
        .into_iter()
        .find(|account| account.id == account_id);

    let Some(account) = account else {
        return Err(ServiceError::rejected(
            "account_id",
            phonix_core::msg!(
                "ledger.error.account_unpostable",
                account = account_id.to_string()
            ),
        ));
    };

    let named = format!("{} \u{b7} {}", account.number, account.name);

    match ledger.account_fit(account_id, role).await {
        Ok(Some(_)) => Ok(account),
        Ok(None) => Err(ServiceError::rejected(
            "account_id",
            phonix_core::msg!("ledger.error.account_not_suited", account = named),
        )),
        Err(err) => Err(ServiceError::rejected("account_id", refusal(err, named))),
    }
}

/// A ledger that would not answer, in words a settings screen can show.
fn refusal(err: LedgerError, account: String) -> phonix_core::Message {
    match err {
        LedgerError::UnpostableAccount(_) => {
            phonix_core::msg!("ledger.error.account_unpostable", account = account)
        }
        LedgerError::Refused(message) => message,
        other => phonix_core::msg!("books.error.not_posted", detail = other.to_string()),
    }
}
