//! `inventory.warehouses`: buildings, and the locations they are made of.
//!
//! # Creating one creates a tree
//!
//! [`insert`] takes a transaction rather than an executor, because a warehouse
//! is a view location, a stock location and the row that points at both. A
//! building whose stock location was created and whose own row was not is worse
//! than neither: the next attempt would find the location, refuse the path as
//! taken, and still leave no warehouse.

use app_inventory::warehouse::{
    DeliverySteps, ReceiptSteps, Warehouse, WarehouseInput, WarehouseSummary,
    required_sublocations,
};
use phonix_core::identity::UserId;
use sqlx::{FromRow, PgConnection, PgExecutor, Row};
use uuid::Uuid;

use crate::error::DbError;

const CODE_INDEX: &str = "warehouses_code";

fn as_conflict(err: sqlx::Error, code: &str) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some(CODE_INDEX) => DbError::CodeExists {
            entity: "warehouse",
            code: code.to_owned(),
        },
        _ => DbError::Query(err),
    }
}

struct RowOf<T>(T);

fn unknown(column: &str, raw: &str) -> sqlx::Error {
    sqlx::Error::Decode(
        format!("warehouses.{column} holds '{raw}', which this build does not know").into(),
    )
}

fn read_steps(row: &sqlx::postgres::PgRow) -> Result<(ReceiptSteps, DeliverySteps), sqlx::Error> {
    let receipt: String = row.try_get("receipt_steps")?;
    let delivery: String = row.try_get("delivery_steps")?;

    Ok((
        ReceiptSteps::parse(&receipt).ok_or_else(|| unknown("receipt_steps", &receipt))?,
        DeliverySteps::parse(&delivery).ok_or_else(|| unknown("delivery_steps", &delivery))?,
    ))
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<Warehouse> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let (receipt_steps, delivery_steps) = read_steps(row)?;

        Ok(Self(Warehouse {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            view_location_id: row.try_get("view_location_id")?,
            stock_location_id: row.try_get("stock_location_id")?,
            receipt_steps,
            delivery_steps,
            is_active: row.try_get("is_active")?,
        }))
    }
}

impl<'r> FromRow<'r, sqlx::postgres::PgRow> for RowOf<WarehouseSummary> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let (receipt_steps, delivery_steps) = read_steps(row)?;

        Ok(Self(WarehouseSummary {
            id: row.try_get("id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            receipt_steps,
            delivery_steps,
            is_active: row.try_get("is_active")?,
            location_count: row.try_get("location_count")?,
        }))
    }
}

/// Every warehouse, with how divided up each one is.
pub async fn list<'e, E>(executor: E) -> Result<Vec<WarehouseSummary>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<WarehouseSummary>>(
        "SELECT w.id, w.code, w.name, w.receipt_steps, w.delivery_steps, w.is_active,
                (SELECT count(*) FROM inventory.locations l
                  WHERE l.warehouse_id = w.id AND l.kind = 'internal')
                    AS location_count
           FROM inventory.warehouses w
          ORDER BY w.code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

/// The warehouses a document may name.
pub async fn selectable<'e, E>(executor: E) -> Result<Vec<Warehouse>, DbError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, RowOf<Warehouse>>(
        "SELECT id, code, name, view_location_id, stock_location_id,
                receipt_steps, delivery_steps, is_active
           FROM inventory.warehouses
          WHERE is_active
          ORDER BY code",
    )
    .fetch_all(executor)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(|row| row.0).collect())
}

