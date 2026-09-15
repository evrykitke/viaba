//! `inventory.item_variants`, `attributes`, and the tables between them.
//!
//! # Retire, do not delete
//!
//! [`retire`] rather than a delete. Stock has moved against a variant and the
//! moves are the audit trail; a combination nobody sells any more is one that
//! is switched off, and its history stays legible.

use app_inventory::item::{ItemKind, Tracking};
use app_inventory::lot::LotRules;
use app_inventory::variant::{
    Attribute, AttributeValue, Display, Selection, SelectionLine, Variant, VariantChoice,
    VariantSummary, VariantValue,
};
use phonix_core::identity::UserId;
use sqlx::{PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(format!("{column} holds '{raw}', which this build does not know").into())
}

/// One picker row, from either of the two queries that produce them.
fn choice_from(row: &sqlx::postgres::PgRow) -> Result<VariantChoice, DbError> {
    Ok(VariantChoice {
        id: row.try_get("id").map_err(DbError::Query)?,
        code: row.try_get("code").map_err(DbError::Query)?,
        item_name: row.try_get("item_name").map_err(DbError::Query)?,
        combination: row.try_get("combination").map_err(DbError::Query)?,
        unit_id: row.try_get("unit_id").map_err(DbError::Query)?,
        unit_code: row.try_get("unit_code").map_err(DbError::Query)?,
        rules: rules_from(row).map_err(DbError::Query)?,
    })
}

/// The item's lot rules, off a row that selected the four `items` columns
/// behind them: `tracking`, `uses_expiry`, `is_tracked` and `kind`.
fn rules_from(row: &sqlx::postgres::PgRow) -> Result<LotRules, sqlx::Error> {
    let tracking: String = row.try_get("tracking")?;
    let kind: String = row.try_get("kind")?;
    let is_tracked: bool = row.try_get("is_tracked")?;

    let kind = ItemKind::parse(&kind).ok_or_else(|| unknown("items.kind", &kind))?;

    Ok(LotRules {
        tracking: Tracking::parse(&tracking).ok_or_else(|| unknown("items.tracking", &tracking))?,
        uses_expiry: row.try_get("uses_expiry")?,
        holds_stock: is_tracked && kind.can_be_stocked(),
    })
}

/// Every attribute, each with its values, in display order.
///
/// One query per table rather than one joined query: the values are grouped
/// afterwards, and a join would send each attribute's name down once per value.
pub async fn attributes<'e, E>(executor: E) -> Result<Vec<Attribute>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let attribute_rows = sqlx::query(
        "SELECT id, name, display, position, is_active
           FROM inventory.attributes
          ORDER BY position, lower(name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let value_rows = sqlx::query(
        "SELECT id, attribute_id, name, swatch, position, is_active
           FROM inventory.attribute_values
          ORDER BY position, lower(name)",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let mut values: Vec<AttributeValue> = Vec::with_capacity(value_rows.len());
    for row in &value_rows {
        values.push(AttributeValue {
            id: row.try_get("id").map_err(DbError::Query)?,
            attribute_id: row.try_get("attribute_id").map_err(DbError::Query)?,
            name: row.try_get("name").map_err(DbError::Query)?,
            swatch: row.try_get("swatch").map_err(DbError::Query)?,
            position: row.try_get("position").map_err(DbError::Query)?,
            is_active: row.try_get("is_active").map_err(DbError::Query)?,
        });
    }

    let mut attributes = Vec::with_capacity(attribute_rows.len());
    for row in &attribute_rows {
        let id: Uuid = row.try_get("id").map_err(DbError::Query)?;
        let display: String = row.try_get("display").map_err(DbError::Query)?;

        attributes.push(Attribute {
            id,
            name: row.try_get("name").map_err(DbError::Query)?,
            display: Display::parse(&display)
                .ok_or_else(|| DbError::Query(unknown("attributes.display", &display)))?,
            position: row.try_get("position").map_err(DbError::Query)?,
            is_active: row.try_get("is_active").map_err(DbError::Query)?,
            values: values
                .iter()
                .filter(|value| value.attribute_id == id)
                .cloned()
                .collect(),
        });
    }

    Ok(attributes)
}

/// Which values an item is offered in, as a form reads it.
pub async fn selection<'e, E>(executor: E, item_id: Uuid) -> Result<Selection, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT iav.attribute_id, a.name AS attribute_name, iav.value_id
           FROM inventory.item_attribute_values iav
           JOIN inventory.attributes a ON a.id = iav.attribute_id
          WHERE iav.item_id = $1
          ORDER BY a.position, a.name",
    )
    .bind(item_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let mut lines: Vec<SelectionLine> = Vec::new();
    for row in &rows {
        let attribute_id: Uuid = row.try_get("attribute_id").map_err(DbError::Query)?;
        let value_id: Uuid = row.try_get("value_id").map_err(DbError::Query)?;

        match lines
            .iter_mut()
            .find(|line| line.attribute_id == attribute_id)
        {
            Some(line) => line.value_ids.push(value_id),
            None => lines.push(SelectionLine {
                attribute_id,
                attribute_name: row.try_get("attribute_name").map_err(DbError::Query)?,
                value_ids: vec![value_id],
            }),
        }
    }

    Ok(Selection { lines })
}

