//! Seeding an inventory: what is there on the first morning.
//!
//! Runs on every migration pass, like the number sequences and for the same
//! reason: an upgrade that adds a unit of measure has to reach the workspaces
//! that already have the app. Every insert is `ON CONFLICT DO NOTHING`, so a
//! re-run can neither put back a row somebody deleted nor overwrite one they
//! edited.
//!
//! # The order matters, and it is the order of this file
//!
//! Units and the counterpart locations stand alone. A warehouse needs its view
//! node and its stock location to exist before it can point at them, and those
//! locations need the warehouse's id before they can say which building they
//! are in - so the two are created in one transaction with the location rows
//! written first and adopted afterwards.
//!
//! Categories are last and are inserted parent-first, which is why
//! `Defaults::check` insists a parent is declared before it is used.

use app_inventory::defaults::Defaults;
use app_inventory::unit;
use app_inventory::warehouse::{DeliverySteps, ReceiptSteps, required_sublocations};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::DbError;

/// What one pass created, for the log line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Seeded {
    pub units: u64,
    pub locations: u64,
    pub warehouses: u64,
    pub categories: u64,
}

/// Install everything the file declares that is not already there.
pub async fn install(pool: &PgPool, defaults: &Defaults) -> Result<Seeded, DbError> {
    let mut seeded = Seeded {
        units: install_units(pool, defaults).await?,
        locations: install_counterpart_locations(pool, defaults).await?,
        ..Seeded::default()
    };

    seeded.warehouses = install_warehouses(pool, defaults).await?;
    seeded.categories = install_categories(pool, defaults).await?;

    Ok(seeded)
}

async fn install_units(pool: &PgPool, defaults: &Defaults) -> Result<u64, DbError> {
    if defaults.unit.is_empty() {
        return Ok(0);
    }

    let mut codes = Vec::with_capacity(defaults.unit.len());
    let mut names = Vec::with_capacity(defaults.unit.len());
    let mut classes = Vec::with_capacity(defaults.unit.len());
    let mut factors = Vec::with_capacity(defaults.unit.len());

    for entry in &defaults.unit {
        // Parsed here rather than bound as text so a factor the file spelled
        // wrongly is a refusal naming the unit, not a NUMERIC cast error.
        let factor = unit::parse_factor(&entry.factor).map_err(|err| DbError::CorruptCatalogRow {
            slug: entry.code.clone(),
            reason: format!("unit factor is unusable: {err}"),
        })?;

        codes.push(entry.code.trim().to_uppercase());
        names.push(entry.name.trim().to_owned());
        classes.push(entry.class.as_str());
        factors.push(unit::factor_to_string(factor));
    }

    let inserted = sqlx::query(
        "INSERT INTO inventory.units (code, name, class, factor, is_base)
              SELECT code, name, class, factor::numeric, factor::numeric = 1
                FROM unnest($1::text[], $2::text[], $3::text[], $4::text[])
                  AS t(code, name, class, factor)
         ON CONFLICT DO NOTHING",
    )
    .bind(&codes)
    .bind(&names)
    .bind(&classes)
    .bind(&factors)
    .execute(pool)
    .await
    .map_err(DbError::Query)?
    .rows_affected();

    Ok(inserted)
}

/// The vendor, customer, inventory-loss, production and transit locations.
///
/// The half of the file a workspace could not know it needed. Without these
/// there is no such thing as a receipt, because a receipt is a move *from*
/// somewhere.
async fn install_counterpart_locations(pool: &PgPool, defaults: &Defaults) -> Result<u64, DbError> {
    if defaults.location.is_empty() {
        return Ok(0);
    }

    let names: Vec<String> = defaults
        .location
        .iter()
        .map(|entry| entry.name.trim().to_owned())
        .collect();
    let kinds: Vec<&str> = defaults
        .location
        .iter()
        .map(|entry| entry.kind.as_str())
        .collect();

    // These have no parent, so the path is the name.
    let inserted = sqlx::query(
        "INSERT INTO inventory.locations (path, name, kind)
              SELECT name, name, kind
                FROM unnest($1::text[], $2::text[]) AS t(name, kind)
         ON CONFLICT DO NOTHING",
    )
    .bind(&names)
    .bind(&kinds)
    .execute(pool)
    .await
    .map_err(DbError::Query)?
    .rows_affected();

    Ok(inserted)
}

