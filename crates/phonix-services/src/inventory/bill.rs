//! Supplier bills: matching one, and posting what it owes.
//!
//! Posting clears goods-received-not-invoiced for the lines the bill covers,
//! books the difference to purchase price variance, and credits payables. It is
//! the only place in Inventory that names `AccountsPayable`, and it reaches it
//! through the `Ledger` port like everything else.

use app_inventory::bill::{
    Bill, BillError, BillInput, BillLineInput, BillState, BillSummary, CheckedBill, MatchGrade,
    Tolerance, UnbilledReceipt,
};
use app_inventory::quantity::Quantity;
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::bill::{self as store, CostedBillLine};
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::{AccountRole, JournalRequest, Ledger, LedgerError, Posting, Side};
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<BillSummary>> {
    caller.require(permissions::BILLS)?;
    Ok(store::list(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Bill> {
    caller.require(permissions::BILLS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("bill", msg!("bills.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<BillInput> {
    Ok(BillInput::from_bill(&detail(pool, caller, id).await?))
}

pub async fn blank(pool: &PgPool, caller: &Caller) -> ServiceResult<BillInput> {
    caller.require(permissions::BILLS_CREATE)?;

    let currency = base_currency(pool).await?;
    Ok(BillInput::blank(today(), currency.code()))
}

/// Goods received and not yet billed, oldest first. The aged GRNI balance.
pub async fn unbilled(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<UnbilledReceipt>> {
    caller.require(permissions::BILLS)?;

    let currency = base_currency(pool).await?;
    Ok(store::unbilled(pool, currency).await?)
}

/// A bill prefilled with everything an order has received and not been billed
/// for. The screen somebody wants with the supplier's invoice in front of them.
pub async fn against_order(
    pool: &PgPool,
    caller: &Caller,
    order_id: Uuid,
) -> ServiceResult<Submission<BillInput>> {
    caller.require(permissions::BILLS_CREATE)?;

    let order = crate::inventory::purchase::detail(pool, caller, order_id).await?;
    let currency = base_currency(pool).await?;

    let lines = store::billable_lines(pool, order_id, currency)
        .await?
        .into_iter()
        .map(|line| BillLineInput {
            id: None,
            receipt_line_id: Some(line.receipt_line_id),
            order_line_id: line.order_line_id,
            variant_id: Some(line.variant_id),
            description: line.description,
            quantity: line.outstanding.to_display_string(),
            unit_id: Some(line.purchase_unit_id),
            unit_price: line.unit_cost.to_storage_string(),
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return Ok(Submission::rejected(
            "order_id",
            msg!("bills.error.nothing_to_bill"),
        ));
    }

    Ok(Submission::Saved(BillInput {
        id: None,
        order_id: Some(order_id),
        supplier_id: Some(order.supplier.party_id),
        supplier_reference: String::new(),
        bill_date: today(),
        due_on: None,
        currency: currency.code().to_owned(),
        note: String::new(),
        lines,
    }))
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: BillInput,
) -> ServiceResult<Submission<BillInput>> {
    caller.require(match draft.id {
        None => permissions::BILLS_CREATE,
        Some(_) => permissions::BILLS_EDIT,
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
                BillError::CurrencyRequired.message(),
            ));
        }
    };

    let supplier =
        match crate::inventory::purchase::supplier_snapshot(pool, checked.supplier_id).await? {
            Ok(supplier) => supplier,
            Err(_) => {
                return Ok(Submission::rejected(
                    "supplier_id",
                    BillError::NotASupplier.message(),
                ));
            }
        };

    if let Some(id) = checked.id {
        let before = detail(pool, caller, id).await?;
        if !before.state.is_editable() {
            return Ok(Submission::rejected("state", BillError::NotEditable.message()));
        }
    }

    let costed = match cost_lines(pool, &checked, currency).await? {
        Ok(costed) => costed,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let net = match Money::total(currency, costed.iter().map(|line| line.net)) {
        Ok(net) => net,
        Err(err) => return Ok(Submission::rejected("unit_price", err.message())),
    };
    let accrued = match Money::total(currency, costed.iter().map(|line| line.accrued)) {
        Ok(accrued) => accrued,
        Err(err) => return Ok(Submission::rejected("unit_price", err.message())),
    };
    let variance = net
        .checked_sub(accrued)
        .unwrap_or_else(|_| Money::zero(currency));

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => {
            match store::insert(
                &mut tx,
                &checked,
                &supplier,
                net,
                accrued,
                variance,
                caller.user_id(),
            )
            .await
            {
                Ok(id) => id,
                // The unique index on (supplier, reference) is the duplicate
                // guard; this is where it becomes a sentence.
                Err(DbError::Query(sqlx::Error::Database(err)))
                    if err.constraint() == Some("bills_supplier_reference") =>
                {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected(
                        "supplier_reference",
                        BillError::DuplicateReference.message(),
                    ));
                }
                Err(err) => return Err(err.into()),
            }
        }
        Some(id) => {
            match store::update(
                &mut tx,
                id,
                &checked,
                &supplier,
                net,
                accrued,
                variance,
                caller.user_id(),
            )
            .await
            {
                Ok(true) => id,
                Ok(false) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected("state", BillError::NotEditable.message()));
                }
                Err(DbError::Query(sqlx::Error::Database(err)))
                    if err.constraint() == Some("bills_supplier_reference") =>
                {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected(
                        "supplier_reference",
                        BillError::DuplicateReference.message(),
                    ));
                }
                Err(err) => return Err(err.into()),
            }
        }
    };

    store::save_lines(&mut tx, id, &costed).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = BillInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::BILL, id)
        .named(&supplier.name)
        .fact("net", &net.to_display_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// How this bill reads against the order and the receipts behind it.
///
/// Read by the screen before anybody presses post, so the reason box appears
/// with the reason for it beside it rather than after a refusal.
pub async fn grade(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<MatchGrade> {
    caller.require(permissions::BILLS)?;

    let bill = detail(pool, caller, id).await?;
    let currency = base_currency(pool).await?;

    Ok(assess(pool, &bill, currency).await?)
}

async fn assess(pool: &PgPool, bill: &Bill, currency: Currency) -> ServiceResult<MatchGrade> {
    let Some(order_id) = bill.order_id else {
        return Ok(MatchGrade::NoOrder);
    };

    let mut confirmed_at = None;
    let mut confirmed_by = None;

    if let Some((at, by)) = store::order_provenance(pool, order_id).await? {
        confirmed_at = at;
        confirmed_by = by;
    }

    for line in &bill.lines {
        let Some(receipt_line_id) = line.receipt_line_id else {
            // A charge line needs no receipt; a goods line without one does.
            if line.is_goods() {
                return Ok(MatchGrade::NoReceipt);
            }
            continue;
        };

        let Some(state) = store::receipt_line_state(pool, receipt_line_id, currency).await? else {
            return Ok(MatchGrade::NoReceipt);
        };

        if state.order_id != Some(order_id) {
            return Ok(MatchGrade::NoReceipt);
        }

        // The order confirmed after the goods landed, or after the supplier
        // wrote the invoice, is one that agrees by construction.
        if let (Some(confirmed), Some(posted)) = (confirmed_at, state.posted_at) {
            if confirmed > posted {
                return Ok(MatchGrade::Circular);
            }
        }
        if let Some(confirmed) = confirmed_at {
            if confirmed.date() > bill.bill_date {
                return Ok(MatchGrade::Circular);
            }
        }

        if confirmed_by.is_some() && confirmed_by == state.posted_by {
            return Ok(MatchGrade::SameHand);
        }

        let available = state
            .quantity
            .checked_sub(state.billed)
            .unwrap_or(Quantity::ZERO);

        if line.quantity.compare(available).is_gt() {
            return Ok(MatchGrade::OverReceived);
        }
    }

    if bill.variance.is_zero() {
        return Ok(MatchGrade::Clean);
    }

    let tolerance = Tolerance::default_for(currency);

    if tolerance.accepts(bill.accrued, bill.variance) {
        Ok(MatchGrade::WithinTolerance)
    } else {
        Ok(MatchGrade::OverTolerance)
    }
}

/// Post a bill: clear GRNI, book the variance, credit payables.
pub async fn post(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    id: Uuid,
    match_note: Option<String>,
) -> ServiceResult<Submission<Bill>> {
    caller.require(permissions::BILLS_POST)?;
    acting_user(caller)?;

    let bill = detail(pool, caller, id).await?;

    if !bill.state.is_editable() {
        return Ok(Submission::rejected("state", BillError::NotEditable.message()));
    }
    if !bill.has_lines() {
        return Ok(Submission::rejected("lines", BillError::NoLines.message()));
    }

    let currency = base_currency(pool).await?;
    let grade = assess(pool, &bill, currency).await?;

    let reason = match_note
        .map(|note| note.trim().to_owned())
        .filter(|note| !note.is_empty());

    if grade.needs_override() {
        // A grade that does not clear needs both the permission and a sentence.
        // Neither substitutes for the other: the permission says who may, the
        // sentence says why, and the sentence is what the next person reads.
        caller.require(permissions::BILLS_OVERRIDE)?;

        if reason.is_none() {
            return Ok(Submission::rejected(
                "match_note",
                BillError::OverrideRequired.message(),
            ));
        }
    }

    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::BILL);
    let allocated = match generator.next(&mut tx, key, bill.bill_date).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected("number", msg!("bills.error.no_series")));
        }
        Err(err) => return Err(err),
    };

    if !store::post(
        &mut tx,
        id,
        &allocated.number,
        reason.as_deref(),
        caller.user_id(),
    )
    .await?
    {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected("state", BillError::NotEditable.message()));
    }

    // What each receipt line has now been charged for. Inside the same
    // transaction as the post, because a bill that posted without advancing
    // these would let the same goods be billed again.
    for line in &bill.lines {
        if let Some(receipt_line_id) = line.receipt_line_id {
            store::advance_billed(&mut tx, receipt_line_id, line.quantity).await?;
        }
    }

    tx.commit().await.map_err(DbError::Query)?;

    let outcome = post_journal(ledger, &bill, &allocated.number, currency).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    match outcome {
        Some(journal_id) => store::record_journal(&mut tx, id, Some(journal_id), "posted").await?,
        None => store::record_journal(&mut tx, id, None, "no_ledger").await?,
    }
    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::BILL, id)
            .named(&stored.number)
            .fact("supplier", &stored.supplier.name)
            .fact("net", &stored.net.to_display_string())
            .fact("match", grade.as_str()),
        &bill,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// DR goods received not invoiced, DR/CR purchase price variance, CR payables.
