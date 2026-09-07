//! `inventory.stock_quants`: how much is in one place.
//!
//! # Every write here locks the row first
//!
//! Two receipts of the same item into the same shelf at the same moment must
//! not both read 40 and both write 45. [`lock`] takes `FOR UPDATE`, so the
//! second waits for the first and reads 45 - which is the only way a running
//! total survives concurrency, and the reason none of these take a pool.
//!
//! # The negative check is here as well as in the schema
//!
//! The trigger in `0004_stock.sql` is the backstop. This asks first, so that
//! somebody who tries to issue forty from a shelf holding thirty is told how
//! many are short rather than shown a constraint name.

use app_inventory::quant::{OnHandFilter, OnHandRow, Quant};
use app_inventory::quantity::Quantity;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{AssertSqlSafe, FromRow, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

struct RowOf<T>(T);

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Quant> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let quantity: String = row.try_get("quantity")?;
        let reserved: String = row.try_get("reserved")?;

        Ok(Self(Quant {
            id: row.try_get("id")?,
            variant_id: row.try_get("variant_id")?,
            location_id: row.try_get("location_id")?,
            lot_id: row.try_get("lot_id")?,
            quantity: read_quantity(&quantity, "stock_quants.quantity")?,
            reserved: read_quantity(&reserved, "stock_quants.reserved")?,
        }))
    }
}

const COLUMNS: &str = "id, variant_id, location_id, lot_id,
                       quantity::text AS quantity, reserved::text AS reserved";

/// One quant, without locking it. For a screen, never for a write.
pub async fn find<'e, E>(
    executor: E,
    variant_id: Uuid,
    location_id: Uuid,
    lot_id: Option<Uuid>,
) -> Result<Option<Quant>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Quant>>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM inventory.stock_quants
          WHERE variant_id = $1 AND location_id = $2
            AND lot_id IS NOT DISTINCT FROM $3"
    )))
    .bind(variant_id)
    .bind(location_id)
    .bind(lot_id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// One quant, held against every other writer until the transaction ends.
///
/// `None` where nothing has ever been here, which is not the same as zero and
/// is what tells [`write`] to insert rather than update.
pub async fn lock(
    conn: &mut PgConnection,
    variant_id: Uuid,
    location_id: Uuid,
    lot_id: Option<Uuid>,
) -> Result<Option<Quant>, DbError> {
    Ok(sqlx::query_as::<_, RowOf<Quant>>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM inventory.stock_quants
          WHERE variant_id = $1 AND location_id = $2
            AND lot_id IS NOT DISTINCT FROM $3
            FOR UPDATE"
    )))
    .bind(variant_id)
    .bind(location_id)
    .bind(lot_id)
    .fetch_optional(conn)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// Put a worked-out total back. The caller has locked the row and applied
/// `app_inventory::quant`'s arithmetic to it.
///
/// `existing` is what [`lock`] answered, so this knows whether to update or
/// insert without an `ON CONFLICT` - which could not be written anyway, because
/// the uniqueness is two partial indexes and a conflict target names one.
pub async fn write(
    conn: &mut PgConnection,
    existing: Option<Uuid>,
    variant_id: Uuid,
    location_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: Quantity,
    reserved: Quantity,
) -> Result<(), DbError> {
    // A quant with nothing left in it is deleted rather than kept at zero: the
    // on-hand report reads this table, and a warehouse that once stocked four
    // thousand things should not draw four thousand empty rows forever.
    if quantity.is_zero() && reserved.is_zero() {
        if let Some(id) = existing {
            sqlx::query("DELETE FROM inventory.stock_quants WHERE id = $1")
                .bind(id)
                .execute(conn)
                .await
                .map_err(DbError::Query)?;
        }

        return Ok(());
    }

    match existing {
        Some(id) => {
            sqlx::query(
                "UPDATE inventory.stock_quants
                    SET quantity = $2::numeric, reserved = $3::numeric, updated_at = now()
                  WHERE id = $1",
            )
            .bind(id)
            .bind(quantity.to_storage_string())
            .bind(reserved.to_storage_string())
            .execute(conn)
            .await
            .map_err(DbError::Query)?;
        }
        None => {
            sqlx::query(
                "INSERT INTO inventory.stock_quants
                     (variant_id, location_id, lot_id, quantity, reserved)
                  VALUES ($1, $2, $3, $4::numeric, $5::numeric)",
            )
            .bind(variant_id)
            .bind(location_id)
            .bind(lot_id)
            .bind(quantity.to_storage_string())
            .bind(reserved.to_storage_string())
            .execute(conn)
            .await
            .map_err(DbError::Query)?;
        }
    }

    Ok(())
}

