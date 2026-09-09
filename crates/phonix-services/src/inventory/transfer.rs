//! Stock transfers: the despatch, the middle, and the arrival.
//!
//! ADR 0006 section 7. Two movements against one document, with a `transit`
//! location between them so that stock which has left one building and not
//! reached the next is somewhere rather than nowhere.
//!
//! # Neither movement is built here
//!
//! Both are `stock::apply`, which values the move, spends or opens the layers
//! and files the journal. `posting_roles` already turns the two ends into the
//! two account roles - internal to transit credits `Inventory` and debits
//! `InventoryInTransit`, and the arrival is the same sentence backwards - so
//! this module names no account at all.
//!
//! # Each line is its own transaction, and each writes itself back at once
//!
//! The receipt's rule, for the receipt's reason: `apply` posts through a port,
//! and holding one transaction open across every line's port call would make
//! the ledger's implementation a participant in Inventory's locking. The price
//! is that a line failing halfway leaves the ones before it moved, and what
//! makes that recoverable is that the movement is recorded against its line
//! immediately - so a retried despatch skips what already went.
//!
//! # An arrival has no such guard, and does not need one
//!
//! A part-load is ordinary, so arriving twice is a legitimate act rather than a
//! retry to be suppressed. What keeps it honest is that the arrival is checked
//! against `despatched - received` at the moment it is keyed: whatever already
//! arrived has already lowered what a second attempt is allowed to move.

use app_inventory::transfer::{
    ArrivalInput, CheckedTransfer, Transfer, TransferError, TransferInput, TransferState,
    TransferSummary,
};
use app_inventory::{LocationKind, movement};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::transfer::{self as store, Ends};
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::Ledger;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

fn reject<T>(err: TransferError) -> Submission<T> {
    Submission::rejected(err.field(), err.message())
}

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<TransferSummary>> {
    caller.require(permissions::TRANSFERS)?;
    Ok(store::list(pool).await?)
}

