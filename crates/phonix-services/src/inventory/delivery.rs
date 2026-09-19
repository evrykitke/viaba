//! Deliveries: writing one, and despatching it.
//!
//! The mirror of [`super::receipt`], and the point where a sales order stops
//! being a promise.
//!
//! # Posting is the only thing here that changes anything outside its own
//! tables
//!
//! And it does it by calling `stock::apply` once per line. Everything the
//! accounting needs - stock credited, cost of sales debited, the layers
//! consumed in the order the category says, the average left alone - falls out
//! of that call, because the move is from a location of ours into the
//! customer's counterpart.
//!
//! # The cost is not known until the move is made
//!
//! A receipt knows what its lines cost before it posts: the supplier said so.
//! A delivery does not. Under average or FIFO the cost of the units leaving is
//! decided by the layers, and only the move knows which it consumed. So the
//! line stores no cost, the move decides it, and it is written back in the same
//! statement as the move id.
//!
//! # What this does NOT post
//!
//! Revenue. A despatch moves goods and posts what they cost; what the customer
//! is charged is the invoice's, against the tax group in force on its own date.
//!
//! And, for now, it posts the cost straight to cost of sales rather than to the
//! goods-delivered-not-invoiced accrual that `AccountRole` has carried since
//! books 0005. That role is what makes a despatch on the thirtieth and its
//! invoice on the second land in the same month, and it needs the invoice to
//! know which delivery it bills - a link that does not exist yet. Until it
//! does, the cost lands the day the goods leave, which is the ordinary answer
//! and wrong only across a month end.

use app_inventory::delivery::{
    CheckedDelivery, Delivery, DeliveryError, DeliveryInput, DeliveryState, DeliverySummary,
    InvoicedOutcome, Outstanding, UninvoicedDelivery,
};
use app_inventory::quantity::Quantity;
use app_inventory::sales_order::CustomerSnapshot;
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_core::query::{Page, PageRequest};
use phonix_db::error::DbError;
use phonix_db::inventory::delivery as store;
use phonix_db::inventory::sales_order as order_store;
use phonix_db::inventory::warehouse as warehouse_store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::deliveries::{
    Deliveries, DeliveriesError, Despatch, DespatchedLine, InvoicedLine, PORT,
};
use phonix_ports::error::PortError;
use phonix_ports::ledger::Ledger;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    request: PageRequest,
) -> ServiceResult<Page<DeliverySummary>> {
    caller.require(permissions::DELIVERIES)?;
    let currency = base_currency(pool).await?;

    Ok(store::page(pool, currency, &request).await?)
}

