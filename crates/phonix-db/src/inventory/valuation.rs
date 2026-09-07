//! `inventory.valuation_layers` and `layer_consumptions`: what stock cost.
//!
//! # The layers are read under a lock, in the order the strategy wants
//!
//! [`open_layers`] takes `FOR UPDATE`, for the reason a quant does: two issues
//! of the same item at the same moment must not both consume the same forty
//! units at 2.00 and leave the layer at minus forty. The order is the
//! category's removal strategy, decided in SQL rather than in the caller so the
//! `LIMIT` can stay on the query.
//!
//! # A consumption is written, never inferred
//!
//! Decrementing `remaining` alone would leave "these forty went out at 2.15" as
//! a number the system asserts. `layer_consumptions` makes it a join, which is
//! what an auditor asks for and what a FIFO cost has to be able to show.

use app_inventory::category::RemovalStrategy;
use app_inventory::quantity::Quantity;
use app_inventory::valuation::{Consumed, Layer};
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{AssertSqlSafe, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

/// How the strategy orders what is consumed first.
///
/// LIFO is the same query read backwards. FEFO sorts by the lot's expiry and
/// falls back to age for anything undated, which is what makes it safe to set
/// on a category holding one item nobody dates. "Closest location" has no
/// meaning for a *cost* layer - a layer is not in a place - so it costs as FIFO
/// does and only its picking order differs.
const fn order_by(strategy: RemovalStrategy) -> &'static str {
    match strategy {
        RemovalStrategy::Lifo => "l.created_at DESC",
        RemovalStrategy::Fefo => "lt.expires_on NULLS LAST, l.created_at",
        RemovalStrategy::Fifo | RemovalStrategy::ClosestLocation => "l.created_at",
    }
}

/// Every layer of this variant with anything left in it, held until the
/// transaction ends.
pub async fn open_layers(
    conn: &mut PgConnection,
    variant_id: Uuid,
    strategy: RemovalStrategy,
    currency: Currency,
) -> Result<Vec<Layer>, DbError> {
    // `AssertSqlSafe` on the same terms as `audit`: the only thing
    // interpolated is one of the three constants `order_by` can return, and
    // the variant is a bound parameter.
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT l.id, l.move_id, l.variant_id,
                l.quantity::text AS quantity, l.remaining::text AS remaining,
                l.unit_cost::text AS unit_cost, l.value::text AS value,
                l.created_at
           FROM inventory.valuation_layers l
           LEFT JOIN inventory.lots lt ON lt.id = l.lot_id
          WHERE l.variant_id = $1 AND l.remaining > 0
          ORDER BY {}
            FOR UPDATE OF l",
        order_by(strategy)
    )))
    .bind(variant_id)
    .fetch_all(conn)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let remaining: String = row.try_get("remaining")?;
            let unit_cost: String = row.try_get("unit_cost")?;
            let value: String = row.try_get("value")?;

            Ok(Layer {
                id: row.try_get("id")?,
                move_id: row.try_get("move_id")?,
                variant_id: row.try_get("variant_id")?,
                quantity: read_quantity(&quantity, "valuation_layers.quantity")?,
                remaining: read_quantity(&remaining, "valuation_layers.remaining")?,
                unit_cost: read_money(&unit_cost, currency, "valuation_layers.unit_cost")?,
                value: read_money(&value, currency, "valuation_layers.value")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Record what a receipt brought in.
pub async fn insert_layer(
    conn: &mut PgConnection,
    move_id: Uuid,
    variant_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: Quantity,
    unit_cost: Money,
    value: Money,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.valuation_layers
             (move_id, variant_id, lot_id, quantity, remaining, unit_cost, value)
          VALUES ($1, $2, $3, $4::numeric, $4::numeric, $5::numeric, $6::numeric)
       RETURNING id",
    )
    .bind(move_id)
    .bind(variant_id)
    .bind(lot_id)
    .bind(quantity.to_storage_string())
    .bind(unit_cost.to_storage_string())
    .bind(value.to_storage_string())
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// Spend what an issue worked out, and write down which layer paid.
pub async fn consume(
    conn: &mut PgConnection,
    move_id: Uuid,
    lines: &[Consumed],
) -> Result<(), DbError> {
    for line in lines {
        // `remaining - $2` cannot go below zero: the layers were locked and the
        // arithmetic was done against what the lock returned. The CHECK on the
        // column is the backstop if that ever stops being true.
        sqlx::query(
            "UPDATE inventory.valuation_layers
                SET remaining = remaining - $2::numeric
              WHERE id = $1",
        )
        .bind(line.layer_id)
        .bind(line.quantity.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

        sqlx::query(
            "INSERT INTO inventory.layer_consumptions
                 (layer_id, move_id, quantity, unit_cost, value)
              VALUES ($1, $2, $3::numeric, $4::numeric, $5::numeric)",
        )
        .bind(line.layer_id)
        .bind(move_id)
        .bind(line.quantity.to_storage_string())
        .bind(line.unit_cost.to_storage_string())
        .bind(line.value.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// What the workspace holds in stock, at what its costing method says.
///
/// The number a stock account is reconciled against - ADR 0006 section 6.7,
/// which is the whole reason this is one query rather than a spreadsheet. FIFO
/// values what is left in each layer at that layer's own cost; the other two
/// have a single cost per item, and asking the layers for it would answer the
/// price of the oldest carton rather than the price the workspace uses.
pub async fn total_value<'e, E>(executor: E, currency: Currency) -> Result<Money, DbError>
where
    E: PgExecutor<'e>,
{
    let raw: String = sqlx::query_scalar(
        "SELECT COALESCE(sum(
                    l.remaining * CASE WHEN c.costing_method = 'fifo'
                                       THEN l.unit_cost
                                       ELSE i.cost END
                ), 0)::numeric(19, 4)::text
           FROM inventory.valuation_layers l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.categories c ON c.id = i.category_id
          WHERE l.remaining > 0",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Money::parse(currency, &raw).map_err(|err| {
        DbError::CorruptRow(format!("valuation_layers sum to '{raw}', which is not an amount: {err}"))
    })
}
