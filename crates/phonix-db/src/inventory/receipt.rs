//! `inventory.receipts` and its lines.
//!
//! # Amounts here are the workspace's own currency
//!
//! Unlike a purchase order, which carries the supplier's. The conversion
//! happens once, when the receipt is posted, at the rate on the receipt's own
//! date - because that is the day the value arrived, and re-converting later
//! from a newer rate would restate a filed period.
//!
//! # `move_id` is the thread from the paperwork to the ledger
//!
//! Set when the receipt is posted, one per line. It is what lets a stock move
//! answer "which delivery note was this", and what a three-way match reads.

use app_inventory::purchase::SupplierSnapshot;
use app_inventory::quantity::Quantity;
use app_inventory::receipt::{CheckedReceipt, Receipt, ReceiptLine, ReceiptState, ReceiptSummary};
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("receipts.{column} holds '{raw}', which this build does not know").into(),
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

pub async fn list<'e, E>(executor: E, currency: Currency) -> Result<Vec<ReceiptSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT r.id, r.number, r.state, r.supplier_name, r.received_on,
                r.delivery_note, r.value::text AS value,
                o.number AS order_number,
                w.name AS warehouse_name,
                (SELECT count(*) FROM inventory.receipt_lines l WHERE l.receipt_id = r.id)
                    AS line_count
           FROM inventory.receipts r
           JOIN inventory.warehouses w ON w.id = r.warehouse_id
           LEFT JOIN inventory.purchase_orders o ON o.id = r.order_id
          ORDER BY r.received_on DESC, r.created_at DESC",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let state: String = row.try_get("state")?;
            let value: String = row.try_get("value")?;
            let order_number: Option<String> = row.try_get("order_number")?;

            Ok(ReceiptSummary {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                state: ReceiptState::parse(&state).ok_or_else(|| unknown("state", &state))?,
                supplier_name: row.try_get("supplier_name")?,
                // Empty for a draft order, which is not the same as no order.
                order_number: order_number.filter(|number| !number.is_empty()),
                warehouse_name: row.try_get("warehouse_name")?,
                received_on: row.try_get("received_on")?,
                delivery_note: row.try_get("delivery_note")?,
                value: read_money(&value, currency, "receipts.value")?,
                line_count: row.try_get("line_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<Receipt>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT r.id, r.number, r.state, r.order_id, r.supplier_id, r.supplier_code,
                r.supplier_name, r.warehouse_id, r.to_location_id, r.received_on,
                r.delivery_note, r.note, r.value::text AS value,
                o.number AS order_number,
                w.name AS warehouse_name,
                loc.path AS to_location_path
           FROM inventory.receipts r
           JOIN inventory.warehouses w ON w.id = r.warehouse_id
           JOIN inventory.locations loc ON loc.id = r.to_location_id
           LEFT JOIN inventory.purchase_orders o ON o.id = r.order_id
          WHERE r.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let state: String = row.try_get("state").map_err(DbError::Query)?;
    let value: String = row.try_get("value").map_err(DbError::Query)?;
    let order_number: Option<String> = row.try_get("order_number").map_err(DbError::Query)?;

    Ok(Some(Receipt {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: ReceiptState::parse(&state)
            .ok_or_else(|| DbError::Query(unknown("state", &state)))?,
        order_id: row.try_get("order_id").map_err(DbError::Query)?,
        order_number: order_number.filter(|number| !number.is_empty()),
        supplier: SupplierSnapshot {
            party_id: row.try_get("supplier_id").map_err(DbError::Query)?,
            code: row.try_get("supplier_code").map_err(DbError::Query)?,
            name: row.try_get("supplier_name").map_err(DbError::Query)?,
        },
        warehouse_id: row.try_get("warehouse_id").map_err(DbError::Query)?,
        warehouse_name: row.try_get("warehouse_name").map_err(DbError::Query)?,
        to_location_id: row.try_get("to_location_id").map_err(DbError::Query)?,
        to_location_path: row.try_get("to_location_path").map_err(DbError::Query)?,
        received_on: row.try_get("received_on").map_err(DbError::Query)?,
        delivery_note: row.try_get("delivery_note").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        value: read_money(&value, currency, "receipts.value").map_err(DbError::Query)?,
        lines: lines_of(executor, id, currency).await?,
    }))
}

pub async fn lines_of<'e, E>(
    executor: E,
    receipt_id: Uuid,
    currency: Currency,
) -> Result<Vec<ReceiptLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.order_line_id, l.variant_id, l.description,
                l.quantity::text AS quantity, l.lot_number, l.expires_on,
                l.unit_cost::text AS unit_cost, l.value::text AS value, l.move_id,
                v.code AS variant_code,
                u.code AS unit_code
           FROM inventory.receipt_lines l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.stock_unit_id
          WHERE l.receipt_id = $1
          ORDER BY l.line_no",
    )
    .bind(receipt_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let unit_cost: String = row.try_get("unit_cost")?;
            let value: String = row.try_get("value")?;

            Ok(ReceiptLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                order_line_id: row.try_get("order_line_id")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "receipt_lines.quantity")?,
                unit_code: row.try_get("unit_code")?,
                lot_number: row.try_get("lot_number")?,
                expires_on: row.try_get("expires_on")?,
                unit_cost: read_money(&unit_cost, currency, "receipt_lines.unit_cost")?,
                value: read_money(&value, currency, "receipt_lines.value")?,
                move_id: row.try_get("move_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn insert(
    conn: &mut PgConnection,
    draft: &CheckedReceipt,
    supplier: &SupplierSnapshot,
    to_location_id: Uuid,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.receipts
             (order_id, supplier_id, supplier_code, supplier_name, warehouse_id,
              to_location_id, received_on, delivery_note, note, created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
       RETURNING id",
    )
    .bind(draft.order_id)
    .bind(supplier.party_id)
    .bind(&supplier.code)
    .bind(&supplier.name)
    .bind(draft.warehouse_id)
    .bind(to_location_id)
    .bind(draft.received_on)
    .bind(draft.delivery_note.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &CheckedReceipt,
    supplier: &SupplierSnapshot,
    to_location_id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.receipts
            SET order_id = $2, supplier_id = $3, supplier_code = $4, supplier_name = $5,
                warehouse_id = $6, to_location_id = $7, received_on = $8,
                delivery_note = $9, note = $10, updated_at = now(), updated_by = $11
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(draft.order_id)
    .bind(supplier.party_id)
    .bind(&supplier.code)
    .bind(&supplier.name)
    .bind(draft.warehouse_id)
    .bind(to_location_id)
    .bind(draft.received_on)
    .bind(draft.delivery_note.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// One line as the service costed it.
pub struct CostedReceiptLine<'a> {
    pub source: &'a app_inventory::receipt::CheckedReceiptLine,
    pub description: String,
    pub unit_cost: Money,
    pub value: Money,
}

/// Replace a draft's lines. Same reasoning as an order's.
pub async fn save_lines(
    conn: &mut PgConnection,
    receipt_id: Uuid,
    lines: &[CostedReceiptLine<'_>],
) -> Result<Vec<Uuid>, DbError> {
    sqlx::query("DELETE FROM inventory.receipt_lines WHERE receipt_id = $1")
        .bind(receipt_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    let mut ids = Vec::with_capacity(lines.len());

    for (index, line) in lines.iter().enumerate() {
        let id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO inventory.receipt_lines
                 (receipt_id, line_no, order_line_id, variant_id, description,
                  quantity, lot_number, expires_on, unit_cost, value)
              VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8, $9::numeric, $10::numeric)
           RETURNING id",
        )
        .bind(receipt_id)
        .bind(index as i32 + 1)
        .bind(line.source.order_line_id)
        .bind(line.source.variant_id)
        .bind(&line.description)
        .bind(line.source.quantity.to_storage_string())
        .bind(line.source.lot_number.as_deref())
        .bind(line.source.expires_on)
        .bind(line.unit_cost.to_storage_string())
        .bind(line.value.to_storage_string())
        .fetch_one(&mut *conn)
        .await
        .map_err(DbError::Query)?;

        ids.push(id);
    }

    Ok(ids)
}

/// Tie a posted line to the movement it became.
pub async fn record_move(
    conn: &mut PgConnection,
    line_id: Uuid,
    move_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query("UPDATE inventory.receipt_lines SET move_id = $2 WHERE id = $1")
        .bind(line_id)
        .bind(move_id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(())
}

/// Give the receipt its number and close it.
pub async fn post(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    value: Money,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.receipts
            SET number = $2, state = 'done', value = $3::numeric,
                posted_at = now(), posted_by = $4, updated_at = now(), updated_by = $4
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(value.to_storage_string())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

pub async fn cancel(
    conn: &mut PgConnection,
    id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.receipts
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

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query("DELETE FROM inventory.receipts WHERE id = $1 AND state = 'draft'")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}
