//! `inventory.units`: what stock is counted in.
//!
//! A few dozen rows at most, so [`list`] reads them all and the caller filters.
//!
//! The factor is a `NUMERIC(19, 6)` and crosses this boundary as **text**, for
//! the reason every exact number in this codebase does: sqlx would hand back a
//! float, and a float factor is how twelve cases of twelve becomes 143.99999.
//! `app_inventory::unit::parse_factor` turns the text into the scaled integer.
//!
//! `updated_at` is set at the call site. There is no trigger - see ADR 0001.

use app_inventory::unit::{Checked, Unit, UnitClass};
use phonix_core::identity::UserId;
use sqlx::{FromRow, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const CODE_INDEX: &str = "units_code";
const BASE_INDEX: &str = "units_one_base_per_class";

fn as_conflict(err: sqlx::Error, code: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(CODE_INDEX) => DbError::CodeExists {
            entity: "unit",
            code: code.to_owned(),
        },
        // A second base for a class is not a duplicate code, and saying so
        // would send a form's error to the wrong field.
        sqlx::Error::Database(db) if db.constraint() == Some(BASE_INDEX) => DbError::CodeExists {
            entity: "unit_base",
            code: code.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

/// A newtype so `FromRow` can live on a type this crate does not own -
/// `app-inventory` compiles to wasm and has no sqlx.
struct RowOf<T>(T);

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Unit> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let class: String = row.try_get("class")?;
        let factor: String = row.try_get("factor")?;

        Ok(Self(Unit {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            // A class the build does not know means a row written by a newer
            // deployment. Counted rather than guessed at: `Count` would put a
            // kilogram in the wrong conversion group.
            class: UnitClass::parse(&class).ok_or_else(|| sqlx::Error::Decode(
                format!("units.class holds '{class}', which this build does not know").into(),
            ))?,
            factor_scaled: app_inventory::unit::parse_factor(&factor).map_err(|err| {
                sqlx::Error::Decode(format!("units.factor holds '{factor}': {err}").into())
            })?,
            is_base: row.try_get("is_base")?,
            is_active: row.try_get("is_active")?,
        }))
    }
}

/// Every unit, base first within each class and then alphabetically.
///
/// Inactive rows included: a retired unit is still named on every historic
/// quantity counted in it, and hiding it would leave those rows unlabelled.
pub async fn list<'e, E>(executor: E) -> Result<Vec<Unit>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<Unit>>(
        "SELECT id, code, name, class, factor::text AS factor, is_base, is_active
           FROM inventory.units
          ORDER BY class, is_base DESC, factor, code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// The units a picker offers: active ones only.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<Unit>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<Unit>>(
        "SELECT id, code, name, class, factor::text AS factor, is_base, is_active
           FROM inventory.units
          WHERE is_active
          ORDER BY class, is_base DESC, factor, code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// One unit, whatever state it is in.
pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Unit>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Unit>>(
        "SELECT id, code, name, class, factor::text AS factor, is_base, is_active
           FROM inventory.units
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
    draft: &Checked,
    actor: Option<UserId>,
) -> Result<Uuid, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "INSERT INTO inventory.units
             (code, name, class, factor, is_base, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4::numeric, $4::numeric = 1, $5, $6, $6)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.class.as_str())
    .bind(app_inventory::unit::factor_to_string(draft.factor_scaled))
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(executor)
    .await
    .map_err(|err| as_conflict(err, &draft.code))
}

/// Change one. Answers whether a row was there to change.
pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &Checked,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.units
            SET code       = $2,
                name       = $3,
                class      = $4,
                factor     = $5::numeric,
                is_base    = $5::numeric = 1,
                is_active  = $6,
                updated_at = now(),
                updated_by = $7
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.class.as_str())
    .bind(app_inventory::unit::factor_to_string(draft.factor_scaled))
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_conflict(err, &draft.code))?;

    Ok(result.rows_affected() > 0)
}

/// How many items count in this unit, asked before retiring one.
pub async fn item_count<'e, E>(executor: E, id: Uuid) -> Result<i64, DbError>
where
    E: PgExecutor<'e>,
{
    sqlx::query_scalar(
        "SELECT count(*) FROM inventory.items
          WHERE stock_unit_id = $1 OR purchase_unit_id = $1",
    )
    .bind(id)
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)
}

/// Remove one. `ON DELETE RESTRICT` from `items` means Postgres refuses a unit
/// anything is counted in, so this answers only for one nothing uses.
pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query("DELETE FROM inventory.units WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}
