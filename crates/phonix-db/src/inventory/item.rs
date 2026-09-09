//! `inventory.items`: the thing itself.
//!
//! # Money and quantities cross this boundary as text
//!
//! `cost::text`, never `cost`. sqlx would hand back a float, and a float cost
//! times a thousand units is wrong by more than the rounding. The caller parses
//! it into `Money` with the workspace's own currency, which is why the currency
//! is a parameter of every read here: the column holds a number and the
//! workspace holds what it is denominated in.
//!
//! # Creating an item creates its default variant
//!
//! [`insert`] takes a transaction for the reason a warehouse's does. Every item
//! has at least one variant and stock hangs off the variant, so an item written
//! without one is an item nothing can be received against.

use app_inventory::item::{Checked, Item, ItemKind, ItemSummary, Tracking};
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const CODE_INDEX: &str = "items_code";
const BARCODE_INDEX: &str = "items_barcode";

fn as_conflict(err: sqlx::Error, code: &str, barcode: Option<&str>) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(CODE_INDEX) => DbError::CodeExists {
            entity: "item",
            code: code.to_owned(),
        },
        // Its own entity so a form puts the message on the barcode field. A
        // duplicate UPC means two rows claim the same physical packet, which is
        // a different mistake from a duplicate code.
        sqlx::Error::Database(db) if db.constraint() == Some(BARCODE_INDEX) => {
            DbError::CodeExists {
                entity: "item_barcode",
                code: barcode.unwrap_or_default().to_owned(),
            }
        }
        _ => DbError::Query(err),
    }
}

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("items.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_money(raw: &str, currency: Currency, column: &str) -> Result<Money, sqlx::Error> {
    Money::parse(currency, raw)
        .map_err(|err| sqlx::Error::Decode(format!("items.{column} holds '{raw}': {err}").into()))
}

fn read_item(row: &sqlx::postgres::PgRow, currency: Currency) -> Result<Item, sqlx::Error> {
    let kind: String = row.try_get("kind")?;
    let tracking: String = row.try_get("tracking")?;
    let cost: String = row.try_get("cost")?;
    let sale_price: Option<String> = row.try_get("sale_price")?;

    Ok(Item {
        id: row.try_get("id")?,
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        barcode: row.try_get("barcode")?,
        description: row.try_get("description")?,
        kind: ItemKind::parse(&kind).ok_or_else(|| unknown("kind", &kind))?,
        is_tracked: row.try_get("is_tracked")?,
        tracking: Tracking::parse(&tracking).ok_or_else(|| unknown("tracking", &tracking))?,
        uses_expiry: row.try_get("uses_expiry")?,
        category_id: row.try_get("category_id")?,
        category_name: row.try_get("category_name")?,
        stock_unit_id: row.try_get("stock_unit_id")?,
        stock_unit_code: row.try_get("stock_unit_code")?,
        purchase_unit_id: row.try_get("purchase_unit_id")?,
        purchase_unit_code: row.try_get("purchase_unit_code")?,
        cost: read_money(&cost, currency, "cost")?,
        sale_price: sale_price
            .map(|raw| read_money(&raw, currency, "sale_price"))
            .transpose()?,
        can_be_purchased: row.try_get("can_be_purchased")?,
        can_be_sold: row.try_get("can_be_sold")?,
        weight_grams: row.try_get("weight_grams")?,
        purchase_lead_days: row.try_get("purchase_lead_days")?,
        is_active: row.try_get("is_active")?,
    })
}

