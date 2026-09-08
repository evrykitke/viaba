//! `inventory.bills`, its lines, and the aged GRNI view.
//!
//! Each bill carries its own currency, like the order it is against. Saving one
//! replaces its lines, for the reason [`super::purchase`] gives.

use app_inventory::bill::{
    Bill, BillLine, BillState, BillSummary, CheckedBill, UnbilledReceipt,
};
use app_inventory::purchase::SupplierSnapshot;
use app_inventory::quantity::Quantity;
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("bills.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_currency(raw: &str) -> Result<Currency, sqlx::Error> {
    Currency::parse(raw).map_err(|_| unknown("currency", raw))
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_state(raw: &str) -> Result<BillState, sqlx::Error> {
    BillState::parse(raw).ok_or_else(|| unknown("state", raw))
}

pub async fn list<'e, E>(executor: E) -> Result<Vec<BillSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT b.id, b.number, b.state, b.supplier_name, b.supplier_reference,
                b.bill_date, b.due_on, b.currency,
                b.net::text AS net, b.variance::text AS variance,
                b.match_note IS NOT NULL AS was_overridden,
                o.number AS order_number,
                (SELECT count(*) FROM inventory.bill_lines l WHERE l.bill_id = b.id)
                    AS line_count
           FROM inventory.bills b
           LEFT JOIN inventory.purchase_orders o ON o.id = b.order_id
          ORDER BY b.bill_date DESC, b.created_at DESC",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            let currency = read_currency(row.try_get("currency").map_err(DbError::Query)?)
                .map_err(DbError::Query)?;

            Ok(BillSummary {
                id: row.try_get("id").map_err(DbError::Query)?,
                number: row.try_get("number").map_err(DbError::Query)?,
                state: read_state(row.try_get("state").map_err(DbError::Query)?)
                    .map_err(DbError::Query)?,
                supplier_name: row.try_get("supplier_name").map_err(DbError::Query)?,
                supplier_reference: row.try_get("supplier_reference").map_err(DbError::Query)?,
                order_number: row.try_get("order_number").map_err(DbError::Query)?,
                bill_date: row.try_get("bill_date").map_err(DbError::Query)?,
                due_on: row.try_get("due_on").map_err(DbError::Query)?,
                net: read_money(
                    row.try_get("net").map_err(DbError::Query)?,
                    currency,
                    "bills.net",
                )
                .map_err(DbError::Query)?,
                variance: read_money(
                    row.try_get("variance").map_err(DbError::Query)?,
                    currency,
                    "bills.variance",
                )
                .map_err(DbError::Query)?,
                line_count: row.try_get("line_count").map_err(DbError::Query)?,
                was_overridden: row.try_get("was_overridden").map_err(DbError::Query)?,
            })
        })
        .collect()
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Bill>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT b.id, b.number, b.state, b.order_id, b.supplier_id, b.supplier_code,
                b.supplier_name, b.supplier_reference, b.bill_date, b.due_on,
                b.currency, b.net::text AS net, b.accrued::text AS accrued,
                b.variance::text AS variance, b.note, b.match_note,
                b.overridden_by, b.overridden_at, b.journal_id,
                o.number AS order_number
           FROM inventory.bills b
           LEFT JOIN inventory.purchase_orders o ON o.id = b.order_id
          WHERE b.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let currency =
        read_currency(row.try_get("currency").map_err(DbError::Query)?).map_err(DbError::Query)?;

    let overridden_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("overridden_at").map_err(DbError::Query)?;

    Ok(Some(Bill {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: read_state(row.try_get("state").map_err(DbError::Query)?)
            .map_err(DbError::Query)?,
        order_id: row.try_get("order_id").map_err(DbError::Query)?,
        order_number: row.try_get("order_number").map_err(DbError::Query)?,
        supplier: SupplierSnapshot {
            party_id: row.try_get("supplier_id").map_err(DbError::Query)?,
            code: row.try_get("supplier_code").map_err(DbError::Query)?,
            name: row.try_get("supplier_name").map_err(DbError::Query)?,
        },
        supplier_reference: row.try_get("supplier_reference").map_err(DbError::Query)?,
        bill_date: row.try_get("bill_date").map_err(DbError::Query)?,
        due_on: row.try_get("due_on").map_err(DbError::Query)?,
        currency: currency.code().to_owned(),
        net: read_money(
            row.try_get("net").map_err(DbError::Query)?,
            currency,
            "bills.net",
        )
        .map_err(DbError::Query)?,
        accrued: read_money(
            row.try_get("accrued").map_err(DbError::Query)?,
            currency,
            "bills.accrued",
        )
        .map_err(DbError::Query)?,
        variance: read_money(
            row.try_get("variance").map_err(DbError::Query)?,
            currency,
            "bills.variance",
        )
        .map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        match_note: row.try_get("match_note").map_err(DbError::Query)?,
        overridden_by: row.try_get("overridden_by").map_err(DbError::Query)?,
        overridden_at: overridden_at.map(|at| at.naive_utc()),
        journal_id: row.try_get("journal_id").map_err(DbError::Query)?,
        lines: lines_of(executor, id, currency).await?,
    }))
}

