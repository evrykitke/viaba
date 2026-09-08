//! Goods receipts: keying what arrived, and posting it.
//!
//! # Posting is one call per line into the stock ledger
//!
//! Everything a receipt has to do - move the stock, open a valuation layer,
//! debit inventory and credit goods-received-not-invoiced - is what
//! [`stock::apply`](crate::inventory::stock::apply) already does for a move
//! from a vendor location. This module's whole job is to work out the three
//! things that call needs and to do it once per line: where the goods land,
//! what a unit cost in the workspace's own currency, and which lot they are.
//!
//! That is why the ledger was built first. A receipt is a form and a header.
//!
//! # Where the goods land
//!
//! The warehouse decides. A one-step warehouse receives onto the shelf; a two-
//! or three-step one receives into `Input`, and the put-away that follows is an
//! internal transfer. Odoo's model, and the reason `Input` exists at all.
//!
//! # The price is the order's, converted on the receipt's own date
//!
//! A purchase order is in the supplier's currency. What a valuation layer needs
//! is the workspace's, at the rate for the day the goods arrived - not today's,
//! because re-converting later from a newer rate would restate a filed period.
//! A receipt with no order behind it is priced at the item's own cost.
//!
//! # Posting twice moves each line once
//!
//! A line's movement is one transaction and the whole receipt is not, because
//! `stock::apply` posts through the `Ledger` port and holding a transaction
//! open across every line's port call would make the ledger's implementation a
//! participant in Inventory's locking.
//!
//! So a receipt that moved four lines and failed on the fifth has moved four
//! lines. What makes that recoverable is that each move is written back
//! against its line as soon as it commits, and a line that already carries a
//! `move_id` is skipped: the fix for a half-posted receipt is to post it
//! again, which picks up where it stopped. The alternative - collecting the
//! moves and writing them all at the end - is what leaves stock moved and
//! nothing saying so.