/// Every item, with the names a grid draws without a join per row.
///
/// `on_hand` is what is at every *internal* location: what somebody could walk
/// up to and pick. `None` for an item nobody counts, which is different from
/// zero and says so.
pub async fn list<'e, E>(executor: E, currency: Currency) -> Result<Vec<ItemSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT i.id, i.code, i.name, i.barcode, i.kind, i.is_tracked, i.tracking,
                i.cost::text AS cost, i.is_active,
                c.name AS category_name,
                u.code AS stock_unit_code,
                COALESCE((
                    SELECT sum(q.quantity)
                      FROM inventory.stock_quants q
                      JOIN inventory.item_variants v ON v.id = q.variant_id
                      JOIN inventory.locations l ON l.id = q.location_id
                     WHERE v.item_id = i.id AND l.kind = 'internal'
                ), 0)::text AS on_hand
           FROM inventory.items i
           JOIN inventory.categories c ON c.id = i.category_id
           JOIN inventory.units u ON u.id = i.stock_unit_id
          ORDER BY i.code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            let kind: String = row.try_get("kind").map_err(DbError::Query)?;
            let tracking: String = row.try_get("tracking").map_err(DbError::Query)?;
            let cost: String = row.try_get("cost").map_err(DbError::Query)?;
            let is_tracked: bool = row.try_get("is_tracked").map_err(DbError::Query)?;
            let on_hand: String = row.try_get("on_hand").map_err(DbError::Query)?;

            Ok(ItemSummary {
                id: row.try_get("id").map_err(DbError::Query)?,
                code: row.try_get("code").map_err(DbError::Query)?,
                name: row.try_get("name").map_err(DbError::Query)?,
                barcode: row.try_get("barcode").map_err(DbError::Query)?,
                kind: ItemKind::parse(&kind)
                    .ok_or_else(|| DbError::Query(unknown("kind", &kind)))?,
                is_tracked,
                tracking: Tracking::parse(&tracking)
                    .ok_or_else(|| DbError::Query(unknown("tracking", &tracking)))?,
                category_name: row.try_get("category_name").map_err(DbError::Query)?,
                stock_unit_code: row.try_get("stock_unit_code").map_err(DbError::Query)?,
                cost: Money::parse(currency, &cost)
                    .map_err(|err| DbError::Query(unknown("cost", &err.to_string())))?,
                is_active: row.try_get("is_active").map_err(DbError::Query)?,
                on_hand: is_tracked
                    .then(|| {
                        app_inventory::quantity::Quantity::parse(&on_hand)
                            .map_err(|err| DbError::Query(unknown("on_hand", &err.to_string())))
                    })
                    .transpose()?,
            })
        })
        .collect()
}

/// One item, with everything a detail screen shows above its tabs.
pub async fn find<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<Item>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT i.id, i.code, i.name, i.barcode, i.description, i.kind, i.is_tracked,
                i.tracking, i.uses_expiry, i.category_id, i.stock_unit_id,
                i.purchase_unit_id, i.cost::text AS cost, i.sale_price::text AS sale_price,
                i.can_be_purchased, i.can_be_sold, i.weight_grams, i.purchase_lead_days,
                i.is_active,
                c.name AS category_name,
                su.code AS stock_unit_code,
                pu.code AS purchase_unit_code
           FROM inventory.items i
           JOIN inventory.categories c ON c.id = i.category_id
           JOIN inventory.units su ON su.id = i.stock_unit_id
           JOIN inventory.units pu ON pu.id = i.purchase_unit_id
          WHERE i.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    row.as_ref()
        .map(|row| read_item(row, currency).map_err(DbError::Query))
        .transpose()
}

