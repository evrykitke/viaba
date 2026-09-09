//! `inventory.consolidations`, its lines, and what an order line was bought for.
//!
//! # A consolidation carries no currency
//!
//! Like a requisition and unlike an order. The supplier is chosen per *line*,
//! so there is no one currency for the document - each line is priced in the
//! currency of whoever is being bought from, and the orders confirming it
//! produces each carry their own.
//!
//! # Saving replaces the lines
//!
//! [`save_lines`] deletes and re-inserts, on the same terms as an order's and a
//! requisition's. Nothing is lost by it: a consolidation line accumulates no
//! `received` or `ordered` figure, because the moment it would start to is the
//! moment it stops being editable.
//!
//! # The allocation is not read from the draft
//!
//! [`outstanding_lines_for_update`] is what a confirm consumes, and it locks the
//! requisition lines it returns. Two buyers confirming two consolidations for
//! the same item at the same time would otherwise both see the same outstanding
//! demand and both allocate it - and the second would be refused, but only by
//! `requisition_lines_ordered_within_request` after the orders had already been
//! raised. The lock makes the second wait and see the truth.

use app_inventory::consolidation::{
    Allocation, Checked, Consolidation, ConsolidationLine, ConsolidationState,
    ConsolidationSummary, LineAllocation, RaisedOrder,
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
        format!("consolidations.{column} holds '{raw}', which this build does not know").into(),
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

fn read_state(raw: &str) -> Result<ConsolidationState, sqlx::Error> {
    ConsolidationState::parse(raw).ok_or_else(|| unknown("state", raw))
}

fn read_currency(code: &str) -> Result<Currency, sqlx::Error> {
    Currency::parse(code).map_err(|_| unknown("currency", code))
}

/// The grid's shape, written once so two screens cannot disagree about it.
///
/// `supplier_count` is the interesting one: it is how many orders confirming
/// this would raise, which is the question the list is really being asked.
const SUMMARY_COLUMNS: &str = "
    c.id, c.number, c.state, c.raised_on,
    w.name AS warehouse_name,
    u.display_name AS raised_by_name,
    (SELECT count(*) FROM inventory.consolidation_lines l
      WHERE l.consolidation_id = c.id) AS line_count,
    (SELECT count(DISTINCT l.supplier_id) FROM inventory.consolidation_lines l
      WHERE l.consolidation_id = c.id AND l.supplier_id IS NOT NULL) AS supplier_count,
    (SELECT count(*) FROM inventory.purchase_orders o
      WHERE o.consolidation_id = c.id) AS order_count
";

fn read_summary(row: &sqlx::postgres::PgRow) -> Result<ConsolidationSummary, sqlx::Error> {
    let state: String = row.try_get("state")?;

    Ok(ConsolidationSummary {
        id: row.try_get("id")?,
        number: row.try_get("number")?,
        state: read_state(&state)?,
        warehouse_name: row.try_get("warehouse_name")?,
        raised_on: row.try_get("raised_on")?,
        raised_by_name: row.try_get("raised_by_name")?,
        line_count: row.try_get("line_count")?,
        supplier_count: row.try_get("supplier_count")?,
        order_count: row.try_get("order_count")?,
    })
}

/// Every consolidation, newest first.
pub async fn list<'e, E>(executor: E) -> Result<Vec<ConsolidationSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = format!(
        "SELECT {SUMMARY_COLUMNS}
           FROM inventory.consolidations c
           JOIN inventory.warehouses w ON w.id = c.warehouse_id
           LEFT JOIN core.users u ON u.id = c.created_by
          ORDER BY c.raised_on DESC, c.created_at DESC"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter()
        .map(read_summary)
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Consolidation>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT c.id, c.number, c.state, c.warehouse_id, c.raised_on, c.note,
                w.name AS warehouse_name,
                u.display_name AS raised_by_name
           FROM inventory.consolidations c
           JOIN inventory.warehouses w ON w.id = c.warehouse_id
           LEFT JOIN core.users u ON u.id = c.created_by
          WHERE c.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let raw: String = row.try_get("state").map_err(DbError::Query)?;
    let state = read_state(&raw).map_err(DbError::Query)?;

    Ok(Some(Consolidation {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state,
        warehouse_id: row.try_get("warehouse_id").map_err(DbError::Query)?,
        warehouse_name: row.try_get("warehouse_name").map_err(DbError::Query)?,
        raised_on: row.try_get("raised_on").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        raised_by_name: row.try_get("raised_by_name").map_err(DbError::Query)?,
        lines: lines_of(executor, id, state).await?,
        orders: orders_of(executor, id).await?,
    }))
}