pub async fn lines_of<'e, E>(
    executor: E,
    bill_id: Uuid,
    currency: Currency,
) -> Result<Vec<BillLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.receipt_line_id, l.order_line_id, l.variant_id,
                l.description, l.quantity::text AS quantity, l.unit_id,
                l.unit_price::text AS unit_price, l.net::text AS net,
                l.accrued::text AS accrued,
                v.code AS variant_code, u.code AS unit_code
           FROM inventory.bill_lines l
           LEFT JOIN inventory.item_variants v ON v.id = l.variant_id
           LEFT JOIN inventory.units u ON u.id = l.unit_id
          WHERE l.bill_id = $1
          ORDER BY l.line_no",
    )
    .bind(bill_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            Ok(BillLine {
                id: row.try_get("id").map_err(DbError::Query)?,
                line_no: row.try_get("line_no").map_err(DbError::Query)?,
                receipt_line_id: row.try_get("receipt_line_id").map_err(DbError::Query)?,
                order_line_id: row.try_get("order_line_id").map_err(DbError::Query)?,
                variant_id: row.try_get("variant_id").map_err(DbError::Query)?,
                variant_code: row.try_get("variant_code").map_err(DbError::Query)?,
                description: row.try_get("description").map_err(DbError::Query)?,
                quantity: read_quantity(
                    row.try_get("quantity").map_err(DbError::Query)?,
                    "bill_lines.quantity",
                )
                .map_err(DbError::Query)?,
                unit_id: row.try_get("unit_id").map_err(DbError::Query)?,
                unit_code: row.try_get("unit_code").map_err(DbError::Query)?,
                unit_price: read_money(
                    row.try_get("unit_price").map_err(DbError::Query)?,
                    currency,
                    "bill_lines.unit_price",
                )
                .map_err(DbError::Query)?,
                net: read_money(
                    row.try_get("net").map_err(DbError::Query)?,
                    currency,
                    "bill_lines.net",
                )
                .map_err(DbError::Query)?,
                accrued: read_money(
                    row.try_get("accrued").map_err(DbError::Query)?,
                    currency,
                    "bill_lines.accrued",
                )
                .map_err(DbError::Query)?,
            })
        })
        .collect()
}

pub async fn insert(
    conn: &mut PgConnection,
    draft: &CheckedBill,
    supplier: &SupplierSnapshot,
    net: Money,
    accrued: Money,
    variance: Money,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.bills
             (order_id, supplier_id, supplier_code, supplier_name, supplier_reference,
              bill_date, due_on, currency, net, accrued, variance, note,
              created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::numeric, $10::numeric,
                  $11::numeric, $12, $13, $13)
       RETURNING id",
    )
    .bind(draft.order_id)
    .bind(supplier.party_id)
    .bind(&supplier.code)
    .bind(&supplier.name)
    .bind(&draft.supplier_reference)
    .bind(draft.bill_date)
    .bind(draft.due_on)
    .bind(&draft.currency)
    .bind(net.to_storage_string())
    .bind(accrued.to_storage_string())
    .bind(variance.to_storage_string())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// The header, on edit. Answers `false` where the bill is gone or posted.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &CheckedBill,
    supplier: &SupplierSnapshot,
    net: Money,
    accrued: Money,
    variance: Money,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.bills
            SET order_id = $2, supplier_id = $3, supplier_code = $4, supplier_name = $5,
                supplier_reference = $6, bill_date = $7, due_on = $8, currency = $9,
                net = $10::numeric, accrued = $11::numeric, variance = $12::numeric,
                note = $13, updated_at = now(), updated_by = $14
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(draft.order_id)
    .bind(supplier.party_id)
    .bind(&supplier.code)
    .bind(&supplier.name)
    .bind(&draft.supplier_reference)
    .bind(draft.bill_date)
    .bind(draft.due_on)
    .bind(&draft.currency)
    .bind(net.to_storage_string())
    .bind(accrued.to_storage_string())
    .bind(variance.to_storage_string())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// One line, as the service worked it out.