use app_inventory::purchase::{OrderState, SupplierSnapshot};
use app_inventory::receipt::{
    Backorder, CheckedReceipt, Receipt, ReceiptError, ReceiptInput, ReceiptState, ReceiptSummary,
};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::{
    purchase as order_store, receipt as store, warehouse as warehouse_store,
};
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::Ledger;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ReceiptSummary>> {
    caller.require(permissions::RECEIPTS)?;
    let currency = base_currency(pool).await?;

    Ok(store::list(pool, currency).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Receipt> {
    caller.require(permissions::RECEIPTS)?;
    let currency = base_currency(pool).await?;

    store::find(pool, id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("receipt", msg!("receipts.gone")))
}

/// A blank receipt for goods arriving with no order behind them.
pub async fn blank(pool: &PgPool, caller: &Caller) -> ServiceResult<ReceiptInput> {
    caller.require(permissions::RECEIPTS_CREATE)?;
    let _ = pool;

    Ok(ReceiptInput::blank(today()))
}

/// A receipt prefilled with everything an order still owes.
///
/// The screen somebody actually wants when the lorry is at the door: the
/// question is which of these forty lines arrived, not which item this is.
pub async fn against_order(
    pool: &PgPool,
    caller: &Caller,
    order_id: Uuid,
) -> ServiceResult<Submission<ReceiptInput>> {
    caller.require(permissions::RECEIPTS_CREATE)?;

    let order = crate::inventory::purchase::detail(pool, caller, order_id).await?;

    if !order.state.accepts_receipts() {
        return Ok(Submission::rejected(
            "order_id",
            ReceiptError::OrderNotReceivable.message(),
        ));
    }

    Ok(Submission::Saved(ReceiptInput::against(&order, today())))
}

/// What an order still owes, for a screen to show after a short delivery.
pub async fn backorder(
    pool: &PgPool,
    caller: &Caller,
    order_id: Uuid,
) -> ServiceResult<Option<Backorder>> {
    caller.require(permissions::RECEIPTS)?;

    let order = crate::inventory::purchase::detail(pool, caller, order_id).await?;
    Ok(Backorder::of(&order))
}

/// Write a draft receipt, or change one.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: ReceiptInput,
) -> ServiceResult<Submission<ReceiptInput>> {
    caller.require(permissions::RECEIPTS_CREATE)?;
    acting_user(caller)?;

    let currency = base_currency(pool).await?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let (supplier, to_location_id) = match prepare(pool, &checked).await? {
        Ok(ready) => ready,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let costed = match cost_lines(pool, &checked, currency).await? {
        Ok(costed) => costed,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => store::insert(&mut tx, &checked, &supplier, to_location_id, caller.user_id()).await?,
        Some(id) => {
            if !store::update(
                &mut tx,
                id,
                &checked,
                &supplier,
                to_location_id,
                caller.user_id(),
            )
            .await?
            {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected("state", ReceiptError::NotEditable.message()));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &costed).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = ReceiptInput {
        id: Some(id),
        ..draft
    };

    audit::created(
        pool,
        caller,
        Target::new(kinds::RECEIPT, id).named(&supplier.name),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Post a receipt: move every line's stock, and let the ledger follow.
///
/// The only thing in this module that changes anything outside its own tables,
/// and it does it by calling `stock::apply` once per line. Everything the
/// accounting needs - inventory debited, goods-received-not-invoiced credited,
/// a valuation layer opened, an average recomputed - falls out of that call
/// because the move is from a vendor location into ours.
pub async fn post(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    id: Uuid,
) -> ServiceResult<Submission<Receipt>> {
    caller.require(permissions::RECEIPTS_POST)?;
    acting_user(caller)?;

    let receipt = detail(pool, caller, id).await?;

    if !receipt.state.is_editable() {
        return Ok(Submission::rejected("state", ReceiptError::NotEditable.message()));
    }
    if !receipt.has_quantity() {
        return Ok(Submission::rejected(
            "lines",
            ReceiptError::NothingReceived.message(),
        ));
    }

    // The vendor's side of every move. Seeded, so reaching here without one
    // means somebody deleted it, and the message says which.
    let vendor =
        crate::inventory::location::counterpart(pool, app_inventory::LocationKind::Vendor).await?;

    let currency = base_currency(pool).await?;
    let generator = crate::numbering::NumberGenerator::open(pool).await?;

    // Each line is its own `stock::apply`, and each is its own transaction:
    // `apply` posts a journal through a port, and holding one transaction open
    // across every line's port call would make the ledger's implementation a
    // participant in Inventory's locking.
    //
    // The price of that is that a line failing halfway leaves the ones before
    // it posted. What makes it recoverable rather than a double count is the
    // line below: each move is written back against its line *immediately*,
    // and a line that already carries a `move_id` is skipped on the next
    // attempt. Posting a receipt twice therefore moves each line once.
    for line in &receipt.lines {
        if line.move_id.is_some() {
            continue;
        }

        let request = app_inventory::movement::MoveRequest {
            lot_id: None,
            unit_cost: Some(line.unit_cost),
            reference: receipt.delivery_note.clone(),
            source: Some(app_inventory::movement::MoveSource::new("goods_receipt", id)),
            ..app_inventory::movement::MoveRequest::new(
                line.variant_id,
                vendor.id,
                receipt.to_location_id,
                line.quantity,
                receipt.received_on,
            )
        };

        let request = match &line.lot_number {
            None => request,
            Some(number) => {
                let lot = resolve_lot(pool, line.variant_id, number, line.expires_on).await?;
                app_inventory::movement::MoveRequest {
                    lot_id: Some(lot),
                    ..request
                }
            }
        };

        let stored = match crate::inventory::stock::apply(pool, caller, ledger, request).await? {
            Submission::Saved(stored) => stored,
            Submission::Rejected(errors) => return Ok(Submission::Rejected(errors)),
        };

        // Written back before the next line is attempted. The move is already
        // committed by this point, so anything that defers this - including
        // "collect them all and write them at the end" - leaves a window where
        // stock has moved and nothing on the receipt says so.
        let mut tx = pool.begin().await.map_err(DbError::Query)?;

        store::record_move(&mut tx, line.id, stored.id).await?;

        // The order learns what arrived. A delta rather than an absolute, so
        // two receipts against one line in the same minute both count.
        if let Some(order_line_id) = line.order_line_id {
            order_store::advance_received(&mut tx, order_line_id, stored.quantity).await?;
        }

        tx.commit().await.map_err(DbError::Query)?;
    }

    let value = Money::total(currency, receipt.lines.iter().map(|line| line.value))
        .unwrap_or_else(|_| Money::zero(currency));

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::RECEIPT);
    let allocated = match generator.next(&mut tx, key, receipt.received_on).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected("number", msg!("receipts.error.no_series")));
        }
        Err(err) => return Err(err),
    };

    if !store::post(&mut tx, id, &allocated.number, value, caller.user_id()).await? {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected("state", ReceiptError::NotEditable.message()));
    }

    tx.commit().await.map_err(DbError::Query)?;

    // An order everything has arrived against closes itself. A buyer should not
    // have to tick a box to say a finished order is finished.
    if let Some(order_id) = receipt.order_id {
        close_if_complete(pool, caller, order_id).await?;
    }

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::RECEIPT, id)
            .named(&stored.number)
            .fact("supplier", &stored.supplier.name)
            .fact("value", &stored.value.to_display_string()),
        &receipt,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::RECEIPTS_CREATE)?;
    acting_user(caller)?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let done = store::cancel(&mut tx, id, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if !done {
        return Ok(Submission::rejected("state", ReceiptError::NotEditable.message()));
    }

    Ok(Submission::Saved(()))
}

pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::RECEIPTS_CREATE)?;
    acting_user(caller)?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    Ok(removed)
}

// --- Shared ---------------------------------------------------------------

/// The supplier snapshot and where the goods land.
async fn prepare(
    pool: &PgPool,
    checked: &CheckedReceipt,
) -> ServiceResult<Result<(SupplierSnapshot, Uuid), ReceiptError>> {
    let Some(party) = phonix_db::master::party::find(pool, checked.supplier_id).await? else {
        return Ok(Err(ReceiptError::SupplierRequired));
    };

    let warehouses = warehouse_store::selectable(pool).await?;
    let Some(warehouse) = warehouses
        .iter()
        .find(|warehouse| warehouse.id == checked.warehouse_id)
    else {
        return Ok(Err(ReceiptError::WarehouseRequired));
    };

    let to_location_id = warehouse_store::receiving_location(pool, warehouse).await?;

    Ok(Ok((
        SupplierSnapshot {
            party_id: party.id,
            code: party.code,
            name: party.name,
        },
        to_location_id,
    )))
}

/// What each line cost, in the workspace's own currency.
///
/// The order's price where there is an order, converted at the receipt's own
/// date; what somebody typed where they typed one; the item's standing cost
/// otherwise.
async fn cost_lines<'a>(
    pool: &PgPool,
    checked: &'a CheckedReceipt,
    currency: Currency,
) -> ServiceResult<Result<Vec<store::CostedReceiptLine<'a>>, ReceiptError>> {
    let order = match checked.order_id {
        None => None,
        Some(id) => phonix_db::inventory::purchase::find(pool, id).await?,
    };

    let mut costed = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let Some(context) =
            phonix_db::inventory::movement::context(pool, line.variant_id, currency).await?
        else {
            return Ok(Err(ReceiptError::ItemRequired));
        };

        let unit_cost = match &line.unit_cost {
            // Typed on the receipt. Already in the workspace's own currency,
            // because that is what the form is denominated in.
            Some(typed) => match Money::parse(currency, typed) {
                Ok(cost) => cost,
                Err(err) => return Ok(Err(ReceiptError::Money(err))),
            },
            None => match ordered_cost(pool, order.as_ref(), line, currency, checked.received_on)
                .await?
            {
                Some(cost) => cost,
                // No order, or a line that is not against one: the item's own
                // standing cost, which is what the workspace last paid.
                None => context.cost,
            },
        };

        let value = match unit_cost.scale_by(
            line.quantity.scaled(),
            app_inventory::quantity::SCALE_FACTOR,
            Rounding::HalfUp,
        ) {
            Ok(value) => value,
            Err(err) => return Ok(Err(ReceiptError::Money(err))),
        };

        costed.push(store::CostedReceiptLine {
            source: line,
            description: if line.description.is_empty() {
                context.item_name.clone()
            } else {
                line.description.clone()
            },
            unit_cost,
            value,
        });
    }

    Ok(Ok(costed))
}

