//! `inventory.lots`: which particular units these are.
//!
//! # A receipt finds or creates one in the same breath
//!
//! Goods arrive against a lot number printed on the carton, and whether this
//! workspace has seen that number before is not something the person keying it
//! should have to know. [`ensure`] is the whole of that: find it, or make it,
//! in the caller's transaction.

use app_inventory::item::Tracking;
use app_inventory::lot::{Lot, LotSummary};
use app_inventory::quantity::Quantity;
use chrono::NaiveDate;
use phonix_core::identity::UserId;
use sqlx::{AssertSqlSafe, FromRow, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const NUMBER_INDEX: &str = "lots_number";

struct RowOf<T>(T);

fn read_tracking(raw: &str) -> Result<Tracking, sqlx::Error> {
    Tracking::parse(raw).ok_or_else(|| {
        sqlx::Error::Decode(
            format!("lots.tracking holds '{raw}', which this build does not know").into(),
        )
    })
}

fn read_quantity(raw: &str, column: &str) -> Result<Quantity, sqlx::Error> {
    Quantity::parse(raw)
        .map_err(|err| sqlx::Error::Decode(format!("{column} holds '{raw}': {err}").into()))
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Lot> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let tracking: String = row.try_get("tracking")?;

        Ok(Self(Lot {
            id: row.try_get("id")?,
            variant_id: row.try_get("variant_id")?,
            number: row.try_get("number")?,
            expires_on: row.try_get("expires_on")?,
            tracking: read_tracking(&tracking)?,
        }))
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<LotSummary> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let tracking: String = row.try_get("tracking")?;
        let on_hand: String = row.try_get("on_hand")?;

        Ok(Self(LotSummary {
            id: row.try_get("id")?,
            variant_id: row.try_get("variant_id")?,
            variant_code: row.try_get("variant_code")?,
            item_name: row.try_get("item_name")?,
            number: row.try_get("number")?,
            expires_on: row.try_get("expires_on")?,
            tracking: read_tracking(&tracking)?,
            on_hand: read_quantity(&on_hand, "lots.on_hand")?,
        }))
    }
}

const COLUMNS: &str = "id, variant_id, number, expires_on, tracking";

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Lot>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(
        sqlx::query_as::<_, RowOf<Lot>>(AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM inventory.lots WHERE id = $1"
        )))
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(DbError::Query)?
            .map(|row| row.0),
    )
}

/// One by the number on the carton, case-insensitively - `lot-1` and `LOT-1`
/// are one batch, on the same terms as an item code.
pub async fn find_by_number<'e, E>(
    executor: E,
    variant_id: Uuid,
    number: &str,
) -> Result<Option<Lot>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Lot>>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM inventory.lots
          WHERE variant_id = $1 AND lower(number) = lower($2)"
    )))
    .bind(variant_id)
    .bind(number)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// Every lot of one variant, newest expiry last, with what is on hand of each.
///
/// Ordered the way FEFO reaches for them - nearest expiry first, undated last -
/// so a picking screen offers them in the order the policy wants without
/// sorting a second time in the browser.
pub async fn for_variant<'e, E>(executor: E, variant_id: Uuid) -> Result<Vec<LotSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<LotSummary>>(
        "SELECT l.id, l.variant_id, l.number, l.expires_on, l.tracking,
                v.code AS variant_code,
                i.name AS item_name,
                COALESCE((
                    SELECT sum(q.quantity)
                      FROM inventory.stock_quants q
                      JOIN inventory.locations loc ON loc.id = q.location_id
                     WHERE q.lot_id = l.id AND loc.kind = 'internal'
                ), 0)::text AS on_hand
           FROM inventory.lots l
           JOIN inventory.item_variants v ON v.id = l.variant_id
           JOIN inventory.items i ON i.id = v.item_id
          WHERE l.variant_id = $1
          ORDER BY l.expires_on NULLS LAST, l.number",
    )
    .bind(variant_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

pub async fn insert(
    conn: &mut PgConnection,
    variant_id: Uuid,
    number: &str,
    expires_on: Option<NaiveDate>,
    tracking: Tracking,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO inventory.lots (variant_id, number, expires_on, tracking, created_by)
              VALUES ($1, $2, $3, $4, $5)
           RETURNING id",
    )
    .bind(variant_id)
    .bind(number)
    .bind(expires_on)
    .bind(tracking.as_str())
    .bind(actor)
    .fetch_one(conn)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(NUMBER_INDEX) => DbError::CodeExists {
            entity: "lot",
            code: number.to_owned(),
        },
        _ => DbError::Query(err),
    })
}

/// The lot this number stands for, creating it if it is new.
///
/// An expiry given for a lot that already exists is *not* written over the one
/// on file: two receipts of the same batch that disagree about its date are a
/// question for a person, and quietly taking the newer answer would move the
/// date every recall depends on.
pub async fn ensure(
    conn: &mut PgConnection,
    variant_id: Uuid,
    number: &str,
    expires_on: Option<NaiveDate>,
    tracking: Tracking,
    actor: Option<UserId>,
) -> Result<Lot, DbError> {
    if let Some(existing) = find_by_number(&mut *conn, variant_id, number).await? {
        return Ok(existing);
    }

    let id = insert(conn, variant_id, number, expires_on, tracking, actor).await?;

    Ok(Lot {
        id,
        variant_id,
        number: number.to_owned(),
        expires_on,
        tracking,
    })
}
