//! `inventory.sales_orders` and its lines.
//!
//! The mirror of [`super::purchase`], and deliberately the same file in the
//! same order: the two documents have the same shape, and a reader who has read
//! one should not have to learn a second set of habits to read the other.
//!
//! # Each order carries its own currency
//!
//! A customer is quoted in theirs, so every read here parses amounts against
//! the row's `currency` column rather than against one passed in. The
//! conversion to the workspace's own happens at the *invoice*, because that is
//! when the claim is made.
//!
//! # Saving an order replaces its lines
//!
//! [`save_lines`] deletes and re-inserts rather than diffing, for the reason
//! the purchase order does: an order is edited as a whole, and a diff would be
//! machinery to reproduce what the form already knows. What it carries across
//! is `delivered` and `invoiced`, so editing a part-shipped order does not
//! forget what has already gone.

use app_inventory::quantity::Quantity;
use app_inventory::sales_order::{
    Checked, CheckedLine, CustomerSnapshot, Progress, SaleLine, SaleState, SaleSummary, SalesOrder,
};
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
        format!("sales_orders.{column} holds '{raw}', which this build does not know").into(),
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
/// depend on each other: here, and `ui::table::config::sales_orders`.
pub const ORDERED: &str = "ordered";

/// The filter key naming which states to show, by group rather than one each -
/// see [`SaleState::group`].
pub const STATE: &str = "state";

/// The filter key naming whether everything has gone out.
pub const DELIVERED: &str = "delivered";

/// The filter key naming whether everything has been charged for.
pub const INVOICED: &str = "invoiced";

/// The columns the order grid may order by.
const SORTABLE: &[Sortable] = &[
    ("number", "o.number"),
    ("customer", "o.customer_name"),
    ("order_date", "o.order_date"),
    ("promised_on", "o.promised_on"),
    ("net", "o.net"),
    ("line_count", "line_count"),
];

/// How far one column of a line has got towards what was agreed.
///
/// Written once and used four times - the select shows both figures and the
/// `WHERE` narrows by both - because two copies of this `CASE` would eventually
/// disagree, and the one that disagreed would be the one nobody was reading.
/// The order matters: over beats everything, which beats partly.
fn progress_of(column: &str) -> String {
    format!(
        "COALESCE((
                    SELECT CASE
                        WHEN bool_or(l.{column} > l.quantity_stock) THEN 'over'
                        WHEN bool_and(l.{column} >= l.quantity_stock) THEN 'everything'
                        WHEN bool_or(l.{column} > 0) THEN 'partly'
                        ELSE 'nothing' END
                      FROM inventory.sales_order_lines l
                     WHERE l.order_id = o.id AND NOT l.is_cancelled
                ), 'nothing')"
    )
}

/// The states one side of a progress question covers, as the bind wants them.
fn progress_names(wanted: Option<&str>) -> Option<Vec<String>> {
    match wanted {
        Some("complete") => Some(true),
        Some("outstanding") => Some(false),
        _ => None,
    }
    .map(|complete| {
        Progress::complete_or_not(complete)
            .into_iter()
            .map(|state| state.as_str().to_owned())
            .collect()
    })
}