/// Each declared warehouse, with the small tree of locations it is made of.
///
/// One transaction per warehouse: a building whose stock location was created
/// and whose own row was not is worse than neither, because the next pass would
/// see the location, skip it, and still have no warehouse.
async fn install_warehouses(pool: &PgPool, defaults: &Defaults) -> Result<u64, DbError> {
    let mut created = 0;

    for entry in &defaults.warehouse {
        let code = entry.code.trim().to_uppercase();

        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM inventory.warehouses WHERE code = $1")
                .bind(&code)
                .fetch_optional(pool)
                .await
                .map_err(DbError::Query)?;

        if exists.is_some() {
            continue;
        }

        let mut tx = pool.begin().await.map_err(DbError::Query)?;

        // The view node the whole building hangs under. It holds nothing
        // itself; its total is the sum of what is beneath it.
        let view_id: Uuid = sqlx::query_scalar(
            "INSERT INTO inventory.locations (path, name, kind)
             VALUES ($1, $1, 'view')
             RETURNING id",
        )
        .bind(&code)
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        // One step in and one step out, which is what almost every workspace
        // wants and all of them can grow out of. `required_sublocations` is the
        // single place that decides which locations a step count needs, so the
        // seed and the warehouse screen cannot disagree.
        let wanted = required_sublocations(ReceiptSteps::One, DeliverySteps::One);
        let mut stock_id = None;

        for segment in wanted {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO inventory.locations (path, name, parent_id, kind)
                 VALUES ($1, $2, $3, 'internal')
                 RETURNING id",
            )
            .bind(format!("{code}/{segment}"))
            .bind(segment)
            .bind(view_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            if segment == "Stock" {
                stock_id = Some(id);
            }
        }

        let Some(stock_id) = stock_id else {
            // `required_sublocations` always yields "Stock". If it ever stops,
            // this is where that is found out rather than three screens later.
            return Err(DbError::CorruptCatalogRow {
                slug: code,
                reason: "a warehouse was built with no stock location".to_owned(),
            });
        };

        sqlx::query(
            "INSERT INTO inventory.warehouses
                    (code, name, view_location_id, stock_location_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&code)
        .bind(entry.name.trim())
        .bind(view_id)
        .bind(stock_id)
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        // Adopted now that the building exists. The locations were written
        // first because the warehouse has to point at them.
        sqlx::query(
            "UPDATE inventory.locations
                SET warehouse_id = (SELECT id FROM inventory.warehouses WHERE code = $1)
              WHERE id = $2 OR parent_id = $2",
        )
        .bind(&code)
        .bind(view_id)
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        tx.commit().await.map_err(DbError::Query)?;
        created += 1;
    }

    Ok(created)
}

/// The category tree, parent before child.
///
/// One statement per row rather than an `unnest`, because each row's path needs
/// the parent's - which the previous statement is what wrote.
async fn install_categories(pool: &PgPool, defaults: &Defaults) -> Result<u64, DbError> {
    let mut created = 0;

    for entry in &defaults.category {
        let name = entry.name.trim();

        let parent: Option<(Uuid, String)> = match entry.parent.as_deref() {
            None => None,
            Some(parent) => {
                let row = sqlx::query(
                    "SELECT id, path FROM inventory.categories WHERE name = $1 ORDER BY path LIMIT 1",
                )
                .bind(parent.trim())
                .fetch_optional(pool)
                .await
                .map_err(DbError::Query)?;

                match row {
                    // The parent was deleted by the workspace. Its children are
                    // then not seeded either, rather than reappearing at the
                    // root: a redeploy may not put back a tree somebody pruned.
                    None => continue,
                    Some(row) => Some((
                        row.try_get("id").map_err(DbError::Query)?,
                        row.try_get("path").map_err(DbError::Query)?,
                    )),
                }
            }
        };

        let path = match parent.as_ref() {
            None => name.to_owned(),
            Some((_, parent_path)) => format!("{parent_path}/{name}"),
        };

        let inserted = sqlx::query(
            "INSERT INTO inventory.categories
                    (path, name, parent_id, costing_method, valuation, removal_strategy)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT DO NOTHING",
        )
        .bind(&path)
        .bind(name)
        .bind(parent.as_ref().map(|(id, _)| *id))
        .bind(entry.costing_method.as_str())
        .bind(entry.valuation.as_str())
        .bind(entry.removal_strategy.as_str())
        .execute(pool)
        .await
        .map_err(DbError::Query)?
        .rows_affected();

        created += inserted;
    }

    Ok(created)
}