/// Goods delivered and not yet invoiced, oldest first. The aged GDNI balance.
///
/// Every posted delivery is in it until something invoices one, which nothing
/// does yet: the invoice cannot name a delivery line, and the port that would
/// let it say so is the next piece. Until then this reads as everything gone,
/// which is what is true rather than a placeholder.
pub async fn uninvoiced(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<UninvoicedDelivery>> {
    caller.require(permissions::DELIVERIES)?;

    let currency = base_currency(pool).await?;
    Ok(store::uninvoiced(pool, currency).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Delivery> {
    caller.require(permissions::DELIVERIES)?;

    let currency = base_currency(pool).await?;

    store::find(pool, id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("delivery", msg!("deliveries.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DeliveryInput> {
    Ok(DeliveryInput::from_delivery(
        &detail(pool, caller, id).await?,
    ))
}

pub async fn blank(_pool: &PgPool, caller: &Caller) -> ServiceResult<DeliveryInput> {
    caller.require(permissions::DELIVERIES_CREATE)?;
    Ok(DeliveryInput::blank(today()))
}

/// A delivery prefilled with everything an order still owes.
///
/// The screen somebody actually wants when the van is at the door. Refuses an
/// order that is not confirmed: goods leaving for something nobody has agreed
/// to buy is the mistake this exists to catch.
pub async fn against_order(
    pool: &PgPool,
    caller: &Caller,
    order_id: Uuid,
) -> ServiceResult<Submission<DeliveryInput>> {
    caller.require(permissions::DELIVERIES_CREATE)?;

    let order = super::sales_order::detail(pool, caller, order_id).await?;

    if !order.state.accepts_deliveries() {
        return Ok(Submission::rejected(
            "order_id",
            DeliveryError::OrderNotDeliverable.message(),
        ));
    }

    Ok(Submission::Saved(DeliveryInput::against(&order, today())))
}

/// What an order still owes, for a screen to show beside what has gone.
pub async fn outstanding(
    pool: &PgPool,
    caller: &Caller,
    order_id: Uuid,
) -> ServiceResult<Option<Outstanding>> {
    caller.require(permissions::SALES_ORDERS)?;

    Ok(Outstanding::of(
        &super::sales_order::detail(pool, caller, order_id).await?,
    ))
}

/// Write a delivery, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: DeliveryInput,
) -> ServiceResult<Submission<DeliveryInput>> {
    caller.require(permissions::DELIVERIES_CREATE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let (customer, from_location_id) = match prepare(pool, &checked).await? {
        Ok(prepared) => prepared,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // Every line's item is read once, for its name and to refuse anything that
    // does not hold stock. A service line on a delivery note is a line that can
    // never move.
    let currency = base_currency(pool).await?;
    let mut lines = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let Some(context) =
            phonix_db::inventory::movement::context(pool, line.variant_id, currency).await?
        else {
            return Ok(Submission::rejected(
                "lines",
                DeliveryError::ItemRequired.message(),
            ));
        };

        if !context.holds_stock() {
            return Ok(Submission::rejected(
                "lines",
                msg!("receipts.error.holds_no_stock", item = context.item_name),
            ));
        }

        lines.push(store::LineToStore {
            source: line,
            description: if line.description.is_empty() {
                context.item_name.clone()
            } else {
                line.description.clone()
            },
        });
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => {
            store::insert(
                &mut tx,
                &checked,
                &customer,
                from_location_id,
                caller.user_id(),
            )
            .await?
        }
        Some(id) => {
            if !store::update(
                &mut tx,
                id,
                &checked,
                &customer,
                from_location_id,
                caller.user_id(),
            )
            .await?
            {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "state",
                    DeliveryError::NotEditable.message(),
                ));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &lines).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = DeliveryInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::DELIVERY, id)
        .named(&customer.name)
        .fact("lines", lines.len().to_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Despatch: move the stock, post the cost, and number the document.
///
/// # Each line is its own transaction
///
/// `stock::apply` posts a journal through a port, and holding one transaction
/// open across every line's port call would make the ledger's implementation a
/// participant in Inventory's locking. The price is that a line failing halfway
/// leaves the ones before it posted; what makes that recoverable rather than a
/// double count is that each move is written back against its line
/// *immediately*, and a line that already carries a `move_id` is skipped on the
/// next attempt. Despatching twice therefore moves each line once. This is the
/// receipt's rule, and it is the same rule because it is the same risk.
pub async fn post(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    id: Uuid,
) -> ServiceResult<Submission<Delivery>> {
    caller.require(permissions::DELIVERIES_POST)?;
    acting_user(caller)?;

    let delivery = detail(pool, caller, id).await?;

    if !delivery.state.is_editable() {
        return Ok(Submission::rejected(
            "state",
            DeliveryError::NotEditable.message(),
        ));
    }
    if !delivery.has_quantity() {
        return Ok(Submission::rejected(
            "lines",
            DeliveryError::NothingDespatched.message(),
        ));
    }

    // The customer's side of every move. Seeded, so reaching here without one
    // means somebody deleted it, and the message says which.
    let customer =
        crate::inventory::location::counterpart(pool, app_inventory::LocationKind::Customer)
            .await?;

    let currency = base_currency(pool).await?;
    let generator = crate::numbering::NumberGenerator::open(pool).await?;

    // Every line is asked its lot question before the first one moves. A
    // refusal that was always going to happen belongs here, where nothing has
    // left the building yet.
    for line in &delivery.lines {
        if line.move_id.is_some() {
            continue;
        }

        let Some(context) =
            phonix_db::inventory::movement::context(pool, line.variant_id, currency).await?
        else {
            return Ok(Submission::rejected(
                "lines",
                DeliveryError::ItemRequired.message(),
            ));
        };

        let rules = app_inventory::lot::LotRules {
            tracking: context.tracking,
            uses_expiry: context.uses_expiry,
            holds_stock: context.holds_stock(),
        };

        if rules.wants_a_number() && line.lot_id.is_none() {
            return Ok(Submission::rejected(
                "lines",
                msg!(
                    "deliveries.error.lot_required_for",
                    item = context.item_name
                ),
            ));
        }
    }

    for line in &delivery.lines {
        if line.move_id.is_some() {
            continue;
        }

        let request = app_inventory::movement::MoveRequest {
            lot_id: line.lot_id,
            // No cost from here. The layers decide it, which is the whole
            // difference between despatching and receiving.
            unit_cost: None,
            reference: delivery.carrier_reference.clone(),
            source: Some(app_inventory::movement::MoveSource::new("delivery", id)),
            ..app_inventory::movement::MoveRequest::new(
                line.variant_id,
                delivery.from_location_id,
                customer.id,
                line.quantity,
                delivery.despatched_on,
            )
        };

        let stored = match crate::inventory::stock::apply(pool, caller, ledger, request).await? {
            Submission::Saved(stored) => stored,
            Submission::Rejected(errors) => return Ok(Submission::Rejected(errors)),
        };

        // Written back before the next line is attempted. The move is already
        // committed by this point, so anything that defers this leaves stock
        // gone and nothing on the delivery saying so.
        let mut tx = pool.begin().await.map_err(DbError::Query)?;

        store::record_move(&mut tx, line.id, stored.id, stored.unit_cost, stored.value).await?;

        // The order learns what went. A delta rather than an absolute, so two
        // deliveries against one line in the same minute both count.
        if let Some(order_line_id) = line.order_line_id {
            order_store::advance_delivered(&mut tx, order_line_id, stored.quantity).await?;
        }

        tx.commit().await.map_err(DbError::Query)?;
    }

    // Read back rather than summed from what was in memory: the costs were
    // decided by the moves and written back a moment ago, and the figures in
    // `delivery` above are the zeroes the draft held.
    let posted_lines = store::lines_of(pool, id, currency).await?;
    let value = Money::total(currency, posted_lines.iter().map(|line| line.value))
        .unwrap_or_else(|_| Money::zero(currency));

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::DELIVERY);
    let allocated = match generator.next(&mut tx, key, delivery.despatched_on).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "number",
                msg!("deliveries.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    if !store::post(&mut tx, id, &allocated.number, value, caller.user_id()).await? {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "state",
            DeliveryError::NotEditable.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    // An order everything has gone against closes itself. Nobody should have to
    // tick a box to say a finished order is finished.
    if let Some(order_id) = delivery.order_id {
        close_if_complete(pool, caller, order_id).await?;
    }

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::DELIVERY, id)
            .named(&stored.number)
            .fact("customer", &stored.customer.name)
            .fact("cost", &stored.value.to_display_string()),
        &delivery,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Abandon a draft. A despatched delivery is never cancelled: the stock has
/// gone, and taking it back is a customer return, which is a receipt.
pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::DELIVERIES_CREATE)?;
    acting_user(caller)?;

    let delivery = detail(pool, caller, id).await?;

    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    if !store::cancel(&mut conn, id, caller.user_id()).await? {
        return Ok(Submission::rejected(
            "state",
            DeliveryError::NotEditable.message(),
        ));
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::DELIVERY, id).named(&delivery.label()),
        &delivery.state.as_str(),
        &DeliveryState::Cancelled.as_str(),
    )
    .await;

    Ok(Submission::Saved(()))
}

pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::DELIVERIES_CREATE)?;
    acting_user(caller)?;

    let delivery = detail(pool, caller, id).await?;

    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut conn, id).await?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::DELIVERY, id).named(&delivery.label()),
            &delivery,
        )
        .await;
    }

    Ok(removed)
}

