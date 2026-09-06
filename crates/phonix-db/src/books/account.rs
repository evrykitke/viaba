//! `books.accounts`: the chart of accounts.
//!
//! So far this is the seeding path and the reads a picker needs. Journals come
//! next; until they exist, nothing has been posted to any of these accounts and
//! the chart is a vocabulary rather than a ledger.

use app_books::account::{Account, AccountInput, AccountType, DefaultChart};
use phonix_core::identity::UserId;
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

/// The unique index a duplicate number lands on.
const NUMBER_INDEX: &str = "accounts_number_key";

fn as_number_conflict(err: sqlx::Error, number: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(NUMBER_INDEX) => DbError::CodeExists {
            entity: "account",
            code: number.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

/// Add an account to the chart.
///
/// `is_default` is not written: a row somebody added by hand is theirs, not
/// something a redeploy may reason about.
pub async fn insert<'e, E>(
    executor: E,
    draft: &AccountInput,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO books.accounts
             (number, name, account_type, description, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $6)
         RETURNING id",
    )
    .bind(&draft.number)
    .bind(&draft.name)
    .bind(draft.account_type.as_str())
    .bind(draft.description.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| as_number_conflict(err, &draft.number))
}

/// Change one. Answers whether a row was there to change.
pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &AccountInput,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE books.accounts
            SET number       = $2,
                name         = $3,
                account_type = $4,
                description  = $5,
                is_active    = $6,
                updated_at   = now(),
                updated_by   = $7
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.number)
    .bind(&draft.name)
    .bind(draft.account_type.as_str())
    .bind(draft.description.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_number_conflict(err, &draft.number))?;

    Ok(result.rows_affected() > 0)
}

/// Install the chart a workspace starts with.
///
/// `ON CONFLICT DO NOTHING` against `accounts_number_key`, which is what makes
/// this safe to run on every migration pass rather than only the first: an
/// upgrade that adds an account reaches the workspaces that already have Books,
/// and a workspace that deleted or renamed one never has it put back.
///
/// Returns how many rows were actually inserted, which on the second run is
/// zero.
///
/// # Why one statement and not a loop
///
/// A chart is two hundred rows. Two hundred round trips inside the provisioning
/// transaction is two hundred chances to be interrupted half-installed, and the
/// unnest form is one statement either way.
pub async fn install_defaults<'e, E>(executor: E, chart: &DefaultChart) -> Result<u64, DbError>
where
    E: PgExecutor<'e>,
{
    if chart.account.is_empty() {
        return Ok(0);
    }

    let numbers: Vec<&str> = chart.account.iter().map(|it| it.number.as_str()).collect();
    let names: Vec<&str> = chart.account.iter().map(|it| it.name.as_str()).collect();
    let types: Vec<&str> = chart
        .account
        .iter()
        .map(|it| it.account_type.as_str())
        .collect();
    let descriptions: Vec<Option<&str>> = chart
        .account
        .iter()
        .map(|it| it.description.as_deref())
        .collect();

    let inserted = sqlx::query(
        "INSERT INTO books.accounts (number, name, account_type, description, is_default)
              SELECT number, name, account_type, description, TRUE
                FROM unnest($1::text[], $2::text[], $3::text[], $4::text[])
                  AS t(number, name, account_type, description)
         ON CONFLICT DO NOTHING",
    )
    .bind(&numbers)
    .bind(&names)
    .bind(&types)
    .bind(&descriptions)
    .execute(executor)
    .await
    .map_err(DbError::Query)?
    .rows_affected();

    Ok(inserted)
}

/// Every account, in the order an accountant reads them.
pub async fn list<'e, E>(executor: E) -> Result<Vec<Account>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id, number, name, account_type, description, is_active, is_default
           FROM books.accounts
          ORDER BY number",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(read_account).collect()
}

/// One account by id.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Account>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, number, name, account_type, description, is_active, is_default
           FROM books.accounts
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read_account).transpose()
}

/// One account by number, case-insensitively — matching `accounts_number_key`.
///
/// How a posting routine reaches "the receivables one" when the workspace has
/// told it which number that is.
pub async fn find_by_number<'e, E>(executor: E, number: &str) -> Result<Option<Account>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT id, number, name, account_type, description, is_active, is_default
           FROM books.accounts
          WHERE lower(number) = lower($1)",
    )
    .bind(number)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.map(read_account).transpose()
}

/// How many accounts there are, and how many are active.
pub async fn counts<'e, E>(executor: E) -> Result<(i64, i64), DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE is_active) AS active
           FROM books.accounts",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Ok((
        row.try_get("total").map_err(DbError::Query)?,
        row.try_get("active").map_err(DbError::Query)?,
    ))
}

