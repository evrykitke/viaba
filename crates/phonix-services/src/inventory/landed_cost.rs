//! Landed costs: freight, duty and handling spread over what carried them.
//!
//! ADR 0006 section 6.2. The document is not the carrier's invoice - that is an
//! ordinary bill against the carrier - it is the decision about which cartons
//! the invoice belongs to, and what that does to what they are worth.
//!
//! # Posting is one transaction, and the ledger is the last thing in it
//!
//! The layers are read under a lock, spread, written, and only then does the
//! journal go. A ledger that refuses - a closed period, an unmapped role -
//! takes the layer changes back with it, on the same terms as a stock movement
//! and for the same reason: a workspace with a ledger never has a stock figure
//! its stock account disagrees with.
//!
//! # Capitalised is not all of it
//!
//! Freight arriving six weeks after the goods is partly freight on goods that
//! have been sold. The share belonging to units still on the shelf raises the
//! layer; the rest is a cost of sales that was understated when it went out and
//! is corrected now. Capitalising the whole of it would put value on stock that
//! is not there.

use std::collections::HashMap;

use app_inventory::accounts::{AccountOverrides, account_for};
use app_inventory::landed_cost::{
    LandedCost, LandedCostError, LandedCostInput, LandedCostState, LandedCostSummary, Landable,
    ReceiptLandedCost, Share, Spread, spread,
};
use app_inventory::movement::JournalOutcome;
use app_inventory::valuation;
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::landed_cost::{self as store, PricedCharge, VariantCosting};
use phonix_db::inventory::{account_mapping, item as item_store, quant as quant_store};
use phonix_db::inventory::valuation as layer_store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::{PgPool, Postgres, Transaction};
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

fn reject<T>(err: LandedCostError) -> Submission<T> {
    Submission::rejected(err.field(), err.message())
}

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<LandedCostSummary>> {
    caller.require(permissions::LANDED_COSTS)?;

    let currency = base_currency(pool).await?;
    Ok(store::list(pool, currency).await?)
}

/// What has been landed on one delivery. The panel on the receipt screen.
pub async fn for_receipt(
    pool: &PgPool,
    caller: &Caller,
    receipt_id: Uuid,
) -> ServiceResult<Vec<LandedCostSummary>> {
    caller.require(permissions::LANDED_COSTS)?;

    let currency = base_currency(pool).await?;
    Ok(store::for_receipt(pool, receipt_id, currency).await?)
}

