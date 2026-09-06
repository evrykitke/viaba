//! `books.journals`, its lines, and the dimensions each line carries.
//!
//! # There is no update and no delete
//!
//! Deliberately. A posted journal is append-only, so this module offers a way
//! to write one and ways to read one, and nothing else. A correction is a
//! second journal that names the first, which is an insert like any other.
//! See ADR 0006 section 5 rule 2.
//!
//! # Amounts cross as text
//!
//! Same reason as `books::invoice`: `NUMERIC` has no lossless integer binding
//! in the driver and the whole point of [`Money`] is that it is exact.

use app_books::account::Side;
use app_books::journal::{
    Dimension, DimensionValue, JournalEntry, JournalSummary, Posted, PostedLine, Source,
};
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// Which journals a screen is asking for.
#[derive(Debug, Clone, Default)]
pub struct JournalQuery {
    pub period_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub source_app: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

/// Post a journal: the header, its lines, and their dimensions.
///
/// Takes a connection rather than an executor because it writes several
/// statements that have to succeed together. The caller opens the transaction,
/// because it is also allocating the number inside it - so a rolled-back post
/// returns that number.
pub async fn insert(
    tx: &mut PgConnection,
    entry: &JournalEntry,
    number: &str,
    period_id: Uuid,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    let source = entry.source();

    let journal_id: Uuid = sqlx::query_scalar(
        "INSERT INTO books.journals
             (number, entry_date, period_id, narration,
              source_app, source_doc_type, source_doc_id, reverses_id, posted_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(number)
    .bind(entry.entry_date())
    .bind(period_id)
    .bind(entry.narration())
    .bind(&source.app)
    .bind(&source.doc_type)
    .bind(source.doc_id)
    .bind(entry.reverses_id())
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .map_err(|err| as_number_conflict(err, number))?;

    for (position, line) in entry.lines().iter().enumerate() {
        let position = i32::try_from(position).map_err(|_| {
            DbError::CorruptRow("a journal with more lines than a column can hold".to_owned())
        })?;

        let line_id: Uuid = sqlx::query_scalar(
            "INSERT INTO books.journal_lines
                 (journal_id, position, account_id, side,
                  currency_code, amount,
                  base_currency_code, base_amount,
                  exchange_rate, rate_date, memo)
             VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8::numeric, $9::numeric, $10, $11)
             RETURNING id",
        )
        .bind(journal_id)
        .bind(position)
        .bind(line.account_id)
        .bind(line.side.as_str())
        .bind(line.amount.currency().code())
        .bind(line.amount.to_storage_string())
        .bind(line.base_amount.currency().code())
        .bind(line.base_amount.to_storage_string())
        .bind(&line.exchange_rate)
        .bind(line.rate_date)
        .bind(line.memo.as_deref())
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        for value in &line.dimensions {
            sqlx::query(
                "INSERT INTO books.journal_line_dimensions
                     (line_id, dimension, value_id, value_code, value_name)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(line_id)
            .bind(value.dimension.as_str())
            .bind(value.id)
            .bind(&value.code)
            .bind(&value.name)
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;
        }
    }

    Ok(journal_id)
}

/// Journals a list screen should show, newest first.
///
/// The total is one side of the journal, which is both: they are equal by
/// construction, so summing the debits answers "how big is this".
pub async fn list<'e, E>(executor: E, query: &JournalQuery) -> Result<Vec<JournalSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT j.id, j.number, j.entry_date, j.narration,
                j.source_app, j.source_doc_type, j.source_doc_id,
                j.reverses_id,
                p.label AS period_label,
                count(l.id)                                      AS line_count,
                coalesce(sum(l.base_amount)
                         FILTER (WHERE l.side = 'debit'), 0)::text AS total,
                max(l.base_currency_code)                        AS base_currency_code
           FROM books.journals j
           JOIN books.periods p ON p.id = j.period_id
           LEFT JOIN books.journal_lines l ON l.journal_id = j.id
          WHERE ($1::uuid IS NULL OR j.period_id = $1)
            AND ($2::text IS NULL OR j.source_app = $2)
            AND ($3::date IS NULL OR j.entry_date >= $3)
            AND ($4::date IS NULL OR j.entry_date <= $4)
            AND ($5::uuid IS NULL OR EXISTS (
                    SELECT 1 FROM books.journal_lines a
                     WHERE a.journal_id = j.id AND a.account_id = $5))
          GROUP BY j.id, p.label
          ORDER BY j.entry_date DESC, j.number DESC",
    )
    .bind(query.period_id)
    .bind(query.source_app.as_deref())
    .bind(query.from)
    .bind(query.to)
    .bind(query.account_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(read_summary).collect()
}

/// One journal, with its lines and their dimensions.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Posted>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let Some(row) = sqlx::query(
        "SELECT j.id, j.number, j.entry_date, j.period_id, j.narration,
                j.source_app, j.source_doc_type, j.source_doc_id,
                j.reverses_id, j.posted_at,
                p.label AS period_label
           FROM books.journals j
           JOIN books.periods p ON p.id = j.period_id
          WHERE j.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    else {
        return Ok(None);
    };

    let lines = lines_of(executor, id).await?;

    Ok(Some(Posted {
        id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        entry_date: row.try_get("entry_date").map_err(DbError::Query)?,
        period_id: row.try_get("period_id").map_err(DbError::Query)?,
        period_label: row.try_get("period_label").map_err(DbError::Query)?,
        narration: row.try_get("narration").map_err(DbError::Query)?,
        source: Source {
            app: row.try_get("source_app").map_err(DbError::Query)?,
            doc_type: row.try_get("source_doc_type").map_err(DbError::Query)?,
            doc_id: row.try_get("source_doc_id").map_err(DbError::Query)?,
        },
        reverses_id: row.try_get("reverses_id").map_err(DbError::Query)?,
        posted_at: row.try_get("posted_at").map_err(DbError::Query)?,
        lines,
    }))
}

/// Whether this journal has already been reversed, and by which one.
///
/// Asked before reversing, so the refusal names the correction that already
/// exists rather than surfacing as a unique-index violation.
pub async fn reversal_of<'e, E>(executor: E, id: Uuid) -> Result<Option<String>, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT number FROM books.journals WHERE reverses_id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(DbError::Query)
}

