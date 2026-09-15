//! What the reports read: balances out of the ledger, documents out of the
//! sales ledger.
//!
//! # Two queries, four statements
//!
//! [`movements`] is the general ledger one - every account, what it held before
//! a date and what moved after it - and the trial balance, the balance sheet
//! and the profit and loss are all arrangements of its answer.
//! [`statement_entries`] is the sales ledger one: every invoice and every
//! payment for one customer, interleaved, for their own statement.
//!
//! Aggregating in Postgres rather than summing journal lines in Rust: a year's
//! ledger is a lot of rows to carry across a connection to add them up, and the
//! two indexes this needs already exist for the account enquiry screen.
//!
//! # Base amounts only
//!
//! Every figure comes back in the workspace's own currency, from the
//! `base_amount` column recorded at post. A report that added a euro line to a
//! sterling one would be adding two different things, and re-converting at
//! today's rate would restate a period that was filed months ago.

use app_books::account::AccountType;
use app_books::report::{AccountMovement, EntryKind, StatementLine};
use chrono::NaiveDate;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// Every account, with what it held before `from` and what moved between
/// `from` and `to` inclusive.
///
/// Accounts with nothing at all come back too, zeroed. Deciding what is worth
/// printing is the report's business, not this one's - and the balance sheet
/// and the trial balance draw that line in different places.
pub async fn movements<'e, E>(
    executor: E,
    from: NaiveDate,
    to: NaiveDate,
    currency: Currency,
) -> Result<Vec<AccountMovement>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT a.id, a.number, a.name, a.account_type,
                coalesce(sum(l.base_amount) FILTER (
                    WHERE l.side = 'debit'  AND j.entry_date < $1), 0)::text AS opening_debits,
                coalesce(sum(l.base_amount) FILTER (
                    WHERE l.side = 'credit' AND j.entry_date < $1), 0)::text AS opening_credits,
                coalesce(sum(l.base_amount) FILTER (
                    WHERE l.side = 'debit'  AND j.entry_date >= $1), 0)::text AS debits,
                coalesce(sum(l.base_amount) FILTER (
                    WHERE l.side = 'credit' AND j.entry_date >= $1), 0)::text AS credits
           FROM books.accounts a
           LEFT JOIN books.journal_lines l ON l.account_id = a.id
           LEFT JOIN books.journals j ON j.id = l.journal_id AND j.entry_date <= $2
          GROUP BY a.id, a.number, a.name, a.account_type
          ORDER BY a.number",
    )
    .bind(from)
    .bind(to)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| read_movement(&row, currency))
        .collect()
}

/// Every posted invoice raised on one customer up to `to`, oldest first.
///
/// Up to the date rather than within the span: what came before it is the
/// opening balance, and a statement that could not open on one would be a list
/// of documents rather than a statement.
///
/// Drafts and voided invoices are not on it. A draft is not a claim on
/// anybody, and a voided one stopped being.
/// Every document on one customer's account up to `to`, in date order.
///
/// Invoices and payments in one query, because the statement prints them
/// interleaved and merging two ordered lists in Rust is a merge somebody has to
/// get right. `UNION ALL` and one `ORDER BY` is the same answer with nothing to
/// get wrong.
///
/// An invoice's `outstanding` is its gross less everything a *posted* payment
/// has been allocated to it - so an invoice settled last week is on the
/// statement and off the ageing ladder, which is the difference between ageing
/// what was billed and ageing what is owed.
pub async fn statement_entries<'e, E>(
    executor: E,
    party_id: Uuid,
    to: NaiveDate,
    base: Currency,
) -> Result<Vec<StatementLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT i.id,
                i.kind,
                coalesce(i.number, '') AS number,
                i.issued_on AS dated_on,
                i.due_on,
                i.currency_code,
                i.gross_amount::text AS document,
                (CASE WHEN i.kind = 'credit_note'
                      THEN -coalesce(i.base_gross_amount, i.gross_amount)
                      ELSE coalesce(i.base_gross_amount, i.gross_amount)
                 END)::text AS amount,
                CASE WHEN i.kind = 'credit_note' THEN 0 ELSE greatest(
                    i.gross_amount
                    - coalesce((
                        SELECT sum(al.amount)
                          FROM books.payment_allocations al
                          JOIN books.payments p ON p.id = al.payment_id
                         WHERE al.invoice_id = i.id AND p.status = 'posted'
                    ), 0)
                    - coalesce((
                        SELECT sum(c.gross_amount)
                          FROM books.invoices c
                         WHERE c.credits_invoice_id = i.id AND c.status = 'posted'
                    ), 0),
                    0
                ) END::text AS outstanding
           FROM books.invoices i
          WHERE i.party_id = $1 AND i.status = 'posted' AND i.issued_on <= $2

          UNION ALL

         SELECT p.id,
                'payment' AS kind,
                coalesce(p.number, '') AS number,
                p.received_on AS dated_on,
                NULL::date AS due_on,
                p.currency_code,
                p.amount::text AS document,
                (-coalesce(p.base_amount, p.amount))::text AS amount,
                '0' AS outstanding
           FROM books.payments p
          WHERE p.party_id = $1 AND p.status = 'posted' AND p.received_on <= $2

          ORDER BY dated_on, number",
    )
    .bind(party_id)
    .bind(to)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| read_statement_line(&row, base))
        .collect()
}