/// What one delivery has been landed with in total. `None` where nothing has.
///
/// Gated on the receipt rather than on landed costs: the figure is part of what
/// the delivery is worth, and somebody who may see a receipt may see what it
/// ended up costing.
pub async fn landed_on_receipt(
    pool: &PgPool,
    caller: &Caller,
    receipt_id: Uuid,
) -> ServiceResult<Option<ReceiptLandedCost>> {
    caller.require(permissions::RECEIPTS)?;

    let currency = base_currency(pool).await?;
    Ok(store::landed_on_receipt(pool, receipt_id, currency).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<LandedCost> {
    caller.require(permissions::LANDED_COSTS)?;

    let currency = base_currency(pool).await?;

    let mut document = store::find(pool, id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("landed_cost", msg!("landed_costs.gone")))?;

    document.charges = store::charges_of(pool, id, currency).await?;
    document.allocations = store::allocations_of(pool, id, currency).await?;

    Ok(document)
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<LandedCostInput> {
    Ok(LandedCostInput::from_document(
        &detail(pool, caller, id).await?,
    ))
}

pub async fn blank(_pool: &PgPool, caller: &Caller) -> ServiceResult<LandedCostInput> {
    caller.require(permissions::LANDED_COSTS_CREATE)?;

    Ok(LandedCostInput::blank(today()))
}

/// A landed cost against one delivery, which is how this screen is reached from
/// the receipt.
pub async fn against(
    pool: &PgPool,
    caller: &Caller,
    receipt_id: Uuid,
) -> ServiceResult<Submission<LandedCostInput>> {
    caller.require(permissions::LANDED_COSTS_CREATE)?;

    let Some(facts) = store::receipt_facts(pool, receipt_id).await? else {
        return Ok(Submission::rejected("receipt_id", msg!("receipts.gone")));
    };

    if !facts.is_posted {
        return Ok(reject(LandedCostError::ReceiptNotPosted));
    }

    Ok(Submission::Saved(LandedCostInput::against(
        receipt_id,
        today(),
    )))
}

/// What this delivery holds that a cost can be spread over, priced.
///
/// Read by the screen so somebody keying freight sees the cartons it will land
/// on before they press post rather than after.
pub async fn landables(
    pool: &PgPool,
    caller: &Caller,
    receipt_id: Uuid,
) -> ServiceResult<Vec<Landable>> {
    caller.require(permissions::LANDED_COSTS)?;

    let currency = base_currency(pool).await?;
    let mut conn = pool.acquire().await.map_err(DbError::Query)?;

    Ok(store::landables_for_update(&mut conn, receipt_id, currency).await?)
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: LandedCostInput,
) -> ServiceResult<Submission<LandedCostInput>> {
    caller.require(permissions::LANDED_COSTS_CREATE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(reject(err)),
    };

    let currency = base_currency(pool).await?;

    let Some(facts) = store::receipt_facts(pool, checked.receipt_id).await? else {
        return Ok(Submission::rejected("receipt_id", msg!("receipts.gone")));
    };

    if !facts.is_posted {
        return Ok(reject(LandedCostError::ReceiptNotPosted));
    }

    let mut priced = Vec::with_capacity(checked.charges.len());
    for charge in &checked.charges {
        match Money::parse(currency, &charge.amount) {
            Ok(amount) => priced.push(PricedCharge { checked: charge, amount }),
            Err(err) => return Ok(reject(LandedCostError::Money(err))),
        }
    }

    if let Some(id) = checked.id {
        let before = detail(pool, caller, id).await?;
        if !before.state.is_editable() {
            return Ok(reject(LandedCostError::NotEditable));
        }
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => {
            store::insert(
                &mut tx,
                checked.receipt_id,
                &facts,
                checked.cost_date,
                checked.note.as_deref(),
                caller.user_id(),
            )
            .await?
        }
        Some(id) => {
            if !store::update(
                &mut tx,
                id,
                checked.receipt_id,
                &facts,
                checked.cost_date,
                checked.note.as_deref(),
                caller.user_id(),
            )
            .await?
            {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(reject(LandedCostError::NotEditable));
            }
            id
        }
    };

    store::save_charges(&mut tx, id, &priced).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = LandedCostInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::LANDED_COST, id)
        .named(&facts.number)
        .fact("supplier", &facts.supplier_name);

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Post: spread the charges, raise what is still on the shelf, charge the rest
/// to cost of sales, and tell the ledger.
pub async fn post(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    id: Uuid,
) -> ServiceResult<Submission<LandedCost>> {
    caller.require(permissions::LANDED_COSTS_POST)?;
    acting_user(caller)?;

    let document = detail(pool, caller, id).await?;

    if !document.state.is_editable() {
        return Ok(reject(LandedCostError::NotEditable));
    }
    if !document.has_charges() {
        return Ok(reject(LandedCostError::NothingToSpread));
    }

    let currency = base_currency(pool).await?;

    let Some(facts) = store::receipt_facts(pool, document.receipt_id).await? else {
        return Ok(Submission::rejected("receipt_id", msg!("receipts.gone")));
    };
    if !facts.is_posted {
        return Ok(reject(LandedCostError::ReceiptNotPosted));
    }

    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let lines = store::landables_for_update(&mut tx, document.receipt_id, currency).await?;

    let spread = match spread(&document.charges, &lines, currency) {
        Ok(spread) => spread,
        Err(err) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(reject(err));
        }
    };

    let key = SequenceKey::new(app_inventory::APP_ID, app_inventory::LANDED_COST);
    let allocated = match generator.next(&mut tx, key, document.cost_date).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "number",
                msg!("landed_costs.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    if !store::post(
        &mut tx,
        id,
        &allocated.number,
        spread.total,
        spread.capitalised,
        spread.expensed,
        caller.user_id(),
    )
    .await?
    {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(reject(LandedCostError::NotEditable));
    }

    store::record_allocations(&mut tx, id, &spread.shares).await?;

    for share in &spread.shares {
        if !share.capitalised.is_zero() {
            layer_store::add_landed_value(&mut tx, share.layer_id, share.capitalised).await?;
        }
    }

    let costing = costing_of(&mut tx, &spread.shares).await?;
    rebase_averages(&mut tx, &spread.shares, &costing, currency).await?;

    let outcome = post_journal(
        &mut tx,
        ledger,
        &document,
        &facts.supplier_name,
        &spread,
        &costing,
        currency,
    )
    .await?;

    store::record_journal(&mut tx, id, &outcome).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = detail(pool, caller, id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::LANDED_COST, id)
            .named(&stored.number)
            .fact("receipt", &stored.receipt_number)
            .fact("capitalised", &stored.capitalised.to_display_string())
            .fact("expensed", &stored.expensed.to_display_string())
            .fact("journal", outcome.as_str()),
        &document,
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

pub async fn cancel(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::LANDED_COSTS_CREATE)?;

    let document = detail(pool, caller, id).await?;
    if !document.state.is_editable() {
        return Ok(reject(LandedCostError::NotEditable));
    }

    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    if !store::cancel(&mut conn, id, caller.user_id()).await? {
        return Ok(reject(LandedCostError::NotEditable));
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::LANDED_COST, id).named(&document.label()),
        &document.state,
        &LandedCostState::Cancelled,
    )
    .await;

    Ok(Submission::Saved(()))
}

pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<bool> {
    caller.require(permissions::LANDED_COSTS_CREATE)?;

    let document = detail(pool, caller, id).await?;
    let mut conn = pool.acquire().await.map_err(DbError::Query)?;
    let gone = store::delete(&mut conn, id).await?;

    if gone {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::LANDED_COST, id).named(&document.label()),
            &document,
        )
        .await;
    }

    Ok(gone)
}