/// Whether anything has been posted into a period. What a close reports and a
/// reopen does not need.
pub async fn count_in_period<'e, E>(executor: E, period_id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT count(*) FROM books.journals WHERE period_id = $1")
        .bind(period_id)
        .fetch_one(executor)
        .await
        .map_err(DbError::Query)
}

async fn lines_of<'e, E>(executor: E, journal_id: Uuid) -> Result<Vec<PostedLine>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let rows = sqlx::query(
        "SELECT l.id, l.position, l.account_id, l.side,
                l.currency_code, l.amount::text AS amount,
                l.base_currency_code, l.base_amount::text AS base_amount,
                l.exchange_rate::text AS exchange_rate, l.rate_date, l.memo,
                a.number AS account_number, a.name AS account_name
           FROM books.journal_lines l
           JOIN books.accounts a ON a.id = l.account_id
          WHERE l.journal_id = $1
          ORDER BY l.position",
    )
    .bind(journal_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let mut lines = Vec::with_capacity(rows.len());

    for row in rows {
        let id: Uuid = row.try_get("id").map_err(DbError::Query)?;
        lines.push(read_line(&row, id, dimensions_of(executor, id).await?)?);
    }

    Ok(lines)
}

async fn dimensions_of<'e, E>(executor: E, line_id: Uuid) -> Result<Vec<DimensionValue>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT dimension, value_id, value_code, value_name
           FROM books.journal_line_dimensions
          WHERE line_id = $1
          ORDER BY dimension",
    )
    .bind(line_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter()
        .map(|row| {
            let stored: String = row.try_get("dimension").map_err(DbError::Query)?;
            let dimension = Dimension::parse(&stored).ok_or_else(|| {
                DbError::CorruptRow(format!("unrecognised dimension '{stored}' on a journal line"))
            })?;

            Ok(DimensionValue {
                dimension,
                id: row.try_get("value_id").map_err(DbError::Query)?,
                code: row.try_get("value_code").map_err(DbError::Query)?,
                name: row.try_get("value_name").map_err(DbError::Query)?,
            })
        })
        .collect()
}