/// The item a scanned barcode names, its own or one of its variants'.
///
/// The query a till and a goods-in screen both run, so it is one statement
/// rather than two attempts: a scanner produces one string and the person
/// holding it does not know which table it is in.
pub async fn by_barcode<'e, E>(
    executor: E,
    barcode: &str,
    currency: Currency,
) -> Result<Option<(Item, Option<Uuid>)>, DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT i.id, i.code, i.name, i.barcode, i.description, i.kind, i.is_tracked,
                i.tracking, i.uses_expiry, i.category_id, i.stock_unit_id,
                i.purchase_unit_id, i.cost::text AS cost, i.sale_price::text AS sale_price,
                i.can_be_purchased, i.can_be_sold, i.weight_grams, i.purchase_lead_days,
                i.is_active,
                c.name AS category_name,
                su.code AS stock_unit_code,
                pu.code AS purchase_unit_code,
                v.id AS variant_id
           FROM inventory.items i
           JOIN inventory.categories c ON c.id = i.category_id
           JOIN inventory.units su ON su.id = i.stock_unit_id
           JOIN inventory.units pu ON pu.id = i.purchase_unit_id
           LEFT JOIN inventory.item_variants v
                  ON v.item_id = i.id AND v.barcode = $1
          WHERE i.barcode = $1 OR v.id IS NOT NULL
          LIMIT 1",
    )
    .bind(barcode)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?;

    match row {
        None => Ok(None),
        Some(row) => {
            let variant_id: Option<Uuid> = row.try_get("variant_id").map_err(DbError::Query)?;
            let item = read_item(&row, currency).map_err(DbError::Query)?;
            Ok(Some((item, variant_id)))
        }
    }
}

/// Create an item and its default variant.
///
/// `draft.code` is already allocated by the service, in this transaction, so a
/// rolled-back insert returns the number rather than burning it.
pub async fn insert(
    conn: &mut PgConnection,
    draft: &Checked,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO inventory.items
             (code, name, barcode, description, kind, is_tracked, tracking, uses_expiry,
              category_id, stock_unit_id, purchase_unit_id, cost, sale_price,
              can_be_purchased, can_be_sold, weight_grams, purchase_lead_days,
              is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                 $12::numeric, $13::numeric, $14, $15, $16, $17, $18, $19, $19)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.barcode.as_deref())
    .bind(draft.description.as_deref())
    .bind(draft.kind.as_str())
    .bind(draft.is_tracked)
    .bind(draft.tracking.as_str())
    .bind(draft.uses_expiry)
    .bind(draft.category_id)
    .bind(draft.stock_unit_id)
    .bind(draft.purchase_unit_id)
    .bind(cost_or_zero(&draft.cost))
    .bind(optional_amount(&draft.sale_price))
    .bind(draft.can_be_purchased)
    .bind(draft.can_be_sold)
    .bind(draft.weight_grams)
    .bind(draft.purchase_lead_days)
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| as_conflict(err, &draft.code, draft.barcode.as_deref()))?;

    // Every item has at least one variant, from the moment it exists. Stock
    // hangs off the variant, so an item written without one is an item nothing
    // can be received against.
    //
    // The barcode goes on the item AND on this variant: a workspace that never
    // uses variants scans the same code either way, and one that starts using
    // them later already has the default carrying it.
    sqlx::query(
        "INSERT INTO inventory.item_variants
             (item_id, code, barcode, is_default, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, TRUE, $4, $5, $5)",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(draft.barcode.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .execute(&mut *conn)
    .await
    .map_err(|err| as_conflict(err, &draft.code, draft.barcode.as_deref()))?;

    Ok(id)
}

/// Change one. Answers whether a row was there to change.
///
/// The stock unit and the tracking mode are written here like any other column;
/// the service is what refuses to change either once stock exists, because only
/// it can see whether any has.
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    draft: &Checked,
    actor: Option<UserId>,
) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE inventory.items
            SET code               = $2,
                name               = $3,
                barcode            = $4,
                description        = $5,
                kind               = $6,
                is_tracked         = $7,
                tracking           = $8,
                uses_expiry        = $9,
                category_id        = $10,
                stock_unit_id      = $11,
                purchase_unit_id   = $12,
                cost               = $13::numeric,
                sale_price         = $14::numeric,
                can_be_purchased   = $15,
                can_be_sold        = $16,
                weight_grams       = $17,
                purchase_lead_days = $18,
                is_active          = $19,
                updated_at         = now(),
                updated_by         = $20
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.barcode.as_deref())
    .bind(draft.description.as_deref())
    .bind(draft.kind.as_str())
    .bind(draft.is_tracked)
    .bind(draft.tracking.as_str())
    .bind(draft.uses_expiry)
    .bind(draft.category_id)
    .bind(draft.stock_unit_id)
    .bind(draft.purchase_unit_id)
    .bind(cost_or_zero(&draft.cost))
    .bind(optional_amount(&draft.sale_price))
    .bind(draft.can_be_purchased)
    .bind(draft.can_be_sold)
    .bind(draft.weight_grams)
    .bind(draft.purchase_lead_days)
    .bind(draft.is_active)
    .bind(actor)
    .execute(&mut *conn)
    .await
    .map_err(|err| as_conflict(err, &draft.code, draft.barcode.as_deref()))?;

    if result.rows_affected() == 0 {
        return Ok(false);
    }

    // The default variant follows the item it stands for. Only the default:
    // a variant somebody named is theirs.
    sqlx::query(
        "UPDATE inventory.item_variants
            SET code       = $2,
                barcode    = $3,
                is_active  = $4,
                updated_at = now(),
                updated_by = $5
          WHERE item_id = $1 AND is_default",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(draft.barcode.as_deref())
    .bind(draft.is_active)
    .bind(actor)
    .execute(&mut *conn)
    .await
    .map_err(|err| as_conflict(err, &draft.code, draft.barcode.as_deref()))?;

    Ok(true)
}

