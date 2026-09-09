//! Consolidation: eleven departments wanting printer paper, bought once.
//!
//! # Confirming is the only interesting thing here
//!
//! Everything before it is an ordinary document: draft, edit, delete. [`confirm`]
//! is where the work is, and it does four things in one transaction -
//!
//!   1. takes the consolidation's own number,
//!   2. groups the lines by supplier and raises one **confirmed** purchase
//!      order per group, each taking its own order number,
//!   3. walks the outstanding requisition lines for each item oldest request
//!      first, writing `purchase_order_line_sources` and advancing `ordered`,
//!   4. leaves whatever the buyer ordered beyond the demand allocated to
//!      nobody.
//!
//! - and either all of it happens or none of it does. A half-confirmed
//! consolidation would be orders sent to suppliers against demand that still
//! reads as outstanding, which is the second delivery nobody ordered.
//!
//! # The orders are raised confirmed, not draft
//!
//! A buyer who consolidated and chose the suppliers has decided to buy. Raising
//! drafts would mean confirming each of them again one at a time, which is the
//! work consolidation exists to remove. It also matters for the allocation:
//! `ordered` is advanced here, and advancing it against orders that might never
//! be confirmed would hide demand that is still real.
//!
//! # Nothing here touches the `Ledger` port
//!
//! Same as the two documents either side of it. A purchase order commits money
//! and posts nothing; the accounting starts at the receipt.
//!
//! # The allocation is recomputed, never read off the draft
//!
//! `consolidation_lines.demand` is a snapshot for the screen. What gets
//! allocated is what `outstanding_lines_for_update` says *now*, under a row
//! lock, so a draft written last week against demand that has since been
//! withdrawn allocates what is actually waiting.