pub struct CostedBillLine<'a> {
    pub source: &'a app_inventory::bill::CheckedBillLine,
    pub description: String,
    pub unit_price: Money,
    pub net: Money,
    pub accrued: Money,
}

pub async fn save_lines(
    conn: &mut PgConnection,
    bill_id: Uuid,
    costed: &[CostedBillLine<'_>],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.bill_lines WHERE bill_id = $1")
        .bind(bill_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (index, line) in costed.iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory.bill_lines
                 (bill_id, line_no, receipt_line_id, order_line_id, variant_id,
                  description, quantity, unit_id, unit_price, net, accrued)
              VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8, $9::numeric,
                      $10::numeric, $11::numeric)",
        )
        .bind(bill_id)
        .bind(index as i32 + 1)
        .bind(line.source.receipt_line_id)
        .bind(line.source.order_line_id)
        .bind(line.source.variant_id)
        .bind(&line.description)
        .bind(line.source.quantity.to_storage_string())
        .bind(line.source.unit_id)
        .bind(line.unit_price.to_storage_string())
        .bind(line.net.to_storage_string())
        .bind(line.accrued.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Every posted receipt line under an order that is not yet billed in full.
pub async fn billable_lines<'e, E>(
    executor: E,
    order_id: Uuid,
    currency: Currency,
) -> Result<Vec<BillableLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.order_line_id, l.variant_id, l.description,
                (l.quantity - l.billed)::text AS outstanding,
                l.unit_cost::text AS unit_cost,
                i.purchase_unit_id
           FROM inventory.receipt_lines l
           JOIN inventory.receipts r ON r.id = l.receipt_id
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
          WHERE r.order_id = $1
            AND r.state = 'done'
            AND l.billed < l.quantity
          ORDER BY r.received_on, l.line_no",
    )
    .bind(order_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            Ok(BillableLine {
                receipt_line_id: row.try_get("id").map_err(DbError::Query)?,
                order_line_id: row.try_get("order_line_id").map_err(DbError::Query)?,
                variant_id: row.try_get("variant_id").map_err(DbError::Query)?,
                description: row.try_get("description").map_err(DbError::Query)?,
                outstanding: read_quantity(
                    row.try_get("outstanding").map_err(DbError::Query)?,
                    "receipt_lines.outstanding",
                )
                .map_err(DbError::Query)?,
                unit_cost: read_money(
                    row.try_get("unit_cost").map_err(DbError::Query)?,
                    currency,
                    "receipt_lines.unit_cost",
                )
                .map_err(DbError::Query)?,
                purchase_unit_id: row.try_get("purchase_unit_id").map_err(DbError::Query)?,
            })
        })
        .collect()
}

pub struct BillableLine {
    pub receipt_line_id: Uuid,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Uuid,
    pub description: String,
    pub outstanding: Quantity,
    pub unit_cost: Money,
    pub purchase_unit_id: Uuid,
}