// --- Costing --------------------------------------------------------------

async fn costing_of(
    tx: &mut Transaction<'_, Postgres>,
    shares: &[Share],
) -> ServiceResult<HashMap<Uuid, VariantCosting>> {
    let mut variant_ids: Vec<Uuid> = shares.iter().map(|share| share.variant_id).collect();
    variant_ids.sort_unstable();
    variant_ids.dedup();

    let facts = store::costing_facts(&mut **tx, &variant_ids).await?;

    Ok(facts
        .into_iter()
        .map(|fact| (fact.variant_id, fact))
        .collect())
}

/// Move the running average of every averaged item this landed on.
///
/// Under FIFO the layer's `additional_value` is the whole answer. Under average
/// the item's own `cost` is what a valuation asks for, and freight that never
/// reached it would sit in the stock account with nothing in the stock ledger
/// to match it.
async fn rebase_averages(
    tx: &mut Transaction<'_, Postgres>,
    shares: &[Share],
    costing: &HashMap<Uuid, VariantCosting>,
    currency: Currency,
) -> ServiceResult<()> {
    let mut by_item: HashMap<Uuid, Money> = HashMap::new();

    for share in shares {
        let Some(fact) = costing.get(&share.variant_id) else {
            continue;
        };
        if !fact.is_averaged || share.capitalised.is_zero() {
            continue;
        }

        let running = by_item
            .entry(fact.item_id)
            .or_insert_with(|| Money::zero(currency));
        *running = running
            .checked_add(share.capitalised)
            .map_err(|err| ServiceError::rejected("charges", err.message()))?;
    }

    for (item_id, landed) in by_item {
        let on_hand = quant_store::item_on_hand(&mut **tx, item_id).await?;
        if !on_hand.is_positive() {
            continue;
        }

        let cost = item_store::cost_of(&mut **tx, item_id, currency)
            .await?
            .unwrap_or_else(|| Money::zero(currency));

        let worth = valuation::value_of(on_hand, cost)
            .and_then(|value| Ok(value.checked_add(landed)?))
            .and_then(|value| valuation::unit_cost_of(on_hand, value))
            .map_err(|err| ServiceError::rejected("charges", err.message()))?;

        item_store::set_cost(&mut **tx, item_id, worth).await?;
    }

    Ok(())
}

// --- The ledger -----------------------------------------------------------