/// The columns behind every picker query, whichever side it is on.
///
/// A constant rather than inline, so the day a column is added to a picker row
/// there is one query to change. `{unit}` is the only difference between the
/// two sides: a buying line opens on the unit the item is bought in and a
/// selling line on the unit it is held in, and neither side wants the other's.
const PICKER_ROW: &str = "SELECT v.id, v.code, i.name AS item_name, i.{unit} AS unit_id,
                u.code AS unit_code,
                i.tracking, i.uses_expiry, i.is_tracked, i.kind,
                (SELECT string_agg(av.name, ' / ' ORDER BY a.position, a.name)
                   FROM inventory.variant_values vv
                   JOIN inventory.attributes a ON a.id = vv.attribute_id
                   JOIN inventory.attribute_values av ON av.id = vv.value_id
                  WHERE vv.variant_id = v.id) AS combination
           FROM inventory.item_variants v
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.{unit}
          WHERE i.{flag}
            AND i.is_active
            AND v.is_active";

/// The purchasable half, with the purchase unit on the row.
fn purchasable() -> String {
    PICKER_ROW
        .replace("{unit}", "purchase_unit_id")
        .replace("{flag}", "can_be_purchased")
}

/// The sellable half, with the stock unit on the row.
///
/// The stock unit rather than a sales unit, because an item has no sales unit:
/// this schema has two, what it is held in and what it is bought in, and the
/// second is the supplier's word. A line may still be quoted in any unit that
/// measures the same thing - the picker chooses where it opens, not what is
/// allowed.
fn sellable() -> String {
    PICKER_ROW
        .replace("{unit}", "stock_unit_id")
        .replace("{flag}", "can_be_sold")
}

/// Variants matching what somebody has typed, capped.
///
/// This is what a picker on a line grid reads. It replaced a query that sent
/// every variant in the workspace to the browser, which is fine for a
/// catalogue of forty and is a page nobody can load for one of forty
/// thousand.
///
/// An empty needle is not an error: it answers with the first `limit` by name,
/// which is what a picker shows before anything has been typed.
///
/// The code is matched with a prefix and the name anywhere. A code is scanned
/// or typed from the left and a leading wildcard on it would forbid the index;
/// a name is how somebody who does not know the code searches, and it has to
/// match in the middle.
pub async fn search_purchasable<'e, E>(
    executor: E,
    needle: &str,
    limit: i64,
) -> Result<Vec<VariantChoice>, DbError>
where
    E: PgExecutor<'e>,
{
    let needle = needle.trim();

    search(executor, &purchasable(), needle, limit).await
}

/// The same, over what this workspace sells.
///
/// A separate entry point rather than a flag, because the two are asked by
/// different screens for different reasons and a boolean parameter at a call
/// site reads as neither.
pub async fn search_sellable<'e, E>(
    executor: E,
    needle: &str,
    limit: i64,
) -> Result<Vec<VariantChoice>, DbError>
where
    E: PgExecutor<'e>,
{
    let needle = needle.trim();

    search(executor, &sellable(), needle, limit).await
}

