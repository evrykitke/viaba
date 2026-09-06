//! The accounting calendar: opening a year, and closing a period.
//!
//! The year's shape is the accountant's decision, not this module's: it reads
//! `fiscal_year_start_month` off the organization profile. A workspace whose
//! year opens in April gets April to March, and nothing here has an opinion
//! about which is right.

use app_books::period::{Period, PeriodError};
use chrono::{Datelike, NaiveDate, Utc};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::books::period as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// Every period, oldest first.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Period>> {
    caller.require(permissions::PERIODS)?;
    Ok(store::list(pool).await?)
}

/// How many periods there are, and how many are open.
pub async fn counts(pool: &PgPool, caller: &Caller) -> ServiceResult<(i64, i64)> {
    caller.require(permissions::PERIODS)?;
    Ok(store::counts(pool).await?)
}

/// Open the financial year that begins in `year`, and say how many periods
/// that created.
///
/// Idempotent, so running it twice is not twenty-four months. A workspace that
/// opened half a year - because the first attempt was interrupted - gets the
/// rest.
pub async fn open_year(pool: &PgPool, caller: &Caller, year: i32) -> ServiceResult<u64> {
    caller.require(permissions::PERIODS_MANAGE)?;
    acting_user(caller)?;

    let periods = year_beginning(pool, year).await?;

    let Some(first) = periods.first() else {
        return Err(ServiceError::rejected(
            "year",
            msg!("periods.error.bad_year"),
        ));
    };

    let label = first.label.clone();
    let created = store::open_year(pool, &periods).await?;

    if created > 0 {
        audit::created(
            pool,
            caller,
            Target::new(kinds::PERIOD, Uuid::nil())
                .named(&label)
                .fact("periods", created.to_string()),
            &label,
        )
        .await;
    }

    Ok(created)
}

/// The twelve months of the financial year starting in `year`, per the
/// organization's own year-start month.
async fn year_beginning(
    pool: &PgPool,
    year: i32,
) -> ServiceResult<Vec<app_books::period::NewPeriod>> {
    let profile = crate::workspace::profile::current(pool).await?;
    let month = u32::from(profile.fiscal_year_start_month).clamp(1, 12);

    let Some(first_day) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Ok(Vec::new());
    };

    Ok(app_books::period::year_from(first_day))
}

/// Shut a period. Nothing dated inside it may be posted afterwards.
pub async fn close(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Period> {
    set_closed(pool, caller, id, true).await
}

/// Open one again.
///
/// Allowed, and audited. A period that could never be reopened would make a
/// late correction impossible, and the correction would happen in a
/// spreadsheet instead - which is the outcome the lock exists to prevent.
pub async fn reopen(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Period> {
    set_closed(pool, caller, id, false).await
}

async fn set_closed(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    closed: bool,
) -> ServiceResult<Period> {
    caller.require(permissions::PERIODS_MANAGE)?;
    acting_user(caller)?;

    let before = find(pool, caller, id).await?;

    if before.is_closed == closed {
        return Ok(before);
    }

    if !store::set_closed(pool, id, closed, caller.user_id()).await? {
        return Err(ServiceError::rejected("period", msg!("periods.gone")));
    }

    let after = find(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::PERIOD, id).named(&after.label),
        &before,
        &after,
    )
    .await;

    Ok(after)
}

/// One period.
pub async fn find(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Period> {
    caller.require(permissions::PERIODS)?;

    store::list(pool)
        .await?
        .into_iter()
        .find(|period| period.id == id)
        .ok_or_else(|| ServiceError::rejected("period", msg!("periods.gone")))
}

/// The period a journal dated here must be posted into.
///
/// Ungated: this is not a person reading the calendar, it is the posting path
/// asking whether it may write. The permission that matters is the one on the
/// posting.
///
/// Rule 4 of ADR 0006 section 5, and the only place it is enforced: a closed
/// period refuses, and it refuses rather than warning.
pub async fn for_posting(pool: &PgPool, date: NaiveDate) -> ServiceResult<Period> {
    let Some(period) = store::covering(pool, date).await? else {
        return Err(rejected(&PeriodError::NoPeriod(date)));
    };

    if period.is_closed {
        return Err(rejected(&PeriodError::Closed(period.label)));
    }

    Ok(period)
}

/// The financial year the workspace is currently in, which is what a screen
/// offers to open when the calendar runs out.
pub async fn current_year(pool: &PgPool) -> ServiceResult<i32> {
    let profile = crate::workspace::profile::current(pool).await?;
    let month = u32::from(profile.fiscal_year_start_month).clamp(1, 12);
    let today = Utc::now().date_naive();

    // A year that opens in April means January is still last year's.
    Ok(if today.month() >= month {
        today.year()
    } else {
        today.year() - 1
    })
}

fn rejected(err: &PeriodError) -> ServiceError {
    ServiceError::rejected("entry_date", err.message())
}