/// What this customer has paid or been credited that is set against nothing,
/// as at `to`.
///
/// A credit belonging to no invoice: an unallocated payment, or a credit note
/// raised against no invoice at all. It is on the statement as its own line
/// rather than spread across the ageing buckets, because spreading it would be
/// guessing which invoice the customer meant.
pub async fn on_account<'e, E>(
    executor: E,
    party_id: Uuid,
    to: NaiveDate,
    base: Currency,
) -> Result<Money, DbError>
where
    E: PgExecutor<'e>,
{
    let digits: String = sqlx::query_scalar(
        "SELECT (
           coalesce((SELECT sum(
                        coalesce(p.base_amount, p.amount)
                        - coalesce((SELECT sum(al.amount)
                                      FROM books.payment_allocations al
                                     WHERE al.payment_id = p.id), 0)
                    )
                      FROM books.payments p
                     WHERE p.party_id = $1
                       AND p.status = 'posted'
                       AND p.received_on <= $2), 0)
           + coalesce((SELECT sum(coalesce(c.base_gross_amount, c.gross_amount))
                         FROM books.invoices c
                        WHERE c.party_id = $1
                          AND c.status = 'posted'
                          AND c.kind = 'credit_note'
                          AND c.credits_invoice_id IS NULL
                          AND c.issued_on <= $2), 0)
         )::text",
    )
    .bind(party_id)
    .bind(to)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Money::parse(base, &digits)
        .map_err(|err| DbError::CorruptRow(format!("unusable payment total: {err}")))
}

pub async fn invoiced_outstanding<'e, E>(executor: E, base: Currency) -> Result<Money, DbError>
where
    E: PgExecutor<'e>,
{
    let digits: String = sqlx::query_scalar(
        "SELECT (
                  coalesce((SELECT sum(coalesce(i.base_gross_amount, i.gross_amount))
                              FROM books.invoices i
                             WHERE i.status = 'posted'
                               AND i.kind = 'sales_invoice'), 0)
                  - coalesce((SELECT sum(coalesce(c.base_gross_amount, c.gross_amount))
                                FROM books.invoices c
                               WHERE c.status = 'posted'
                                 AND c.kind = 'credit_note'), 0)
                  - coalesce((SELECT sum(coalesce(p.base_amount, p.amount))
                                FROM books.payments p WHERE p.status = 'posted'), 0)
                )::text",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Money::parse(base, &digits)
        .map_err(|err| DbError::CorruptRow(format!("unusable invoice total: {err}")))
}

fn read_movement(
    row: &sqlx::postgres::PgRow,
    currency: Currency,
) -> Result<AccountMovement, DbError> {
    let number: String = row.try_get("number").map_err(DbError::Query)?;
    let raw: String = row.try_get("account_type").map_err(DbError::Query)?;

    // Same refusal as the chart's own reader: a type this build cannot parse
    // would land on the wrong side of a statement, and a wrong report is worse
    // than a missing one.
    let Some(account_type) = AccountType::parse(&raw) else {
        return Err(DbError::CorruptCatalogRow {
            slug: number,
            reason: format!("account_type '{raw}' is not one this build knows"),
        });
    };

    let opening = money_of(row, "opening_debits", currency)?
        .checked_sub(money_of(row, "opening_credits", currency)?)
        .map_err(|err| DbError::CorruptRow(format!("unusable opening balance: {err}")))?;

    Ok(AccountMovement {
        account_id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        name: row.try_get("name").map_err(DbError::Query)?,
        account_type,
        opening,
        debits: money_of(row, "debits", currency)?,
        credits: money_of(row, "credits", currency)?,
    })
}

fn read_statement_line(
    row: &sqlx::postgres::PgRow,
    base: Currency,
) -> Result<StatementLine, DbError> {
    let code: String = row.try_get("currency_code").map_err(DbError::Query)?;
    let raised_in = Currency::parse(&code)
        .map_err(|err| DbError::CorruptRow(format!("unusable currency on a document: {err}")))?;

    let kind: String = row.try_get("kind").map_err(DbError::Query)?;
    let kind = match kind.as_str() {
        "sales_invoice" => EntryKind::Invoice,
        "credit_note" => EntryKind::CreditNote,
        "payment" => EntryKind::Payment,
        other => {
            return Err(DbError::CorruptRow(format!(
                "a statement row calls itself '{other}'"
            )));
        }
    };

    Ok(StatementLine {
        id: row.try_get("id").map_err(DbError::Query)?,
        kind,
        number: row.try_get("number").map_err(DbError::Query)?,
        dated_on: row.try_get("dated_on").map_err(DbError::Query)?,
        due_on: row.try_get("due_on").map_err(DbError::Query)?,
        document: money_of(row, "document", raised_in)?,
        amount: money_of(row, "amount", base)?,
        outstanding: money_of(row, "outstanding", base)?,
        // Filled in as the statement is assembled, which is where the order of
        // the lines is known.
        running: Money::zero(base),
    })
}

/// An amount column, read back as text so no digit is lost in the driver.
fn money_of(
    row: &sqlx::postgres::PgRow,
    column: &str,
    currency: Currency,
) -> Result<Money, DbError> {
    let digits: String = row.try_get(column).map_err(DbError::Query)?;

    Money::parse(currency, &digits)
        .map_err(|err| DbError::CorruptRow(format!("unusable amount on a report row: {err}")))
}