async fn search<'e, E>(
    executor: E,
    columns: &str,
    needle: &str,
    limit: i64,
) -> Result<Vec<VariantChoice>, DbError>
where
    E: PgExecutor<'e>,
{
    let statement = sqlx::AssertSqlSafe(format!(
        "{columns}
            AND ($1 = ''
                 OR v.code ILIKE $2 || '%'
                 OR i.name ILIKE '%' || $2 || '%')
          ORDER BY i.name, v.code
          LIMIT $3"
    ));

    let rows = sqlx::query(statement)
        .bind(needle)
        .bind(crate::search::escaped(needle))
        .bind(limit)
        .fetch_all(executor)
        .await
        .map_err(DbError::Query)?;

    rows.iter().map(choice_from).collect()
}

/// The lot rules of the variants a document already names, in one query.
///
/// What a reopened receipt and one prefilled from an order need: their lines
/// carry variant ids that nobody picked in this browser, and the lot box on
/// each of them is drawn or not drawn from the answer.
pub async fn rules_for<'e, E>(
    executor: E,
    variant_ids: &[Uuid],
) -> Result<Vec<(Uuid, LotRules)>, DbError>
where
    E: PgExecutor<'e>,
{
    if variant_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT v.id, i.tracking, i.uses_expiry, i.is_tracked, i.kind
           FROM inventory.item_variants v
           JOIN inventory.items i ON i.id = v.item_id
          WHERE v.id = ANY($1)",
    )
    .bind(variant_ids)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| Ok((row.try_get("id")?, rules_from(row)?)))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(DbError::Query)
}

