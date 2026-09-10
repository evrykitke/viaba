//! Moving stock, and everything that follows from it.
//!
//! # One function does all of it
//!
//! [`apply`] is the only way a quantity in this workspace changes. A receipt, a
//! delivery, a transfer, a write-off and a count difference are all one call
//! with different locations on it, because that is what they are - ADR 0006
//! section 7. Every document built on top of this hands over a
//! [`MoveRequest`] and gets back a move that has already been costed, quanted
//! and posted, or gets back nothing at all.
//!
//! # All of it, or none of it
//!
//! One transaction covers the move, both quants, the valuation layers and the
//! journal. There is no state in which the shelf changed and the stock account
//! did not, which is the whole of ADR 0006 section 6.1 in one sentence.
//!
//! # What "no ledger" means, and what it does not
//!
//! A workspace that never bought the accounting module still receives goods:
//! [`LedgerError::NoLedger`] leaves the movement standing and records
//! [`JournalOutcome::NoLedger`] against it. Every *other* answer from the ledger
//! - a closed period, an unmapped role, an account somebody retired - rolls the
//! whole thing back and is shown to the person who tried. The distinction is
//! not a nicety: the first is a workspace that does not keep books, and the
//! second is a workspace whose books would be wrong.
//!
//! # Where the cost comes from
//!
//! The category's costing method decides. FIFO reads the layers and consumes
//! them oldest first; standard and average take the one number the item
//! carries. Under average, a receipt then rewrites that number - and only a
//! receipt does, because recomputing an average on the way out would let the
//! order two pickers happened to work in change what the month cost.

use app_inventory::accounts::AccountOverrides;
use app_inventory::category::CostingMethod;
use app_inventory::location::{Location, LocationKind};
use app_inventory::lot::{self, Lot};
use app_inventory::movement::{
    JournalOutcome, MoveContext, MoveError, MoveFilter, MoveRequest, MoveState, MoveSummary,
    StockMove, check_ends, posting_roles,
};
use app_inventory::quant::{self, OnHandFilter, OnHandRow};
use app_inventory::quantity::Quantity;
use app_inventory::valuation;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::{
    account_mapping, item as item_store, lot as lot_store, movement as store, quant as quant_store,
    valuation as layer_store,
};
use phonix_db::sqlx::{PgPool, Postgres, Transaction};
use phonix_ports::ledger::{JournalRequest, Ledger, LedgerError, Posting, Side};
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::Caller;
use crate::error::{ServiceError, ServiceResult};

/// How many movements a grid asks for at once.
const PAGE: i64 = 500;

/// What is on hand, wherever it is.
pub async fn on_hand(
    pool: &PgPool,
    caller: &Caller,
    filter: OnHandFilter,
) -> ServiceResult<Vec<OnHandRow>> {
    caller.require(permissions::STOCK)?;
    let currency = base_currency(pool).await?;

    Ok(quant_store::on_hand(pool, &filter, currency).await?)
}

