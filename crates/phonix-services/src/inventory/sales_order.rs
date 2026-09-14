//! Sales orders: quoting one, confirming it, and closing it out.
//!
//! The mirror of [`super::purchase`], and deliberately the same file in the
//! same order.
//!
//! # The number is taken when the document leaves the building
//!
//! A purchase order takes its number at confirm, because nothing before that
//! has gone anywhere. A quotation has: it is sent to somebody who quotes it
//! back on their own paperwork, so [`mark_sent`] numbers it, and [`confirm`]
//! numbers one that was agreed without ever being quoted. Either way it is one
//! number for the life of the document - confirming a quotation keeps the one
//! the customer was given rather than spending a second.
//!
//! # There is no ledger consequence
//!
//! A sales order promises to ship something. Nothing has moved, nothing is
//! owed, and nothing is posted - which is why this module never touches the
//! `Ledger` port. The accounting starts at the delivery, which relieves stock,
//! and at the invoice, which makes the claim.
//!
//! # A line with no price is refused
//!
//! Unlike a purchase line, which falls back to the item's standing cost. There
//! is no price list in this workspace - one standing `sale_price` per item -
//! and that price is what a new line *opens* on in the browser. What a line
//! must not do is fall back to it silently at save: a zero or a stale price on
//! a purchase order costs the workspace a conversation with a supplier, and on
//! a sales order it is revenue that was given away.

use app_inventory::quantity::Quantity;
use app_inventory::sales_order::{
    Checked, CheckedLine, CustomerSnapshot, SaleError, SaleInput, SaleState, SaleSummary,
    SalesOrder,
};
use app_inventory::unit::Unit;
use app_inventory::variant::VariantChoice;
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_core::query::{Page, PageRequest};
use phonix_db::error::DbError;
use phonix_db::inventory::sales_order as store;
use phonix_db::inventory::variant as variants;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    request: PageRequest,
) -> ServiceResult<Page<SaleSummary>> {
    caller.require(permissions::SALES_ORDERS)?;
    Ok(store::page(pool, &request).await?)
}

/// The variants matching what somebody has typed in a line's item box.
///
/// What this workspace *sells*, which is a different list from what it buys:
/// an item marked bought and not sold is a raw material, and offering it on a
/// quotation is how one gets quoted.
pub async fn find_variants(
    pool: &PgPool,
    caller: &Caller,
    needle: &str,
) -> ServiceResult<Vec<VariantChoice>> {
    caller.require_any(&[permissions::SALES_ORDERS, permissions::STOCK])?;
    Ok(variants::search_sellable(pool, needle, super::purchase::PICKER_LIMIT).await?)
}