/// Every variant of an item, each with the combination it stands for.
pub async fn of_item<'e, E>(executor: E, item_id: Uuid) -> Result<Vec<Variant>, DbError>
where
    E: PgExecutor<'e> + Copy,
{
    let variant_rows = sqlx::query(
        "SELECT id, item_id, code, barcode, price_extra::text AS price_extra,
                cost_extra::text AS cost_extra, is_default, is_active
           FROM inventory.item_variants
          WHERE item_id = $1
          ORDER BY is_default DESC, code",
    )
    .bind(item_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let value_rows = sqlx::query(
        "SELECT vv.variant_id, vv.attribute_id, a.name AS attribute_name,
                vv.value_id, av.name AS value_name, av.swatch
           FROM inventory.variant_values vv
           JOIN inventory.item_variants v ON v.id = vv.variant_id
           JOIN inventory.attributes a ON a.id = vv.attribute_id
           JOIN inventory.attribute_values av ON av.id = vv.value_id
          WHERE v.item_id = $1
          ORDER BY a.position, a.name",
    )
    .bind(item_id)
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    let mut variants = Vec::with_capacity(variant_rows.len());
    for row in &variant_rows {
        let id: Uuid = row.try_get("id").map_err(DbError::Query)?;

        let mut values = Vec::new();
        for value in &value_rows {
            let variant_id: Uuid = value.try_get("variant_id").map_err(DbError::Query)?;
            if variant_id != id {
                continue;
            }
            values.push(VariantValue {
                attribute_id: value.try_get("attribute_id").map_err(DbError::Query)?,
                attribute_name: value.try_get("attribute_name").map_err(DbError::Query)?,
                value_id: value.try_get("value_id").map_err(DbError::Query)?,
                value_name: value.try_get("value_name").map_err(DbError::Query)?,
                swatch: value.try_get("swatch").map_err(DbError::Query)?,
            });
        }

        variants.push(Variant {
            id,
            item_id: row.try_get("item_id").map_err(DbError::Query)?,
            code: row.try_get("code").map_err(DbError::Query)?,
            barcode: row.try_get("barcode").map_err(DbError::Query)?,
            price_extra: row.try_get("price_extra").map_err(DbError::Query)?,
            cost_extra: row.try_get("cost_extra").map_err(DbError::Query)?,
            is_default: row.try_get("is_default").map_err(DbError::Query)?,
            is_active: row.try_get("is_active").map_err(DbError::Query)?,
            values,
        });
    }

    Ok(variants)
}

/// The rows a variants tab draws.
///
/// `tile` and `on_hand` are looked up by the caller rather than joined here,
/// because both are one query for the whole tab and a join would make them one
/// per row.
pub fn summarise(
    variants: &[Variant],
    tile: impl Fn(Uuid) -> Option<Uuid>,
    on_hand: impl Fn(Uuid) -> Option<app_inventory::quantity::Quantity>,
) -> Vec<VariantSummary> {
    variants
        .iter()
        .map(|variant| VariantSummary {
            id: variant.id,
            code: variant.code.clone(),
            barcode: variant.barcode.clone(),
            combination: variant.combination_label(),
            is_default: variant.is_default,
            is_active: variant.is_active,
            on_hand: on_hand(variant.id),
            image_file_id: tile(variant.id),
        })
        .collect()
}

/// Replace which values an item is offered in.
///
/// Deleted and rewritten rather than reconciled: the set is a handful of rows,
/// and a reconcile would be three statements to save two.
pub async fn set_selection(
    conn: &mut PgConnection,
    item_id: Uuid,
    selection: &Selection,
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM inventory.item_attribute_values WHERE item_id = $1")
        .bind(item_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;

    for line in &selection.lines {
        for value_id in &line.value_ids {
            sqlx::query(
                "INSERT INTO inventory.item_attribute_values (item_id, attribute_id, value_id)
                 VALUES ($1, $2, $3)
                 ON CONFLICT DO NOTHING",
            )
            .bind(item_id)
            .bind(line.attribute_id)
            .bind(value_id)
            .execute(&mut *conn)
            .await
            .map_err(DbError::Query)?;
        }
    }

    Ok(())
}

/// Create one variant for a combination.
pub async fn create(
    conn: &mut PgConnection,
    item_id: Uuid,
    code: &str,
    combination: &[(Uuid, Uuid)],
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO inventory.item_variants
             (item_id, code, is_default, created_by, updated_by)
         VALUES ($1, $2, FALSE, $3, $3)
         RETURNING id",
    )
    .bind(item_id)
    .bind(code)
    .bind(actor)
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db) if db.constraint() == Some("item_variants_code") => {
            DbError::CodeExists {
                entity: "item_variant",
                code: code.to_owned(),
            }
        }
        _ => DbError::Query(err),
    })?;

    for (attribute_id, value_id) in combination {
        sqlx::query(
            "INSERT INTO inventory.variant_values (variant_id, attribute_id, value_id)
             VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(attribute_id)
        .bind(value_id)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
    }

    Ok(id)
}

/// Switch a variant off, or back on.
///
/// Never a delete. Stock has moved against it and the moves are the audit
/// trail; the combination stops being offered and its history stays legible.
pub async fn retire<'e, E>(
    executor: E,
    id: Uuid,
    active: bool,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.item_variants
            SET is_active = $2, updated_at = now(), updated_by = $3
          WHERE id = $1 AND NOT is_default",
    )
    .bind(id)
    .bind(active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(result.rows_affected() > 0)
}

/// Change what one variant carries: its own barcode, and what it adds to the
/// item's price and cost.
pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    barcode: Option<&str>,
    price_extra: &str,
    cost_extra: &str,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.item_variants
            SET barcode     = $2,
                price_extra = $3::numeric,
                cost_extra  = $4::numeric,
                updated_at  = now(),
                updated_by  = $5
          WHERE id = $1",
    )
    .bind(id)
    .bind(barcode)
    .bind(price_extra)
    .bind(cost_extra)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db) if db.constraint() == Some("item_variants_barcode") => {
            DbError::CodeExists {
                entity: "item_barcode",
                code: barcode.unwrap_or_default().to_owned(),
            }
        }
        _ => DbError::Query(err),
    })?;

    Ok(result.rows_affected() > 0)
}