/// A consolidation's lines, each with what is outstanding *now* beside the
/// snapshot taken when it was drawn.
///
/// The live figure is only fetched for a draft. On a confirmed document the
/// allocation has already happened and `demand_now` would show what is left
/// over *after* it - a true number answering a question nobody asked, and one
/// that reads as a stale-draft warning on a document that cannot be stale.
pub async fn lines_of<'e, E>(
    executor: E,
    consolidation_id: Uuid,
    state: ConsolidationState,
) -> Result<Vec<ConsolidationLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let live = state.is_editable();

    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.variant_id, l.description,
                l.quantity::text AS quantity, l.demand::text AS demand,
                l.supplier_id, l.supplier_code, l.supplier_name,
                l.unit_price::text AS unit_price, l.currency, l.note,
                v.code AS variant_code,
                su.code AS unit_code,
                CASE WHEN $2 THEN d.outstanding::text END AS demand_now
           FROM inventory.consolidation_lines l
           JOIN inventory.consolidations c ON c.id = l.consolidation_id
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units su ON su.id = i.stock_unit_id
           LEFT JOIN inventory.requisition_demand d
                  ON d.variant_id = l.variant_id AND d.warehouse_id = c.warehouse_id
          WHERE l.consolidation_id = $1
          ORDER BY l.line_no",
    )
    .bind(consolidation_id)
    .bind(live)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let demand: String = row.try_get("demand")?;
            let unit_price: Option<String> = row.try_get("unit_price")?;
            let currency: Option<String> = row.try_get("currency")?;
            let supplier_id: Option<Uuid> = row.try_get("supplier_id")?;
            let demand_now: Option<String> = row.try_get("demand_now")?;

            let unit_price = match (unit_price, currency) {
                (Some(raw), Some(code)) => Some(read_money(
                    &raw,
                    read_currency(&code)?,
                    "consolidation_lines.unit_price",
                )?),
                _ => None,
            };

            Ok(ConsolidationLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "consolidation_lines.quantity")?,
                demand: read_quantity(&demand, "consolidation_lines.demand")?,
                unit_code: row.try_get("unit_code")?,
                supplier: supplier_id
                    .map(|party_id| -> Result<SupplierSnapshot, sqlx::Error> {
                        Ok(SupplierSnapshot {
                            party_id,
                            code: row.try_get("supplier_code")?,
                            name: row.try_get("supplier_name")?,
                        })
                    })
                    .transpose()?,
                unit_price,
                note: row.try_get("note")?,
                // Nothing outstanding is no row in the view, which is a demand
                // of zero rather than an unknown one.
                demand_now: live
                    .then(|| match demand_now {
                        Some(raw) => read_quantity(&raw, "requisition_demand.outstanding"),
                        None => Ok(Quantity::ZERO),
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The orders a consolidation raised.
pub async fn orders_of<'e, E>(
    executor: E,
    consolidation_id: Uuid,
) -> Result<Vec<RaisedOrder>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT o.id, o.number, o.supplier_name, o.currency, o.net::text AS net,
                (SELECT count(*) FROM inventory.purchase_order_lines l
                  WHERE l.order_id = o.id) AS line_count
           FROM inventory.purchase_orders o
          WHERE o.consolidation_id = $1
          ORDER BY o.supplier_name, o.number",
    )
    .bind(consolidation_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let net: String = row.try_get("net")?;
            let code: String = row.try_get("currency")?;
            let currency = read_currency(&code)?;

            Ok(RaisedOrder {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                supplier_name: row.try_get("supplier_name")?,
                currency: code,
                net: read_money(&net, currency, "purchase_orders.net")?,
                line_count: row.try_get("line_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The header, on create.
pub async fn insert(
    conn: &mut PgConnection,
    draft: &Checked,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.consolidations
             (warehouse_id, raised_on, note, created_by, updated_by)
          VALUES ($1, $2, $3, $4, $4)
       RETURNING id",
    )
    .bind(draft.warehouse_id)
    .bind(draft.raised_on)
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// The header, on edit. `false` where the row is gone or is no longer a draft.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &Checked,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.consolidations
            SET warehouse_id = $2, raised_on = $3, note = $4,
                updated_at = now(), updated_by = $5
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(draft.warehouse_id)
    .bind(draft.raised_on)
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// One line, as the service worked it out: the supplier looked up and
/// snapshotted, the price parsed in that supplier's own currency.
pub struct SourcedLine<'a> {
    pub source: &'a app_inventory::consolidation::CheckedLine,
    pub supplier: Option<SupplierSnapshot>,
    pub unit_price: Option<Money>,
}

/// Replace a consolidation's lines with what the form holds.
pub async fn save_lines(
    conn: &mut PgConnection,
    consolidation_id: Uuid,
    lines: &[SourcedLine<'_>],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.consolidation_lines WHERE consolidation_id = $1")
        .bind(consolidation_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (index, line) in lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory.consolidation_lines
                 (consolidation_id, line_no, variant_id, description, quantity,
                  demand, supplier_id, supplier_code, supplier_name,
                  unit_price, currency, note)
              VALUES ($1, $2, $3, $4, $5::numeric, $6::numeric, $7, $8, $9,
                      $10::numeric, $11, $12)",
        )
        .bind(consolidation_id)
        .bind(index as i32 + 1)
        .bind(line.source.variant_id)
        .bind(line.source.description.as_str())
        .bind(line.source.quantity.to_storage_string())
        .bind(line.source.demand.to_storage_string())
        .bind(line.supplier.as_ref().map(|supplier| supplier.party_id))
        .bind(line.supplier.as_ref().map(|supplier| supplier.code.as_str()))
        .bind(line.supplier.as_ref().map(|supplier| supplier.name.as_str()))
        .bind(line.unit_price.map(|price| price.to_storage_string()))
        .bind(line.unit_price.map(|price| price.currency().to_string()))
        .bind(line.source.note.as_deref())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Give a draft its number and mark it as having raised its orders.
///
/// `WHERE state = 'draft'` in the statement, so two clicks raise one set of
/// orders rather than two.
pub async fn confirm(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.consolidations
            SET state = 'confirmed', number = $2,
                confirmed_at = now(), confirmed_by = $3,
                updated_at = now(), updated_by = $3
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Abandon one. Only a draft: a confirmed consolidation has raised orders, and
/// the way to undo those is to cancel them.
pub async fn cancel(
    conn: &mut PgConnection,
    id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.consolidations
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

/// A draft, thrown away. Lines go with it by `ON DELETE CASCADE`.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query("DELETE FROM inventory.consolidations WHERE id = $1 AND state = 'draft'")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Say that an order was raised by a consolidation.
pub async fn attach_order(
    conn: &mut PgConnection,
    order_id: Uuid,
    consolidation_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query("UPDATE inventory.purchase_orders SET consolidation_id = $2 WHERE id = $1")
        .bind(order_id)
        .bind(consolidation_id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(())
}

/// The outstanding lines behind one item's demand, oldest request first, locked.
///
/// The same query as `requisition::outstanding_lines_for` with `FOR UPDATE`
/// added, and it is a separate function rather than a flag because the lock is
/// only correct inside the transaction that is about to write. Reading it for a
/// screen would hold rows for as long as the page took to render.
pub async fn outstanding_lines_for_update(
    conn: &mut PgConnection,
    variant_id: Uuid,
    warehouse_id: Uuid,
) -> Result<Vec<(Uuid, Quantity)>, DbError> {
    let rows = sqlx::query(
        "SELECT l.id, (l.quantity_stock - l.ordered)::text AS outstanding
           FROM inventory.requisition_lines l
           JOIN inventory.requisitions r ON r.id = l.requisition_id
          WHERE r.state = 'approved'
            AND r.warehouse_id = $2
            AND l.variant_id = $1
            AND l.ordered < l.quantity_stock
          ORDER BY r.raised_on, r.created_at, l.line_no
            FOR UPDATE OF l",
    )
    .bind(variant_id)
    .bind(warehouse_id)
    .fetch_all(conn)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let outstanding: String = row.try_get("outstanding")?;

            Ok((
                row.try_get("id")?,
                read_quantity(&outstanding, "requisition_lines.outstanding")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Record that an order line was raised for part of a requisition line.
pub async fn record_source(
    conn: &mut PgConnection,
    order_line_id: Uuid,
    requisition_line_id: Uuid,
    consolidation_id: Uuid,
    quantity: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO inventory.purchase_order_line_sources
             (order_line_id, requisition_line_id, consolidation_id, quantity)
          VALUES ($1, $2, $3, $4::numeric)",
    )
    .bind(order_line_id)
    .bind(requisition_line_id)
    .bind(consolidation_id)
    .bind(quantity.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// The ids of an order's lines, in line order.
///
/// `purchase::save_lines` inserts without returning ids because the ordinary
/// order form has no use for them. Consolidation does: each line needs its
/// sources written against it.
pub async fn order_line_ids(conn: &mut PgConnection, order_id: Uuid) -> Result<Vec<Uuid>, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM inventory.purchase_order_lines
          WHERE order_id = $1 ORDER BY line_no",
    )
    .bind(order_id)
    .fetch_all(conn)
    .await
    .map_err(DbError::Query)
}

/// What each line of an order was bought for: which requisitions, whose cost
/// centres, and how much of it nobody asked for.
///
/// Reads `purchase_order_line_allocation` for the totals and joins the sources
/// back through the requisition for the names. The cost centre comes off the
/// requisition rather than a snapshot here - see `0008`'s header for why there
/// is no second copy.
pub async fn allocations_of<'e, E>(
    executor: E,
    order_id: Uuid,
) -> Result<Vec<LineAllocation>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let rows = sqlx::query(
        "SELECT a.order_line_id, a.quantity_stock::text AS quantity_stock,
                a.allocated::text AS allocated, a.unallocated::text AS unallocated,
                l.description, l.line_no
           FROM inventory.purchase_order_line_allocation a
           JOIN inventory.purchase_order_lines l ON l.id = a.order_line_id
          WHERE a.order_id = $1
          ORDER BY l.line_no",
    )
    .bind(order_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let sources = sqlx::query(
        "SELECT s.order_line_id, s.requisition_line_id, s.quantity::text AS quantity,
                r.id AS requisition_id, r.number AS requisition_number,
                r.cost_centre_id, r.cost_centre_name
           FROM inventory.purchase_order_line_sources s
           JOIN inventory.purchase_order_lines l ON l.id = s.order_line_id
           JOIN inventory.requisition_lines rl ON rl.id = s.requisition_line_id
           JOIN inventory.requisitions r ON r.id = rl.requisition_id
          WHERE l.order_id = $1
          ORDER BY r.raised_on, r.number",
    )
    .bind(order_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let order_line_id: Uuid = row.try_get("order_line_id")?;
            let quantity_stock: String = row.try_get("quantity_stock")?;
            let allocated: String = row.try_get("allocated")?;
            let unallocated: String = row.try_get("unallocated")?;

            let mine = sources
                .iter()
                .filter(|source| {
                    source
                        .try_get::<Uuid, _>("order_line_id")
                        .is_ok_and(|id| id == order_line_id)
                })
                .map(|source| {
                    let quantity: String = source.try_get("quantity")?;

                    Ok(Allocation {
                        requisition_line_id: source.try_get("requisition_line_id")?,
                        requisition_id: source.try_get("requisition_id")?,
                        requisition_number: source.try_get("requisition_number")?,
                        cost_centre_id: source.try_get("cost_centre_id")?,
                        cost_centre_name: source.try_get("cost_centre_name")?,
                        quantity: read_quantity(&quantity, "purchase_order_line_sources.quantity")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;

            Ok(LineAllocation {
                order_line_id,
                description: row.try_get("description")?,
                quantity_stock: read_quantity(
                    &quantity_stock,
                    "purchase_order_lines.quantity_stock",
                )?,
                allocated: read_quantity(&allocated, "purchase_order_line_allocation.allocated")?,
                unallocated: read_quantity(
                    &unallocated,
                    "purchase_order_line_allocation.unallocated",
                )?,
                sources: mine,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}