/// The confirmed orders with something still to ship, for a despatch screen.
pub async fn awaiting_despatch(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<SaleSummary>> {
    caller.require(permissions::SALES_ORDERS)?;
    Ok(store::awaiting_despatch(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<SalesOrder> {
    caller.require(permissions::SALES_ORDERS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("order", msg!("sales_orders.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<SaleInput> {
    Ok(SaleInput::from_order(&detail(pool, caller, id).await?))
}

/// A blank order, on the workspace's own currency and today's date.
pub async fn blank(pool: &PgPool, caller: &Caller) -> ServiceResult<SaleInput> {
    caller.require(permissions::SALES_ORDERS_CREATE)?;

    let currency = base_currency(pool).await?;
    Ok(SaleInput::blank(today(), currency.code()))
}

/// Write an order, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: SaleInput,
) -> ServiceResult<Submission<SaleInput>> {
    caller.require(match draft.id {
        None => permissions::SALES_ORDERS_CREATE,
        Some(_) => permissions::SALES_ORDERS_EDIT,
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
                SaleError::CurrencyRequired.message(),
            ));
        }
    };

    let customer = match customer_snapshot(pool, checked.customer_id).await? {
        Ok(customer) => customer,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // What has already gone and been billed, so re-saving a part-shipped order
    // does not forget it. Read before the lines are replaced, keyed by line id.
    let carried = match checked.id {
        None => Vec::new(),
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            if !before.state.is_editable() {
                return Ok(Submission::rejected("state", SaleError::NotEditable.message()));
            }

            before.lines
        }
    };

    let priced = match price_lines(pool, &checked, currency, &carried).await? {
        Ok(priced) => priced,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let net = match Money::total(currency, priced.iter().map(|line| line.net)) {
        Ok(net) => net,
        Err(err) => return Ok(Submission::rejected("unit_price", err.message())),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => store::insert(&mut tx, &checked, &customer, net, caller.user_id()).await?,
        Some(id) => {
            if !store::update(&mut tx, id, &checked, &customer, net, caller.user_id()).await? {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected("state", SaleError::NotEditable.message()));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &priced).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = SaleInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::SALES_ORDER, id)
        .named(&customer.name)
        .fact("net", &net.to_display_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Send the quotation. This is where the number is spent.
///
/// Still editable afterwards: a quotation is an offer, and a customer who asks
/// for a change gets a revised version of the same document rather than a
/// second one with a second number.
pub async fn mark_sent(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<SalesOrder>> {
    caller.require(permissions::SALES_ORDERS_EDIT)?;
    issue(pool, caller, id, SaleState::Sent).await
}

/// Accept the order: from here the quantities are what deliveries are measured
/// against.
pub async fn confirm(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<SalesOrder>> {
    caller.require(permissions::SALES_ORDERS_CONFIRM)?;
    issue(pool, caller, id, SaleState::Confirmed).await
}

/// Move a draft or a quotation onward, numbering it if it has no number yet.
///
/// One function for both transitions because the numbering rule is one rule,
/// and two copies of "allocate unless it already has one" is one copy too many.
async fn issue(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    state: SaleState,
) -> ServiceResult<Submission<SalesOrder>> {
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    if !order.state.is_editable() {
        return Ok(Submission::rejected("state", SaleError::NotEditable.message()));
    }
    if order.lines.is_empty() {
        return Ok(Submission::rejected("lines", SaleError::NoLines.message()));
    }

    // Outside the transaction: every order queues through the sequence's one
    // row, so anything that can happen before that lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    // A quotation being confirmed already has its number and must keep it.
    // Asked inside the transaction, because between the read above and here
    // somebody else may have sent it.
    let existing = store::number_of(&mut *tx, id).await?.unwrap_or_default();

    let number = if existing.is_empty() {
        let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::SALES_ORDER);

        match generator.next(&mut tx, key, order.order_date).await {
            Ok(allocated) => allocated.number,
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "number",
                    msg!("sales_orders.error.no_series"),
                ));
            }
            Err(err) => return Err(err),
        }
    } else {
        existing
    };

    if !store::issue(&mut tx, id, state, &number, caller.user_id()).await? {
        // Somebody moved it between the read and the write. Rolling back
        // returns the number rather than leaving a hole.
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected("state", SaleError::NotEditable.message()));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::SALES_ORDER, id)
            .named(&stored.number)
            .fact("customer", &stored.customer.name)
            .fact("net", &stored.net.to_display_string()),
        &order,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Stop an order. One that has been quoted is cancelled and kept, because
/// somebody outside has a copy of it.
pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::SALES_ORDERS_CANCEL)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    // Goods have gone against it. Cancelling now would leave a delivery
    // pointing at an order the workspace says never happened.
    if order.lines.iter().any(|line| line.delivered.is_positive()) {
        return Ok(Submission::rejected(
            "state",
            msg!("sales_orders.error.already_delivered"),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::set_state(&mut tx, id, SaleState::Cancelled, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::SALES_ORDER, id).named(&order.label()),
        &order.state.as_str(),
        &SaleState::Cancelled.as_str(),
    )
    .await;

    Ok(Submission::Saved(()))
}

/// Close a confirmed order that will never be completed.
///
/// The customer took what they wanted, or the remaining two of forty are not
/// worth shipping. Distinct from cancelling: what went, went.
pub async fn close(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::SALES_ORDERS_CONFIRM)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    if !matches!(order.state, SaleState::Confirmed) {
        return Ok(Submission::rejected(
            "state",
            SaleError::NotDeliverable.message(),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::set_state(&mut tx, id, SaleState::Done, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    Ok(Submission::Saved(()))
}

/// Remove a draft. A quotation that has been sent is cancelled, never deleted.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::SALES_ORDERS_EDIT)?;
    acting_user(caller)?;

    let order = detail(pool, caller, id).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::SALES_ORDER, id).named(&order.label()),
            &order,
        )
        .await;
    }

    Ok(removed)
}

// --- Shared ---------------------------------------------------------------

/// The customer's code and name as they stand now, refusing a party that is not
/// one.
pub(crate) async fn customer_snapshot(
    pool: &PgPool,
    party_id: Uuid,
) -> ServiceResult<Result<CustomerSnapshot, SaleError>> {
    let Some(party) = phonix_db::master::party::find(pool, party_id).await? else {
        return Ok(Err(SaleError::CustomerRequired));
    };

    if !party
        .roles
        .iter()
        .any(|role| role.as_str() == phonix_master::party::roles::CUSTOMER)
    {
        return Ok(Err(SaleError::NotACustomer));
    }

    Ok(Ok(CustomerSnapshot {
        party_id: party.id,
        code: party.code,
        name: party.name,
    }))
}

/// Work out each line's stock-unit quantity and its two amounts.
///
/// `carried` is the order's lines as they stood, so a re-save keeps what has
/// already been delivered and invoiced against a line rather than resetting it.
async fn price_lines<'a>(
    pool: &PgPool,
    checked: &'a Checked,
    currency: Currency,
    carried: &[app_inventory::sales_order::SaleLine],
) -> ServiceResult<Result<Vec<store::PricedLine<'a>>, SaleError>> {
    let units = phonix_db::inventory::unit::list(pool).await?;
    let mut priced = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let Some(context) =
            phonix_db::inventory::movement::context(pool, line.variant_id, currency).await?
        else {
            return Ok(Err(SaleError::ItemRequired));
        };

        let quantity_stock = match convert(&units, line, context.stock_unit_id) {
            Ok(quantity) => quantity,
            Err(err) => return Ok(Err(err)),
        };

        // No fallback. See the module header: a price nobody typed is revenue
        // nobody decided.
        if line.unit_price.is_empty() {
            return Ok(Err(SaleError::PriceRequired));
        }

        let unit_price = match Money::parse(currency, &line.unit_price) {
            Ok(price) => price,
            Err(err) => return Ok(Err(SaleError::Money(err))),
        };

        let net = match unit_price.scale_by(
            line.quantity.scaled(),
            app_inventory::quantity::SCALE_FACTOR,
            Rounding::HalfUp,
        ) {
            Ok(net) => net,
            Err(err) => return Ok(Err(SaleError::Money(err))),
        };

        let previous = line.id.and_then(|id| carried.iter().find(|old| old.id == id));

        priced.push(store::PricedLine {
            source: line,
            description: if line.description.is_empty() {
                context.item_name.clone()
            } else {
                line.description.clone()
            },
            quantity_stock,
            unit_price,
            net,
            delivered: previous.map_or(Quantity::ZERO, |old| old.delivered),
            invoiced: previous.map_or(Quantity::ZERO, |old| old.invoiced),
        });
    }

    Ok(Ok(priced))
}

/// The line's quantity in the item's stock unit.
///
/// The two units have to measure the same thing - a case converts to eaches and
/// does not convert to kilograms. The same refusal the purchase order makes,
/// made again here because a line may name a unit the item does not.
fn convert(units: &[Unit], line: &CheckedLine, stock_unit_id: Uuid) -> Result<Quantity, SaleError> {
    if line.unit_id == stock_unit_id {
        return Ok(line.quantity);
    }

    let from = units
        .iter()
        .find(|unit| unit.id == line.unit_id)
        .ok_or(SaleError::UnitRequired)?;
    let to = units
        .iter()
        .find(|unit| unit.id == stock_unit_id)
        .ok_or(SaleError::UnitRequired)?;

    app_inventory::unit::Conversion::between(from, to)
        .and_then(|conversion| conversion.apply(line.quantity))
        .map_err(|_| SaleError::UnitMismatch)
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
