//! `inventory.deliveries` and its lines.
//!
//! The mirror of [`super::receipt`].
//!
//! # Amounts here are the workspace's own currency, and they are COST
//!
//! A delivery posts what the goods cost, not what they sold for. The selling
//! price is the order's and the invoice's business; this table holds the figure
//! the journal moved out of stock.
//!
//! # The cost is written back AFTER the move
//!
//! A receipt knows what a line costs before it posts - the supplier said so. A
//! delivery does not: under average or FIFO the cost of the units going out is
//! decided by the layers, and only the move knows which layers it consumed. So
//! the line is stored with nothing, and [`record_move`] writes back the id, the
//! unit cost and the value together.
//!
//! # `move_id` is the thread from the paperwork to the ledger
//!
//! One per line, set at post. It is what lets a stock move answer "which
//! despatch note was this".

use app_inventory::delivery::{
    CheckedDelivery, Delivery, DeliveryLine, DeliveryState, DeliverySummary,
};
use app_inventory::quantity::Quantity;
use app_inventory::sales_order::CustomerSnapshot;
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::query::{Page, PageRequest};
use sqlx::{AssertSqlSafe, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::listing::{self, Sortable};

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("deliveries.{column} holds '{raw}', which this build does not know").into(),
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

/// The range key the deliveries grid declares, and so the pair of filter keys -
/// `despatched_from` and `despatched_to` - that arrive with a request.
///
/// A constant because it is written in two crates that must agree and do not
/// depend on each other: here, and `ui::table::config::deliveries`.
pub const DESPATCHED: &str = "despatched";

/// The filter key naming which state to show.
pub const STATE: &str = "state";

/// The columns the deliveries grid may order by.
const SORTABLE: &[Sortable] = &[
    ("number", "d.number"),
    ("customer", "d.customer_name"),
    ("despatched_on", "d.despatched_on"),
    ("order", "o.number"),
    ("value", "d.value"),
    ("line_count", "line_count"),
];

/// Everything a row is read from or narrowed by, shared between the count and
/// the select so the pager and the page cannot disagree about which rows exist.
const FROM: &str = "FROM inventory.deliveries d
           JOIN inventory.warehouses w ON w.id = d.warehouse_id
           LEFT JOIN inventory.sales_orders o ON o.id = d.order_id";

/// A filter nobody set is a NULL that discards its own line, so one clause
/// serves every combination and nothing is interpolated.
const WHERE: &str = "WHERE ($1::text IS NULL
                 OR d.number ILIKE $1
                 OR d.customer_name ILIKE $1
                 OR d.carrier_reference ILIKE $1
                 OR o.number ILIKE $1
                 OR w.name ILIKE $1)
            AND ($2::date IS NULL OR d.despatched_on >= $2)
            AND ($3::date IS NULL OR d.despatched_on <= $3)
            AND ($4::text IS NULL OR d.state = $4)";