/// The standing cost alone, for arithmetic that has to add to it.
///
/// What a landed cost reads before rebasing an average - the whole item row is
/// a form's worth of columns to answer one number.
pub async fn cost_of<'e, E>(
    executor: E,
    id: Uuid,
    currency: Currency,
) -> Result<Option<Money>, DbError>
where
    E: PgExecutor<'e>,
{
    let raw: Option<String> =
        sqlx::query_scalar("SELECT i.cost::text FROM inventory.items i WHERE i.id = $1")
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(DbError::Query)?;

    raw.map(|raw| read_money(&raw, currency, "cost"))
        .transpose()
        .map_err(DbError::Query)
}

/// Write a new standing cost, without touching anything else.
///
/// What a receipt does under average costing, in the same transaction as the
/// movement that caused it. Not [`update`]: that takes a whole `Checked` item
/// and would need a form's worth of fields to change one number the machine
/// worked out, and it would stamp `updated_by` with a person who typed nothing.
pub async fn set_cost(conn: &mut PgConnection, id: Uuid, cost: Money) -> Result<(), DbError> {
    sqlx::query("UPDATE inventory.items SET cost = $2::numeric WHERE id = $1")
        .bind(id)
        .bind(cost.to_storage_string())
        .execute(conn)
        .await
        .map_err(DbError::Query)?;

    Ok(())
}

/// Remove one. Its variants, images and account mappings go with it -
/// `ON DELETE CASCADE` for the first two, and the third explicitly, because
/// `account_mappings` carries a discriminator rather than a foreign key.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, DbError> {
    sqlx::query("DELETE FROM inventory.account_mappings WHERE owner_kind = 'item' AND owner_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    let result = sqlx::query("DELETE FROM inventory.items WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// How many items there are, and how many of them are counted.
///
/// Two numbers for the home page. The gap between them is the interesting one:
/// a workspace where every item is tracked is counting its stationery.
pub async fn counts<'e, E>(executor: E) -> Result<(i64, i64), DbError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE is_tracked) AS tracked
           FROM inventory.items
          WHERE is_active",
    )
    .fetch_one(executor)
    .await
    .map_err(DbError::Query)?;

    Ok((
        row.try_get("total").map_err(DbError::Query)?,
        row.try_get("tracked").map_err(DbError::Query)?,
    ))
}

/// An empty cost field means nothing was typed, which is zero rather than a
/// refusal: an item whose cost is not known yet is ordinary.
fn cost_or_zero(raw: &str) -> String {
    if raw.trim().is_empty() {
        "0".to_owned()
    } else {
        raw.trim().to_owned()
    }
}

/// An empty sale price is `NULL` rather than zero. Free and unpriced are
/// different, and a grid showing 0.00 for every unpriced item is a grid nobody
/// can scan for the ones that need pricing.
fn optional_amount(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
