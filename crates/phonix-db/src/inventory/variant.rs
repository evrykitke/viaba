//! `inventory.item_variants`, `attributes`, and the tables between them.
//!
//! # Retire, do not delete
//!
//! [`retire`] rather than a delete. Stock has moved against a variant and the
//! moves are the audit trail; a combination nobody sells any more is one that
//! is switched off, and its history stays legible.

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

        match lines.iter_mut().find(|line| line.attribute_id == attribute_id) {
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

/// Every variant a purchase order or receipt line may name.
///
/// The combination is aggregated in the query rather than fetched as rows and
/// stitched together here, because this list is read whole by a picker and
/// never row by row.
pub async fn purchasable<'e, E>(executor: E) -> Result<Vec<VariantChoice>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT v.id, v.code, i.name AS item_name, i.purchase_unit_id,
                u.code AS purchase_unit_code,
                (SELECT string_agg(av.name, ' / ' ORDER BY a.position, a.name)
                   FROM inventory.variant_values vv
                   JOIN inventory.attributes a ON a.id = vv.attribute_id
                   JOIN inventory.attribute_values av ON av.id = vv.value_id
                  WHERE vv.variant_id = v.id) AS combination
           FROM inventory.item_variants v
           JOIN inventory.items i ON i.id = v.item_id
           JOIN inventory.units u ON u.id = i.purchase_unit_id
          WHERE i.can_be_purchased
            AND i.is_active
            AND v.is_active
          ORDER BY i.name, v.code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    rows.iter()
        .map(|row| {
            Ok(VariantChoice {
                id: row.try_get("id").map_err(DbError::Query)?,
                code: row.try_get("code").map_err(DbError::Query)?,
                item_name: row.try_get("item_name").map_err(DbError::Query)?,
                combination: row.try_get("combination").map_err(DbError::Query)?,
                purchase_unit_id: row.try_get("purchase_unit_id").map_err(DbError::Query)?,
                purchase_unit_code: row.try_get("purchase_unit_code").map_err(DbError::Query)?,
            })
        })
        .collect()
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