/// One page of the deliveries list, newest first.
///
/// Paged in SQL because goods leaving is a thing that happened: nothing deletes one,
/// and the list grows for as long as the workspace trades.
///
/// Two statements, a count and a select, so the page can be pulled back to one
/// that exists before the rows are fetched.
pub async fn page(
    pool: &sqlx::PgPool,
    currency: Currency,
    request: &PageRequest,
) -> Result<Page<DeliverySummary>, DbError> {
    let request = request.sanitised();
    let needle = request
        .needle()
        .map(|needle| crate::search::contains(&needle));
    let state = request.filter(STATE).and_then(DeliveryState::parse);
    let despatched = request.range(DESPATCHED);

    // `AssertSqlSafe` because these statements are composed rather than
    // written: `FROM` and `WHERE` are constants, and `order` can only be a
    // string this file put in `SORTABLE`. Nothing from a browser reaches the
    // text of the query.
    let counting = AssertSqlSafe(format!("SELECT count(*) {FROM} {WHERE}"));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(needle.as_deref())
        .bind(despatched.first_day())
        .bind(despatched.last_day())
        .bind(state.map(DeliveryState::as_str))
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    // Newest first, and `created_at` after it whatever the sort: two documents
    // on the same day would otherwise swap places between one page and the
    // next, which shows up as a row that appears twice.
    let order = listing::order_by(request.sort.as_ref(), SORTABLE, "d.despatched_on DESC");

    let selecting = AssertSqlSafe(format!(
        "SELECT d.id, d.number, d.state, d.customer_name, d.despatched_on,
                d.carrier_reference, d.value::text AS value,
                o.number AS order_number,
                w.name AS warehouse_name,
                (SELECT count(*) FROM inventory.delivery_lines l WHERE l.delivery_id = d.id)
                    AS line_count
           {FROM}
           {WHERE}
          ORDER BY {order}, d.created_at DESC
          LIMIT $5 OFFSET $6"
    ));

    let rows = sqlx::query(selecting)
        .bind(needle.as_deref())
        .bind(despatched.first_day())
        .bind(despatched.last_day())
        .bind(state.map(DeliveryState::as_str))
        .bind(request.limit() as i64)
        .bind(request.offset() as i64)
        .fetch_all(pool)
        .await
        .map_err(DbError::Query)?;

    let summaries = rows
        .into_iter()
        .map(|row| {
            let state: String = row.try_get("state")?;
            let value: String = row.try_get("value")?;
            let order_number: Option<String> = row.try_get("order_number")?;

            Ok(DeliverySummary {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                state: DeliveryState::parse(&state).ok_or_else(|| unknown("state", &state))?,
                customer_name: row.try_get("customer_name")?,
                // Empty for a draft order, which is not the same as no order.
                order_number: order_number.filter(|number| !number.is_empty()),
                warehouse_name: row.try_get("warehouse_name")?,
                despatched_on: row.try_get("despatched_on")?,
                carrier_reference: row.try_get("carrier_reference")?,
                value: read_money(&value, currency, "deliveries.value")?,
                line_count: row.try_get("line_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)?;

    Ok(Page::new(summaries, total, &request))
}

pub async fn find<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<Delivery>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT d.id, d.number, d.state, d.order_id, d.customer_id, d.customer_code,
                d.customer_name, d.warehouse_id, d.from_location_id, d.despatched_on,
                d.carrier_reference, d.note, d.value::text AS value,
                o.number AS order_number,
                w.name AS warehouse_name,
                loc.path AS from_location_path
           FROM inventory.deliveries d
           JOIN inventory.warehouses w ON w.id = d.warehouse_id
           JOIN inventory.locations loc ON loc.id = d.from_location_id
           LEFT JOIN inventory.sales_orders o ON o.id = d.order_id
          WHERE d.id = $1",
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

    Ok(Some(Delivery {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: DeliveryState::parse(&state)
            .ok_or_else(|| DbError::Query(unknown("state", &state)))?,
        order_id: row.try_get("order_id").map_err(DbError::Query)?,
        order_number: order_number.filter(|number| !number.is_empty()),
        customer: CustomerSnapshot {
            party_id: row.try_get("customer_id").map_err(DbError::Query)?,
            code: row.try_get("customer_code").map_err(DbError::Query)?,
            name: row.try_get("customer_name").map_err(DbError::Query)?,
        },
        warehouse_id: row.try_get("warehouse_id").map_err(DbError::Query)?,
        warehouse_name: row.try_get("warehouse_name").map_err(DbError::Query)?,
        from_location_id: row.try_get("from_location_id").map_err(DbError::Query)?,
        from_location_path: row.try_get("from_location_path").map_err(DbError::Query)?,
        despatched_on: row.try_get("despatched_on").map_err(DbError::Query)?,
        carrier_reference: row.try_get("carrier_reference").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        value: read_money(&value, currency, "deliveries.value").map_err(DbError::Query)?,
        lines: lines_of(executor, id, currency).await?,
    }))
}

pub async fn lines_of<'e, E>(
    executor: E,
    delivery_id: Uuid,
    currency: Currency,
) -> Result<Vec<DeliveryLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.order_line_id, l.variant_id, l.description,
                l.quantity::text AS quantity, l.lot_id,
                l.unit_cost::text AS unit_cost, l.value::text AS value, l.move_id,
                v.code AS variant_code,
                u.code AS unit_code,
                lot.number AS lot_number
           FROM inventory.delivery_lines l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.stock_unit_id
           LEFT JOIN inventory.lots lot ON lot.id = l.lot_id
          WHERE l.delivery_id = $1
          ORDER BY l.line_no",
    )
    .bind(delivery_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let unit_cost: String = row.try_get("unit_cost")?;
            let value: String = row.try_get("value")?;

            Ok(DeliveryLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                order_line_id: row.try_get("order_line_id")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "delivery_lines.quantity")?,
                unit_code: row.try_get("unit_code")?,
                lot_id: row.try_get("lot_id")?,
                lot_number: row.try_get("lot_number")?,
                unit_cost: read_money(&unit_cost, currency, "delivery_lines.unit_cost")?,
                value: read_money(&value, currency, "delivery_lines.value")?,
                move_id: row.try_get("move_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

pub async fn insert(
    conn: &mut PgConnection,
    draft: &CheckedDelivery,
    customer: &CustomerSnapshot,
    from_location_id: Uuid,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.deliveries
             (order_id, customer_id, customer_code, customer_name, warehouse_id,
              from_location_id, despatched_on, carrier_reference, note,
              created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
       RETURNING id",
    )
    .bind(draft.order_id)
    .bind(customer.party_id)
    .bind(&customer.code)
    .bind(&customer.name)
    .bind(draft.warehouse_id)
    .bind(from_location_id)
    .bind(draft.despatched_on)
    .bind(draft.carrier_reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &CheckedDelivery,
    customer: &CustomerSnapshot,
    from_location_id: Uuid,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.deliveries
            SET order_id = $2, customer_id = $3, customer_code = $4, customer_name = $5,
                warehouse_id = $6, from_location_id = $7, despatched_on = $8,
                carrier_reference = $9, note = $10, updated_at = now(), updated_by = $11
          WHERE id = $1 AND state = 'draft'",
    )
    .bind(id)
    .bind(draft.order_id)
    .bind(customer.party_id)
    .bind(&customer.code)
    .bind(&customer.name)
    .bind(draft.warehouse_id)
    .bind(from_location_id)
    .bind(draft.despatched_on)
    .bind(draft.carrier_reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Replace a draft's lines. Same reasoning as an order's.
///
/// No cost goes in. A delivery's cost is decided by the layers when the move is
/// applied, and a number written here would be a guess that the post then
/// contradicts.
pub async fn save_lines(
    conn: &mut PgConnection,
    delivery_id: Uuid,
    lines: &[LineToStore<'_>],
) -> Result<Vec<Uuid>, DbError> {
    sqlx::query("DELETE FROM inventory.delivery_lines WHERE delivery_id = $1")
        .bind(delivery_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    let mut ids = Vec::with_capacity(lines.len());

    for (index, line) in lines.iter().enumerate() {
        let id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO inventory.delivery_lines
                 (delivery_id, line_no, order_line_id, variant_id, description,
                  quantity, lot_id)
              VALUES ($1, $2, $3, $4, $5, $6::numeric, $7)
           RETURNING id",
        )
        .bind(delivery_id)
        .bind(index as i32 + 1)
        .bind(line.source.order_line_id)
        .bind(line.source.variant_id)
        .bind(&line.description)
        .bind(line.source.quantity.to_storage_string())
        .bind(line.source.lot_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(DbError::Query)?;

        ids.push(id);
    }

    Ok(ids)
}

/// One line as the service resolved it. Only the description is worked out -
/// everything else on a delivery line is what somebody typed.
pub struct LineToStore<'a> {
    pub source: &'a app_inventory::delivery::CheckedDeliveryLine,
    pub description: String,
}

/// Tie a posted line to the movement it became, and record what it cost.
///
/// One statement for all three, because they are one fact: this line moved,
/// and the move is what decided the cost. Writing the id without the cost would
/// leave a posted line claiming the goods were free.
pub async fn record_move(
    conn: &mut PgConnection,
    line_id: Uuid,
    move_id: Uuid,
    unit_cost: Money,
    value: Money,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.delivery_lines
            SET move_id = $2, unit_cost = $3::numeric, value = $4::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(move_id)
    .bind(unit_cost.to_storage_string())
    .bind(value.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Give the delivery its number and close it.
pub async fn post(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    value: Money,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.deliveries
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
        "UPDATE inventory.deliveries
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
    let done = sqlx::query("DELETE FROM inventory.deliveries WHERE id = $1 AND state = 'draft'")
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}
