//! The four statements, assembled for whoever asked for one.
//!
//! Thin on purpose. The arithmetic is [`app_books::report`], the aggregation is
//! [`phonix_db::books::report`], and what is left here is the three things
//! neither of those may know: who is allowed to read a report, what currency
//! the workspace keeps its books in, and when its financial year opened.
//!
//! # Reading a report is one permission
//!
//! Not four. A trial balance and a profit and loss are the same figures
//! arranged twice, and somebody who may see one may work out the other in a
//! minute with a pencil. Splitting them would be a control that looks like one
//! without being one.
//!
//! # Every statement takes its dates from the caller
//!
//! Including "today". A default worked out down here would be the server's
//! today, which is not necessarily the reader's, and a report is a document
//! about a date somebody chose.

use app_books::report::{
    AccountMovement, BalanceSheet, CustomerStatement, IncomeStatement, LedgerSummary, TrialBalance,
};
use chrono::{Datelike, NaiveDate};
use phonix_core::locale::Currency;
use phonix_core::money::MoneyError;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::sqlx::PgPool;
use phonix_master::party::PartySummary;
use uuid::Uuid;

use crate::caller::Caller;
use crate::error::{ServiceError, ServiceResult};

/// Every account, both columns, for a span.
pub async fn trial_balance(
    pool: &PgPool,
    caller: &Caller,
    from: NaiveDate,
    to: NaiveDate,
) -> ServiceResult<TrialBalance> {
    caller.require(permissions::REPORTS)?;

    let (currency, movements) = ledger(pool, from, to).await?;

    TrialBalance::assemble(from, to, currency, &movements).map_err(unusable)
}

/// What was earned and what it cost, between two dates.
pub async fn profit_and_loss(
    pool: &PgPool,
    caller: &Caller,
    from: NaiveDate,
    to: NaiveDate,
) -> ServiceResult<IncomeStatement> {
    caller.require(permissions::REPORTS)?;

    let (currency, movements) = ledger(pool, from, to).await?;

    IncomeStatement::assemble(from, to, currency, &movements).map_err(unusable)
}

/// What is owned and what is owed, at one date.
///
/// The span runs from the day the financial year opened, so the profit and
/// loss accounts arrive split in two: what they held when the year opened is
/// brought forward, and what they have done since is this year's result. See
/// [`BalanceSheet`] for why both are on the face of it.
pub async fn balance_sheet(
    pool: &PgPool,
    caller: &Caller,
    as_at: NaiveDate,
) -> ServiceResult<BalanceSheet> {
    caller.require(permissions::REPORTS)?;

    let profile = crate::workspace::profile::current(pool).await?;
    let start_month = u32::from(profile.fiscal_year_start_month).clamp(1, 12);
    let opened = year_opened(as_at, start_month)
        .ok_or_else(|| ServiceError::rejected("as_at", msg!("reports.error.no_year")))?;

    let currency = crate::workspace::profile::require_currency(&profile)?;
    let movements = phonix_db::books::report::movements(pool, opened, as_at, currency).await?;

    BalanceSheet::assemble(as_at, opened, currency, &movements).map_err(unusable)
}