/// The movement history, newest first.
pub async fn moves(
    pool: &PgPool,
    caller: &Caller,
    filter: MoveFilter,
) -> ServiceResult<Vec<MoveSummary>> {
    caller.require(permissions::STOCK)?;
    let currency = base_currency(pool).await?;

    Ok(store::list(pool, &filter, currency, PAGE).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<StockMove> {
    caller.require(permissions::STOCK)?;
    let currency = base_currency(pool).await?;

    store::find(pool, id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("move", msg!("moves.gone")))
}

/// The lots of one variant, in the order a pick reaches for them.
pub async fn lots_of(
    pool: &PgPool,
    caller: &Caller,
    variant_id: Uuid,
) -> ServiceResult<Vec<app_inventory::lot::LotSummary>> {
    caller.require(permissions::STOCK)?;
    Ok(lot_store::for_variant(pool, variant_id).await?)
}

/// What the workspace holds in stock, at cost.
///
/// The figure a stock account is reconciled against. One query, because ADR
/// 0006 section 6.7 is about not having to do this by hand.
pub async fn total_value(pool: &PgPool, caller: &Caller) -> ServiceResult<Money> {
    caller.require(permissions::STOCK)?;
    let currency = base_currency(pool).await?;

    Ok(layer_store::total_value(pool, currency).await?)
}

/// Move stock, cost it, and post what follows.
///
/// The one way a quantity changes. Ungated: whoever is calling has already
/// asked whether this person may receive goods or adjust a count, and a second
/// permission here would mean every document's permission implied a movement
/// permission that nothing else ever granted.
pub async fn apply(
    pool: &PgPool,
    caller: &Caller,
    ledger: &dyn Ledger,
    request: MoveRequest,
) -> ServiceResult<Submission<StockMove>> {
    let currency = base_currency(pool).await?;

    let checked = match request.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let Some(context) = store::context(pool, checked.variant_id, currency).await? else {
        return Ok(Submission::rejected(
            "variant_id",
            msg!("items.gone"),
        ));
    };

    if !context.holds_stock() {
        return Ok(reject(MoveError::ItemHoldsNoStock));
    }

    let from = load_location(pool, checked.from_location_id).await?;
    let to = load_location(pool, checked.to_location_id).await?;

    if !from.is_active || !to.is_active {
        return Ok(reject(MoveError::LocationInactive));
    }

    if let Err(err) = check_ends(from.kind, to.kind) {
        return Ok(reject(err));
    }

    let lot = match checked.lot_id {
        None => None,
        Some(id) => Some(
            lot_store::find(pool, id)
                .await?
                .ok_or_else(|| ServiceError::rejected("lot_id", msg!("lots.gone")))?,
        ),
    };

    if let Err(err) = lot::check_on_move(
        context.tracking,
        lot.as_ref(),
        context.variant_id,
        checked.quantity,
    ) {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let costed = match cost(&mut tx, &context, &from, &to, &checked, currency).await? {
        Ok(costed) => costed,
        Err(err) => return Ok(err),
    };

    let draft = store::NewMove {
        variant_id: context.variant_id,
        from_location_id: from.id,
        to_location_id: to.id,
        lot_id: lot.as_ref().map(|lot| lot.id),
        quantity: checked.quantity,
        unit_id: context.stock_unit_id,
        state: MoveState::Done,
        moved_on: checked.moved_on,
        unit_cost: costed.unit_cost,
        value: costed.value,
        reference: checked.reference.as_deref(),
        source: checked.source.as_ref(),
        adjustment_type_id: checked.adjustment_type_id,
    };

    let move_id = store::insert(&mut tx, &draft, caller.user_id()).await?;

    if let Err(err) = shift(&mut tx, &context, &from, &to, &checked, lot.as_ref()).await? {
        return Ok(err);
    }

    settle(&mut tx, &context, &costed, move_id, lot.as_ref(), currency).await?;

    let outcome = post(
        &mut tx,
        ledger,
        &context,
        &from,
        &to,
        &checked,
        &costed,
        move_id,
        currency,
    )
    .await?;

    store::record_journal(&mut tx, move_id, &outcome).await?;

    tx.commit().await.map_err(DbError::Query)?;

    let stored = store::find(pool, move_id, currency)
        .await?
        .ok_or_else(|| ServiceError::rejected("move", msg!("moves.gone")))?;

    audit::created(
        pool,
        caller,
        Target::new(kinds::STOCK_MOVE, move_id)
            .named(&format!(
                "{} {} {} → {}",
                checked.quantity.to_display_string(),
                context.stock_unit_code,
                from.code,
                to.code
            ))
            .fact("item", &context.variant_code)
            .fact("journal", outcome.as_str()),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

// --- Costing --------------------------------------------------------------

/// What a move turned out to be worth, and what it did to the layers.
struct Costed {
    quantity: Quantity,
    unit_cost: Money,
    value: Money,
    /// A layer to open, for a move that brought value in.
    opens_layer: bool,
    /// Layers to spend, for a move that took value out.
    consumes: Vec<valuation::Consumed>,
    /// The item's new running average, where a receipt moved it.
    new_item_cost: Option<Money>,
}

/// Whether value arrived, left, or only changed shelves.
///
/// Ownership rather than location kind: a move from a vendor's location into
/// ours brings value in whatever the document calls itself, and the accounting
/// follows from that rather than from a type somebody chose on a form.
const fn direction(from: LocationKind, to: LocationKind) -> (bool, bool) {
    (
        to.is_owned() && !from.is_owned(),
        from.is_owned() && !to.is_owned(),
    )
}

async fn cost(
    tx: &mut Transaction<'_, Postgres>,
    context: &MoveContext,
    from: &Location,
    to: &Location,
    request: &MoveRequest,
    currency: Currency,
) -> ServiceResult<Result<Costed, Submission<StockMove>>> {
    let (value_in, value_out) = direction(from.kind, to.kind);

    if value_in {
        // What the document says it cost, or what the item stands at. A receipt
        // is the one movement that may name a price, because it is the one
        // where somebody outside the business set one.
        let unit_cost = request.unit_cost.unwrap_or(context.cost);
        let value = valuation::value_of(request.quantity, unit_cost).map_err(rejected_valuation)?;

        // Only average moves. A standard cost that drifted with every delivery
        // would not be a standard, and the difference goes to variance instead.
        let new_item_cost = if matches!(context.costing_method, CostingMethod::Average) {
            // The item's on-hand, not the variant's: the cost column being
            // blended is the item's, and averaging it over one combination
            // would write the whole item's cost out of a corner of it.
            let on_hand = quant_store::item_on_hand(&mut **tx, context.item_id).await?;

            Some(
                valuation::weighted_average(on_hand, context.item_cost, request.quantity, unit_cost)
                    .map_err(rejected_valuation)?,
            )
        } else {
            None
        };

        return Ok(Ok(Costed {
            quantity: request.quantity,
            unit_cost,
            value,
            opens_layer: true,
            consumes: Vec::new(),
            new_item_cost,
        }));
    }

    if value_out && context.needs_layers() {
        let layers = layer_store::open_layers(
            &mut **tx,
            context.variant_id,
            context.removal_strategy,
            currency,
        )
        .await?;

        let issue = match valuation::consume_fifo(&layers, request.quantity) {
            Ok(issue) => issue,
            // Under FIFO a unit with no layer behind it has no cost, and
            // inventing one is the guesswork section 6.6 refuses. The quant
            // check would have caught this too; saying it here names the
            // valuation rather than the shelf.
            Err(err) => return Ok(Err(Submission::rejected("quantity", err.message()))),
        };

        let unit_cost = issue.unit_cost(request.quantity).map_err(rejected_valuation)?;

        return Ok(Ok(Costed {
            quantity: request.quantity,
            unit_cost,
            value: issue.value,
            opens_layer: false,
            consumes: issue.lines,
            new_item_cost: None,
        }));
    }

    // Everything else: one standing cost. An internal move is here too - no
    // value leaves the business, and the row still carries what a unit was
    // worth so a transfer report is not a column of zeros.
    //
    // A despatch into transit *does* post, because the two ends stand for
    // different accounts. Its second leg has to be valued at the same number,
    // or the pair would leave a residue in the in-transit account when the
    // standing cost moved in between - which is why `MoveRequest::unit_cost`
    // exists on every move and not only on a receipt: a transfer document
    // carries the outbound cost onto the inbound leg.
    let unit_cost = request.unit_cost.unwrap_or(context.cost);
    let value = valuation::value_of(request.quantity, unit_cost).map_err(rejected_valuation)?;

    let consumes = if value_out {
        let layers = layer_store::open_layers(
            &mut **tx,
            context.variant_id,
            context.removal_strategy,
            currency,
        )
        .await?;

        // Spent for their *quantity*, not for their price: under standard and
        // average the price is the item's. Keeping `remaining` in step anyway
        // is what lets a workspace switch to FIFO without its layers already
        // being nonsense.
        match valuation::consume_fifo(&layers, request.quantity) {
            Ok(issue) => issue.lines,
            // Layers can fall short where stock was booked in before this
            // schema existed. The move is still right; there is simply nothing
            // to spend.
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    Ok(Ok(Costed {
        quantity: request.quantity,
        unit_cost,
        value,
        opens_layer: false,
        consumes,
        new_item_cost: None,
    }))
}

// --- Quants ---------------------------------------------------------------

/// Take from one end and put at the other, both under a lock.
async fn shift(
    tx: &mut Transaction<'_, Postgres>,
    context: &MoveContext,
    from: &Location,
    to: &Location,
    request: &MoveRequest,
    lot: Option<&Lot>,
) -> ServiceResult<Result<(), Submission<StockMove>>> {
    let lot_id = lot.map(|lot| lot.id);

    let source = quant_store::lock(&mut **tx, context.variant_id, from.id, lot_id).await?;
    let held = source.as_ref().map_or(Quantity::ZERO, |q| q.quantity);
    let reserved = source.as_ref().map_or(Quantity::ZERO, |q| q.reserved);

    let left = match quant::take(held, request.quantity, from.kind.is_owned()) {
        Ok(left) => left,
        Err(err) => return Ok(Err(Submission::rejected("quantity", err.message()))),
    };

    // Stock that is promised to somebody else is stock this move may not take.
    // The floor is what is *available*, not what is on the shelf.
    if from.kind.is_owned() && left.compare(reserved).is_lt() {
        return Ok(Err(Submission::rejected(
            "quantity",
            quant::QuantError::NotEnoughAvailable {
                available: held.checked_sub(reserved).unwrap_or(Quantity::ZERO),
            }
            .message(),
        )));
    }

    quant_store::write(
        &mut **tx,
        source.as_ref().map(|q| q.id),
        context.variant_id,
        from.id,
        lot_id,
        left,
        reserved,
    )
    .await?;

    let target = quant_store::lock(&mut **tx, context.variant_id, to.id, lot_id).await?;
    let there = target.as_ref().map_or(Quantity::ZERO, |q| q.quantity);
    let target_reserved = target.as_ref().map_or(Quantity::ZERO, |q| q.reserved);

    let arrived = match quant::put(there, request.quantity) {
        Ok(arrived) => arrived,
        Err(err) => return Ok(Err(Submission::rejected("quantity", err.message()))),
    };

    quant_store::write(
        &mut **tx,
        target.as_ref().map(|q| q.id),
        context.variant_id,
        to.id,
        lot_id,
        arrived,
        target_reserved,
    )
    .await?;

    Ok(Ok(()))
}

/// Open the layer a receipt created, spend the ones an issue used, and write
/// back an average a receipt moved.
async fn settle(
    tx: &mut Transaction<'_, Postgres>,
    context: &MoveContext,
    costed: &Costed,
    move_id: Uuid,
    lot: Option<&Lot>,
    _currency: Currency,
) -> ServiceResult<()> {
    if costed.opens_layer {
        layer_store::insert_layer(
            &mut **tx,
            move_id,
            context.variant_id,
            lot.map(|lot| lot.id),
            costed.quantity,
            costed.unit_cost,
            costed.value,
        )
        .await?;
    }

    if !costed.consumes.is_empty() {
        layer_store::consume(&mut **tx, move_id, &costed.consumes).await?;
    }

    if let Some(cost) = costed.new_item_cost {
        item_store::set_cost(&mut **tx, context.item_id, cost).await?;
    }

    Ok(())
}

// --- The ledger -----------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn post(
    tx: &mut Transaction<'_, Postgres>,
    ledger: &dyn Ledger,
    context: &MoveContext,
    from: &Location,
    to: &Location,
    request: &MoveRequest,
    costed: &Costed,
    move_id: Uuid,
    _currency: Currency,
) -> ServiceResult<JournalOutcome> {
    // A category on manual valuation posts at period end by hand, and a move
    // worth nothing has nothing to say.
    if !context.valuation.posts_a_journal() || costed.value.is_zero() {
        return Ok(JournalOutcome::NotRequired);
    }

    let Some((debit, credit)) = posting_roles(from.kind, to.kind) else {
        return Ok(JournalOutcome::NotRequired);
    };

    let (item_accounts, category_accounts) = overrides(&mut **tx, context).await?;

    let amount = costed.value.abs().to_storage_string();
    let line = |role, side| Posting {
        role,
        // The adjustment's *type* names the account a discrepancy is charged
        // to, and nothing else on a movement does. Every other role resolves
        // the ordinary way; `InventoryAdjustment` resolves to `None` there by
        // design - it is workspace-wide policy, not an item's business - so
        // this is the one place a per-document answer can be given. `None`
        // still falls back to whatever the workspace mapped the role to, which
        // is what every adjustment did before types existed.
        account_id: match role {
            phonix_ports::ledger::AccountRole::InventoryAdjustment => {
                request.adjustment_account_id
            }
            role => {
                app_inventory::accounts::account_for(role, &item_accounts, &category_accounts)
            }
        },
        side,
        amount: amount.clone(),
        memo: Some(format!(
            "{} {} {}",
            request.quantity.to_display_string(),
            context.stock_unit_code,
            context.variant_code
        )),
        cost_centre_id: request.cost_centre_id,
    };

    let entry = JournalRequest {
        entry_date: request.moved_on,
        narration: format!("{} · {} → {}", context.item_name, from.code, to.code),
        source_app: app_inventory::APP_ID.to_owned(),
        source_doc_type: request
            .source
            .as_ref()
            .map_or("stock_move", |source| source.doc_type.as_str())
            .to_owned(),
        source_doc_id: move_id,
        currency: costed.value.currency().code().to_owned(),
        postings: vec![line(debit, Side::Debit), line(credit, Side::Credit)],
    };

    match ledger.post(entry).await {
        Ok(posted) => Ok(JournalOutcome::Posted {
            journal_id: posted.journal_id,
            number: posted.number,
        }),
        // The one answer that is not a refusal. The stock still moved.
        Err(LedgerError::NoLedger) => Ok(JournalOutcome::NoLedger),
        Err(err) => Err(refused(err)),
    }
}

/// What the item overrides, and what its category does.
async fn overrides(
    executor: &mut phonix_db::sqlx::PgConnection,
    context: &MoveContext,
) -> ServiceResult<(AccountOverrides, AccountOverrides)> {
    let item = account_mapping::for_owner(
        &mut *executor,
        account_mapping::Owner::Item,
        context.item_id,
    )
    .await?;

    let category = account_mapping::for_owner(
        &mut *executor,
        account_mapping::Owner::Category,
        context.category_id,
    )
    .await?;

    Ok((item, category))
}

// --- Shared ---------------------------------------------------------------

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    Ok(crate::workspace::profile::current(pool).await?.currency)
}

async fn load_location(pool: &PgPool, id: Uuid) -> ServiceResult<Location> {
    phonix_db::inventory::location::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("from_location_id", msg!("locations.gone")))
}

fn reject(err: MoveError) -> Submission<StockMove> {
    Submission::rejected(err.field(), err.message())
}

fn rejected_valuation(err: valuation::ValuationError) -> ServiceError {
    ServiceError::rejected("quantity", err.message())
}

/// A ledger that refused, turned into something the person who moved the stock
/// can act on.
///
/// Every variant here rolls the movement back with it - see the module header.
/// The field each one lands on is the field somebody can change: a closed
/// period is the date, an unmapped role is a setting behind the item.
pub(crate) fn refused(err: LedgerError) -> ServiceError {
    match err {
        LedgerError::Refused(message) => ServiceError::rejected("moved_on", message),
        LedgerError::PeriodClosed(detail) => {
            ServiceError::rejected("moved_on", msg!("moves.error.period_closed", detail = detail))
        }
        LedgerError::UnmappedRole(role) => ServiceError::rejected(
            "variant_id",
            msg!("moves.error.unmapped_role", role = role.to_owned()),
        ),
        other => ServiceError::rejected(
            "moved_on",
            msg!("moves.error.not_posted", detail = other.to_string()),
        ),
    }
}
