//! Shift types: what somebody was expected to work.
//!
//! The same shape as [`super::holiday`] — a delete refused once anybody has
//! ever been assigned to one, deactivation offered instead, and a typed code
//! rather than a generated one because `NIGHTS` is what belongs in a picker.

use app_hr::shift::{ShiftError, ShiftType, ShiftTypeInput, ShiftTypeSummary};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::shift as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ShiftTypeSummary>> {
    caller.require(permissions::SHIFT_TYPES)?;
    Ok(store::list(pool).await?)
}

/// The ones the employee form may offer.
///
/// Gated on the employee permission, like the other assignment pickers:
/// putting somebody on a shift is not the same act as defining one.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ShiftType>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::selectable(pool).await?)
}

/// The shift somebody was on for one date.
///
/// Resolved through the assignment in force then, not the current one - the
/// same rule the calendar follows, and for the same reason: punctuality last
/// March is read against the shift they were on in March.
pub async fn on_date(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
    date: NaiveDate,
) -> ServiceResult<Option<ShiftType>> {
    caller.require(permissions::ATTENDANCE)?;
    Ok(store::on_date(pool, employee_id, date).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<ShiftType> {
    caller.require(permissions::SHIFT_TYPES)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("shift_type", ShiftError::Gone.message()))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<ShiftTypeInput> {
    Ok(ShiftTypeInput::from_shift(&detail(pool, caller, id).await?))
}

pub fn blank(caller: &Caller) -> ServiceResult<ShiftTypeInput> {
    caller.require(permissions::SHIFT_TYPES_MANAGE)?;
    Ok(ShiftTypeInput::blank())
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: ShiftTypeInput,
) -> ServiceResult<Submission<ShiftTypeInput>> {
    caller.require(permissions::SHIFT_TYPES_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::save(pool, &checked, caller.user_id()).await {
        Ok(Some(id)) => id,
        Ok(None) => return Ok(Submission::rejected("id", ShiftError::Gone.message())),
        Err(DbError::CodeExists { .. }) => {
            return Ok(Submission::rejected(
                "code",
                ShiftError::CodeTaken.message(),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    let stored = ShiftTypeInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::SHIFT_TYPE, id)
        .named(&stored.name)
        .fact("code", &stored.code);

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Remove one nobody has ever been on.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::SHIFT_TYPES_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if store::assignment_count(pool, id).await? > 0 {
        return Ok(Submission::rejected("id", ShiftError::StillUsed.message()));
    }

    let removed = store::delete(pool, id).await?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::SHIFT_TYPE, id).named(&before.name),
            &before,
        )
        .await;
    }

    Ok(Submission::Saved(()))
}
