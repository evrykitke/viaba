//! `inventory.adjustment_types`: why a stock figure was corrected by hand.
//!
//! A few dozen rows at most, so [`list`] reads them all and the caller filters.
//!
//! The account is three columns and no foreign key, exactly as
//! `account_mappings` is: the chart of accounts belongs to Books, and an app
//! holding a key into another app's schema is an app that can never be
//! uninstalled. See migration 0001.
//!
//! `updated_at` is set at the call site. There is no trigger - see ADR 0001.

use app_inventory::accounts::AccountRef;
use app_inventory::adjustment::{
    AdjustmentType, AdjustmentTypeSummary, CheckedAdjustmentType, Direction,
};
use phonix_core::identity::UserId;
use sqlx::{FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const CODE_INDEX: &str = "adjustment_types_code_unique";

fn as_conflict(err: sqlx::Error, code: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(CODE_INDEX) => DbError::CodeExists {
            entity: "adjustment_type",
            code: code.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

/// A newtype so `FromRow` can live on a type this crate does not own -
/// `app-inventory` compiles to wasm and has no sqlx.
struct RowOf<T>(T);

fn account_of(row: &sqlx::postgres::PgRow) -> Result<Option<AccountRef>, sqlx::Error> {
    let account_id: Option<Uuid> = row.try_get("account_id")?;

    // The row constraint keeps the three columns whole, so a set id with no
    // number beside it is a row written past the constraint rather than an
    // ordinary absence - and guessing at it would put a blank line on a screen.
    match account_id {
        None => Ok(None),
        Some(account_id) => Ok(Some(AccountRef {
            account_id,
            number: row.try_get("account_number")?,
            name: row.try_get("account_name")?,
        })),
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<AdjustmentType> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let direction: String = row.try_get("direction")?;

        Ok(Self(AdjustmentType {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            // A direction this build does not know means a row written by a
            // newer deployment. Refused rather than guessed at: `Both` would
            // let stock be booked the one way the workspace ruled out.
            direction: Direction::parse(&direction).ok_or_else(|| {
                sqlx::Error::Decode(
                    format!(
                        "adjustment_types.direction holds '{direction}', \
                         which this build does not know"
                    )
                    .into(),
                )
            })?,
            account: account_of(row)?,
            needs_approval: row.try_get("needs_approval")?,
            is_active: row.try_get("is_active")?,
            is_system: row.try_get("is_system")?,
            note: row.try_get("note")?,
        }))
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<AdjustmentTypeSummary> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let direction: String = row.try_get("direction")?;

        Ok(Self(AdjustmentTypeSummary {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            direction: Direction::parse(&direction).ok_or_else(|| {
                sqlx::Error::Decode(
                    format!(
                        "adjustment_types.direction holds '{direction}', \
                         which this build does not know"
                    )
                    .into(),
                )
            })?,
            account_number: row.try_get("account_number")?,
            account_name: row.try_get("account_name")?,
            needs_approval: row.try_get("needs_approval")?,
            is_active: row.try_get("is_active")?,
            is_system: row.try_get("is_system")?,
            move_count: row.try_get("move_count")?,
        }))
    }
}

/// Every type, with how much has been booked under each.
///
/// Retired rows included: a retired reason is still named on every movement
/// booked under it, and hiding it here would leave the grid unable to explain
/// what the movements screen shows.
pub async fn list<'e, E>(executor: E) -> Result<Vec<AdjustmentTypeSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<AdjustmentTypeSummary>>(
        "SELECT t.id, t.code, t.name, t.direction,
                t.account_number, t.account_name,
                t.needs_approval, t.is_active, t.is_system,
                (SELECT count(*) FROM inventory.stock_moves m
                  WHERE m.adjustment_type_id = t.id) AS move_count
           FROM inventory.adjustment_types t
          ORDER BY t.is_active DESC, t.code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// The types the adjust screen offers: active ones only.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<AdjustmentType>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<AdjustmentType>>(
        "SELECT id, code, name, direction,
                account_id, account_number, account_name,
                needs_approval, is_active, is_system, note
           FROM inventory.adjustment_types
          WHERE is_active
          ORDER BY code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// One type, whatever state it is in.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<AdjustmentType>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<AdjustmentType>>(
        "SELECT id, code, name, direction,
                account_id, account_number, account_name,
                needs_approval, is_active, is_system, note
           FROM inventory.adjustment_types
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

pub async fn insert<'e, E>(
    executor: E,
    draft: &CheckedAdjustmentType,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO inventory.adjustment_types
             (code, name, direction, account_id, account_number, account_name,
              needs_approval, is_active, note, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.direction.as_str())
    .bind(draft.account.as_ref().map(|account| account.account_id))
    .bind(draft.account.as_ref().map(|account| account.number.clone()))
    .bind(draft.account.as_ref().map(|account| account.name.clone()))
    .bind(draft.needs_approval)
    .bind(draft.is_active)
    .bind(draft.note.as_deref())
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| as_conflict(err, &draft.code))
}

/// Change one. Answers whether a row was there to change.
///
/// `is_system` is not among the columns: what the app seeded is a fact about
/// where the row came from, and no edit makes it untrue.
pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &CheckedAdjustmentType,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.adjustment_types
            SET code           = $2,
                name           = $3,
                direction      = $4,
                account_id     = $5,
                account_number = $6,
                account_name   = $7,
                needs_approval = $8,
                is_active      = $9,
                note           = $10,
                updated_at     = now(),
                updated_by     = $11
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.direction.as_str())
    .bind(draft.account.as_ref().map(|account| account.account_id))
    .bind(draft.account.as_ref().map(|account| account.number.clone()))
    .bind(draft.account.as_ref().map(|account| account.name.clone()))
    .bind(draft.needs_approval)
    .bind(draft.is_active)
    .bind(draft.note.as_deref())
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_conflict(err, &draft.code))?;

    Ok(result.rows_affected() > 0)
}

/// How many movements have been booked under this type, asked before deleting.
pub async fn move_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "SELECT count(*) FROM inventory.stock_moves WHERE adjustment_type_id = $1",
    )
    .bind(id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)
}

/// Remove one. `ON DELETE RESTRICT` from `stock_moves` means Postgres refuses a
/// type anything was booked under, so this answers only for one nothing used.
///
/// A seeded type is excluded here as well as in the service: the row is not the
/// workspace's to throw away, and the two guards cost one predicate.
pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "DELETE FROM inventory.adjustment_types WHERE id = $1 AND NOT is_system",
    )
    .bind(id)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}
