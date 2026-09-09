//! Places: where people work.
//!
//! The same shape as [`super::job_position`] throughout - a generated code, a
//! delete refused once anybody has ever been assigned there, deactivation
//! offered instead. Written out rather than made generic because the two
//! diverge the moment either grows a field the other has not got, and a
//! generic over two rows is a generic that has not earned itself.

use app_hr::work_location::{
    WorkLocation, WorkLocationError, WorkLocationInput, WorkLocationSummary,
};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::work_location as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<WorkLocationSummary>> {
    caller.require(permissions::WORK_LOCATIONS)?;
    Ok(store::list(pool).await?)
}

/// The ones a form may offer.
///
/// Gated on the employee permission rather than this app's own, for the reason
/// the role picker is: it is a field on the employee form.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<WorkLocation>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<WorkLocation> {
    caller.require(permissions::WORK_LOCATIONS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("work_location", msg!("work_locations.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<WorkLocationInput> {
    Ok(WorkLocationInput::from_location(
        &detail(pool, caller, id).await?,
    ))
}

pub fn blank(caller: &Caller) -> ServiceResult<WorkLocationInput> {
    caller.require(permissions::WORK_LOCATIONS_MANAGE)?;
    Ok(WorkLocationInput::blank())
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: WorkLocationInput,
) -> ServiceResult<Submission<WorkLocationInput>> {
    caller.require(permissions::WORK_LOCATIONS_MANAGE)?;
    acting_user(caller)?;

    let mut checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // Outside the transaction: every role queues through the sequence's one
    // row, so anything that can happen before the lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if checked.id.is_none() && checked.code.is_empty() {
        let key = SequenceKey::new(app_hr::APP_ID, app_hr::WORK_LOCATION);
        match generator
            .next(&mut tx, key, chrono::Utc::now().date_naive())
            .await
        {
            Ok(allocated) => checked.code = allocated.number,
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "code",
                    msg!("work_locations.error.no_series"),
                ));
            }
            Err(err) => return Err(err),
        }
    }

    let written = match checked.id {
        None => store::insert(&mut *tx, &checked, caller.user_id()).await.map(Some),
        Some(id) => store::update(&mut *tx, id, &checked, caller.user_id())
            .await
            .map(|done| done.then_some(id)),
    };

    let id = match written {
        Ok(Some(id)) => id,
        Ok(None) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected("id", msg!("work_locations.gone")));
        }
        Err(DbError::CodeExists { .. }) => {
            // Rolling back returns the number.
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "code",
                WorkLocationError::CodeTaken.message(),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    tx.commit().await.map_err(DbError::Query)?;

    let stored = WorkLocationInput {
        id: Some(id),
        code: checked.code.clone(),
        ..draft
    };

    let target = Target::new(kinds::WORK_LOCATION, id)
        .named(&stored.name)
        .fact("code", &stored.code);

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Remove one nobody has ever worked at.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::WORK_LOCATIONS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if store::assignment_count(pool, id).await? > 0 {
        return Ok(Submission::rejected(
            "id",
            WorkLocationError::StillUsed.message(),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut *tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::WORK_LOCATION, id).named(&before.name),
            &before,
        )
        .await;
    }

    Ok(Submission::Saved(()))
}