///
/// `None` where the workspace has no ledger: the bill is still what the
/// supplier charged, and a warehouse does not stop for the accounting module.
async fn post_journal(
    ledger: &dyn Ledger,
    bill: &Bill,
    number: &str,
    currency: Currency,
) -> ServiceResult<Option<Uuid>> {
    let mut postings = vec![Posting {
        role: AccountRole::GoodsReceivedNotInvoiced,
        account_id: None,
        side: Side::Debit,
        amount: bill.accrued.abs().to_storage_string(),
        memo: Some(bill.supplier_reference.clone()),
        cost_centre_id: None,
    }];

    if !bill.variance.is_zero() {
        postings.push(Posting {
            role: AccountRole::PurchasePriceVariance,
            account_id: None,
            // Charged more than accrued is a debit: the extra is a cost, not a
            // higher stock value. ADR 0006 section 6.1.
            side: if bill.is_overcharge() {
                Side::Debit
            } else {
                Side::Credit
            },
            amount: bill.variance.abs().to_storage_string(),
            memo: Some(bill.supplier_reference.clone()),
            cost_centre_id: None,
        });
    }

    postings.push(Posting {
        role: AccountRole::AccountsPayable,
        account_id: None,
        side: Side::Credit,
        amount: bill.net.abs().to_storage_string(),
        memo: Some(bill.supplier_reference.clone()),
        cost_centre_id: None,
    });

    let entry = JournalRequest {
        entry_date: bill.bill_date,
        narration: format!("{} · {}", bill.supplier.name, bill.supplier_reference),
        source_app: app_inventory::APP_ID.to_owned(),
        source_doc_type: app_inventory::BILL.to_owned(),
        source_doc_id: bill.id,
        currency: currency.code().to_owned(),
        postings,
    };

    let _ = number;

    match ledger.post(entry).await {
        Ok(posted) => Ok(Some(posted.journal_id)),
        Err(LedgerError::NoLedger) => Ok(None),
        Err(err) => Err(crate::inventory::stock::refused(err)),
    }
}

pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::BILLS_CREATE)?;

    let bill = detail(pool, caller, id).await?;
    if !bill.state.is_editable() {
        return Ok(Submission::rejected("state", BillError::NotEditable.message()));
    }

    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    if !store::cancel(&mut conn, id).await? {
        return Ok(Submission::rejected("state", BillError::NotEditable.message()));
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::BILL, id).named(&bill.label()),
        &bill.state,
        &BillState::Cancelled,
    )
    .await;

    Ok(Submission::Saved(()))
}

pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::BILLS_CREATE)?;

    let bill = detail(pool, caller, id).await?;
    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    let gone = store::delete(&mut conn, id).await?;

    if gone {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::BILL, id).named(&bill.label()),
            &bill,
        )
        .await;
    }

    Ok(gone)
}

/// What each line costs, and what the receipt behind it accrued.
async fn cost_lines<'a>(
    pool: &PgPool,
    checked: &'a CheckedBill,
    currency: Currency,
) -> ServiceResult<Result<Vec<CostedBillLine<'a>>, BillError>> {
    let mut costed = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let unit_price = match Money::parse(currency, &line.unit_price) {
            Ok(price) => price,
            Err(err) => return Ok(Err(BillError::Money(err))),
        };

        let net = match unit_price.scale_by(
            line.quantity.scaled(),
            app_inventory::quantity::SCALE_FACTOR,
            Rounding::HalfUp,
        ) {
            Ok(net) => net,
            Err(err) => return Ok(Err(BillError::Money(err))),
        };

        // What the receipt put into GRNI for this quantity. A charge line
        // accrued nothing, so it is all variance - which is correct: freight
        // nobody accrued is a cost that arrives with the invoice.
        let accrued = match line.receipt_line_id {
            None => Money::zero(currency),
            Some(receipt_line_id) => {
                let Some(state) = store::receipt_line_state(pool, receipt_line_id, currency).await?
                else {
                    return Ok(Err(BillError::AlreadyBilled));
                };

                let available = state
                    .quantity
                    .checked_sub(state.billed)
                    .unwrap_or(Quantity::ZERO);

                if !available.is_positive() {
                    return Ok(Err(BillError::AlreadyBilled));
                }

                match state.unit_cost.scale_by(
                    line.quantity.scaled(),
                    app_inventory::quantity::SCALE_FACTOR,
                    Rounding::HalfUp,
                ) {
                    Ok(accrued) => accrued,
                    Err(err) => return Ok(Err(BillError::Money(err))),
                }
            }
        };

        costed.push(CostedBillLine {
            source: line,
            description: line.description.clone(),
            unit_price,
            net,
            accrued,
        });
    }

    Ok(Ok(costed))
}
