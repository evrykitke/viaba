//! Holiday calendars: the days nobody is expected to work.
//!
//! The same shape as [`super::work_location`] - a delete refused once anybody
//! has ever been assigned to one, deactivation offered instead - with one
//! difference: a calendar carries its days, so [`save`] writes two tables and
//! the code is typed rather than generated. A calendar is named after the
//! region and the year it covers, and `UK-2026` is what somebody wants to see
//! in a picker, not `HL-0004`.

use app_hr::holiday::{
    HolidayError, HolidayList, HolidayListInput, HolidayListSummary, WorkingDay,
};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::holiday as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<HolidayListSummary>> {
    caller.require(permissions::HOLIDAY_LISTS)?;
    Ok(store::list(pool).await?)
}

/// The ones the employee form may offer.
///
/// Gated on the employee permission rather than this app's own, for the reason
/// the role and place pickers are: it is a field on the employee form, and
/// somebody who may assign a person to a calendar does not thereby become
/// somebody who may edit one.
pub async fn selectable(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<(Uuid, String, String)>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<HolidayList> {
    caller.require(permissions::HOLIDAY_LISTS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("holiday_list", msg!("holidays.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<HolidayListInput> {
    Ok(HolidayListInput::from_list(
        &detail(pool, caller, id).await?,
    ))
}

pub fn blank(caller: &Caller) -> ServiceResult<HolidayListInput> {
    caller.require(permissions::HOLIDAY_LISTS_MANAGE)?;
    Ok(HolidayListInput::blank())
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: HolidayListInput,
) -> ServiceResult<Submission<HolidayListInput>> {
    caller.require(permissions::HOLIDAY_LISTS_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::save(pool, &checked, caller.user_id()).await {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(Submission::rejected("id", msg!("holidays.gone"))),
        Err(DbError::CodeExists { .. }) => {
            return Ok(Submission::rejected(
                "code",
                HolidayError::CodeTaken.message(),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    let stored = HolidayListInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::HOLIDAY_LIST, id)
        .named(&stored.name)
        .fact("code", &stored.code);

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Remove one nobody has ever been assigned to.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::HOLIDAY_LISTS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if store::assignment_count(pool, id).await? > 0 {
        return Ok(Submission::rejected(
            "id",
            HolidayError::StillUsed.message(),
        ));
    }

    let removed = store::delete(pool, id).await?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::HOLIDAY_LIST, id).named(&before.name),
            &before,
        )
        .await;
    }

    Ok(Submission::Saved(()))
}

/// Whether one employee is expected in on one date.
///
/// Gated on the employee permission: this is a fact about a person, and the
/// screens that will ask it - attendance, and leave if it is ever built - are
/// looking at somebody rather than at a calendar.
pub async fn working_day(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
    date: NaiveDate,
) -> ServiceResult<WorkingDay> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::working_day(pool, employee_id, date).await?)
}
