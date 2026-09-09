//! Roles: what the organization is made of.
//!
//! A create with a blank code allocates one inside the insert's transaction, so
//! a rolled-back save returns the number rather than leaving a hole - the same
//! rule [`super::department`] follows.
//!
//! # Deleting is refused once anybody has ever held it
//!
//! Not "once anybody holds it now". A role three people held last year is cited
//! by their assignment history, and `ON DELETE RESTRICT` would refuse the
//! delete at the last moment with a database error rather than a sentence.
//! Deactivation is offered instead, which is the same shape as a department's
//! and a party's.

use app_hr::job_position::{JobPosition, JobPositionError, JobPositionInput, JobPositionSummary};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::job_position as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<JobPositionSummary>> {
    caller.require(permissions::JOB_POSITIONS)?;
    Ok(store::list(pool).await?)
}

/// The ones a form may offer.
///
/// Gated on the employee permission rather than this app's own: it is a picker
/// on the employee form, and somebody who may not administer roles may still
/// have to put somebody into one.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<JobPosition>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<JobPosition> {
    caller.require(permissions::JOB_POSITIONS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("job_position", msg!("job_positions.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<JobPositionInput> {
    Ok(JobPositionInput::from_position(
        &detail(pool, caller, id).await?,
    ))
}

pub fn blank(caller: &Caller) -> ServiceResult<JobPositionInput> {
    caller.require(permissions::JOB_POSITIONS_MANAGE)?;
    Ok(JobPositionInput::blank())
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: JobPositionInput,
) -> ServiceResult<Submission<JobPositionInput>> {
    caller.require(permissions::JOB_POSITIONS_MANAGE)?;
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
        let key = SequenceKey::new(app_hr::APP_ID, app_hr::JOB_POSITION);
        match generator
            .next(&mut tx, key, chrono::Utc::now().date_naive())
            .await
        {
            Ok(allocated) => checked.code = allocated.number,
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "code",
                    msg!("job_positions.error.no_series"),
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
            return Ok(Submission::rejected("id", msg!("job_positions.gone")));
        }
        Err(DbError::CodeExists { .. }) => {
            // Rolling back returns the number.
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "code",
                JobPositionError::CodeTaken.message(),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    tx.commit().await.map_err(DbError::Query)?;

    let stored = JobPositionInput {
        id: Some(id),
        code: checked.code.clone(),
        ..draft
    };

    let target = Target::new(kinds::JOB_POSITION, id)
        .named(&stored.title)
        .fact("code", &stored.code);

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Remove one nobody has ever held.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::JOB_POSITIONS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if store::assignment_count(pool, id).await? > 0 {
        return Ok(Submission::rejected(
            "id",
            JobPositionError::StillHeld.message(),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut *tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::JOB_POSITION, id).named(&before.title),
            &before,
        )
        .await;
    }

    Ok(Submission::Saved(()))
}