use app_inventory::consolidation::{
    Checked, Consolidation, ConsolidationError, ConsolidationInput, ConsolidationSummary,
    LineAllocation,
};
use app_inventory::purchase::SupplierSnapshot;
use app_inventory::quantity::Quantity;
use app_inventory::requisition::Demand;
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::consolidation as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ConsolidationSummary>> {
    caller.require(permissions::CONSOLIDATIONS)?;
    Ok(store::list(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Consolidation> {
    caller.require(permissions::CONSOLIDATIONS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("consolidation", msg!("consolidations.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<ConsolidationInput> {
    Ok(ConsolidationInput::from_consolidation(
        &detail(pool, caller, id).await?,
    ))
}

/// What approved requisitions are still waiting for, grouped by item and place.
///
/// The screen a consolidation is started from. Gated on `CONSOLIDATIONS` rather
/// than on the requisition permission: this is the buyer's view of demand, and
/// a buyer who may not read individual requests may still see what has to be
/// bought.
pub async fn demand(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Demand>> {
    caller.require(permissions::CONSOLIDATIONS)?;
    Ok(phonix_db::inventory::requisition::demand(pool).await?)
}

/// A blank consolidation, on today's date.
pub fn blank(caller: &Caller) -> ServiceResult<ConsolidationInput> {
    caller.require(permissions::CONSOLIDATIONS_MANAGE)?;
    Ok(ConsolidationInput::blank(today()))
}

/// A consolidation drawn from everything one warehouse is waiting for.
///
/// The ordinary way one starts: the buyer picks a warehouse and the form opens
/// already holding every outstanding item, quantities equal to the demand. From
/// there it is edited - rounded up to case sizes, cut back to a budget, given
/// suppliers.
///
/// An empty answer is a real one and is returned rather than refused: "nothing
/// is waiting" is what the screen should say, not an error.
pub async fn from_demand(
    pool: &PgPool,
    caller: &Caller,
    warehouse_id: Uuid,
) -> ServiceResult<ConsolidationInput> {
    caller.require(permissions::CONSOLIDATIONS_MANAGE)?;

    let waiting = phonix_db::inventory::requisition::demand(pool).await?;

    Ok(ConsolidationInput {
        warehouse_id: Some(warehouse_id),
        lines: waiting
            .into_iter()
            .filter(|row| row.warehouse_id == warehouse_id)
            .map(|row| {
                app_inventory::consolidation::ConsolidationLineInput::from_demand(
                    row.variant_id,
                    row.item_name,
                    row.outstanding,
                )
            })
            .collect(),
        ..ConsolidationInput::blank(today())
    })
}

/// Write a consolidation, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: ConsolidationInput,
) -> ServiceResult<Submission<ConsolidationInput>> {
    caller.require(permissions::CONSOLIDATIONS_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // A draft only. Read before anything is written: `save_lines` replaces the
    // lines, and editing a confirmed consolidation would rewrite the record of
    // what a set of already-sent orders was raised for.
    if let Some(id) = checked.id {
        let before = detail(pool, caller, id).await?;

        if !before.state.is_editable() {
            return Ok(Submission::rejected(
                "state",
                ConsolidationError::NotEditable.message(),
            ));
        }
    }

    let lines = match source_lines(pool, &checked).await? {
        Ok(lines) => lines,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => store::insert(&mut tx, &checked, caller.user_id()).await?,
        Some(id) => {
            if !store::update(&mut tx, id, &checked, caller.user_id()).await? {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "state",
                    ConsolidationError::NotEditable.message(),
                ));
            }
            id
        }
    };

    store::save_lines(&mut tx, id, &lines).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = ConsolidationInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::CONSOLIDATION, id).fact("lines", &lines.len().to_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Raise the orders.
///
/// See the module header for what this does and why it is one transaction.
pub async fn confirm(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Submission<Consolidation>> {
    caller.require(permissions::CONSOLIDATIONS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !before.state.is_editable() {
        return Ok(Submission::rejected(
            "state",
            ConsolidationError::NotEditable.message(),
        ));
    }
    if before.lines.is_empty() {
        return Ok(Submission::rejected(
            "lines",
            ConsolidationError::NoLines.message(),
        ));
    }
    if !before.unsourced().is_empty() {
        // Refused rather than partially confirmed. Raising orders for the
        // sourced lines and silently dropping the rest is how demand
        // disappears - a buyer would see a confirmed document and assume all
        // of it was bought.
        return Ok(Submission::rejected(
            "supplier_id",
            ConsolidationError::SupplierRequired.message(),
        ));
    }

    let base = base_currency(pool).await?;

    // Outside the transaction, for the reason `purchase::confirm` gives: every
    // numbered document queues through the sequence's one row, so anything that
    // can happen before that lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::CONSOLIDATION);
    let allocated = match generator.next(&mut tx, key, before.raised_on).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "number",
                msg!("consolidations.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    if !store::confirm(&mut tx, id, &allocated.number, caller.user_id()).await? {
        // Somebody confirmed it between the read and the write. Rolling back
        // returns the number rather than leaving a hole in the series - and,
        // more to the point, stops a second set of orders being raised.
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "state",
            ConsolidationError::NotEditable.message(),
        ));
    }

    let order_key = SequenceKey::new(app_inventory::APP_ID, app_inventory::PURCHASE_ORDER);

    for group in group_by_supplier(&before, base) {
        let currency = group.currency;

        let number = match generator.next(&mut tx, order_key, before.raised_on).await {
            Ok(allocated) => allocated.number,
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "number",
                    msg!("purchase_orders.error.no_series"),
                ));
            }
            Err(err) => return Err(err),
        };

        let mut costed = Vec::with_capacity(group.lines.len());
        let mut checked_lines = Vec::with_capacity(group.lines.len());

        for line in &group.lines {
            let Some(context) =
                phonix_db::inventory::movement::context(&mut *tx, line.variant_id, currency).await?
            else {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "lines",
                    ConsolidationError::ItemRequired.message(),
                ));
            };

            // The consolidation measures in the stock unit throughout - that
            // is what demand is summed in - so the order line is written in the
            // stock unit and there is no conversion to do.
            let unit_id = context.stock_unit_id;

            let unit_price = match line.unit_price {
                Some(price) if price.currency() == currency => price,
                // Priced in something else, or not priced at all: fall back to
                // what the workspace last paid, which is what the order form
                // does with a blank price.
                _ => Money::parse(currency, &context.cost.to_storage_string())
                    .unwrap_or_else(|_| Money::zero(currency)),
            };

            let net = match unit_price.scale_by(
                line.quantity.scaled(),
                app_inventory::quantity::SCALE_FACTOR,
                Rounding::HalfUp,
            ) {
                Ok(net) => net,
                Err(err) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected(
                        "unit_price",
                        ConsolidationError::Money(err).message(),
                    ));
                }
            };

            checked_lines.push(app_inventory::purchase::CheckedLine {
                id: None,
                variant_id: line.variant_id,
                description: line.description.clone(),
                quantity: line.quantity,
                unit_id,
                unit_price: unit_price.to_storage_string(),
                expected_on: None,
            });
            costed.push((unit_price, net));
        }

        let net_total = match Money::total(currency, costed.iter().map(|(_, net)| *net)) {
            Ok(total) => total,
            Err(err) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "unit_price",
                    ConsolidationError::Money(err).message(),
                ));
            }
        };

        let order = app_inventory::purchase::Checked {
            id: None,
            supplier_id: group.supplier.party_id,
            warehouse_id: before.warehouse_id,
            order_date: before.raised_on,
            expected_on: None,
            currency: currency.to_string(),
            // No cost centre on the order. A consolidated order serves several
            // departments by construction, and naming one of them here would be
            // a charge somebody could act on. Which department gets what is in
            // `purchase_order_line_sources`, per line, in the quantities that
            // were actually allocated.
            cost_centre_id: None,
            supplier_reference: None,
            note: before.note.clone(),
            lines: checked_lines,
        };

        let order_id = phonix_db::inventory::purchase::insert(
            &mut tx,
            &order,
            &group.supplier,
            net_total,
            caller.user_id(),
        )
        .await?;

        let order_lines: Vec<_> = order
            .lines
            .iter()
            .zip(&costed)
            .map(
                |(source, (unit_price, net))| phonix_db::inventory::purchase::CostedLine {
                    source,
                    description: source.description.clone(),
                    quantity_stock: source.quantity,
                    unit_price: *unit_price,
                    net: *net,
                    received: Quantity::ZERO,
                    billed: Quantity::ZERO,
                },
            )
            .collect();

        phonix_db::inventory::purchase::save_lines(&mut tx, order_id, &order_lines).await?;

        if !phonix_db::inventory::purchase::confirm(&mut tx, order_id, &number, caller.user_id())
            .await?
        {
            tx.rollback().await.map_err(DbError::Query)?;
            return Err(ServiceError::rejected(
                "state",
                msg!("consolidations.error.not_editable"),
            ));
        }

        store::attach_order(&mut tx, order_id, id).await?;

        // And the allocation, line by line, against what is outstanding NOW.
        let line_ids = store::order_line_ids(&mut tx, order_id).await?;

        for (order_line_id, line) in line_ids.iter().zip(&group.lines) {
            let mut left = line.quantity;

            let outstanding =
                store::outstanding_lines_for_update(&mut tx, line.variant_id, before.warehouse_id)
                    .await?;

            for (requisition_line_id, available) in outstanding {
                if !left.is_positive() {
                    break;
                }

                let taken = if left.compare(available).is_lt() {
                    left
                } else {
                    available
                };

                store::record_source(&mut tx, *order_line_id, requisition_line_id, id, taken)
                    .await?;

                phonix_db::inventory::requisition::advance_ordered(
                    &mut tx,
                    requisition_line_id,
                    taken,
                )
                .await?;

                left = left.checked_sub(taken).unwrap_or(Quantity::ZERO);
            }

            // Whatever is left over is the buyer's own decision and is charged
            // to nobody. Not an error - see the module header and `0008`.
        }
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::CONSOLIDATION, id)
            .named(&stored.number)
            .fact("orders", &stored.orders.len().to_string()),
        &before,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Abandon a draft, keeping the row.
pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::CONSOLIDATIONS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if !store::cancel(&mut tx, id, caller.user_id()).await? {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "state",
            ConsolidationError::NotEditable.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let after = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::CONSOLIDATION, id).named(&after.label()),
        &before,
        &after,
    )
    .await;

    Ok(Submission::Saved(()))
}

/// Throw a draft away. A draft only - a confirmed one has raised orders.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::CONSOLIDATIONS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !before.state.is_editable() {
        return Ok(false);
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::CONSOLIDATION, id).named(&before.label()),
            &before,
        )
        .await;
    }

    Ok(removed)
}

/// What each line of a purchase order was raised for.
///
/// Gated on the order permission rather than this app's own: it is a panel on
/// the order screen, and somebody who may read an order may read why it exists.
pub async fn allocation(
    pool: &PgPool,
    caller: &Caller,
    order_id: Uuid,
) -> ServiceResult<Vec<LineAllocation>> {
    caller.require(permissions::PURCHASE_ORDERS)?;
    Ok(store::allocations_of(pool, order_id).await?)
}

// ---------------------------------------------------------------------------
// Working out
// ---------------------------------------------------------------------------

/// One supplier's share of a consolidation: the order that is about to be
/// raised for them.
struct Group<'a> {
    supplier: SupplierSnapshot,
    currency: Currency,
    lines: Vec<&'a app_inventory::consolidation::ConsolidationLine>,
}