/// Journeys with stock still on them. What the in-transit account is made of.
pub async fn in_transit(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<TransferSummary>> {
    caller.require(permissions::TRANSFERS)?;
    Ok(store::in_transit(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Transfer> {
    caller.require(permissions::TRANSFERS)?;

    let mut document = store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("transfer", msg!("transfers.gone")))?;

    document.lines = store::lines_of(pool, id).await?;

    Ok(document)
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<TransferInput> {
    Ok(TransferInput::from_document(
        &detail(pool, caller, id).await?,
    ))
}

pub async fn blank(_pool: &PgPool, caller: &Caller) -> ServiceResult<TransferInput> {
    caller.require(permissions::TRANSFERS_CREATE)?;

    Ok(TransferInput::blank(today()))
}

/// The arrival form, pre-filled with everything still on the road.
pub async fn arrival(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<ArrivalInput>> {
    caller.require(permissions::TRANSFERS_RECEIVE)?;

    let document = detail(pool, caller, id).await?;

    if !document.state.has_left() {
        return Ok(reject(TransferError::NotDespatched));
    }
    if !document.is_carrying() {
        return Ok(reject(TransferError::NothingArriving));
    }

    Ok(Submission::Saved(ArrivalInput::everything(
        &document,
        today(),
    )))
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: TransferInput,
) -> ServiceResult<Submission<TransferInput>> {
    caller.require(permissions::TRANSFERS_CREATE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(reject(err)),
    };

    let ends = match resolve_ends(pool, &checked).await? {
        Ok(ends) => ends,
        Err(err) => return Ok(reject(err)),
    };

    if let Some(id) = checked.id {
        let before = detail(pool, caller, id).await?;
        if !before.state.is_editable() {
            return Ok(reject(TransferError::NotEditable));
        }
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => {
            store::insert(
                &mut tx,
                &ends,
                checked.planned_on,
                checked.reference.as_deref(),
                checked.note.as_deref(),
                caller.user_id(),
            )
            .await?
        }
        Some(id) => {
            if !store::update(
                &mut tx,
                id,
                &ends,
                checked.planned_on,
                checked.reference.as_deref(),
                checked.note.as_deref(),
                caller.user_id(),
            )
            .await?
            {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(reject(TransferError::NotEditable));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &checked.lines).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = TransferInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::STOCK_TRANSFER, id)
        .named(&format!("{} → {}", ends.from_path, ends.to_path))
        .fact("lines", &checked.lines.len().to_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Send it: every line moves from the origin to the transit location.
///
/// Resumable. A line already carrying a despatch movement is skipped, so a
/// second attempt after a failure halfway moves each pallet exactly once.
pub async fn despatch(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    id: Uuid,
) -> ServiceResult<Submission<Transfer>> {
    caller.require(permissions::TRANSFERS_DESPATCH)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if matches!(before.state, TransferState::Done | TransferState::Cancelled) {
        return Ok(reject(TransferError::AlreadyDespatched));
    }
    if !before.has_lines() {
        return Ok(reject(TransferError::NothingToMove));
    }

    let despatched_on = before.despatched_on.unwrap_or_else(today);

    // The document is claimed before any stock moves, so two people pressing
    // Despatch at once cannot both start moving the same pallets. Losing the
    // claim is not an error: the winner may itself have failed halfway, and
    // the loop below is what finishes the job either way.
    if before.state.is_editable() {
        let generator = crate::numbering::NumberGenerator::open(pool).await?;
        let mut tx = pool.begin().await.map_err(DbError::Query)?;

        let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::INTERNAL_TRANSFER);
        let allocated = match generator.next(&mut tx, key, despatched_on).await {
            Ok(allocated) => allocated,
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "number",
                    msg!("transfers.error.no_series"),
                ));
            }
            Err(err) => return Err(err),
        };

        if store::despatch(
            &mut tx,
            id,
            &allocated.number,
            despatched_on,
            caller.user_id(),
        )
        .await?
        {
            tx.commit().await.map_err(DbError::Query)?;
        } else {
            tx.rollback().await.map_err(DbError::Query)?;
        }
    }

    let already = store::moves_of(pool, id).await?;

    for line in &before.lines {
        let moved = already
            .iter()
            .find(|(line_id, _, _)| *line_id == line.id)
            .and_then(|(_, despatch_move_id, _)| *despatch_move_id);

        if moved.is_some() {
            continue;
        }

        let request = movement::MoveRequest {
            lot_id: line.lot_id,
            reference: before.reference.clone(),
            source: Some(movement::MoveSource::new(
                app_inventory::INTERNAL_TRANSFER,
                id,
            )),
            ..movement::MoveRequest::new(
                line.variant_id,
                before.from_location_id,
                before.transit_location_id,
                line.quantity,
                despatched_on,
            )
        };

        let stored = match crate::inventory::stock::apply(pool, caller, ledger, request).await? {
            Submission::Saved(stored) => stored,
            Submission::Rejected(errors) => return Ok(Submission::Rejected(errors)),
        };

        // Written back before the next line is attempted. The move is already
        // committed by this point, and anything that defers this leaves a
        // window where stock has left the shelf and nothing says so.
        let mut tx = pool.begin().await.map_err(DbError::Query)?;
        store::record_despatch(&mut tx, line.id, stored.id, stored.quantity).await?;
        tx.commit().await.map_err(DbError::Query)?;
    }

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::STOCK_TRANSFER, id)
            .named(&stored.label())
            .fact("from", &stored.from_path)
            .fact("to", &stored.to_path)
            .fact("in_transit", &stored.in_transit().to_display_string()),
        &before,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Receive it: what turned up moves off the transit location onto the
/// destination's shelf.
///
/// A part-load is ordinary, so this may be called more than once. What has
/// already arrived has already lowered what a second call is allowed to move.
pub async fn receive(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    arrival: ArrivalInput,
) -> ServiceResult<Submission<Transfer>> {
    caller.require(permissions::TRANSFERS_RECEIVE)?;
    acting_user(caller)?;

    let id = arrival.transfer_id;
    let before = detail(pool, caller, id).await?;

    if !before.state.has_left() {
        return Ok(reject(TransferError::NotDespatched));
    }

    let arriving = match arrival.check(&before) {
        Ok(arriving) => arriving,
        Err(err) => return Ok(reject(err)),
    };

    for line in &arriving {
        let request = movement::MoveRequest {
            lot_id: line.lot_id,
            reference: before.reference.clone(),
            source: Some(movement::MoveSource::new(
                app_inventory::INTERNAL_TRANSFER,
                id,
            )),
            ..movement::MoveRequest::new(
                line.variant_id,
                before.transit_location_id,
                before.to_location_id,
                line.quantity,
                arrival.arrived_on,
            )
        };

        let stored = match crate::inventory::stock::apply(pool, caller, ledger, request).await? {
            Submission::Saved(stored) => stored,
            Submission::Rejected(errors) => return Ok(Submission::Rejected(errors)),
        };

        let mut tx = pool.begin().await.map_err(DbError::Query)?;
        store::record_arrival(&mut tx, line.line_id, stored.id, stored.quantity).await?;
        tx.commit().await.map_err(DbError::Query)?;
    }

    // Read back rather than worked out from what was asked for: the lines have
    // just been advanced, and whether anything is left on the road is a fact
    // about the rows rather than about this request.
    let after_lines = store::lines_of(pool, id).await?;
    let complete = after_lines.iter().all(|line| line.has_arrived());

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::arrive(&mut tx, id, arrival.arrived_on, complete, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::STOCK_TRANSFER, id)
            .named(&stored.label())
            .fact("arrived", &arrival.arrived_on.to_string())
            .fact("still_out", &stored.in_transit().to_display_string()),
        &before,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::TRANSFERS_CREATE)?;

    let document = detail(pool, caller, id).await?;
    if !document.state.is_editable() {
        return Ok(reject(TransferError::NotEditable));
    }

    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    if !store::cancel(&mut conn, id, caller.user_id()).await? {
        return Ok(reject(TransferError::NotEditable));
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::STOCK_TRANSFER, id).named(&document.label()),
        &document.state,
        &TransferState::Cancelled,
    )
    .await;

    Ok(Submission::Saved(()))
}

pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::TRANSFERS_CREATE)?;

    let document = detail(pool, caller, id).await?;
    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    let gone = store::delete(&mut conn, id).await?;

    if gone {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::STOCK_TRANSFER, id).named(&document.label()),
            &document,
        )
        .await;
    }

    Ok(gone)
}

/// The two ends, their paths, and the place in between.
///
/// A grouping is refused here rather than at despatch: it holds nothing of its
/// own, and letting somebody save a transfer out of one only to be told at the
/// gate is a worse place to find out.
async fn resolve_ends(
    pool: &PgPool,
    checked: &CheckedTransfer,
) -> ServiceResult<Result<Ends, TransferError>> {
    let from = phonix_db::inventory::location::find(pool, checked.from_location_id).await?;
    let to = phonix_db::inventory::location::find(pool, checked.to_location_id).await?;

    let (Some(from), Some(to)) = (from, to) else {
        return Ok(Err(TransferError::OriginRequired));
    };

    if !from.kind.can_hold_stock() || !to.kind.can_hold_stock() {
        return Ok(Err(TransferError::EndsCannotHoldStock));
    }

    let transit = crate::inventory::location::counterpart(pool, LocationKind::Transit).await?;

    Ok(Ok(Ends {
        from_location_id: from.id,
        to_location_id: to.id,
        transit_location_id: transit.id,
        from_path: from.code,
        to_path: to.code,
    }))
}
