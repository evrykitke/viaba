//! `inventory.purchase_orders` and its lines.
//!
//! # Each order carries its own currency
//!
//! Unlike everything else in this schema, which is denominated in the
//! workspace's own. A supplier quotes in theirs, so every read here parses the
//! amounts against the row's `currency` column rather than against a currency
//! passed in - and the conversion to the workspace's own happens at the
//! *receipt*, because that is when the value arrives.
//!
//! # Saving an order replaces its lines
//!
//! [`save_lines`] deletes and re-inserts rather than diffing. An order is edited
//! as a whole - rows added, removed and reordered in one form - and a diff would
//! be machinery to reproduce what the form already knows. It takes a
//! transaction, because an order whose header saved and whose lines did not is
//! worse than neither.

use app_inventory::purchase::{
    Checked, CheckedLine, OrderLine, OrderState, OrderSummary, PurchaseOrder, ReceiptState,
    SupplierSnapshot,
};
use app_inventory::quantity::Quantity;
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::query::{MAX_PER_PAGE, Page, PageRequest};
use sqlx::{AssertSqlSafe, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::listing::{self, Sortable};

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("purchase_orders.{column} holds '{raw}', which this build does not know").into(),
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

/// The range key the order grid declares, and so the pair of filter keys -
/// `ordered_from` and `ordered_to` - that arrive with a request.
///
/// A constant because it is written in two crates that must agree and do not
/// depend on each other: here, and `ui::table::config::purchase_orders`.
pub const ORDERED: &str = "ordered";

/// The filter key naming which states to show, by group rather than one each -
/// see [`OrderState::group`].
pub const STATE: &str = "state";

/// The filter key naming whether everything has arrived.
pub const RECEIVED: &str = "received";

/// The columns the order grid may order by.
const SORTABLE: &[Sortable] = &[
    ("number", "o.number"),
    ("supplier", "o.supplier_name"),
    ("order_date", "o.order_date"),
    ("expected_on", "o.expected_on"),
    ("net", "o.net"),
    ("line_count", "line_count"),
];

/// What the supplier has done, worked out from the lines.
///
/// Written once because three places need the same answer: the select shows it,
/// the `WHERE` narrows by it, and neither may be allowed to drift from the
/// other. The `CASE` order matters - over beats everything, which beats partly.
const RECEIPT_STATE: &str = "COALESCE((
                    SELECT CASE
                        WHEN bool_or(l.received > l.quantity_stock) THEN 'over'
                        WHEN bool_and(l.received >= l.quantity_stock) THEN 'everything'
                        WHEN bool_or(l.received > 0) THEN 'partly'
                        ELSE 'nothing' END
                      FROM inventory.purchase_order_lines l
                     WHERE l.order_id = o.id AND NOT l.is_cancelled
                ), 'nothing')";