/// The order line's price per stock unit, in the workspace's own currency.
///
/// Converted at the rate for the *receipt's* date. A missing rate is refused
/// rather than guessed - the same rule the ledger applies, and for the same
/// reason: a value converted at a rate nobody recorded is a number with no
/// provenance.
async fn ordered_cost(
    pool: &PgPool,
    order: Option<&app_inventory::purchase::PurchaseOrder>,
    line: &app_inventory::receipt::CheckedReceiptLine,
    currency: Currency,
    on: NaiveDate,
) -> ServiceResult<Option<Money>> {
    let (Some(order), Some(order_line_id)) = (order, line.order_line_id) else {
        return Ok(None);
    };

    let Some(order_line) = order.lines.iter().find(|row| row.id == order_line_id) else {
        return Ok(None);
    };

    let Ok(in_order_currency) = order_line.stock_unit_price() else {
        return Ok(None);
    };

    let order_currency = match Currency::parse(&order.currency) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };

    if order_currency == currency {
        return Ok(Some(in_order_currency));
    }

    let Some(rate) = crate::currency::rate_on(pool, order_currency, currency, on, None).await?
    else {
        return Err(ServiceError::rejected(
            "unit_cost",
            msg!(
                "journals.error.no_rate",
                pair = format!("{}/{}", order_currency.code(), currency.code()),
                date = on
            ),
        ));
    };

    Ok(Some(
        in_order_currency
            .convert(&rate, Rounding::HalfUp)
            .map_err(|err| ServiceError::rejected("unit_cost", err.message()))?
            .base_amount,
    ))
}

/// Find or create the lot this batch number stands for.
async fn resolve_lot(
    pool: &PgPool,
    variant_id: Uuid,
    number: &str,
    expires_on: Option<NaiveDate>,
) -> ServiceResult<Uuid> {
    let currency = base_currency(pool).await?;

    let tracking = phonix_db::inventory::movement::context(pool, variant_id, currency)
        .await?
        .map_or(app_inventory::item::Tracking::Lot, |context| {
            context.tracking
        });

    let mut conn = pool.acquire().await.map_err(DbError::Query)?;

    Ok(
        phonix_db::inventory::lot::ensure(&mut conn, variant_id, number, expires_on, tracking, None)
            .await?
            .id,
    )
}

/// Close an order everything has arrived against.
async fn close_if_complete(pool: &PgPool, caller: &Caller, order_id: Uuid) -> ServiceResult<()> {
    let order = crate::inventory::purchase::detail(pool, caller, order_id).await?;

    if matches!(order.state, OrderState::Confirmed) && !order.has_outstanding() {
        let mut tx = pool.begin().await.map_err(DbError::Query)?;
        order_store::set_state(&mut tx, order_id, OrderState::Done, caller.user_id()).await?;
        tx.commit().await.map_err(DbError::Query)?;
    }

    Ok(())
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

/// The state a receipt reaches when it is posted, named so a caller does not
/// have to know the enum's spelling.
pub const POSTED: ReceiptState = ReceiptState::Done;
