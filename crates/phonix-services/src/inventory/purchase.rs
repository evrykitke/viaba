//! Purchase orders: writing one, confirming it, and closing it out.
//!
//! # Confirming is the act that makes it a document
//!
//! A draft is a plan. Confirming takes a number from `core.number_sequences` in
//! the same transaction as the write, freezes the supplier's code and name onto
//! the record, and from that moment the quantities are what receipts and bills
//! are measured against. Books does this to an invoice; ADR 0006 section 3 says
//! why, and the reason is the same here.
//!
//! # There is no ledger consequence
//!
//! A purchase order commits the workspace to buy something. Nothing has
//! arrived, nothing is owed, and nothing is posted - which is why this module
//! never touches the `Ledger` port. The accounting starts at the receipt.
//!
//! # The stock-unit quantity is worked out once, here
//!
//! A line is placed in the supplier's unit - a case, a reel - and every receipt
//! against it is measured in the item's stock unit. The conversion happens when
//! the line is written and is stored beside it, because a factor somebody edits
//! next year must not restate how much was ordered.

use app_inventory::purchase::{
    Checked, CheckedLine, OrderError, OrderInput, OrderState, OrderSummary, PurchaseOrder,
    SupplierSnapshot,
};
use app_inventory::quantity::Quantity;
use app_inventory::unit::Unit;
use app_inventory::variant::VariantChoice;
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::purchase as store;
use phonix_db::inventory::variant as variants;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<OrderSummary>> {
    caller.require(permissions::PURCHASE_ORDERS)?;
    Ok(store::list(pool).await?)
}

