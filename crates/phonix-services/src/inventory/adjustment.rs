//! Adjustment types, and the adjustment that names one.
//!
//! # Two things live here because they are one decision
//!
//! The type is reference data - a code, a name, an account - and the adjustment
//! is a movement. Splitting them across two modules would put the rule that
//! joins them ("damage cannot bring stock in", "a write-off needs a manager")
//! in neither of them.
//!
//! # The adjustment itself is still `stock::apply`
//!
//! Nothing here values a move, spends a layer or builds a journal. It resolves
//! the reason, decides the two ends from the direction, and hands over a
//! [`MoveRequest`] carrying the account the type named. See ADR 0006 section 7.
//!
//! # Approval is a permission, not a queue
//!
//! A type marked `needs_approval` asks for `STOCK_ADJUST_APPROVE` at the moment
//! the adjustment is made. It does not create a second state: an adjustment is
//! one movement, and a movement that has half happened is precisely what the
//! stock ledger is built to make impossible.

use app_inventory::adjustment::{
    AdjustmentError, AdjustmentInput, AdjustmentType, AdjustmentTypeInput, AdjustmentTypeSummary,
};
use app_inventory::location::LocationKind;
use app_inventory::movement::{MoveRequest, StockMove};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::adjustment as store;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::Ledger;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

fn reject<T>(err: AdjustmentError) -> Submission<T> {
    Submission::rejected(err.field(), err.message())
}

// --- The reasons -----------------------------------------------------------

/// Every reason, retired ones included, with how much has been booked under
/// each.
pub async fn list(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<AdjustmentTypeSummary>> {
    caller.require(permissions::ADJUSTMENT_TYPES)?;
    Ok(store::list(pool).await?)
}

/// The reasons the adjust screen offers.
///
/// Gated on `STOCK_ADJUST` rather than `ADJUSTMENT_TYPES`: somebody who may
/// book a count difference has to be able to say what it was, without also
/// being allowed to decide which account damage posts to.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<AdjustmentType>> {
    caller.require(permissions::STOCK_ADJUST)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<AdjustmentType> {
    caller.require(permissions::ADJUSTMENT_TYPES)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("type_id", msg!("adjustment_types.gone")))
}

pub async fn edit(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<AdjustmentTypeInput> {
    Ok(AdjustmentTypeInput::from_type(
        &detail(pool, caller, id).await?,
    ))
}

pub fn blank() -> AdjustmentTypeInput {
    AdjustmentTypeInput::blank()
}

/// Add a reason, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: AdjustmentTypeInput,
) -> ServiceResult<Submission<AdjustmentTypeInput>> {
    caller.require(permissions::ADJUSTMENT_TYPES_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(reject(err)),
    };

    match draft.id {
        None => {
            let id = match store::insert(pool, &checked, caller.user_id()).await {
                Ok(id) => id,
                Err(DbError::CodeExists { .. }) => return Ok(reject(AdjustmentError::CodeTaken)),
                Err(err) => return Err(err.into()),
            };

            let stored = AdjustmentTypeInput {
                id: Some(id),
                ..draft
            };

            audit::created(
                pool,
                caller,
                Target::new(kinds::ADJUSTMENT_TYPE, id)
                    .named(&checked.name)
                    .fact("code", &checked.code)
                    .fact("direction", checked.direction.as_str()),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            match store::update(pool, id, &checked, caller.user_id()).await {
                Ok(true) => {}
                Ok(false) => return Ok(reject(AdjustmentError::TypeGone)),
                Err(DbError::CodeExists { .. }) => return Ok(reject(AdjustmentError::CodeTaken)),
                Err(err) => return Err(err.into()),
            }

            let stored = AdjustmentTypeInput {
                id: Some(id),
                // Where the row came from is not the form's to change, and a
                // draft that arrived saying otherwise does not get to say it.
                is_system: before.is_system,
                ..draft
            };

            audit::updated(
                pool,
                caller,
                Target::new(kinds::ADJUSTMENT_TYPE, id).named(&checked.name),
                &AdjustmentTypeInput::from_type(&before),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
    }
}

/// Throw a reason away.
///
/// Two refusals, and each is a different sentence on the screen: a seeded type
/// is not the workspace's to delete, and one that movements point at would take
/// the answer to "what did that cost us" with it. Retiring is the way out of
/// both, and it leaves the history readable.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::ADJUSTMENT_TYPES_MANAGE)?;
    acting_user(caller)?;

    let kind = detail(pool, caller, id).await?;

    if !kind.is_deletable() {
        return Ok(reject(AdjustmentError::SystemType));
    }
    if store::move_count(pool, id).await? > 0 {
        return Ok(reject(AdjustmentError::TypeInUse));
    }

    if store::delete(pool, id).await? {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::ADJUSTMENT_TYPE, id)
                .named(&kind.name)
                .fact("code", &kind.code),
            &AdjustmentTypeInput::from_type(&kind),
        )
        .await;
    }

    Ok(Submission::Saved(()))
}

// --- The adjustment --------------------------------------------------------

/// Write stock off, scrap it, or book in a count difference.
///
/// The one movement a person makes by hand rather than through a document, and
/// the reason it has its own permission: everything else that moves stock is
/// the consequence of a purchase or a sale, and this is somebody saying the
/// shelf disagrees with the system.
pub async fn record(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    draft: AdjustmentInput,
) -> ServiceResult<Submission<StockMove>> {
    caller.require(permissions::STOCK_ADJUST)?;

    let kind = match draft.type_id {
        None => None,
        Some(id) => match store::find(pool, id).await? {
            Some(kind) => Some(kind),
            // Named apart from "no reason chosen": one is a form somebody has
            // not finished and the other is a reason somebody deleted while
            // this screen was open.
            None => return Ok(reject(AdjustmentError::TypeGone)),
        },
    };

    let checked = match draft.check(kind.as_ref()) {
        Ok(checked) => checked,
        Err(err) => return Ok(reject(err)),
    };

    if checked.needs_approval && !caller.can(permissions::STOCK_ADJUST_APPROVE) {
        return Ok(reject(AdjustmentError::NeedsApproval));
    }

    let loss = crate::inventory::location::counterpart(pool, LocationKind::InventoryLoss).await?;

    // The direction decides the two ends and nothing else does. Stock is never
    // created or destroyed; a write-off is a move to inventory loss and a
    // count that came out high is a move back from it.
    let (from, to) = if checked.found {
        (loss.id, checked.location_id)
    } else {
        (checked.location_id, loss.id)
    };

    let request = MoveRequest {
        lot_id: checked.lot_id,
        reference: checked.reason,
        adjustment_type_id: Some(checked.type_id),
        adjustment_account_id: checked.account_id,
        ..MoveRequest::new(
            checked.variant_id,
            from,
            to,
            checked.quantity,
            checked.moved_on,
        )
    };

    crate::inventory::stock::apply(pool, caller, ledger, request).await
}
