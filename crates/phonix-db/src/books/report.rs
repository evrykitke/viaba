//! What the reports read: balances out of the ledger, documents out of the
//! sales ledger.
//!
//! # Two queries, four statements
//!
//! [`movements`] is the general ledger one - every account, what it held before
//! a date and what moved after it - and the trial balance, the balance sheet
//! and the profit and loss are all arrangements of its answer. [`billed_to`] is
//! the sales ledger one, for a customer's own statement.
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
use app_books::report::{AccountMovement, StatementLine};
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
pub async fn billed_to<'e, E>(
    executor: E,
    party_id: Uuid,
    to: NaiveDate,
    base: Currency,
) -> Result<Vec<StatementLine>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, number, issued_on, due_on,
                currency_code, gross_amount::text AS gross,
                coalesce(base_gross_amount, gross_amount)::text AS base_gross
           FROM books.invoices
          WHERE party_id = $1
            AND status = 'posted'
            AND issued_on <= $2
          ORDER BY issued_on, number",
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

/// Every posted invoice, added up.
///
/// What is owed, for as long as nothing records a customer paying. A voided
/// invoice is not on it and a draft never was.
pub async fn invoiced_outstanding<'e, E>(executor: E, base: Currency) -> Result<Money, DbError>
where
    E: PgExecutor<'e>,
{
    let digits: String = sqlx::query_scalar(
        "SELECT coalesce(sum(coalesce(base_gross_amount, gross_amount)), 0)::text
           FROM books.invoices
          WHERE status = 'posted'",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Money::parse(base, &digits)
        .map_err(|err| DbError::CorruptRow(format!("unusable invoice total: {err}")))
}

fn read_movement(row: &sqlx::postgres::PgRow, currency: Currency) -> Result<AccountMovement, DbError> {
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
    let invoiced_in = Currency::parse(&code)
        .map_err(|err| DbError::CorruptRow(format!("unusable currency on an invoice: {err}")))?;

    Ok(StatementLine {
        invoice_id: row.try_get("id").map_err(DbError::Query)?,
        number: row
            .try_get::<Option<String>, _>("number")
            .map_err(DbError::Query)?
            .unwrap_or_default(),
        issued_on: row.try_get("issued_on").map_err(DbError::Query)?,
        due_on: row.try_get("due_on").map_err(DbError::Query)?,
        invoiced: money_of(row, "gross", invoiced_in)?,
        amount: money_of(row, "base_gross", base)?,
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
