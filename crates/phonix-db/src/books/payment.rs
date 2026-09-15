//! `books.payments` and what each one settles.
//!
//! # Each payment carries its own currency
//!
//! Like a purchase order and unlike everything else in this schema, because a
//! customer pays in what they were invoiced in. Every read parses amounts
//! against the row's own `currency_code`, and the conversion to the workspace's
//! own happens once, at post, at the rate for the day the money arrived.
//!
//! # What is outstanding is a query, never a column
//!
//! [`settleable`] subtracts what has been allocated from what was invoiced, in
//! one statement, from the two tables that hold those facts. There is no
//! `paid_amount` on an invoice to keep in step - see the header of
//! `migrations/apps/books/0008_payments.sql` for why that column is the obvious
//! shortcut and the one that cannot answer any of the questions that follow.
//!
//! # Only a posted payment settles anything
//!
//! Every statement here that adds allocations up filters on
//! `payments.status = 'posted'`. A draft is not money and a withdrawn payment
//! has been taken back, and leaving either in would make an invoice look
//! settled by a cheque that bounced.

use app_books::payment::{
    Allocation, CheckedPayment, Direction, PayerSnapshot, Payment, PaymentStatus, PaymentSummary,
    Settleable,
};
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::{ExchangeRate, Money, Rate};
use phonix_core::query::{Page, PageRequest};
use sqlx::{AssertSqlSafe, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::listing::{self, Sortable};

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("payments.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_currency(raw: &str) -> Result<Currency, sqlx::Error> {
    Currency::parse(raw).map_err(|_| unknown("currency_code", raw))
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

/// The range key the payment grid declares, and so the pair of filter keys -
/// `received_from` and `received_to` - that arrive with a request.
///
/// A constant because it is written in two crates that must agree and do not
/// depend on each other: here, and `ui::table::config::payments`.
pub const RECEIVED: &str = "received";

/// The filter key naming which state to show.
pub const STATUS: &str = "status";

/// The filter key naming whether any of it is still against nothing.
pub const ALLOCATION: &str = "allocation";

/// The value of [`ALLOCATION`] that means "money sitting against nothing".
pub const UNALLOCATED: &str = "unallocated";

/// What is received and set against nothing. Written once because the select
/// shows it, the filter asks about it and the sort orders by it.
const ON_ACCOUNT: &str = "(p.amount - coalesce((SELECT sum(al.amount)
                              FROM books.payment_allocations al
                             WHERE al.payment_id = p.id), 0))";

/// The columns the payment grid may order by.
///
/// A whitelist, not a convenience: `sort.field` arrives from a browser, and the
/// only safe way to put it in an `ORDER BY` is to not put it there at all.
const SORTABLE: &[Sortable] = &[
    ("number", "p.number"),
    ("customer", "p.party_name"),
    ("received_on", "p.received_on"),
    ("amount", "p.amount"),
    ("on_account", "on_account"),
];