// --- Shared ---------------------------------------------------------------

/// The customer's snapshot and where the goods leave from.
async fn prepare(
    pool: &PgPool,
    checked: &CheckedDelivery,
) -> ServiceResult<Result<(CustomerSnapshot, Uuid), DeliveryError>> {
    let customer = match super::sales_order::customer_snapshot(pool, checked.customer_id).await? {
        Ok(customer) => customer,
        // The sales order's refusals, in this document's vocabulary. A party
        // that is not a customer is not a customer on either screen.
        Err(app_inventory::sales_order::SaleError::NotACustomer) => {
            return Ok(Err(DeliveryError::NotACustomer));
        }
        Err(_) => return Ok(Err(DeliveryError::CustomerRequired)),
    };

    let warehouses = warehouse_store::selectable(pool).await?;
    let Some(warehouse) = warehouses
        .iter()
        .find(|warehouse| warehouse.id == checked.warehouse_id)
    else {
        return Ok(Err(DeliveryError::WarehouseRequired));
    };

    let from_location_id = warehouse_store::despatch_location(pool, warehouse).await?;

    Ok(Ok((customer, from_location_id)))
}

/// Close an order nothing is outstanding on.
async fn close_if_complete(pool: &PgPool, caller: &Caller, order_id: Uuid) -> ServiceResult<()> {
    let order = super::sales_order::detail(pool, caller, order_id).await?;

    if order.state == app_inventory::sales_order::SaleState::Confirmed && !order.has_outstanding() {
        let mut tx = pool.begin().await.map_err(DbError::Query)?;
        order_store::set_state(
            &mut tx,
            order_id,
            app_inventory::sales_order::SaleState::Done,
            caller.user_id(),
        )
        .await?;
        tx.commit().await.map_err(DbError::Query)?;
    }

    Ok(())
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    crate::workspace::profile::base_currency(pool).await
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

// ---------------------------------------------------------------------------
// Inventory's side of the `Deliveries` port
// ---------------------------------------------------------------------------
//
// Here rather than in its own file, because this module owns deliveries and a
// `deliveries.rs` beside `delivery.rs` is a filename nobody would guess right
// twice. Same shape as `hr::cost_centre`: rules in the app crate, statements in
// `phonix-db`, the seam in a service.
//
// Ungated, like the other port implementations: it is not a screen, it is what
// Books calls while posting an invoice, and Books has already checked its
// caller.

/// The `Deliveries` port, over this workspace's deliveries. Owns its pool so it
/// can be handed over as a `dyn Deliveries` with no lifetime to thread.
#[derive(Clone)]
pub struct InventoryDeliveries {
    pool: PgPool,
}

impl InventoryDeliveries {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Deliveries for InventoryDeliveries {
    async fn invoice(&self, lines: &[InvoicedLine]) -> Result<(), DeliveriesError> {
        let parsed = lines
            .iter()
            .map(|line| {
                Quantity::parse(&line.quantity)
                    .map(|quantity| (line.delivery_line_id, quantity))
                    .map_err(|_| DeliveriesError::NotAQuantity(line.quantity.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if parsed.is_empty() {
            return Ok(());
        }

        let outcome = store::mark_invoiced(&self.pool, &parsed)
            .await
            .map_err(|err| DeliveriesError::from(PortError::unavailable(PORT, err)))?;

        match outcome {
            InvoicedOutcome::Recorded => Ok(()),
            InvoicedOutcome::UnknownLine(id) => Err(DeliveriesError::UnknownLine(id)),
            InvoicedOutcome::NotDespatched(id) => Err(DeliveriesError::NotDespatched(id)),
            InvoicedOutcome::MoreThanDelivered {
                delivery_line_id,
                left,
                asked,
            } => Err(DeliveriesError::MoreThanDelivered {
                delivery_line_id,
                left: left.to_display_string(),
                asked: asked.to_display_string(),
            }),
        }
    }

    async fn despatch(&self, delivery_id: Uuid) -> Result<Option<Despatch>, DeliveriesError> {
        let currency = base_currency(&self.pool)
            .await
            .map_err(|err| DeliveriesError::Unavailable(err.to_string()))?;

        let found = store::find(&self.pool, delivery_id, currency)
            .await
            .map_err(|err| DeliveriesError::from(PortError::unavailable(PORT, err)))?;

        let Some(delivery) = found.filter(|delivery| delivery.state == DeliveryState::Done) else {
            return Ok(None);
        };

        let lines = store::invoiceable_lines(&self.pool, delivery_id)
            .await
            .map_err(|err| DeliveriesError::from(PortError::unavailable(PORT, err)))?;

        Ok(Some(Despatch {
            customer_id: delivery.customer.party_id,
            number: delivery.number,
            lines: lines
                .into_iter()
                .map(
                    |(delivery_line_id, description, quantity, unit_price)| DespatchedLine {
                        delivery_line_id,
                        description,
                        quantity,
                        unit_price: unit_price.unwrap_or_default(),
                    },
                )
                .collect(),
        }))
    }
}
