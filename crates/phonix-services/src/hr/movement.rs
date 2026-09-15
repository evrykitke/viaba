//! Movements: the document behind a promotion, a transfer or an exit.
//!
//! # Confirming writes through the existing paths
//!
//! [`confirm`] calls [`super::employee::move_to`] and
//! [`super::employee::record_leaver`] rather than writing assignment rows of
//! its own. Those two already know that an assignment closes the day before the
//! next one opens, that a reporting line must not close a loop, and that an
//! engagement's end closes the open assignment with it. A second writer would
//! be those rules written twice, and the second copy is the one that drifts.
//!
//! # Gated at confirm, not at create
//!
//! A draft is a draft: it can be written, reviewed and edited freely. Confirming
//! is the act with consequences, so that is where the checks and the permission
//! live — the same rule Books applies to posting.

use app_hr::employee::AssignmentInput;
use app_hr::movement::{
    Movement, MovementError, MovementInput, MovementKind, MovementStatus, MovementSummary,
};
use phonix_core::form::Submission;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::movement as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<MovementSummary>> {
    caller.require(permissions::MOVEMENTS)?;
    Ok(store::list(pool).await?)
}

/// Everything that has happened to one person.
pub async fn for_employee(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
) -> ServiceResult<Vec<MovementSummary>> {
    caller.require(permissions::MOVEMENTS)?;
    Ok(store::for_employee(pool, employee_id).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Movement> {
    caller.require(permissions::MOVEMENTS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("movement", MovementError::Gone.message()))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<MovementInput> {
    let movement = detail(pool, caller, id).await?;

    if !movement.status.is_editable() {
        return Err(ServiceError::rejected(
            "id",
            MovementError::NotEditable.message(),
        ));
    }

    Ok(MovementInput::from_movement(&movement))
}

/// A blank movement, pre-filled from what the person is doing now.
///
/// The same courtesy the move form does: a promotion only has to change the
/// thing that moved, and a form that opened empty would invite somebody to
/// blank four fields by forgetting them.
pub async fn blank(
    pool: &PgPool,
    caller: &Caller,
    kind: MovementKind,
    employee_id: Uuid,
) -> ServiceResult<MovementInput> {
    caller.require(permissions::MOVEMENTS_RAISE)?;

    let current = current_assignment(pool, employee_id).await?;

    Ok(match current {
        Some(current) if kind.opens_an_assignment() => {
            MovementInput::moving(kind, employee_id, &current)
        }
        _ => MovementInput {
            employee_id: Some(employee_id),
            ..MovementInput::blank(kind)
        },
    })
}

/// What somebody is on now, as the shape a movement would replace.
async fn current_assignment(
    pool: &PgPool,
    employee_id: Uuid,
) -> ServiceResult<Option<AssignmentInput>> {
    let employee = phonix_db::hr::employee::find(pool, employee_id).await?;

    Ok(employee
        .and_then(|employee| employee.current_assignment().cloned())
        .map(|current| AssignmentInput::next(Some(&current), current.effective_from)))
}

/// Write a draft, or rewrite one.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: MovementInput,
) -> ServiceResult<Submission<MovementInput>> {
    caller.require(permissions::MOVEMENTS_RAISE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::save_draft(pool, &checked, caller.user_id()).await? {
        Some(id) => id,
        // The update names `status = 'draft'`, so nothing came back either
        // because the row is gone or because somebody confirmed it first.
        None => {
            return Ok(Submission::rejected(
                "id",
                MovementError::NotEditable.message(),
            ));
        }
    };

    let stored = MovementInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::MOVEMENT, id)
        .fact("kind", stored.kind.as_str())
        .fact("effective_on", checked.effective_on.to_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Make it true.
///
/// Takes the number, writes the rows through the existing paths, and stamps the
/// document with what it wrote. The number comes last of the checks and first of
/// the writes, for the reason an invoice's does: a refusal after a number is
/// allocated is a gap in the sequence nobody can explain.
pub async fn confirm(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<Movement>> {
    caller.require(permissions::MOVEMENTS_CONFIRM)?;
    acting_user(caller)?;

    let movement = detail(pool, caller, id).await?;

    if movement.status != MovementStatus::Draft {
        return Ok(Submission::rejected(
            "id",
            MovementError::NotConfirmable.message(),
        ));
    }

    // A promotion that moves nobody is a document saying nothing happened, and
    // somebody will later read it as evidence that something did. Checked here
    // rather than at save, because what they are on now can change under a
    // draft that has been sitting for a fortnight.
    if movement.kind.opens_an_assignment() {
        let current = current_assignment(pool, movement.employee_id).await?;
        let checked = match MovementInput::from_movement(&movement).check() {
            Ok(checked) => checked,
            Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
        };

        if !checked.changes_anything(current.as_ref()) {
            return Ok(Submission::rejected(
                "department_id",
                MovementError::MovesNobody.message(),
            ));
        }
    }

    // Through the existing paths: they own the rules about how an assignment
    // closes and what an engagement's end takes with it.
    let outcome = match (movement.as_assignment(), movement.as_leaving()) {
        (Some(assignment), _) => {
            super::employee::move_to(pool, caller, movement.employee_id, assignment).await?
        }
        (_, Some(leaving)) => {
            super::employee::record_leaver(pool, caller, movement.employee_id, leaving).await?
        }
        // Unreachable while `MovementKind` has three variants, each of which
        // produces one or the other. Refused rather than panicked, because this
        // crate is the one that must not take the request down.
        (None, None) => {
            return Ok(Submission::rejected(
                "kind",
                MovementError::NotConfirmable.message(),
            ));
        }
    };

    // The move or the leaving refused. Its message is the one to show: it knows
    // why, and this function would only be guessing.
    if let Submission::Rejected(errors) = outcome {
        return Ok(Submission::Rejected(errors));
    }

    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_hr::APP_ID, app_hr::MOVEMENT);
    let allocated = match generator.next(&mut tx, key, movement.effective_on).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "id",
                phonix_core::msg!("movements.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    // Which assignment the move opened, so the document can point at its effect.
    let assignment_id = current_assignment_id(pool, movement.employee_id).await?;

    let stamped = store::confirm(
        &mut *tx,
        id,
        &allocated.number,
        assignment_id,
        caller.user_id(),
    )
    .await?;

    if !stamped {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "id",
            MovementError::NotConfirmable.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let after = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::MOVEMENT, id)
            .named(&after.employee_name)
            .fact("number", after.number.as_deref().unwrap_or_default()),
        &movement,
        &after,
    )
    .await;

    Ok(Submission::Saved(after))
}

/// The id of the assignment somebody is on now.
async fn current_assignment_id(pool: &PgPool, employee_id: Uuid) -> ServiceResult<Option<Uuid>> {
    let employee = phonix_db::hr::employee::find(pool, employee_id).await?;

    Ok(employee.and_then(|employee| employee.current_assignment().map(|current| current.id)))
}

/// Withdraw a draft that should not have been raised.
pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::MOVEMENTS_RAISE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !store::cancel(pool, id, caller.user_id()).await? {
        return Ok(Submission::rejected(
            "id",
            MovementError::NotEditable.message(),
        ));
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::MOVEMENT, id).named(&before.employee_name),
        &before,
        &detail(pool, caller, id).await?,
    )
    .await;

    Ok(Submission::Saved(()))
}