/// One page of the order list, newest first.
///
/// Paged in SQL because a purchase ledger is a list nothing deletes from - a
/// cancelled order is still what was cancelled - so it only grows.
///
/// Two statements, a count and a select, so the page can be pulled back to one
/// that exists before the rows are fetched. Both narrow by the same `WHERE`,
/// which is built here rather than written twice.
pub async fn page(
    pool: &sqlx::PgPool,
    request: &PageRequest,
) -> Result<Page<OrderSummary>, DbError> {
    let request = request.sanitised();
    let needle = request
        .needle()
        .map(|needle| crate::search::contains(&needle));
    let ordered = request.range(ORDERED);

    // Groups rather than states: see `OrderState::group`. A name this build
    // does not know covers no states, which would match nothing, so it narrows
    // nothing instead - a browser running a newer screen must not empty a list.
    let states: Option<Vec<String>> = request.filter(STATE).and_then(|group| {
        let states = OrderState::in_group(group);

        (!states.is_empty()).then(|| {
            states
                .into_iter()
                .map(|state| state.as_str().to_owned())
                .collect()
        })
    });

    let received: Option<Vec<String>> = match request.filter(RECEIVED) {
        Some("complete") => Some(true),
        Some("outstanding") => Some(false),
        _ => None,
    }
    .map(|complete| {
        ReceiptState::complete_or_not(complete)
            .into_iter()
            .map(|state| state.as_str().to_owned())
            .collect()
    });

    let where_clause = format!(
        "WHERE ($1::text IS NULL
                 OR o.number ILIKE $1
                 OR o.supplier_name ILIKE $1
                 OR w.name ILIKE $1)
            AND ($2::date IS NULL OR o.order_date >= $2)
            AND ($3::date IS NULL OR o.order_date <= $3)
            AND ($4::text[] IS NULL OR o.state = ANY($4::text[]))
            AND ($5::text[] IS NULL OR {RECEIPT_STATE} = ANY($5::text[]))"
    );

    // `AssertSqlSafe` because these statements are composed rather than
    // written: every piece is a constant of this file, and `order` can only be
    // a string it put in `SORTABLE`. Nothing from a browser reaches the text of
    // the query - the search, the states and the page are bound parameters.
    let counting = AssertSqlSafe(format!(
        "SELECT count(*)
           FROM inventory.purchase_orders o
           JOIN inventory.warehouses w ON w.id = o.warehouse_id
           {where_clause}"
    ));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(needle.as_deref())
        .bind(ordered.first_day())
        .bind(ordered.last_day())
        .bind(states.as_deref())
        .bind(received.as_deref())
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    // Newest first, and `created_at` after it whatever the sort: two orders
    // raised on the same day would otherwise swap places between one page and
    // the next, which shows up as a row that appears twice.
    let order = listing::order_by(request.sort.as_ref(), SORTABLE, "o.order_date DESC");

    let selecting = AssertSqlSafe(format!(
        "SELECT o.id, o.number, o.state, o.supplier_name, o.order_date, o.expected_on,
                o.currency, o.net::text AS net,
                w.name AS warehouse_name,
                (SELECT count(*) FROM inventory.purchase_order_lines l
                  WHERE l.order_id = o.id AND NOT l.is_cancelled) AS line_count,
                {RECEIPT_STATE} AS receipt_state
           FROM inventory.purchase_orders o
           JOIN inventory.warehouses w ON w.id = o.warehouse_id
           {where_clause}
          ORDER BY {order}, o.created_at DESC
          LIMIT $6 OFFSET $7"
    ));

    let rows = sqlx::query(selecting)
        .bind(needle.as_deref())
        .bind(ordered.first_day())
        .bind(ordered.last_day())
        .bind(states.as_deref())
        .bind(received.as_deref())
        .bind(request.limit() as i64)
        .bind(request.offset() as i64)
        .fetch_all(pool)
        .await
        .map_err(DbError::Query)?;

    let summaries = rows
        .into_iter()
        .map(|row| {
            let state: String = row.try_get("state")?;
            let currency: String = row.try_get("currency")?;
            let net: String = row.try_get("net")?;
            let received: String = row.try_get("receipt_state")?;

            let currency = read_currency(&currency)?;

            Ok(OrderSummary {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                state: OrderState::parse(&state).ok_or_else(|| unknown("state", &state))?,
                receipt_state: ReceiptState::parse(&received)
                    .ok_or_else(|| unknown("receipt_state", &received))?,
                supplier_name: row.try_get("supplier_name")?,
                warehouse_name: row.try_get("warehouse_name")?,
                order_date: row.try_get("order_date")?,
                expected_on: row.try_get("expected_on")?,
                currency: currency.code().to_owned(),
                net: read_money(&net, currency, "purchase_orders.net")?,
                line_count: row.try_get("line_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)?;

    Ok(Page::new(summaries, total, &request))
}

/// One order, with its lines.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<PurchaseOrder>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT o.id, o.number, o.state, o.supplier_id, o.supplier_code, o.supplier_name,
                o.warehouse_id, o.order_date, o.expected_on, o.currency,
                o.net::text AS net, o.cost_centre_id, o.supplier_reference, o.note,
                w.name AS warehouse_name
           FROM inventory.purchase_orders o
           JOIN inventory.warehouses w ON w.id = o.warehouse_id
          WHERE o.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let state: String = row.try_get("state").map_err(DbError::Query)?;
    let currency_code: String = row.try_get("currency").map_err(DbError::Query)?;
    let net: String = row.try_get("net").map_err(DbError::Query)?;
    let currency = read_currency(&currency_code).map_err(DbError::Query)?;

    let lines = lines_of(executor, id, currency).await?;

    Ok(Some(PurchaseOrder {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: OrderState::parse(&state).ok_or_else(|| DbError::Query(unknown("state", &state)))?,
        supplier: SupplierSnapshot {
            party_id: row.try_get("supplier_id").map_err(DbError::Query)?,
            code: row.try_get("supplier_code").map_err(DbError::Query)?,
            name: row.try_get("supplier_name").map_err(DbError::Query)?,
        },
        warehouse_id: row.try_get("warehouse_id").map_err(DbError::Query)?,
        warehouse_name: row.try_get("warehouse_name").map_err(DbError::Query)?,
        order_date: row.try_get("order_date").map_err(DbError::Query)?,
        expected_on: row.try_get("expected_on").map_err(DbError::Query)?,
        currency: currency.code().to_owned(),
        net: read_money(&net, currency, "purchase_orders.net").map_err(DbError::Query)?,
        cost_centre_id: row.try_get("cost_centre_id").map_err(DbError::Query)?,
        supplier_reference: row.try_get("supplier_reference").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        lines,
    }))
}

pub async fn lines_of<'e, E>(
    executor: E,
    order_id: Uuid,
    currency: Currency,
) -> Result<Vec<OrderLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.variant_id, l.description,
                l.quantity::text AS quantity, l.unit_id,
                l.quantity_stock::text AS quantity_stock,
                l.unit_price::text AS unit_price, l.net::text AS net,
                l.received::text AS received, l.billed::text AS billed,
                l.expected_on, l.is_cancelled,
                v.code AS variant_code,
                u.code AS unit_code
           FROM inventory.purchase_order_lines l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.units u ON u.id = l.unit_id
          WHERE l.order_id = $1
          ORDER BY l.line_no",
    )
    .bind(order_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let quantity: String = row.try_get("quantity")?;
            let quantity_stock: String = row.try_get("quantity_stock")?;
            let unit_price: String = row.try_get("unit_price")?;
            let net: String = row.try_get("net")?;
            let received: String = row.try_get("received")?;
            let billed: String = row.try_get("billed")?;

            Ok(OrderLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "purchase_order_lines.quantity")?,
                unit_id: row.try_get("unit_id")?,
                unit_code: row.try_get("unit_code")?,
                quantity_stock: read_quantity(
                    &quantity_stock,
                    "purchase_order_lines.quantity_stock",
                )?,
                unit_price: read_money(&unit_price, currency, "purchase_order_lines.unit_price")?,
                net: read_money(&net, currency, "purchase_order_lines.net")?,
                received: read_quantity(&received, "purchase_order_lines.received")?,
                billed: read_quantity(&billed, "purchase_order_lines.billed")?,
                expected_on: row.try_get("expected_on")?,
                is_cancelled: row.try_get("is_cancelled")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// The header, on create.
pub async fn insert(
    conn: &mut PgConnection,
    draft: &Checked,
    supplier: &SupplierSnapshot,
    net: Money,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.purchase_orders
             (supplier_id, supplier_code, supplier_name, warehouse_id, order_date,
              expected_on, currency, net, cost_centre_id, supplier_reference, note,
              created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8::numeric, $9, $10, $11, $12, $12)
       RETURNING id",
    )
    .bind(supplier.party_id)
    .bind(&supplier.code)
    .bind(&supplier.name)
    .bind(draft.warehouse_id)
    .bind(draft.order_date)
    .bind(draft.expected_on)
    .bind(&draft.currency)
    .bind(net.to_storage_string())
    .bind(draft.cost_centre_id)
    .bind(draft.supplier_reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// The header, on edit. Answers `false` where the row is gone.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &Checked,
    supplier: &SupplierSnapshot,
    net: Money,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.purchase_orders
            SET supplier_id = $2, supplier_code = $3, supplier_name = $4,
                warehouse_id = $5, order_date = $6, expected_on = $7,
                currency = $8, net = $9::numeric, cost_centre_id = $10,
                supplier_reference = $11, note = $12,
                updated_at = now(), updated_by = $13
          WHERE id = $1 AND state IN ('draft', 'sent')",
    )
    .bind(id)
    .bind(supplier.party_id)
    .bind(&supplier.code)
    .bind(&supplier.name)
    .bind(draft.warehouse_id)
    .bind(draft.order_date)
    .bind(draft.expected_on)
    .bind(&draft.currency)
    .bind(net.to_storage_string())
    .bind(draft.cost_centre_id)
    .bind(draft.supplier_reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Replace an order's lines with what the form holds.
///
/// Delete and re-insert rather than diff - see the module header. `costed`
/// carries what the service worked out per line: the stock-unit quantity and
/// the two amounts.
pub async fn save_lines(
    conn: &mut PgConnection,
    order_id: Uuid,
    costed: &[CostedLine<'_>],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.purchase_order_lines WHERE order_id = $1")
        .bind(order_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (index, line) in costed.iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory.purchase_order_lines
                 (order_id, line_no, variant_id, description, quantity, unit_id,
                  quantity_stock, unit_price, net, received, billed, expected_on)
              VALUES ($1, $2, $3, $4, $5::numeric, $6, $7::numeric, $8::numeric,
                      $9::numeric, $10::numeric, $11::numeric, $12)",
        )
        .bind(order_id)
        .bind(index as i32 + 1)
        .bind(line.source.variant_id)
        .bind(&line.description)
        .bind(line.source.quantity.to_storage_string())
        .bind(line.source.unit_id)
        .bind(line.quantity_stock.to_storage_string())
        .bind(line.unit_price.to_storage_string())
        .bind(line.net.to_storage_string())
        .bind(line.received.to_storage_string())
        .bind(line.billed.to_storage_string())
        .bind(line.source.expected_on)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// One line, as the service worked it out.
pub struct CostedLine<'a> {
    pub source: &'a CheckedLine,
    pub description: String,
    pub quantity_stock: Quantity,
    pub unit_price: Money,
    pub net: Money,
    /// Carried across a re-save, so editing a partly-received order does not
    /// forget what already arrived.
    pub received: Quantity,
    pub billed: Quantity,
}

/// Give a draft its number and make it a commitment.
///
/// Answers `false` where the order was not in a state to be confirmed, which is
/// somebody clicking twice rather than a fault.
pub async fn confirm(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.purchase_orders
            SET number = $2, state = 'confirmed', confirmed_at = now(), confirmed_by = $3,
                updated_at = now(), updated_by = $3
          WHERE id = $1 AND state IN ('draft', 'sent')",
    )
    .bind(id)
    .bind(number)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Move an order to a state that is not confirm. Used for `sent`, `cancelled`
/// and `done`.
pub async fn set_state(
    conn: &mut PgConnection,
    id: Uuid,
    state: OrderState,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.purchase_orders
            SET state = $2, updated_at = now(), updated_by = $3
          WHERE id = $1",
    )
    .bind(id)
    .bind(state.as_str())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Advance what a line has received, by a delta in stock units.
///
/// A delta rather than an absolute, because two receipts against one line in
/// the same second must both count. The row is locked by the update itself.
pub async fn advance_received(
    conn: &mut PgConnection,
    line_id: Uuid,
    delta: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.purchase_order_lines
            SET received = received + $2::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(delta.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Remove a draft. A confirmed order is cancelled, never deleted: it was sent.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done = sqlx::query(
        "DELETE FROM inventory.purchase_orders WHERE id = $1 AND state IN ('draft', 'sent')",
    )
    .bind(id)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// The confirmed orders with something still outstanding, for a receipt screen
/// to open on.
///
/// The same two questions the grid's filters ask, asked of the same `WHERE`
/// rather than by fetching every order ever raised and sifting it here - which
/// is what this did, and what it cost grew with the ledger rather than with the
/// backlog.
///
/// Capped at one page, and that is a picker's list rather than a ledger: five
/// hundred confirmed orders with nothing received against them is a problem no
/// dropdown is going to solve.
pub async fn awaiting_delivery(pool: &sqlx::PgPool) -> Result<Vec<OrderSummary>, DbError> {
    page(
        pool,
        &PageRequest::first(MAX_PER_PAGE)
            .filtered_by(STATE, "open")
            .filtered_by(RECEIVED, "outstanding"),
    )
    .await
    .map(|page| page.rows)
}