/// One page of the order list, newest first.
///
/// Paged in SQL because a sales ledger is a list nothing deletes from - a
/// quotation nobody accepted is still what was quoted - so it only grows.
///
/// Two statements, a count and a select, so the page can be pulled back to one
/// that exists before the rows are fetched.
pub async fn page(pool: &sqlx::PgPool, request: &PageRequest) -> Result<Page<SaleSummary>, DbError> {
    let request = request.sanitised();
    let needle = request.needle().map(|needle| crate::search::contains(&needle));
    let ordered = request.range(ORDERED);

    let delivery = progress_of("delivered");
    let invoicing = progress_of("invoiced");

    // Groups rather than states: see `SaleState::group`. A name this build does
    // not know covers no states, which would match nothing, so it narrows
    // nothing instead - a browser running a newer screen must not empty a list.
    let states: Option<Vec<String>> = request.filter(STATE).and_then(|group| {
        let states = SaleState::in_group(group);

        (!states.is_empty()).then(|| {
            states
                .into_iter()
                .map(|state| state.as_str().to_owned())
                .collect()
        })
    });

    let delivered = progress_names(request.filter(DELIVERED));
    let invoiced = progress_names(request.filter(INVOICED));

    let where_clause = format!(
        "WHERE ($1::text IS NULL
                 OR o.number ILIKE $1
                 OR o.customer_name ILIKE $1
                 OR w.name ILIKE $1)
            AND ($2::date IS NULL OR o.order_date >= $2)
            AND ($3::date IS NULL OR o.order_date <= $3)
            AND ($4::text[] IS NULL OR o.state = ANY($4::text[]))
            AND ($5::text[] IS NULL OR {delivery} = ANY($5::text[]))
            AND ($6::text[] IS NULL OR {invoicing} = ANY($6::text[]))"
    );

    // `AssertSqlSafe` because these statements are composed rather than
    // written: every piece is written by this file - `progress_of` takes a
    // column name from two call sites below and nowhere else - and `order` can
    // only be a string it put in `SORTABLE`. Nothing from a browser reaches the
    // text of the query.
    let counting = AssertSqlSafe(format!(
        "SELECT count(*)
           FROM inventory.sales_orders o
           JOIN inventory.warehouses w ON w.id = o.warehouse_id
           {where_clause}"
    ));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(needle.as_deref())
        .bind(ordered.first_day())
        .bind(ordered.last_day())
        .bind(states.as_deref())
        .bind(delivered.as_deref())
        .bind(invoiced.as_deref())
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    // Newest first, and `created_at` after it whatever the sort: two orders
    // taken on the same day would otherwise swap places between one page and
    // the next, which shows up as a row that appears twice.
    let order = listing::order_by(request.sort.as_ref(), SORTABLE, "o.order_date DESC");

    let selecting = AssertSqlSafe(format!(
        "SELECT o.id, o.number, o.state, o.customer_name, o.order_date, o.promised_on,
                o.valid_until, o.currency, o.net::text AS net,
                w.name AS warehouse_name,
                (SELECT count(*) FROM inventory.sales_order_lines l
                  WHERE l.order_id = o.id AND NOT l.is_cancelled) AS line_count,
                {delivery} AS delivery_state,
                {invoicing} AS invoice_state
           FROM inventory.sales_orders o
           JOIN inventory.warehouses w ON w.id = o.warehouse_id
           {where_clause}
          ORDER BY {order}, o.created_at DESC
          LIMIT $7 OFFSET $8"
    ));

    let rows = sqlx::query(selecting)
        .bind(needle.as_deref())
        .bind(ordered.first_day())
        .bind(ordered.last_day())
        .bind(states.as_deref())
        .bind(delivered.as_deref())
        .bind(invoiced.as_deref())
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
            let delivered: String = row.try_get("delivery_state")?;
            let invoiced: String = row.try_get("invoice_state")?;

            let currency = read_currency(&currency)?;

            Ok(SaleSummary {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                state: SaleState::parse(&state).ok_or_else(|| unknown("state", &state))?,
                delivery_state: Progress::parse(&delivered)
                    .ok_or_else(|| unknown("delivery_state", &delivered))?,
                invoice_state: Progress::parse(&invoiced)
                    .ok_or_else(|| unknown("invoice_state", &invoiced))?,
                customer_name: row.try_get("customer_name")?,
                warehouse_name: row.try_get("warehouse_name")?,
                order_date: row.try_get("order_date")?,
                promised_on: row.try_get("promised_on")?,
                valid_until: row.try_get("valid_until")?,
                currency: currency.code().to_owned(),
                net: read_money(&net, currency, "sales_orders.net")?,
                line_count: row.try_get("line_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)?;

    Ok(Page::new(summaries, total, &request))
}

/// One order, with its lines.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<SalesOrder>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT o.id, o.number, o.state, o.customer_id, o.customer_code, o.customer_name,
                o.warehouse_id, o.order_date, o.promised_on, o.valid_until, o.currency,
                o.net::text AS net, o.customer_reference, o.note,
                w.name AS warehouse_name
           FROM inventory.sales_orders o
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

    Ok(Some(SalesOrder {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        state: SaleState::parse(&state).ok_or_else(|| DbError::Query(unknown("state", &state)))?,
        customer: CustomerSnapshot {
            party_id: row.try_get("customer_id").map_err(DbError::Query)?,
            code: row.try_get("customer_code").map_err(DbError::Query)?,
            name: row.try_get("customer_name").map_err(DbError::Query)?,
        },
        warehouse_id: row.try_get("warehouse_id").map_err(DbError::Query)?,
        warehouse_name: row.try_get("warehouse_name").map_err(DbError::Query)?,
        order_date: row.try_get("order_date").map_err(DbError::Query)?,
        promised_on: row.try_get("promised_on").map_err(DbError::Query)?,
        valid_until: row.try_get("valid_until").map_err(DbError::Query)?,
        currency: currency.code().to_owned(),
        net: read_money(&net, currency, "sales_orders.net").map_err(DbError::Query)?,
        customer_reference: row.try_get("customer_reference").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        lines,
    }))
}

pub async fn lines_of<'e, E>(
    executor: E,
    order_id: Uuid,
    currency: Currency,
) -> Result<Vec<SaleLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT l.id, l.line_no, l.variant_id, l.description,
                l.quantity::text AS quantity, l.unit_id,
                l.quantity_stock::text AS quantity_stock,
                l.unit_price::text AS unit_price, l.net::text AS net,
                l.delivered::text AS delivered, l.invoiced::text AS invoiced,
                l.promised_on, l.is_cancelled,
                v.code AS variant_code,
                u.code AS unit_code
           FROM inventory.sales_order_lines l
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
            let delivered: String = row.try_get("delivered")?;
            let invoiced: String = row.try_get("invoiced")?;

            Ok(SaleLine {
                id: row.try_get("id")?,
                line_no: row.try_get("line_no")?,
                variant_id: row.try_get("variant_id")?,
                variant_code: row.try_get("variant_code")?,
                description: row.try_get("description")?,
                quantity: read_quantity(&quantity, "sales_order_lines.quantity")?,
                unit_id: row.try_get("unit_id")?,
                unit_code: row.try_get("unit_code")?,
                quantity_stock: read_quantity(&quantity_stock, "sales_order_lines.quantity_stock")?,
                unit_price: read_money(&unit_price, currency, "sales_order_lines.unit_price")?,
                net: read_money(&net, currency, "sales_order_lines.net")?,
                delivered: read_quantity(&delivered, "sales_order_lines.delivered")?,
                invoiced: read_quantity(&invoiced, "sales_order_lines.invoiced")?,
                promised_on: row.try_get("promised_on")?,
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
    customer: &CustomerSnapshot,
    net: Money,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.sales_orders
             (customer_id, customer_code, customer_name, warehouse_id, order_date,
              promised_on, valid_until, currency, net, customer_reference, note,
              created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::numeric, $10, $11, $12, $12)
       RETURNING id",
    )
    .bind(customer.party_id)
    .bind(&customer.code)
    .bind(&customer.name)
    .bind(draft.warehouse_id)
    .bind(draft.order_date)
    .bind(draft.promised_on)
    .bind(draft.valid_until)
    .bind(&draft.currency)
    .bind(net.to_storage_string())
    .bind(draft.customer_reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

/// The header, on edit. Answers `false` where the row is gone or no longer
/// editable.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &Checked,
    customer: &CustomerSnapshot,
    net: Money,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.sales_orders
            SET customer_id = $2, customer_code = $3, customer_name = $4,
                warehouse_id = $5, order_date = $6, promised_on = $7, valid_until = $8,
                currency = $9, net = $10::numeric,
                customer_reference = $11, note = $12,
                updated_at = now(), updated_by = $13
          WHERE id = $1 AND state IN ('draft', 'sent')",
    )
    .bind(id)
    .bind(customer.party_id)
    .bind(&customer.code)
    .bind(&customer.name)
    .bind(draft.warehouse_id)
    .bind(draft.order_date)
    .bind(draft.promised_on)
    .bind(draft.valid_until)
    .bind(&draft.currency)
    .bind(net.to_storage_string())
    .bind(draft.customer_reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Replace an order's lines with what the form holds.
pub async fn save_lines(
    conn: &mut PgConnection,
    order_id: Uuid,
    priced: &[PricedLine<'_>],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.sales_order_lines WHERE order_id = $1")
        .bind(order_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for (index, line) in priced.iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory.sales_order_lines
                 (order_id, line_no, variant_id, description, quantity, unit_id,
                  quantity_stock, unit_price, net, delivered, invoiced, promised_on)
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
        .bind(line.delivered.to_storage_string())
        .bind(line.invoiced.to_storage_string())
        .bind(line.source.promised_on)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// One line, as the service worked it out.
pub struct PricedLine<'a> {
    pub source: &'a CheckedLine,
    pub description: String,
    pub quantity_stock: Quantity,
    pub unit_price: Money,
    pub net: Money,
    /// Carried across a re-save, so editing a part-shipped order does not
    /// forget what has already gone or been billed.
    pub delivered: Quantity,
    pub invoiced: Quantity,
}

/// Number a draft and move it out of the building.
///
/// One statement for `sent` and `confirmed` because the rule is one rule: a
/// document that leaves takes a number, and a document that already has one
/// keeps it. `COALESCE` is what makes confirming a quotation keep the number it
/// was quoted under instead of burning a second.
///
/// Answers `false` where the order was not in a state to move, which is
/// somebody clicking twice rather than a fault.
pub async fn issue(
    conn: &mut PgConnection,
    id: Uuid,
    state: SaleState,
    number: &str,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.sales_orders
            SET number = CASE WHEN number = '' THEN $3 ELSE number END,
                state = $2,
                issued_at = COALESCE(issued_at, now()),
                issued_by = COALESCE(issued_by, $4),
                confirmed_at = CASE WHEN $2 = 'confirmed' THEN now() ELSE confirmed_at END,
                confirmed_by = CASE WHEN $2 = 'confirmed' THEN $4 ELSE confirmed_by END,
                updated_at = now(), updated_by = $4
          WHERE id = $1 AND state IN ('draft', 'sent')",
    )
    .bind(id)
    .bind(state.as_str())
    .bind(number)
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Whether this order already carries a number.
///
/// Asked before a number is allocated: a quotation being confirmed has one
/// already, and spending a second would leave a gap in the series for no
/// reason.
pub async fn number_of<'e, E>(executor: E, id: Uuid) -> Result<Option<String>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT number FROM inventory.sales_orders WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)
}

/// Move an order to a state that needs no number. `cancelled` and `done`.
pub async fn set_state(
    conn: &mut PgConnection,
    id: Uuid,
    state: SaleState,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE inventory.sales_orders
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

/// Advance what a line has shipped, by a delta in stock units.
///
/// A delta rather than an absolute, because two deliveries against one line in
/// the same second must both count. The row is locked by the update itself.
pub async fn advance_delivered(
    conn: &mut PgConnection,
    line_id: Uuid,
    delta: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.sales_order_lines
            SET delivered = delivered + $2::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(delta.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Advance what a line has been billed, by a delta in stock units.
pub async fn advance_invoiced(
    conn: &mut PgConnection,
    line_id: Uuid,
    delta: Quantity,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE inventory.sales_order_lines
            SET invoiced = invoiced + $2::numeric
          WHERE id = $1",
    )
    .bind(line_id)
    .bind(delta.to_storage_string())
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(())
}

/// Remove a draft. A quotation that has been sent is cancelled, never deleted:
/// somebody outside has a copy of it and its number.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    let done =
        sqlx::query("DELETE FROM inventory.sales_orders WHERE id = $1 AND state = 'draft'")
            .bind(id)
            .execute(conn)
            .await
            .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// The confirmed orders with something still to ship, for a delivery screen to
/// open on.
///
/// The same two questions the grid's filters ask, asked of the same `WHERE`
/// rather than by fetching every order ever taken and sifting it here - which
/// is what this did, and what it cost grew with the ledger rather than with the
/// backlog.
///
/// Capped at one page, and that is a picker's list rather than a ledger: five
/// hundred confirmed orders with nothing shipped against them is a problem no
/// dropdown is going to solve.
pub async fn awaiting_despatch(pool: &sqlx::PgPool) -> Result<Vec<SaleSummary>, DbError> {
    page(
        pool,
        &PageRequest::first(MAX_PER_PAGE)
            .filtered_by(STATE, "open")
            .filtered_by(DELIVERED, "outstanding"),
    )
    .await
    .map(|page| page.rows)
}