/// Everything on hand, filtered, with what it is worth.
///
/// A location filter matches the whole subtree beneath it: "how much is in the
/// Manchester warehouse" is one question, not one per shelf. `include_empty`
/// exists for a stock take, which needs the rows that say nothing is there.
///
/// # What a row is worth
///
/// Under FIFO, the layers still open, weighted by what is left in them - which
/// is what the stock account was posted from and is not the same number as the
/// item's cost. Under standard and average there is one cost for everything and
/// the item's column *is* the answer, so the query asks the category rather than
/// valuing every method the same way and being wrong for two of them.
pub async fn on_hand<'e, E>(
    executor: E,
    filter: &OnHandFilter,
    currency: Currency,
) -> Result<Vec<OnHandRow>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "WITH scope AS (
             SELECT id FROM inventory.locations
              WHERE $3::uuid IS NULL
                 OR id = $3
                 OR path LIKE (SELECT path || '/%' FROM inventory.locations WHERE id = $3)
         ),
         layer_cost AS (
             SELECT variant_id,
                    sum(remaining * unit_cost) / sum(remaining) AS unit_cost
               FROM inventory.valuation_layers
              WHERE remaining > 0
              GROUP BY variant_id
         )
         SELECT q.variant_id, q.location_id, q.lot_id,
                q.quantity::text AS quantity, q.reserved::text AS reserved,
                v.code AS variant_code,
                i.id AS item_id, i.name AS item_name,
                loc.path AS location_path,
                lt.number AS lot_number, lt.expires_on,
                u.code AS unit_code,
                (SELECT string_agg(av.name, ' / ' ORDER BY a.position, a.name)
                   FROM inventory.variant_values vv
                   JOIN inventory.attribute_values av ON av.id = vv.value_id
                   JOIN inventory.attributes a ON a.id = vv.attribute_id
                  WHERE vv.variant_id = q.variant_id) AS combination,
                (q.quantity * CASE WHEN cat.costing_method = 'fifo'
                                   THEN COALESCE(lc.unit_cost, i.cost)
                                   ELSE i.cost END)::numeric(19, 4)::text AS value
           FROM inventory.stock_quants q
           JOIN inventory.item_variants v ON v.id = q.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.stock_unit_id
           JOIN inventory.categories cat ON cat.id = i.category_id
           JOIN inventory.locations loc ON loc.id = q.location_id
           LEFT JOIN inventory.lots lt ON lt.id = q.lot_id
           LEFT JOIN layer_cost lc ON lc.variant_id = q.variant_id
          WHERE loc.id IN (SELECT id FROM scope)
            AND ($1::uuid IS NULL OR i.id = $1)
            AND ($2::uuid IS NULL OR q.variant_id = $2)
            AND ($4::uuid IS NULL OR loc.warehouse_id = $4)
            AND ($5::uuid IS NULL OR q.lot_id = $5)
            AND ($6 OR q.quantity <> 0)
          ORDER BY i.name, v.code, loc.path, lt.number NULLS FIRST",
    )
    .bind(filter.item_id)
    .bind(filter.variant_id)
    .bind(filter.location_id)
    .bind(filter.warehouse_id)
    .bind(filter.lot_id)
    .bind(filter.include_empty)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let reserved: String = row.try_get("reserved")?;
            let value: String = row.try_get("value")?;

            Ok(OnHandRow {
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                item_id: row.try_get("item_id")?,
                item_name: row.try_get("item_name")?,
                combination: row.try_get("combination")?,
                location_id: row.try_get("location_id")?,
                location_path: row.try_get("location_path")?,
                lot_id: row.try_get("lot_id")?,
                lot_number: row.try_get("lot_number")?,
                expires_on: row.try_get("expires_on")?,
                quantity: read_quantity(&quantity, "stock_quants.quantity")?,
                reserved: read_quantity(&reserved, "stock_quants.reserved")?,
                unit_code: row.try_get("unit_code")?,
                value: read_money(&value, currency, "stock_quants.value")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What is on hand of one variant across every internal location.
///
/// Internal only: stock at a vendor's is not ours and stock in transit is not
/// on a shelf anybody can pick from.
pub async fn total_on_hand<'e, E>(executor: E, variant_id: Uuid) -> Result<Quantity, DbError>
where
    E: PgExecutor<'e>,
{
    let total: String = sqlx::query_scalar(
        "SELECT COALESCE(sum(q.quantity), 0)::text
           FROM inventory.stock_quants q
           JOIN inventory.locations l ON l.id = q.location_id
          WHERE q.variant_id = $1 AND l.kind = 'internal'",
    )
    .bind(variant_id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Quantity::parse(&total).map_err(|err| {
        DbError::CorruptRow(format!("stock_quants sums to '{total}', which is not a quantity: {err}"))
    })
}

/// What is on hand of one item across all of its variants, at every internal
/// location.
///
/// What the running average is blended over. Item-level because the `cost`
/// column is: blending a receipt of the red medium one against the red medium
/// one's own on-hand would write an average for the whole item out of a corner
/// of it.
pub async fn item_on_hand<'e, E>(executor: E, item_id: Uuid) -> Result<Quantity, DbError>
where
    E: PgExecutor<'e>,
{
    let total: String = sqlx::query_scalar(
        "SELECT COALESCE(sum(q.quantity), 0)::text
           FROM inventory.stock_quants q
           JOIN inventory.item_variants v ON v.id = q.variant_id
           JOIN inventory.locations l ON l.id = q.location_id
          WHERE v.item_id = $1 AND l.kind = 'internal'",
    )
    .bind(item_id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Quantity::parse(&total).map_err(|err| {
        DbError::CorruptRow(format!(
            "stock_quants sums to '{total}', which is not a quantity: {err}"
        ))
    })
}

/// What is on hand of each variant of one item, at every internal location.
///
/// One query for a whole variants tab. A variant nothing has ever been received
/// of is absent rather than zero, and the caller decides which of the two it
/// wants to draw.
pub async fn on_hand_by_variant<'e, E>(
    executor: E,
    item_id: Uuid,
) -> Result<std::collections::HashMap<Uuid, Quantity>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT q.variant_id, sum(q.quantity)::text AS quantity
           FROM inventory.stock_quants q
           JOIN inventory.item_variants v ON v.id = q.variant_id
           JOIN inventory.locations l ON l.id = q.location_id
          WHERE v.item_id = $1 AND l.kind = 'internal'
          GROUP BY q.variant_id",
    )
    .bind(item_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;

            Ok((
                row.try_get("variant_id")?,
                read_quantity(&quantity, "stock_quants.quantity")?,
            ))
        })
        .collect::<Result<std::collections::HashMap<_, _>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Whether any quantity of this item is anywhere, in any lot.
///
/// The question `item::save` asks before letting somebody change the stock unit
/// or the tracking mode, and the one `delete` asks before removing an item.
/// Counterpart locations count: a receipt that was later returned still means
/// this item has moved.
pub async fn item_has_stock<'e, E>(executor: E, item_id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1
               FROM inventory.stock_quants q
               JOIN inventory.item_variants v ON v.id = q.variant_id
              WHERE v.item_id = $1 AND q.quantity <> 0
         )",
    )
    .bind(item_id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)
}