fn read_line(
    row: &sqlx::postgres::PgRow,
    id: Uuid,
    dimensions: Vec<DimensionValue>,
) -> Result<PostedLine, DbError> {
    let stored_side: String = row.try_get("side").map_err(DbError::Query)?;
    let side = Side::parse(&stored_side).ok_or_else(|| {
        DbError::CorruptRow(format!("unrecognised side '{stored_side}' on a journal line"))
    })?;

    let currency = currency_of(row, "currency_code")?;
    let base_currency = currency_of(row, "base_currency_code")?;

    Ok(PostedLine {
        id,
        position: row.try_get("position").map_err(DbError::Query)?,
        account_id: row.try_get("account_id").map_err(DbError::Query)?,
        account_number: row.try_get("account_number").map_err(DbError::Query)?,
        account_name: row.try_get("account_name").map_err(DbError::Query)?,
        side,
        amount: money_of(row, "amount", currency)?,
        base_amount: money_of(row, "base_amount", base_currency)?,
        exchange_rate: row.try_get("exchange_rate").map_err(DbError::Query)?,
        rate_date: row.try_get("rate_date").map_err(DbError::Query)?,
        memo: row.try_get("memo").map_err(DbError::Query)?,
        dimensions,
    })
}

fn read_summary(row: sqlx::postgres::PgRow) -> Result<JournalSummary, DbError> {
    // A journal always has lines, so the aggregate always has a currency. The
    // fallback is the workspace default rather than a failure: a header with no
    // lines is corruption worth showing rather than a screen that will not draw.
    let currency = currency_of(&row, "base_currency_code").unwrap_or_default();
    let reverses_id: Option<Uuid> = row.try_get("reverses_id").map_err(DbError::Query)?;

    Ok(JournalSummary {
        id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        entry_date: row.try_get("entry_date").map_err(DbError::Query)?,
        period_label: row.try_get("period_label").map_err(DbError::Query)?,
        narration: row.try_get("narration").map_err(DbError::Query)?,
        source_app: row.try_get("source_app").map_err(DbError::Query)?,
        source_doc_type: row.try_get("source_doc_type").map_err(DbError::Query)?,
        source_doc_id: row.try_get("source_doc_id").map_err(DbError::Query)?,
        is_reversal: reverses_id.is_some(),
        total: money_of(&row, "total", currency)?,
        line_count: row.try_get("line_count").map_err(DbError::Query)?,
    })
}

fn currency_of(row: &sqlx::postgres::PgRow, column: &str) -> Result<Currency, DbError> {
    let code: String = row.try_get(column).map_err(DbError::Query)?;

    Currency::parse(&code)
        .map_err(|err| DbError::CorruptRow(format!("unusable currency on a journal line: {err}")))
}

/// An amount column, read back as text so no digit is lost in the driver.
fn money_of(row: &sqlx::postgres::PgRow, column: &str, currency: Currency) -> Result<Money, DbError> {
    let digits: String = row.try_get(column).map_err(DbError::Query)?;

    Money::parse(currency, &digits)
        .map_err(|err| DbError::CorruptRow(format!("unusable amount on a journal: {err}")))
}

/// The unique index a duplicate number lands on.
const NUMBER_INDEX: &str = "journals_number_key";

fn as_number_conflict(err: sqlx::Error, number: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(NUMBER_INDEX) => DbError::CodeExists {
            entity: "journal",
            code: number.to_owned(),
        },
        _ => DbError::Query(err),
    }
}