/// Every payment, one page at a time, newest first.
///
/// `allocated` is summed in SQL rather than by reading each payment's lines: a
/// page of twenty-five would otherwise be twenty-five queries to show one
/// column.
///
/// Paged because a receipt is evidence and nothing deletes one - the list only
/// grows, and it grows faster than the invoice list in any workspace whose
/// customers pay in instalments.
pub async fn page(
    pool: &sqlx::PgPool,
    request: &PageRequest,
) -> Result<Page<PaymentSummary>, DbError> {
    let request = request.sanitised();
    let needle = request
        .needle()
        .map(|needle| crate::search::contains(&needle));
    let status = request.filter(STATUS).and_then(PaymentStatus::parse);
    let unallocated = request.filter_is(ALLOCATION, UNALLOCATED);

    let received = request.range(RECEIVED);
    let from_day = received.first_day();
    let to_day = received.last_day();

    // A filter nobody set is a NULL that discards its own line, so one clause
    // serves every combination and nothing is interpolated.
    let where_clause = format!(
        "WHERE ($1::text IS NULL
                 OR p.number ILIKE $1
                 OR p.party_name ILIKE $1
                 OR a.name ILIKE $1
                 OR p.reference ILIKE $1)
            AND ($2::text IS NULL OR p.status = $2)
            AND ($3::date IS NULL OR p.received_on >= $3)
            AND ($4::date IS NULL OR p.received_on <= $4)
            AND (NOT $5::bool OR {ON_ACCOUNT} <> 0)"
    );

    // `AssertSqlSafe` because these statements are composed rather than
    // written: every piece of them is a constant of this file, and `order` can
    // only be a string it put in `SORTABLE`. Nothing from a browser reaches the
    // text of the query.
    let counting = AssertSqlSafe(format!(
        "SELECT count(*)
           FROM books.payments p
           JOIN books.accounts a ON a.id = p.account_id
           {where_clause}"
    ));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(needle.as_deref())
        .bind(status.map(PaymentStatus::as_str))
        .bind(from_day)
        .bind(to_day)
        .bind(unallocated)
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    // Newest first, and `created_at` after it whatever the sort: two payments
    // received on the same day would otherwise swap places between one page and
    // the next, which shows up as a row that appears twice.
    let order = listing::order_by(request.sort.as_ref(), SORTABLE, "p.received_on DESC");

    let selecting = AssertSqlSafe(format!(
        "SELECT p.id, p.number, p.status, p.party_id, p.party_name, p.received_on,
                p.currency_code, p.amount::text AS amount, p.reference,
                a.name AS account_name,
                coalesce((SELECT sum(al.amount) FROM books.payment_allocations al
                           WHERE al.payment_id = p.id), 0)::text AS allocated,
                {ON_ACCOUNT} AS on_account
           FROM books.payments p
           JOIN books.accounts a ON a.id = p.account_id
           {where_clause}
          ORDER BY {order}, p.created_at DESC
          LIMIT $6 OFFSET $7"
    ));

    let rows = sqlx::query(selecting)
        .bind(needle.as_deref())
        .bind(status.map(PaymentStatus::as_str))
        .bind(from_day)
        .bind(to_day)
        .bind(unallocated)
        .bind(request.limit() as i64)
        .bind(request.offset() as i64)
        .fetch_all(pool)
        .await
        .map_err(DbError::Query)?;

    let summaries = rows
        .into_iter()
        .map(|row| {
            let status: String = row.try_get("status")?;
            let code: String = row.try_get("currency_code")?;
            let amount: String = row.try_get("amount")?;
            let allocated: String = row.try_get("allocated")?;

            let currency = read_currency(&code)?;

            Ok(PaymentSummary {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                status: PaymentStatus::parse(&status).ok_or_else(|| unknown("status", &status))?,
                party_id: row.try_get("party_id")?,
                party_name: row.try_get("party_name")?,
                received_on: row.try_get("received_on")?,
                account_name: row.try_get("account_name")?,
                currency,
                amount: read_money(&amount, currency, "payments.amount")?,
                allocated: read_money(&allocated, currency, "payment_allocations.amount")?,
                reference: row.try_get("reference")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)?;

    Ok(Page::new(summaries, total, &request))
}

/// One payment, with what it settles.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Payment>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT p.id, p.number, p.status, p.direction, p.party_id, p.party_code,
                p.party_name, p.received_on, p.account_id, p.currency_code,
                p.amount::text AS amount, p.base_currency_code,
                p.exchange_rate::text AS exchange_rate, p.rate_date,
                p.base_amount::text AS base_amount, p.reference, p.note,
                p.posted_at, p.posted_by, p.created_at, p.updated_at,
                a.number AS account_number, a.name AS account_name
           FROM books.payments p
           JOIN books.accounts a ON a.id = p.account_id
          WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let status: String = row.try_get("status").map_err(DbError::Query)?;
    let direction: String = row.try_get("direction").map_err(DbError::Query)?;
    let code: String = row.try_get("currency_code").map_err(DbError::Query)?;
    let amount: String = row.try_get("amount").map_err(DbError::Query)?;
    let currency = read_currency(&code).map_err(DbError::Query)?;

    let base_code: Option<String> = row.try_get("base_currency_code").map_err(DbError::Query)?;
    let rate_text: Option<String> = row.try_get("exchange_rate").map_err(DbError::Query)?;
    let rate_date: Option<chrono::NaiveDate> = row.try_get("rate_date").map_err(DbError::Query)?;
    let base_text: Option<String> = row.try_get("base_amount").map_err(DbError::Query)?;

    // All four or none: a CHECK constraint says so, and this reads it the same
    // way rather than assembling half a conversion out of whatever was present.
    let (rate, base_amount) = match (base_code, rate_text, rate_date, base_text) {
        (Some(base_code), Some(rate_text), Some(rate_date), Some(base_text)) => {
            let base = read_currency(&base_code).map_err(DbError::Query)?;
            let parsed = Rate::parse(&rate_text)
                .map_err(|err| DbError::CorruptRow(format!("unusable exchange rate: {err}")))?;
            let rate = ExchangeRate::new(currency, base, parsed, rate_date, "payment")
                .map_err(|err| DbError::CorruptRow(format!("unusable rate snapshot: {err}")))?;
            let converted =
                read_money(&base_text, base, "payments.base_amount").map_err(DbError::Query)?;

            (Some(rate), Some(converted))
        }
        _ => (None, None),
    };

    Ok(Some(Payment {
        id,
        number: row.try_get("number").map_err(DbError::Query)?,
        status: PaymentStatus::parse(&status)
            .ok_or_else(|| DbError::Query(unknown("status", &status)))?,
        direction: Direction::parse(&direction)
            .ok_or_else(|| DbError::Query(unknown("direction", &direction)))?,
        party: PayerSnapshot {
            party_id: row.try_get("party_id").map_err(DbError::Query)?,
            code: row.try_get("party_code").map_err(DbError::Query)?,
            name: row.try_get("party_name").map_err(DbError::Query)?,
        },
        received_on: row.try_get("received_on").map_err(DbError::Query)?,
        account_id: row.try_get("account_id").map_err(DbError::Query)?,
        account_number: row.try_get("account_number").map_err(DbError::Query)?,
        account_name: row.try_get("account_name").map_err(DbError::Query)?,
        currency,
        amount: read_money(&amount, currency, "payments.amount").map_err(DbError::Query)?,
        rate,
        base_amount,
        reference: row.try_get("reference").map_err(DbError::Query)?,
        note: row.try_get("note").map_err(DbError::Query)?,
        allocations: allocations_of(executor, id, currency).await?,
        posted_at: row.try_get("posted_at").map_err(DbError::Query)?,
        posted_by: row.try_get("posted_by").map_err(DbError::Query)?,
        created_at: row.try_get("created_at").map_err(DbError::Query)?,
        updated_at: row.try_get("updated_at").map_err(DbError::Query)?,
    }))
}

pub async fn allocations_of<'e, E>(
    executor: E,
    payment_id: Uuid,
    currency: Currency,
) -> Result<Vec<Allocation>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT al.id, al.invoice_id, al.amount::text AS amount,
                i.number AS invoice_number, i.issued_on, i.due_on,
                i.gross_amount::text AS invoiced
           FROM books.payment_allocations al
           JOIN books.invoices i ON i.id = al.invoice_id
          WHERE al.payment_id = $1
          ORDER BY i.issued_on, i.number",
    )
    .bind(payment_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let amount: String = row.try_get("amount")?;
            let invoiced: String = row.try_get("invoiced")?;

            Ok(Allocation {
                id: row.try_get("id")?,
                invoice_id: row.try_get("invoice_id")?,
                invoice_number: row.try_get("invoice_number")?,
                issued_on: row.try_get("issued_on")?,
                due_on: row.try_get("due_on")?,
                invoiced: read_money(&invoiced, currency, "invoices.gross_amount")?,
                amount: read_money(&amount, currency, "payment_allocations.amount")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What this customer still owes, invoice by invoice.
///
/// `except` leaves one payment's allocations out of the settled figure. A draft
/// is never in it anyway - only posted payments count - so this matters only if
/// a posted payment is ever reopened, and it is here so that the day one is,
/// the screen does not show its own settlements as somebody else's.
///
/// Only invoices in the payment's own currency. Settling across currencies
/// realises an exchange difference this ledger does not post, and offering a
/// row that would be refused is worse than not offering it.
pub async fn settleable<'e, E>(
    executor: E,
    party_id: Uuid,
    currency: Currency,
    except: Option<Uuid>,
) -> Result<Vec<Settleable>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT i.id, i.number, i.issued_on, i.due_on,
                i.gross_amount::text AS invoiced,
                coalesce((
                    SELECT sum(al.amount)
                      FROM books.payment_allocations al
                      JOIN books.payments p ON p.id = al.payment_id
                     WHERE al.invoice_id = i.id
                       AND p.status = 'posted'
                       AND ($4::uuid IS NULL OR p.id <> $4)
                ), 0)::text AS settled
           FROM books.invoices i
          WHERE i.party_id = $1
            AND i.status = 'posted'
            AND i.currency_code = $2
            AND i.gross_amount > coalesce((
                    SELECT sum(al.amount)
                      FROM books.payment_allocations al
                      JOIN books.payments p ON p.id = al.payment_id
                     WHERE al.invoice_id = i.id
                       AND p.status = 'posted'
                       AND ($4::uuid IS NULL OR p.id <> $4)
                ), 0)
          ORDER BY coalesce(i.due_on, i.issued_on), i.issued_on, i.number
          LIMIT $3",
    )
    .bind(party_id)
    .bind(currency.code())
    .bind(i64::from(app_books::payment::MAX_ALLOCATIONS as i32))
    .bind(except)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let invoiced: String = row.try_get("invoiced")?;
            let settled: String = row.try_get("settled")?;

            let invoiced = read_money(&invoiced, currency, "invoices.gross_amount")?;
            let settled = read_money(&settled, currency, "payment_allocations.amount")?;
            let outstanding = invoiced.checked_sub(settled).map_err(|err| {
                sqlx::Error::Decode(format!("outstanding does not subtract: {err}").into())
            })?;

            Ok(Settleable {
                invoice_id: row.try_get("id")?,
                number: row
                    .try_get::<Option<String>, _>("number")?
                    .unwrap_or_default(),
                issued_on: row.try_get("issued_on")?,
                due_on: row.try_get("due_on")?,
                currency,
                invoiced,
                settled,
                outstanding,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What is left on each of these invoices, and the currency it was raised in.
///
/// What the over-settlement check reads. Only posted payments count, so a draft
/// never counts against itself and there is nothing to exclude. Called once
/// before the transaction for the message somebody sees, and again inside it
/// after [`lock_invoices`] for the answer that is actually enforced.
pub async fn settled_on<'e, E>(
    executor: E,
    invoice_ids: &[Uuid],
) -> Result<Vec<(Uuid, String, String)>, DbError>
where
    E: PgExecutor<'e>,
{
    if invoice_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT i.id,
                i.currency_code,
                (i.gross_amount - coalesce((
                    SELECT sum(al.amount)
                      FROM books.payment_allocations al
                      JOIN books.payments p ON p.id = al.payment_id
                     WHERE al.invoice_id = i.id
                       AND p.status = 'posted'
                ), 0))::text AS outstanding
           FROM books.invoices i
          WHERE i.id = ANY($1) AND i.status = 'posted'",
    )
    .bind(invoice_ids)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("id")?,
                row.try_get("currency_code")?,
                row.try_get("outstanding")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Take a row lock on the invoices a payment is about to settle.
///
/// # Why this exists
///
/// Two people posting two payments against the same invoice at the same moment
/// both read "four hundred outstanding", both allocate four hundred, and both
/// commit. Nothing about the two writes conflicts - they insert into different
/// rows of `payment_allocations` - so nothing stops them, and the invoice ends
/// up settled twice.
///
/// Locking the *invoice* rows makes the second wait for the first. It is the
/// invoice that is being over-settled, so it is the invoice that has to be the
/// point of contention.
///
/// # And why it is its own statement
///
/// Postgres takes a fresh snapshot at the start of each statement under READ
/// COMMITTED. Locking here and measuring in the next statement is therefore the
/// only ordering that sees what the transaction it waited for actually did; a
/// single `SELECT ... FOR UPDATE` with the allocation sum in its select list
/// would return the sum from the snapshot it started with.
pub async fn lock_invoices(conn: &mut PgConnection, invoice_ids: &[Uuid]) -> Result<(), DbError> {
    if invoice_ids.is_empty() {
        return Ok(());
    }

    sqlx::query("SELECT id FROM books.invoices WHERE id = ANY($1) FOR UPDATE")
        .bind(invoice_ids)
        .fetch_all(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(())
}

/// Which customer this invoice belongs to, for the check that a payment is not
/// settling somebody else's.
pub async fn party_of<'e, E>(
    executor: E,
    invoice_ids: &[Uuid],
) -> Result<Vec<(Uuid, Uuid)>, DbError>
where
    E: PgExecutor<'e>,
{
    if invoice_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT id, party_id FROM books.invoices WHERE id = ANY($1)")
            .bind(invoice_ids)
            .fetch_all(executor)
            .await
            .map_err(DbError::Query)?;

    Ok(rows)
}

pub async fn insert(
    conn: &mut PgConnection,
    draft: &CheckedPayment,
    party: &PayerSnapshot,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO books.payments
             (party_id, party_code, party_name, received_on, account_id,
              currency_code, amount, reference, note, created_by, updated_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8, $9, $10, $10)
       RETURNING id",
    )
    .bind(party.party_id)
    .bind(&party.code)
    .bind(&party.name)
    .bind(draft.received_on)
    .bind(draft.account_id)
    .bind(draft.currency.code())
    .bind(draft.amount.to_storage_string())
    .bind(draft.reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(DbError::Query)
}

pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &CheckedPayment,
    party: &PayerSnapshot,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE books.payments
            SET party_id = $2, party_code = $3, party_name = $4, received_on = $5,
                account_id = $6, currency_code = $7, amount = $8::numeric,
                reference = $9, note = $10, updated_at = now(), updated_by = $11
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(id)
    .bind(party.party_id)
    .bind(&party.code)
    .bind(&party.name)
    .bind(draft.received_on)
    .bind(draft.account_id)
    .bind(draft.currency.code())
    .bind(draft.amount.to_storage_string())
    .bind(draft.reference.as_deref())
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Replace a draft's allocations with what the form holds.
///
/// Delete and re-insert rather than diff, on the same terms an order's lines
/// are saved: the screen is edited as a whole, and a diff would be machinery to
/// reproduce what the form already knows.
pub async fn save_allocations(
    conn: &mut PgConnection,
    payment_id: Uuid,
    allocations: &[app_books::payment::CheckedAllocation],
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM books.payment_allocations WHERE payment_id = $1")
        .bind(payment_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for line in allocations {
        sqlx::query(
            "INSERT INTO books.payment_allocations (payment_id, invoice_id, amount)
             VALUES ($1, $2, $3::numeric)",
        )
        .bind(payment_id)
        .bind(line.invoice_id)
        .bind(line.amount.to_storage_string())
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(())
}

/// Number the payment and freeze it.
pub async fn post(
    conn: &mut PgConnection,
    id: Uuid,
    number: &str,
    conversion: Option<&ExchangeRate>,
    base_amount: Option<Money>,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE books.payments
            SET number             = $2,
                status             = 'posted',
                base_currency_code = $3,
                exchange_rate      = $4::numeric,
                rate_date          = $5,
                base_amount        = $6::numeric,
                posted_at          = now(),
                posted_by          = $7,
                updated_at         = now(),
                updated_by         = $7
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(id)
    .bind(number)
    .bind(conversion.map(|rate| rate.quote.code()))
    .bind(conversion.map(|rate| rate.rate.to_storage_string()))
    .bind(conversion.map(|rate| rate.as_of))
    .bind(base_amount.map(Money::to_storage_string))
    .bind(actor)
    .execute(conn)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Withdraw a posted payment. It keeps its number and its allocations.
///
/// The allocations stay because the document is evidence of what it was set
/// against; what changes is that nothing counts them any more, because every
/// statement that adds them up filters on `status = 'posted'`.
pub async fn void<'e, E>(executor: E, id: Uuid, actor: Option<UserId>) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE books.payments
            SET status = 'voided', updated_at = now(), updated_by = $2
          WHERE id = $1 AND status = 'posted'",
    )
    .bind(id)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Remove a draft. A posted payment has a number, and a numbered document that
/// vanishes is the gap the sequence design exists to prevent.
pub async fn delete_draft<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query("DELETE FROM books.payments WHERE id = $1 AND status = 'draft'")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(done.rows_affected() == 1)
}

/// Everything one customer has paid in a span, for the statement.
///
/// Posted only, in the workspace's own currency: the statement adds invoices
/// and payments together and cannot do that in two currencies. `base_amount`
/// where there was a conversion, `amount` where the payment was already in the
/// base currency.
pub async fn received_by<'e, E>(
    executor: E,
    party_id: Uuid,
    to: chrono::NaiveDate,
    base: Currency,
) -> Result<Vec<(Uuid, Option<String>, chrono::NaiveDate, Money)>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, number, received_on,
                coalesce(base_amount, amount)::text AS amount
           FROM books.payments
          WHERE party_id = $1
            AND status = 'posted'
            AND received_on <= $2
          ORDER BY received_on, number",
    )
    .bind(party_id)
    .bind(to)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let amount: String = row.try_get("amount")?;

            Ok((
                row.try_get("id")?,
                row.try_get("number")?,
                row.try_get("received_on")?,
                read_money(&amount, base, "payments.amount")?,
            ))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// What every customer has paid, in the workspace's own currency.
///
/// The other half of `invoiced_outstanding`: subtracting one from the other is
/// what the front page's "owed by customers" figure is.
pub async fn received_total<'e, E>(executor: E, base: Currency) -> Result<Money, DbError>
where
    E: PgExecutor<'e>,
{
    let total: String = sqlx::query_scalar(
        "SELECT coalesce(sum(coalesce(base_amount, amount)), 0)::text
           FROM books.payments
          WHERE status = 'posted'",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    read_money(&total, base, "payments.amount").map_err(DbError::Query)
}