/// Read a row, refusing an `account_type` this build does not know.
///
/// A stored type that will not parse is corruption, not a default: guessing
/// would put a credit balance on the debit side of a report. Same reason
/// `InvoiceStatus::parse` returns an `Option`.
fn read_account(row: sqlx::postgres::PgRow) -> Result<Account, DbError> {
    let raw: String = row.try_get("account_type").map_err(DbError::Query)?;
    let number: String = row.try_get("number").map_err(DbError::Query)?;

    let Some(account_type) = AccountType::parse(&raw) else {
        return Err(DbError::CorruptCatalogRow {
            slug: number,
            reason: format!("account_type '{raw}' is not one this build knows"),
        });
    };

    Ok(Account {
        id: row.try_get("id").map_err(DbError::Query)?,
        number: row.try_get("number").map_err(DbError::Query)?,
        name: row.try_get("name").map_err(DbError::Query)?,
        account_type,
        description: row.try_get("description").map_err(DbError::Query)?,
        is_active: row.try_get("is_active").map_err(DbError::Query)?,
        is_default: row.try_get("is_default").map_err(DbError::Query)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chart that actually ships, loaded the way provisioning loads it.
    ///
    /// This is the test that matters most in this module: everything else here
    /// needs a database, but a broken `config/defaults/books.toml` would fail
    /// at provisioning time on somebody's live deployment, and it costs nothing
    /// to find out here instead.
    #[test]
    fn the_shipped_chart_of_accounts_loads_and_is_valid() {
        let chart: DefaultChart = phonix_config::defaults::load_for(app_books::APP_ID)
            .expect("config/defaults/books.toml has to load");

        chart.check().expect("the shipped chart has to be valid");

        // Exhaustive rather than a starter set - see ADR 0006 section 4. The
        // floor is deliberately loose; it is here to catch the file going
        // missing or being truncated, not to police the exact count.
        assert!(
            chart.account.len() > 150,
            "the default chart is meant to be exhaustive, found {}",
            chart.account.len()
        );
    }

    /// The accounts a starter chart leaves out, which is the whole argument of
    /// ADR 0006 section 4. Named by type rather than by number, because the
    /// number is a convention and the type is not.
    #[test]
    fn the_chart_has_the_accounts_the_incumbents_leave_out() {
        let chart: DefaultChart =
            phonix_config::defaults::load_for(app_books::APP_ID).expect("loads");

        let has = |wanted: AccountType| chart.account.iter().any(|it| it.account_type == wanted);

        // Without this there is no three-way match (section 6.5).
        assert!(has(AccountType::GoodsReceivedNotInvoiced));
        // Without these the sub-ledger cannot be the ledger (section 6.1).
        assert!(has(AccountType::Inventory));
        assert!(has(AccountType::AccountsReceivable));
        assert!(has(AccountType::AccountsPayable));
        // Contra accounts, which are why the balance is per type (section 4).
        assert!(has(AccountType::AccumulatedDepreciation));
        assert!(has(AccountType::ContraAsset));
        assert!(has(AccountType::ContraRevenue));
        assert!(has(AccountType::ContraEquity));

        // And the named ones, by number, because these are the ones the record
        // calls out by name.
        let numbered = |number: &str| chart.account.iter().any(|it| it.number == number);
        assert!(numbered("5090"), "landed cost absorbed");
        assert!(numbered("5230"), "purchase price variance");
        assert!(numbered("1240"), "inventory in transit");
        assert!(numbered("8040"), "foreign exchange gain");
        assert!(numbered("8540"), "foreign exchange loss");
        assert!(numbered("8550"), "rounding differences");
        assert!(numbered("1410"), "suspense");
    }

    /// Input and output tax are separate accounts, per section 4. Netting them
    /// into one is how a tax return stops being checkable.
    #[test]
    fn input_and_output_tax_are_held_apart() {
        let chart: DefaultChart =
            phonix_config::defaults::load_for(app_books::APP_ID).expect("loads");

        let input = chart.account.iter().find(|it| it.number == "1170");
        let output = chart.account.iter().find(|it| it.number == "2100");

        let input = input.expect("input tax recoverable");
        let output = output.expect("output tax payable");

        // One is an asset and the other a liability, which is the point.
        assert_eq!(input.account_type.class(), app_books::AccountClass::Asset);
        assert_eq!(
            output.account_type.class(),
            app_books::AccountClass::Liability
        );
    }
}
