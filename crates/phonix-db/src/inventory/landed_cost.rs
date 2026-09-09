//! `inventory.landed_costs`, its charges, and what they did to the layers.
//!
//! # Saving replaces the charges
//!
//! [`save_charges`] deletes and re-inserts, on the same terms as an order's
//! lines. Nothing is lost by it: a charge accumulates nothing, because the
//! moment it would start to - the post - is the moment it stops being editable.
//!
//! # The lines are read under a lock
//!
//! [`landables_for_update`] takes `FOR UPDATE OF l` on the valuation layers it
//! returns, for the reason [`super::valuation::open_layers`] does. What it is
//! guarding against here is different, though, and worth naming: the split
//! between capitalised and expensed is computed from `remaining`, and an issue
//! committing between the read and the write would land freight on units that
//! had gone. The lock makes the issue wait.
//!
//! # An allocation is written, never inferred
//!
//! The basis it was computed from is the receipt as it stood at the moment of
//! posting, and a receipt's own value moves afterwards when a bill posts a price
//! difference against it. Recomputing later answers a different question than
//! the one that was posted, so the arithmetic is kept.

use app_inventory::landed_cost::{
    Allocation, AllocationBasis, Charge, CheckedCharge, LandedCost, LandedCostState,
    LandedCostSummary, Landable, ReceiptLandedCost, Share,
};
use app_inventory::movement::JournalOutcome;
use app_inventory::quantity::Quantity;
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("landed_costs.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_state(raw: &str) -> Result<LandedCostState, sqlx::Error> {
    LandedCostState::parse(raw).ok_or_else(|| unknown("state", raw))
}

fn read_basis(raw: &str) -> Result<AllocationBasis, sqlx::Error> {
    AllocationBasis::parse(raw).ok_or_else(|| unknown("basis", raw))
}

/// The grid's shape, written once so two screens cannot disagree about it.
const SUMMARY_COLUMNS: &str = "
    c.id, c.number, c.state, c.receipt_id, c.receipt_number, c.supplier_name,
    c.cost_date, c.total::text AS total, c.capitalised::text AS capitalised,
    (SELECT count(*) FROM inventory.landed_cost_charges g
      WHERE g.landed_cost_id = c.id) AS charge_count
";

fn read_summary(
    row: &sqlx::postgres::PgRow,
    currency: Currency,
) -> Result<LandedCostSummary, sqlx::Error> {
    let state: String = row.try_get("state")?;
    let total: String = row.try_get("total")?;
    let capitalised: String = row.try_get("capitalised")?;

    Ok(LandedCostSummary {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        state: read_state(&state)?,
        receipt_id: row.try_get("receipt_id")?,
        receipt_number: row.try_get("receipt_number")?,
        supplier_name: row.try_get("supplier_name")?,
        cost_date: row.try_get("cost_date")?,
        total: read_money(&total, currency, "landed_costs.total")?,
        capitalised: read_money(&capitalised, currency, "landed_costs.capitalised")?,
        charge_count: row.try_get("charge_count")?,
    })
}

/// Every landed cost, newest first.
pub async fn list<'e, E>(executor: E, currency: Currency) -> Result<Vec<LandedCostSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.landed_costs c
          ORDER BY c.cost_date DESC, c.created_at DESC"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| read_summary(row, currency))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What has been landed on one delivery. The receipt screen's question.
pub async fn for_receipt<'e, E>(
    executor: E,
    receipt_id: Uuid,
    currency: Currency,
) -> Result<Vec<LandedCostSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.landed_costs c
          WHERE c.receipt_id = $1
          ORDER BY c.cost_date DESC, c.created_at DESC"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .bind(receipt_id)
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| read_summary(row, currency))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What one delivery has been landed with in total, from the view that keeps
/// the sum in step - `inventory.receipt_landed_cost`.
///
/// `None` where nothing has been landed on it, which is the ordinary case and
/// the reason this is a view rather than a column on the receipt: a delivery
/// nobody charged freight to has no row to keep correct.
pub async fn landed_on_receipt<'e, E>(
    executor: E,
    receipt_id: Uuid,
    currency: Currency,
) -> Result<Option<ReceiptLandedCost>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT r.receipt_id, r.document_count,
                r.total::text AS total,
                r.capitalised::text AS capitalised,
                r.expensed::text AS expensed,
                r.latest_on
           FROM inventory.receipt_landed_cost r
          WHERE r.receipt_id = $1",
    )
    .bind(receipt_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let read = |column: &str| -> Result<Money, DbError> {
        let raw: String = row.try_get(column).map_err(DbError::Query)?;
        read_money(&raw, currency, column).map_err(DbError::Query)
    };

    Ok(Some(ReceiptLandedCost {
        receipt_id: row.try_get("receipt_id").map_err(DbError::Query)?,
        document_count: row.try_get("document_count").map_err(DbError::Query)?,
        total: read("total")?,
        capitalised: read("capitalised")?,
        expensed: read("expensed")?,
        latest_on: row.try_get("latest_on").map_err(DbError::Query)?,
    }))
}