/// DR stock, DR cost of sales, CR landed cost absorbed.
///
/// The stock and cost-of-sales legs are split by the account each item resolves
/// to, so a workspace whose categories point at different stock accounts gets a
/// journal that reconciles against each of them - ADR 0006 section 6.7. The
/// credit is one line: landed cost is workspace-wide policy and carries no item
/// override, which is what `AccountOverrides::for_role` says.
async fn post_journal(
    tx: &mut Transaction<'_, Postgres>,
    ledger: &dyn Ledger,
    document: &LandedCost,
    supplier_name: &str,
    spread: &Spread,
    costing: &HashMap<Uuid, VariantCosting>,
    currency: Currency,
) -> ServiceResult<JournalOutcome> {
    if spread.total.is_zero() && spread.capitalised.is_zero() && spread.expensed.is_zero() {
        return Ok(JournalOutcome::NotRequired);
    }

    let mut overrides: HashMap<Uuid, (AccountOverrides, AccountOverrides)> = HashMap::new();

    for fact in costing.values() {
        if overrides.contains_key(&fact.item_id) {
            continue;
        }

        let item =
            account_mapping::for_owner(&mut **tx, account_mapping::Owner::Item, fact.item_id)
                .await?;
        let category = account_mapping::for_owner(
            &mut **tx,
            account_mapping::Owner::Category,
            fact.category_id,
        )
        .await?;

        overrides.insert(fact.item_id, (item, category));
    }

    let mut stock: HashMap<Option<Uuid>, Money> = HashMap::new();
    let mut sold: HashMap<Option<Uuid>, Money> = HashMap::new();

    for share in &spread.shares {
        let accounts = costing
            .get(&share.variant_id)
            .and_then(|fact| overrides.get(&fact.item_id));

        let (item, category) = match accounts {
            Some((item, category)) => (item, category),
            None => continue,
        };

        accumulate(
            &mut stock,
            account_for(AccountRole::Inventory, item, category),
            share.capitalised,
            currency,
        )?;
        accumulate(
            &mut sold,
            account_for(AccountRole::CostOfSales, item, category),
            share.expensed,
            currency,
        )?;
    }

    let memo = Some(format!("{} · {}", supplier_name, document.receipt_number));
    let mut postings = Vec::new();

    for (account_id, amount) in stock {
        push_leg(
            &mut postings,
            AccountRole::Inventory,
            account_id,
            amount,
            &memo,
        );
    }
    for (account_id, amount) in sold {
        push_leg(
            &mut postings,
            AccountRole::CostOfSales,
            account_id,
            amount,
            &memo,
        );
    }

    // The credit is the whole charge, whichever way the debits split.
    if !spread.total.is_zero() {
        postings.push(Posting {
            role: AccountRole::LandedCost,
            account_id: None,
            side: if spread.total.is_negative() {
                Side::Debit
            } else {
                Side::Credit
            },
            amount: spread.total.abs().to_storage_string(),
            memo: memo.clone(),
            cost_centre_id: None,
        });
    }

    if postings.is_empty() {
        return Ok(JournalOutcome::NotRequired);
    }

    let entry = JournalRequest {
        entry_date: document.cost_date,
        narration: format!("{supplier_name} · {}", document.receipt_number),
        source_app: app_inventory::APP_ID.to_owned(),
        source_doc_type: app_inventory::LANDED_COST.to_owned(),
        source_doc_id: document.id,
        currency: currency.code().to_owned(),
        postings,
    };

    match ledger.post(entry).await {
        Ok(posted) => Ok(JournalOutcome::Posted {
            journal_id: posted.journal_id,
            number: posted.number,
        }),
        Err(LedgerError::NoLedger) => Ok(JournalOutcome::NoLedger),
        Err(err) => Err(crate::inventory::stock::refused(err)),
    }
}

fn accumulate(
    into: &mut HashMap<Option<Uuid>, Money>,
    account_id: Option<Uuid>,
    amount: Money,
    currency: Currency,
) -> ServiceResult<()> {
    if amount.is_zero() {
        return Ok(());
    }

    let running = into.entry(account_id).or_insert_with(|| Money::zero(currency));
    *running = running
        .checked_add(amount)
        .map_err(|err| ServiceError::rejected("charges", err.message()))?;

    Ok(())
}

/// A negative charge is a credit note for freight, and it posts the other way
/// round rather than as a negative debit.
fn push_leg(
    postings: &mut Vec<Posting>,
    role: AccountRole,
    account_id: Option<Uuid>,
    amount: Money,
    memo: &Option<String>,
) {
    if amount.is_zero() {
        return;
    }

    postings.push(Posting {
        role,
        account_id,
        side: if amount.is_negative() {
            Side::Credit
        } else {
            Side::Debit
        },
        amount: amount.abs().to_storage_string(),
        memo: memo.clone(),
        cost_centre_id: None,
    });
}