/// What an order or receipt line may be written against.
///
/// Either permission opens it: a receipts clerk who may not raise an order
/// still has to be able to name what turned up on the pallet.
pub async fn pickable_variants(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<VariantChoice>> {
    caller.require_any(&[permissions::PURCHASE_ORDERS, permissions::RECEIPTS])?;
    Ok(variants::purchasable(pool).await?)
}

/// The confirmed orders with something still to come, for a receipt screen.
pub async fn awaiting_delivery(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<OrderSummary>> {
    caller.require(permissions::RECEIPTS)?;
    Ok(store::awaiting_delivery(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<PurchaseOrder> {
    caller.require(permissions::PURCHASE_ORDERS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("order", msg!("purchase_orders.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<OrderInput> {
    Ok(OrderInput::from_order(&detail(pool, caller, id).await?))
}

/// A blank order, on the workspace's own currency and today's date.
pub async fn blank(pool: &PgPool, caller: &Caller) -> ServiceResult<OrderInput> {
    caller.require(permissions::PURCHASE_ORDERS_CREATE)?;

    let currency = base_currency(pool).await?;
    Ok(OrderInput::blank(today(), currency.code()))
}

/// Write an order, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: OrderInput,
) -> ServiceResult<Submission<OrderInput>> {
    caller.require(match draft.id {
        None => permissions::PURCHASE_ORDERS_CREATE,
        Some(_) => permissions::PURCHASE_ORDERS_EDIT,
    })?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let currency = match Currency::parse(&checked.currency) {
        Ok(currency) => currency,
        Err(_) => {
            return Ok(Submission::rejected(
                "currency",
                OrderError::CurrencyRequired.message(),
            ));
        }
    };

    let supplier = match supplier_snapshot(pool, checked.supplier_id).await? {
        Ok(supplier) => supplier,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // What already arrived, so re-saving a partly received order does not
    // forget it. Read before the lines are replaced, keyed by line id.
    let carried = match checked.id {
        None => Vec::new(),
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            if !before.state.is_editable() {
                return Ok(Submission::rejected("state", OrderError::NotEditable.message()));
            }

            before.lines
        }
    };

    let costed = match cost_lines(pool, &checked, currency, &carried).await? {
        Ok(costed) => costed,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let net = match Money::total(currency, costed.iter().map(|line| line.net)) {
        Ok(net) => net,
        Err(err) => return Ok(Submission::rejected("unit_price", err.message())),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => store::insert(&mut tx, &checked, &supplier, net, caller.user_id()).await?,
        Some(id) => {
            if !store::update(&mut tx, id, &checked, &supplier, net, caller.user_id()).await? {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected("state", OrderError::NotEditable.message()));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &costed).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = OrderInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::PURCHASE_ORDER, id)
        .named(&supplier.name)
        .fact("net", &net.to_display_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Mark a draft as sent to the supplier for a price.
///
/// Still editable afterwards: a quotation is not a commitment, and this is the
/// state Odoo calls `sent`. It exists so a buyer can tell the orders they are
/// still writing from the ones they are waiting on.
pub async fn mark_sent(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<OrderState> {
    caller.require(permissions::PURCHASE_ORDERS_EDIT)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;
    if !matches!(order.state, OrderState::Draft) {
        return Ok(order.state);
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::set_state(&mut tx, id, OrderState::Sent, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    Ok(OrderState::Sent)
}

/// Turn a draft into a commitment: allocate its number and freeze it.
pub async fn confirm(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<PurchaseOrder>> {
    caller.require(permissions::PURCHASE_ORDERS_CONFIRM)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    if !order.state.is_editable() {
        return Ok(Submission::rejected("state", OrderError::NotEditable.message()));
    }
    if order.lines.is_empty() {
        return Ok(Submission::rejected("lines", OrderError::NoLines.message()));
    }

    // Outside the transaction: every order queues through the sequence's one
    // row, so anything that can happen before that lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::PURCHASE_ORDER);
    let allocated = match generator.next(&mut tx, key, order.order_date).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "number",
                msg!("purchase_orders.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    if !store::confirm(&mut tx, id, &allocated.number, caller.user_id()).await? {
        // Somebody confirmed it between the read and the write. Rolling back
        // returns the number rather than leaving a hole.
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected("state", OrderError::NotEditable.message()));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::PURCHASE_ORDER, id)
            .named(&stored.number)
            .fact("supplier", &stored.supplier.name)
            .fact("net", &stored.net.to_display_string()),
        &order,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Stop an order. A confirmed one is cancelled and kept, because it was sent.
pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::PURCHASE_ORDERS_CANCEL)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    // Goods have arrived against it. Cancelling now would leave a receipt
    // pointing at an order the workspace says never happened.
    if order.lines.iter().any(|line| line.received.is_positive()) {
        return Ok(Submission::rejected(
            "state",
            msg!("purchase_orders.error.already_received"),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::set_state(&mut tx, id, OrderState::Cancelled, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::PURCHASE_ORDER, id).named(&order.label()),
        &order.state.as_str(),
        &OrderState::Cancelled.as_str(),
    )
    .await;

    Ok(Submission::Saved(()))
}

/// Close a confirmed order that will never be completed.
///
/// The supplier discontinued the line, or the remaining two of forty are not
/// worth chasing. Distinct from cancelling: what arrived, arrived.
pub async fn close(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::PURCHASE_ORDERS_CONFIRM)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    if !matches!(order.state, OrderState::Confirmed) {
        return Ok(Submission::rejected(
            "state",
            OrderError::NotReceivable.message(),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::set_state(&mut tx, id, OrderState::Done, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    Ok(Submission::Saved(()))
}

/// Remove a draft. A confirmed order is cancelled, never deleted.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::PURCHASE_ORDERS_EDIT)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::PURCHASE_ORDER, id).named(&order.label()),
            &order,
        )
        .await;
    }

    Ok(removed)
}

// --- Shared ---------------------------------------------------------------

/// The supplier's code and name as they stand now, refusing a party that is not
/// one.
///
/// Through `phonix_master`, which is always on. Not a port: master data is not
/// an app that can be switched off, and ADR 0006 section 2's table lists
/// `Parties` as a port over something that always exists - a distinction that
/// costs nothing to honour here because the read is one function either way.
pub(crate) async fn supplier_snapshot(
    pool: &PgPool,
    party_id: Uuid,
) -> ServiceResult<Result<SupplierSnapshot, OrderError>> {
    let Some(party) = phonix_db::master::party::find(pool, party_id).await? else {
        return Ok(Err(OrderError::SupplierRequired));
    };

    if !party
        .roles
        .iter()
        .any(|role| role.as_str() == phonix_master::party::roles::SUPPLIER)
    {
        return Ok(Err(OrderError::NotASupplier));
    }

    Ok(Ok(SupplierSnapshot {
        party_id: party.id,
        code: party.code,
        name: party.name,
    }))
}

/// Work out each line's stock-unit quantity and its two amounts.
///
/// `carried` is the order's lines as they stood, so a re-save keeps what has
/// already been received and billed against a line rather than resetting it.
async fn cost_lines<'a>(
    pool: &PgPool,
    checked: &'a Checked,
    currency: Currency,
    carried: &[app_inventory::purchase::OrderLine],
) -> ServiceResult<Result<Vec<store::CostedLine<'a>>, OrderError>> {
    let units = phonix_db::inventory::unit::list(pool).await?;
    let mut costed = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let Some(context) =
            phonix_db::inventory::movement::context(pool, line.variant_id, currency).await?
        else {
            return Ok(Err(OrderError::ItemRequired));
        };

        let quantity_stock = match convert(&units, line, context.stock_unit_id) {
            Ok(quantity) => quantity,
            Err(err) => return Ok(Err(err)),
        };

        let unit_price = if line.unit_price.is_empty() {
            // Nothing typed: the item's own cost, which is what the workspace
            // last paid. A buyer overwrites it where the supplier disagrees.
            Money::parse(currency, &context.cost.to_storage_string())
                .unwrap_or_else(|_| Money::zero(currency))
        } else {
            match Money::parse(currency, &line.unit_price) {
                Ok(price) => price,
                Err(err) => return Ok(Err(OrderError::Money(err))),
            }
        };

        let net = match unit_price.scale_by(
            line.quantity.scaled(),
            app_inventory::quantity::SCALE_FACTOR,
            Rounding::HalfUp,
        ) {
            Ok(net) => net,
            Err(err) => return Ok(Err(OrderError::Money(err))),
        };

        let previous = line
            .id
            .and_then(|id| carried.iter().find(|old| old.id == id));

        costed.push(store::CostedLine {
            source: line,
            description: if line.description.is_empty() {
                context.item_name.clone()
            } else {
                line.description.clone()
            },
            quantity_stock,
            unit_price,
            net,
            received: previous.map_or(Quantity::ZERO, |old| old.received),
            billed: previous.map_or(Quantity::ZERO, |old| old.billed),
        });
    }

    Ok(Ok(costed))
}

/// The line's quantity in the item's stock unit.
///
/// The two units have to measure the same thing - a case converts to eaches and
/// does not convert to kilograms - which is the same refusal `item::save` makes
/// about a purchase unit, made again here because a line may name a unit the
/// item does not.
fn convert(units: &[Unit], line: &CheckedLine, stock_unit_id: Uuid) -> Result<Quantity, OrderError> {
    if line.unit_id == stock_unit_id {
        return Ok(line.quantity);
    }

    let from = units
        .iter()
        .find(|unit| unit.id == line.unit_id)
        .ok_or(OrderError::UnitRequired)?;
    let to = units
        .iter()
        .find(|unit| unit.id == stock_unit_id)
        .ok_or(OrderError::UnitRequired)?;

    app_inventory::unit::Conversion::between(from, to)
        .and_then(|conversion| conversion.apply(line.quantity))
        .map_err(|_| OrderError::UnitMismatch)
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