/// What one customer has been invoiced, what they have paid, and how long ago.
pub async fn customer_statement(
    pool: &PgPool,
    caller: &Caller,
    party_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> ServiceResult<CustomerStatement> {
    caller.require(permissions::REPORTS)?;

    let Some(party) = phonix_db::master::party::find(pool, party_id).await? else {
        return Err(ServiceError::rejected("party_id", msg!("error.party.gone")));
    };

    let currency = crate::workspace::profile::base_currency(pool).await?;
    let entries = phonix_db::books::report::statement_entries(pool, party_id, to, currency).await?;
    // Asked separately rather than derived from the entries: what is on account
    // is every payment less every allocation, and the entries carry the
    // allocation only per invoice.
    let on_account = phonix_db::books::report::on_account(pool, party_id, to, currency).await?;

    CustomerStatement::assemble(
        party.id, party.code, party.name, from, to, currency, entries, on_account,
    )
    .map_err(unusable)
}

/// The customers a statement may be run for.
///
/// Its own list rather than the master-data one, because reading a statement
/// is not the same grant as reading the customer file: somebody in credit
/// control holds this and need not hold that.
pub async fn statement_customers(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<PartySummary>> {
    caller.require(permissions::REPORTS)?;

    Ok(
        phonix_db::master::party::list(pool, Some(app_books::CUSTOMER_ROLE))
            .await?
            .into_iter()
            .filter(|party| party.is_active)
            .collect(),
    )
}

/// The span a report opens on: the financial year, so far.
///
/// Asked of the server rather than worked out in the browser. Two reasons, and
/// both matter: only this side knows when the workspace's financial year began,
/// and a date computed during the server's render and again during hydration is
/// a mismatch on any night the two disagree about the date.
pub async fn default_span(pool: &PgPool, caller: &Caller) -> ServiceResult<(NaiveDate, NaiveDate)> {
    caller.require(permissions::REPORTS)?;

    let profile = crate::workspace::profile::current(pool).await?;
    let today = chrono::Utc::now().date_naive();
    let start_month = u32::from(profile.fiscal_year_start_month).clamp(1, 12);

    let opened = year_opened(today, start_month)
        .ok_or_else(|| ServiceError::rejected("as_at", msg!("reports.error.no_year")))?;

    Ok((opened, today))
}

/// The figures the app's front page carries.
///
/// Worked out here, in one call, so the page renders them from one answer
/// rather than three that could be taken at different moments. The dates are
/// the server's own - a front page has no date picker, and one worked out in
/// the browser as well would disagree with itself at midnight.
pub async fn summary(pool: &PgPool, caller: &Caller) -> ServiceResult<LedgerSummary> {
    caller.require(permissions::REPORTS)?;

    let profile = crate::workspace::profile::current(pool).await?;
    let currency = crate::workspace::profile::require_currency(&profile)?;
    let as_at = chrono::Utc::now().date_naive();
    let start_month = u32::from(profile.fiscal_year_start_month).clamp(1, 12);
    let opened = year_opened(as_at, start_month)
        .ok_or_else(|| ServiceError::rejected("as_at", msg!("reports.error.no_year")))?;

    let movements = phonix_db::books::report::movements(pool, opened, as_at, currency).await?;

    let sheet = BalanceSheet::assemble(as_at, opened, currency, &movements).map_err(unusable)?;
    let earned =
        IncomeStatement::assemble(opened, as_at, currency, &movements).map_err(unusable)?;

    Ok(LedgerSummary {
        currency,
        as_at,
        year_opened: opened,
        revenue: earned.revenue.total,
        result: earned.net_profit,
        total_assets: sheet.total_assets,
        owed_by_customers: phonix_db::books::report::invoiced_outstanding(pool, currency).await?,
    })
}

/// The ledger in the workspace's own currency, for a span.
async fn ledger(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> ServiceResult<(Currency, Vec<AccountMovement>)> {
    let currency = crate::workspace::profile::base_currency(pool).await?;
    let movements = phonix_db::books::report::movements(pool, from, to, currency).await?;

    Ok((currency, movements))
}

/// The first day of the financial year `as_at` falls in.
///
/// A year that opens in April means January belongs to the one before.
fn year_opened(as_at: NaiveDate, start_month: u32) -> Option<NaiveDate> {
    let year = if as_at.month() >= start_month {
        as_at.year()
    } else {
        as_at.year() - 1
    };

    NaiveDate::from_ymd_opt(year, start_month, 1)
}

/// An amount the arithmetic could not carry. Not a rejection of anything
/// anybody typed - it means a balance has grown past what the column holds, or
/// two currencies reached the same total, and either way the report is the
/// wrong place to find out quietly.
fn unusable(err: MoneyError) -> ServiceError {
    ServiceError::rejected("amount", err.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn january_belongs_to_the_year_that_opened_in_april() {
        let january = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        assert_eq!(year_opened(january, 4), NaiveDate::from_ymd_opt(2025, 4, 1));
        assert_eq!(year_opened(january, 1), NaiveDate::from_ymd_opt(2026, 1, 1));
    }
}
