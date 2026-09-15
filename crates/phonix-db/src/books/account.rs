//! `books.accounts`: the chart of accounts.
//!
//! So far this is the seeding path and the reads a picker needs. Journals come
//! next; until they exist, nothing has been posted to any of these accounts and
//! the chart is a vocabulary rather than a ledger.

use app_books::account::{Account, AccountClass, AccountInput, AccountType, DefaultChart};
use phonix_core::identity::UserId;
use phonix_core::query::{Page, PageRequest, Sort};
use sqlx::{AssertSqlSafe, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;
use crate::listing::{self, Sortable};

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

/// The type filter: one `AccountType`.
pub const ACCOUNT_TYPE: &str = "account_type";

/// The class filter: one `AccountClass`, answered as the types in it.
pub const CLASS: &str = "class";

/// The postable filter: `yes` or `no`.
pub const POSTABLE: &str = "postable";

/// The status filter: `active` or `inactive`.
pub const STATUS: &str = "status";

/// The types a person may post to by hand, as a SQL list.
///
/// Generated from `AccountType::ALL` rather than written out here: the rule is
/// `allows_manual_posting`, and a second copy of it in SQL is a copy that stops
/// agreeing the first time a type is added.
fn manual_types() -> String {
    let types: Vec<&str> = AccountType::ALL
        .iter()
        .filter(|account_type| account_type.allows_manual_posting())
        .map(|account_type| account_type.as_str())
        .collect();

    format!("'{}'", types.join("', '"))
}

/// `is_postable`, as the database sees it: active, and not a control account.
fn postable_sql() -> String {
    format!("(a.is_active AND a.account_type IN ({}))", manual_types())
}

/// The class of each type, as an ordering. Same reasoning as [`manual_types`].
fn class_order() -> String {
    let arms: String = AccountType::ALL
        .iter()
        .map(|account_type| {
            let class = AccountClass::ALL
                .iter()
                .position(|class| *class == account_type.class())
                .unwrap_or(AccountClass::ALL.len());

            format!(" WHEN '{}' THEN {class}", account_type.as_str())
        })
        .collect();

    format!(
        "CASE a.account_type{arms} ELSE {} END",
        AccountClass::ALL.len()
    )
}

/// The types in one class, for the class filter. Empty for a value that names
/// no class, which matches no row - as the grid's closure did.
fn types_in(class: &str) -> Vec<String> {
    AccountType::ALL
        .iter()
        .filter(|account_type| account_type.class().as_str() == class)
        .map(|account_type| account_type.as_str().to_owned())
        .collect()
}

const SORTABLE: &[Sortable] = &[
    ("number", "a.number"),
    ("name", "a.name"),
    ("type", "a.account_type"),
    ("is_active", "a.is_active"),
];

/// The `ORDER BY` fragment, defaulting to number - the order an accountant
/// reads a chart in, because the ranges are the classification.
///
/// `class` and `postable` are generated expressions rather than columns, so
/// they cannot sit in [`SORTABLE`], which holds `&'static str`. The direction is
/// one of two literals; see `listing::order_by`.
fn order_for(sort: Option<&Sort>) -> String {
    match sort.map(|sort| (sort.field.as_str(), sort.direction.sql())) {
        Some(("class", direction)) => format!("{} {direction}", class_order()),
        Some(("postable", direction)) => format!("{} {direction}", postable_sql()),
        _ => listing::order_by(sort, SORTABLE, "a.number"),
    }
}

const FROM: &str = "FROM books.accounts a";

/// A filter nobody set is a NULL that discards its own line.
///
/// `a.account_type` is matched as stored, not as the word the screen draws for
/// it: the label is translated in the browser and SQL cannot see it.
const WHERE: &str = "WHERE ($1::text IS NULL OR a.account_type = $1)
            AND ($2::text[] IS NULL OR a.account_type = ANY($2))
            AND ($4::bool IS NULL OR a.is_active = $4)
            AND ($5::text IS NULL
                 OR a.number ILIKE $5
                 OR a.name ILIKE $5
                 OR a.account_type ILIKE $5)";

/// One page of the chart.
///
/// The count and the select share [`FROM`] and [`WHERE`], so a filtered page
/// cannot be counted against a different set of rows than it draws.
pub async fn page(pool: &sqlx::PgPool, request: &PageRequest) -> Result<Page<Account>, DbError> {
    let request = request.sanitised();
    let needle = request
        .needle()
        .map(|needle| crate::search::contains(&needle));

    let account_type = request.filter(ACCOUNT_TYPE);
    let class = request.filter(CLASS).map(types_in);

    let postable = match request.filter(POSTABLE) {
        Some("yes") => Some(true),
        Some("no") => Some(false),
        _ => None,
    };

    let active = match request.filter(STATUS) {
        Some("active") => Some(true),
        Some("inactive") => Some(false),
        _ => None,
    };

    // `$3` is the postable predicate, which is an expression rather than a
    // column and so cannot be compared in the `WHERE` constant.
    let postable_clause = format!("AND ($3::bool IS NULL OR {} = $3)", postable_sql());
    let counting = AssertSqlSafe(format!("SELECT count(*) {FROM} {WHERE} {postable_clause}"));

    let total: i64 = sqlx::query_scalar(counting)
        .bind(account_type)
        .bind(class.as_deref())
        .bind(postable)
        .bind(active)
        .bind(needle.as_deref())
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

    let total = u64::try_from(total).unwrap_or(0);
    let request = request.clamped_to(total);

    let order = order_for(request.sort.as_ref());

    let selecting = AssertSqlSafe(format!(
        "SELECT a.id, a.number, a.name, a.account_type, a.description,
                a.is_active, a.is_default
           {FROM}
           {WHERE} {postable_clause}
          ORDER BY {order}, a.number
          LIMIT $6 OFFSET $7"
    ));

    let rows = sqlx::query(selecting)
        .bind(account_type)
        .bind(class.as_deref())
        .bind(postable)
        .bind(active)
        .bind(needle.as_deref())
        .bind(request.limit() as i64)
        .bind(request.offset() as i64)
        .fetch_all(pool)
        .await
        .map_err(DbError::Query)?;

    let accounts = rows
        .into_iter()
        .map(read_account)
        .collect::<Result<Vec<_>, DbError>>()?;

    Ok(Page::new(accounts, total, &request))
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