/// What a receipt line has already been billed for, and what it accrued.
///
/// Read before a bill is saved, so a line cannot be billed twice.
pub async fn receipt_line_state<'e, E>(
    executor: E,
    receipt_line_id: Uuid,
    currency: Currency,
) -> Result<Option<ReceiptLineState>, DbError>
where
    E: PgExecutor<'e>,
{
    let Some(row) = sqlx::query(
        "SELECT l.quantity::text AS quantity, l.billed::text AS billed,
                l.unit_cost::text AS unit_cost, l.order_line_id, l.variant_id,
                r.state AS receipt_state, r.order_id, r.posted_at, r.posted_by
           FROM inventory.receipt_lines l
           JOIN inventory.receipts r ON r.id = l.receipt_id
          WHERE l.id = $1",
    )
    .bind(receipt_line_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let posted_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("posted_at").map_err(DbError::Query)?;

    Ok(Some(ReceiptLineState {
        quantity: read_quantity(
            row.try_get("quantity").map_err(DbError::Query)?,
            "receipt_lines.quantity",
        )
        .map_err(DbError::Query)?,
        billed: read_quantity(
            row.try_get("billed").map_err(DbError::Query)?,
            "receipt_lines.billed",
        )
        .map_err(DbError::Query)?,
        unit_cost: read_money(
            row.try_get("unit_cost").map_err(DbError::Query)?,
            currency,
            "receipt_lines.unit_cost",
        )
        .map_err(DbError::Query)?,
        order_line_id: row.try_get("order_line_id").map_err(DbError::Query)?,
        variant_id: row.try_get("variant_id").map_err(DbError::Query)?,
        receipt_state: row.try_get("receipt_state").map_err(DbError::Query)?,
        order_id: row.try_get("order_id").map_err(DbError::Query)?,
        posted_at: posted_at.map(|at| at.naive_utc()),
        posted_by: row.try_get("posted_by").map_err(DbError::Query)?,
    }))
}

pub struct ReceiptLineState {
    pub quantity: Quantity,
    pub billed: Quantity,
    pub unit_cost: Money,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Uuid,
    pub receipt_state: String,
    pub order_id: Option<Uuid>,
    pub posted_at: Option<chrono::NaiveDateTime>,
    pub posted_by: Option<Uuid>,
}

/// When the order was confirmed and by whom: the two facts a circular match and
/// a same-hand match are read from.
pub async fn order_provenance<'e, E>(
    executor: E,
    order_id: Uuid,
) -> Result<Option<(Option<chrono::NaiveDateTime>, Option<Uuid>)>, DbError>
where
    E: PgExecutor<'e>,
{
    let Some(row) = sqlx::query(
        "SELECT confirmed_at, confirmed_by FROM inventory.purchase_orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("confirmed_at").map_err(DbError::Query)?;

    Ok(Some((
        at.map(|at| at.naive_utc()),
        row.try_get("confirmed_by").map_err(DbError::Query)?,
    )))
}

/// Advance what a receipt line has been billed for. A delta, so two bills
/// against one line both count.
pub async fn advance_billed(
    conn: &mut PgConnection,
    receipt_line_id: Uuid,
    quantity: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.receipt_lines
            SET billed = billed + $2::numeric
          WHERE id = $1",
    )
    .bind(receipt_line_id)
    .bind(quantity.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

pub async fn post(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    match_note: Option<&str>,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.bills
            SET state = 'posted', number = $2,
                match_note = COALESCE($3, match_note),
                overridden_by = CASE WHEN $3 IS NULL THEN overridden_by ELSE $4 END,
                overridden_at = CASE WHEN $3 IS NULL THEN overridden_at ELSE now() END,
                posted_at = now(), posted_by = $4,
                updated_at = now(), updated_by = $4
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(match_note)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

pub async fn record_journal(
    conn: &mut PgConnection,
    id: Uuid,
    journal_id: Option<Uuid>,
    state: &str,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.bills SET journal_id = $2, journal_state = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(journal_id)
    .bind(state)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

pub async fn cancel(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.bills SET state = 'cancelled', updated_at = now()
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query("DELETE FROM inventory.bills WHERE id = $1 AND state = 'draft'")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Goods received and not yet billed, oldest first.
pub async fn unbilled<'e, E>(
    executor: E,
    currency: Currency,
) -> Result<Vec<UnbilledReceipt>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT receipt_id, number, received_on, supplier_id, supplier_name,
                order_number, unbilled::text AS unbilled, age_days
           FROM inventory.unbilled_receipts
          ORDER BY age_days DESC, received_on",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            Ok(UnbilledReceipt {
                receipt_id: row.try_get("receipt_id").map_err(DbError::Query)?,
                number: row.try_get("number").map_err(DbError::Query)?,
                received_on: row.try_get("received_on").map_err(DbError::Query)?,
                supplier_id: row.try_get("supplier_id").map_err(DbError::Query)?,
                supplier_name: row.try_get("supplier_name").map_err(DbError::Query)?,
                order_number: row.try_get("order_number").map_err(DbError::Query)?,
                unbilled: read_money(
                    row.try_get("unbilled").map_err(DbError::Query)?,
                    currency,
                    "unbilled_receipts.unbilled",
                )
                .map_err(DbError::Query)?,
                age_days: row.try_get("age_days").map_err(DbError::Query)?,
            })
        })
        .collect()
}