/// The header, without its charges or its allocations.
pub async fn find<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<LandedCost>, DbError>
where
    E: PgExecutor<'e>,
{
    let Some(row) = sqlx::query(
        "SELECT c.id, c.number, c.state, c.receipt_id, c.receipt_number,
                c.supplier_name, c.cost_date, c.note,
                c.total::text AS total,
                c.capitalised::text AS capitalised,
                c.expensed::text AS expensed,
                c.journal_number
           FROM inventory.landed_costs c
          WHERE c.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let state: String = row.try_get("state").map_err(DbError::Query)?;
    let total: String = row.try_get("total").map_err(DbError::Query)?;
    let capitalised: String = row.try_get("capitalised").map_err(DbError::Query)?;
    let expensed: String = row.try_get("expensed").map_err(DbError::Query)?;

    let document = LandedCost {
        id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: read_state(&state).map_err(DbError::Query)?,
        receipt_id: row.try_get("receipt_id").map_err(DbError::Query)?,
        receipt_number: row.try_get("receipt_number").map_err(DbError::Query)?,
        supplier_name: row.try_get("supplier_name").map_err(DbError::Query)?,
        cost_date: row.try_get("cost_date").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        total: read_money(&total, currency, "landed_costs.total").map_err(DbError::Query)?,
        capitalised: read_money(&capitalised, currency, "landed_costs.capitalised")
            .map_err(DbError::Query)?,
        expensed: read_money(&expensed, currency, "landed_costs.expensed")
            .map_err(DbError::Query)?,
        journal_number: row.try_get("journal_number").map_err(DbError::Query)?,
        charges: Vec::new(),
        allocations: Vec::new(),
    };

    Ok(Some(document))
}

pub async fn charges_of<'e, E>(
    executor: E,
    landed_cost_id: Uuid,
    currency: Currency,
) -> Result<Vec<Charge>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT g.id, g.line_no, g.description, g.basis, g.amount::text AS amount
           FROM inventory.landed_cost_charges g
          WHERE g.landed_cost_id = $1
          ORDER BY g.line_no",
    )
    .bind(landed_cost_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let basis: String = row.try_get("basis")?;
            let amount: String = row.try_get("amount")?;

            Ok(Charge {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                description: row.try_get("description")?,
                basis: read_basis(&basis)?,
                amount: read_money(&amount, currency, "landed_cost_charges.amount")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The arithmetic, in the order it reads on the screen: by charge, then by the
/// line's position on the delivery.
pub async fn allocations_of<'e, E>(
    executor: E,
    landed_cost_id: Uuid,
    currency: Currency,
) -> Result<Vec<Allocation>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT a.id, a.charge_id, g.description AS charge_description,
                a.receipt_line_id, a.layer_id, a.variant_id,
                a.basis, a.basis_amount::text AS basis_amount,
                a.amount::text AS amount,
                a.capitalised::text AS capitalised,
                a.expensed::text AS expensed,
                v.code AS variant_code,
                rl.description
           FROM inventory.landed_cost_allocations a
           JOIN inventory.landed_cost_charges g ON g.id = a.charge_id
           JOIN inventory.receipt_lines rl ON rl.id = a.receipt_line_id
           JOIN inventory.item_variants v ON v.id = a.variant_id
          WHERE a.landed_cost_id = $1
          ORDER BY g.line_no, rl.line_no",
    )
    .bind(landed_cost_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let basis: String = row.try_get("basis")?;
            let basis_amount: String = row.try_get("basis_amount")?;
            let amount: String = row.try_get("amount")?;
            let capitalised: String = row.try_get("capitalised")?;
            let expensed: String = row.try_get("expensed")?;

            Ok(Allocation {
                id: row.try_get("id")?,
                charge_id: row.try_get("charge_id")?,
                charge_description: row.try_get("charge_description")?,
                receipt_line_id: row.try_get("receipt_line_id")?,
                layer_id: row.try_get("layer_id")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                basis: read_basis(&basis)?,
                basis_amount: read_quantity(
                    &basis_amount,
                    "landed_cost_allocations.basis_amount",
                )?,
                amount: read_money(&amount, currency, "landed_cost_allocations.amount")?,
                capitalised: read_money(
                    &capitalised,
                    currency,
                    "landed_cost_allocations.capitalised",
                )?,
                expensed: read_money(&expensed, currency, "landed_cost_allocations.expensed")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Every line of a posted delivery that carries value, locked for the length of
/// the transaction.
///
/// The join is the thread ADR 0006 section 7 describes, walked backwards:
/// receipt line to the move it became, move to the layer it opened, layer to
/// the item that says what it weighs. A line whose item holds no stock has no
/// layer and is not here - it is dropped from the basis rather than given a
/// share it has nowhere to put.
pub async fn landables_for_update(
    conn: &mut PgConnection,
    receipt_id: Uuid,
    currency: Currency,
) -> Result<Vec<Landable>, DbError> {
    let rows = sqlx::query(
        "SELECT rl.id AS receipt_line_id, rl.description,
                l.id AS layer_id, l.variant_id,
                l.quantity::text AS quantity,
                l.remaining::text AS remaining,
                l.value::text AS value,
                v.code AS variant_code,
                i.weight_grams
           FROM inventory.receipt_lines rl
           JOIN inventory.valuation_layers l ON l.move_id = rl.move_id
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
          WHERE rl.receipt_id = $1
          ORDER BY rl.line_no
            FOR UPDATE OF l",
    )
    .bind(receipt_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let remaining: String = row.try_get("remaining")?;
            let value: String = row.try_get("value")?;

            Ok(Landable {
                receipt_line_id: row.try_get("receipt_line_id")?,
                layer_id: row.try_get("layer_id")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "valuation_layers.quantity")?,
                remaining: read_quantity(&remaining, "valuation_layers.remaining")?,
                value: read_money(&value, currency, "valuation_layers.value")?,
                weight_grams: row.try_get("weight_grams")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What the receipt was called, and who it came from. Snapshotted onto the
/// document so a list draws without a join.
pub struct ReceiptFacts {
    pub number: String,
    pub supplier_name: String,
    pub is_posted: bool,
}

pub async fn receipt_facts<'e, E>(
    executor: E,
    receipt_id: Uuid,
) -> Result<Option<ReceiptFacts>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT r.number, r.supplier_name, r.state
           FROM inventory.receipts r
          WHERE r.id = $1",
    )
    .bind(receipt_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let state: String = row.try_get("state").map_err(DbError::Query)?;

    Ok(Some(ReceiptFacts {
        number: row.try_get("number").map_err(DbError::Query)?,
        supplier_name: row.try_get("supplier_name").map_err(DbError::Query)?,
        is_posted: state == "done",
    }))
}

/// What the items behind these variants cost by, and which accounts they may
/// name.
///
/// A landed cost raises `additional_value` on the layer, which is the whole
/// answer under FIFO. Under average the item's own `cost` column is what stock
/// is valued at, so leaving it alone would put freight in the stock account
/// that the stock ledger cannot see - the disagreement ADR 0006 section 6.7
/// exists to prevent. A standard cost is not moved: a standard that drifted
/// with the freight bill would not be a standard.
///
/// The category comes back for the accounts, not for the costing. One
/// delivery's lines can sit in categories pointing at different stock
/// accounts, and a single debit to the default would reconcile against
/// neither.
pub struct VariantCosting {
    pub variant_id: Uuid,
    pub item_id: Uuid,
    pub category_id: Uuid,
    pub is_averaged: bool,
}

pub async fn costing_facts<'e, E>(
    executor: E,
    variant_ids: &[Uuid],
) -> Result<Vec<VariantCosting>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT v.id AS variant_id, i.id AS item_id, i.category_id,
                (c.costing_method = 'average') AS is_averaged
           FROM inventory.item_variants v
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.categories c ON c.id = i.category_id
          WHERE v.id = ANY($1)",
    )
    .bind(variant_ids)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok(VariantCosting {
                variant_id: row.try_get("variant_id")?,
                item_id: row.try_get("item_id")?,
                category_id: row.try_get("category_id")?,
                is_averaged: row.try_get("is_averaged")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn insert(
    conn: &mut PgConnection,
    receipt_id: Uuid,
    facts: &ReceiptFacts,
    cost_date: chrono::NaiveDate,
    note: Option<&str>,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.landed_costs
             (receipt_id, receipt_number, supplier_name, cost_date, note, created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $6)
       RETURNING id",
    )
    .bind(receipt_id)
    .bind(&facts.number)
    .bind(&facts.supplier_name)
    .bind(cost_date)
    .bind(note)
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// Answers `false` where the document is no longer a draft, so a screen can say
/// so rather than silently changing nothing.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    receipt_id: Uuid,
    facts: &ReceiptFacts,
    cost_date: chrono::NaiveDate,
    note: Option<&str>,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.landed_costs
            SET receipt_id = $2, receipt_number = $3, supplier_name = $4,
                cost_date = $5, note = $6, updated_at = now(), updated_by = $7
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(receipt_id)
    .bind(&facts.number)
    .bind(&facts.supplier_name)
    .bind(cost_date)
    .bind(note)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// One charge, priced. The amount is parsed against the workspace's currency by
/// the service, which is why this is not `CheckedCharge`.
pub struct PricedCharge<'a> {
    pub checked: &'a CheckedCharge,
    pub amount: Money,
}

/// Replace the charges. Delete-and-reinsert - see the module header.
pub async fn save_charges(
    conn: &mut PgConnection,
    landed_cost_id: Uuid,
    charges: &[PricedCharge<'_>],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.landed_cost_charges WHERE landed_cost_id = $1")
        .bind(landed_cost_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (position, charge) in charges.iter().enumerate() {
        let line_no = i32::try_from(position + 1).unwrap_or(i32::MAX);

        sqlx::query(
            "INSERT INTO inventory.landed_cost_charges
                 (landed_cost_id, line_no, description, basis, amount)
              VALUES ($1, $2, $3, $4, $5::numeric)",
        )
        .bind(landed_cost_id)
        .bind(line_no)
        .bind(&charge.checked.description)
        .bind(charge.checked.basis.as_str())
        .bind(charge.amount.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Write the arithmetic down. One row per charge per line it reached.
pub async fn record_allocations(
    conn: &mut PgConnection,
    landed_cost_id: Uuid,
    shares: &[Share],
) -> Result<(), DbError> {
    for share in shares {
        sqlx::query(
            "INSERT INTO inventory.landed_cost_allocations
                 (landed_cost_id, charge_id, receipt_line_id, layer_id, variant_id,
                  basis, basis_amount, amount, capitalised, expensed)
              VALUES ($1, $2, $3, $4, $5, $6,
                      $7::numeric, $8::numeric, $9::numeric, $10::numeric)",
        )
        .bind(landed_cost_id)
        .bind(share.charge_id)
        .bind(share.receipt_line_id)
        .bind(share.layer_id)
        .bind(share.variant_id)
        .bind(share.basis.as_str())
        .bind(share.basis_amount.to_storage_string())
        .bind(share.amount.to_storage_string())
        .bind(share.capitalised.to_storage_string())
        .bind(share.expensed.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Take the number and the totals, in the transaction that did the work.
///
/// Answers `false` where somebody else posted it first, on the same terms as a
/// receipt's post: the caller rolls back rather than posting twice.
pub async fn post(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    total: Money,
    capitalised: Money,
    expensed: Money,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.landed_costs
            SET state = 'done', number = $2,
                total = $3::numeric, capitalised = $4::numeric, expensed = $5::numeric,
                posted_at = now(), posted_by = $6, updated_at = now(), updated_by = $6
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(total.to_storage_string())
    .bind(capitalised.to_storage_string())
    .bind(expensed.to_storage_string())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Write down where the journal for this document landed.
pub async fn record_journal(
    conn: &mut PgConnection,
    id: Uuid,
    outcome: &JournalOutcome,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.landed_costs
            SET journal_state = $2, journal_id = $3, journal_number = $4
          WHERE id = $1",
    )
    .bind(id)
    .bind(outcome.as_str())
    .bind(outcome.journal_id())
    .bind(outcome.number())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// A draft may still be abandoned; a posted document may not.
pub async fn cancel(
    conn: &mut PgConnection,
    id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.landed_costs
            SET state = 'cancelled', updated_at = now(), updated_by = $2
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Only a draft. A posted landed cost is corrected by a second one with a
/// negative charge, which is section 6.3's rule and not this function's job.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query("DELETE FROM inventory.landed_costs WHERE id = $1 AND state = 'draft'")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}