pub async fn find<'e, E>(executor: E, id: Uuid) -> Result<Option<Warehouse>, DbError>
where
    E: PgExecutor<'e>,
{
    Ok(sqlx::query_as::<_, RowOf<Warehouse>>(
        "SELECT id, code, name, view_location_id, stock_location_id,
                receipt_steps, delivery_steps, is_active
           FROM inventory.warehouses
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
    .map_err(DbError::Query)?
    .map(|row| row.0))
}

/// Create a warehouse and the locations it is made of.
///
/// Takes a `&mut PgConnection` so the caller holds the transaction: the view
/// node, the sublocations and the warehouse row are one act or none of them.
pub async fn insert(
    conn: &mut PgConnection,
    draft: &WarehouseInput,
    actor: Option<UserId>,
) -> Result<Uuid, DbError> {
    let view_id: Uuid = sqlx::query_scalar(
        "INSERT INTO inventory.locations (path, name, kind, created_by, updated_by)
         VALUES ($1, $1, 'view', $2, $2)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(actor)
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| as_conflict(err, &draft.code))?;

    // The single place that decides which locations a step count needs, so the
    // seed, this and the warehouse screen cannot disagree.
    let mut stock_id = None;
    for segment in required_sublocations(draft.receipt_steps, draft.delivery_steps) {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO inventory.locations
                 (path, name, parent_id, kind, created_by, updated_by)
             VALUES ($1, $2, $3, 'internal', $4, $4)
             RETURNING id",
        )
        .bind(format!("{}/{segment}", draft.code))
        .bind(segment)
        .bind(view_id)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await
        .map_err(DbError::Query)?;

        if segment == "Stock" {
            stock_id = Some(id);
        }
    }

    let Some(stock_id) = stock_id else {
        // `required_sublocations` always yields "Stock". If it ever stops, this
        // is where that is found out rather than three screens later.
        return Err(DbError::CorruptCatalogRow {
            slug: draft.code.clone(),
            reason: "a warehouse was built with no stock location".to_owned(),
        });
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO inventory.warehouses
             (code, name, view_location_id, stock_location_id,
              receipt_steps, delivery_steps, is_active, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
         RETURNING id",
    )
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(view_id)
    .bind(stock_id)
    .bind(draft.receipt_steps.as_str())
    .bind(draft.delivery_steps.as_str())
    .bind(draft.is_active)
    .bind(actor)
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| as_conflict(err, &draft.code))?;

    // Adopted now that the building exists; the locations were written first
    // because the warehouse has to point at them.
    sqlx::query(
        "UPDATE inventory.locations
            SET warehouse_id = $1
          WHERE id = $2 OR parent_id = $2",
    )
    .bind(id)
    .bind(view_id)
    .execute(&mut *conn)
    .await
    .map_err(DbError::Query)?;

    Ok(id)
}

/// Change a warehouse's own row.
///
/// Renaming the code renames every location beneath it, which the service does
/// through [`super::location::rename_subtree`] - not here, because a rename is
/// two tables and this one owns only one of them.
pub async fn update<'e, E>(
    executor: E,
    id: Uuid,
    draft: &WarehouseInput,
    actor: Option<UserId>,
) -> Result<bool, DbError>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        "UPDATE inventory.warehouses
            SET code           = $2,
                name           = $3,
                receipt_steps  = $4,
                delivery_steps = $5,
                is_active      = $6,
                updated_at     = now(),
                updated_by     = $7
          WHERE id = $1",
    )
    .bind(id)
    .bind(&draft.code)
    .bind(&draft.name)
    .bind(draft.receipt_steps.as_str())
    .bind(draft.delivery_steps.as_str())
    .bind(draft.is_active)
    .bind(actor)
    .execute(executor)
    .await
    .map_err(|err| as_conflict(err, &draft.code))?;

    Ok(result.rows_affected() > 0)
}

/// Add whatever locations a changed step count now needs.
///
/// Only adds. A warehouse switched from three steps to one keeps its unused
/// `Input`, because stock may still be sitting in it and a location that
/// disappears takes its history with it.
pub async fn ensure_sublocations(
    conn: &mut PgConnection,
    warehouse: &Warehouse,
    actor: Option<UserId>,
) -> Result<u64, DbError> {
    let mut created = 0;

    for segment in required_sublocations(warehouse.receipt_steps, warehouse.delivery_steps) {
        let inserted = sqlx::query(
            "INSERT INTO inventory.locations
                 (path, name, parent_id, kind, warehouse_id, created_by, updated_by)
             VALUES ($1, $2, $3, 'internal', $4, $5, $5)
             ON CONFLICT DO NOTHING",
        )
        .bind(format!("{}/{segment}", warehouse.code))
        .bind(segment)
        .bind(warehouse.view_location_id)
        .bind(warehouse.id)
        .bind(actor)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?
        .rows_affected();

        created += inserted;
    }

    Ok(created)
}