/// Split a consolidation into the orders it becomes.
///
/// In the order the suppliers first appear on the document, so a buyer reading
/// the confirmation sees the orders in the order they keyed the lines.
///
/// An order carries one currency, and the lines of one supplier may in
/// principle have been priced in different ones - the form offers the
/// supplier's own, but a supplier whose currency was changed between two edits
/// could leave a document holding both. The group takes the currency of its
/// first line and `confirm` re-prices anything that disagrees at the item's
/// cost, rather than raising two orders to one supplier or storing a price in a
/// currency the order does not name.
fn group_by_supplier(consolidation: &Consolidation, base: Currency) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();

    for line in &consolidation.lines {
        let Some(supplier) = &line.supplier else {
            continue;
        };

        let currency = line.unit_price.map_or(base, |price| price.currency());

        match groups
            .iter_mut()
            .find(|group| group.supplier.party_id == supplier.party_id)
        {
            Some(group) => group.lines.push(line),
            None => groups.push(Group {
                supplier: supplier.clone(),
                currency,
                lines: vec![line],
            }),
        }
    }

    groups
}

/// Look each line's supplier up and parse its price in that supplier's currency.
///
/// The two things `ConsolidationInput::check` could not do: a party is not the
/// domain's to read, and the currency a price is in follows from the party.
///
/// A line with no supplier yet is allowed through - that is an ordinary draft,
/// and the refusal happens at confirm where it means something.
async fn source_lines<'a>(
    pool: &PgPool,
    checked: &'a Checked,
) -> ServiceResult<Result<Vec<store::SourcedLine<'a>>, ConsolidationError>> {
    let base = base_currency(pool).await?;
    let mut sourced = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let (supplier, currency) = match line.supplier_id {
            None => (None, base),
            Some(party_id) => {
                let Some(party) = phonix_db::master::party::find(pool, party_id).await? else {
                    return Ok(Err(ConsolidationError::SupplierRequired));
                };

                if !party
                    .roles
                    .iter()
                    .any(|role| role.as_str() == phonix_master::party::roles::SUPPLIER)
                {
                    return Ok(Err(ConsolidationError::NotASupplier));
                }

                let currency = party.currency.unwrap_or(base);

                (
                    Some(SupplierSnapshot {
                        party_id: party.id,
                        code: party.code,
                        name: party.name,
                    }),
                    currency,
                )
            }
        };

        let unit_price = if line.unit_price.is_empty() {
            None
        } else {
            match Money::parse(currency, &line.unit_price) {
                Ok(price) => Some(price),
                Err(err) => return Ok(Err(ConsolidationError::Money(err))),
            }
        };

        sourced.push(store::SourcedLine {
            source: line,
            supplier,
            unit_price,
        });
    }

    Ok(Ok(sourced))
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
