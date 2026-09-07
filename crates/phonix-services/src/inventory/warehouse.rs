//! Warehouses: creating one, and what that creates.
//!
//! # Creating a warehouse creates a tree, and so does changing its steps
//!
//! A warehouse is a view node, a `Stock` location and the row that points at
//! both. Switching a workspace from one-step receiving to two adds an `Input`
//! location; switching back does **not** remove it, because stock may be
//! sitting in it and a location that disappears takes its history with it.
//!
//! Renaming the code renames every location beneath it, because the code is the
//! first segment of every path in the building.

use app_inventory::warehouse::{Warehouse, WarehouseInput, WarehouseSummary};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::{location as locations, warehouse as store};
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<WarehouseSummary>> {
    caller.require(permissions::WAREHOUSES)?;
    Ok(store::list(pool).await?)
}

/// The warehouses a document may name. Gated on `ITEMS`, like the location and
/// unit pickers and for the same reason.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Warehouse>> {
    caller.require(permissions::ITEMS)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Warehouse> {
    caller.require(permissions::WAREHOUSES)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("warehouse", msg!("warehouses.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<WarehouseInput> {
    Ok(WarehouseInput::from_warehouse(
        &detail(pool, caller, id).await?,
    ))
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: WarehouseInput,
) -> ServiceResult<Submission<WarehouseInput>> {
    caller.require(permissions::WAREHOUSES_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    match checked.id {
        None => {
            // One transaction: the view node, the sublocations and the
            // warehouse row are one act or none of them.
            let mut tx = pool.begin().await.map_err(DbError::Query)?;

            let id = match store::insert(&mut tx, &checked, caller.user_id()).await {
                Ok(id) => id,
                Err(DbError::CodeExists { code, .. }) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(code_taken(&code));
                }
                Err(err) => return Err(err.into()),
            };

            tx.commit().await.map_err(DbError::Query)?;

            let stored = WarehouseInput {
                id: Some(id),
                is_default: false,
                ..checked
            };

            audit::created(
                pool,
                caller,
                Target::new(kinds::WAREHOUSE, id)
                    .named(&stored.name)
                    .fact("code", &stored.code),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            // The default warehouse keeps its code, its step counts and its
            // active flag. All three are load-bearing for rows the workspace
            // did not create - the code is the first segment of every location
            // path in the building, the steps decide which of those locations
            // exist, and a workspace with its only warehouse retired has
            // nowhere for stock to be. The name is its own and changes freely.
            //
            // Refused rather than silently ignored: a save that appears to work
            // and does not is worse than one that says why.
            if before.is_default
                && (before.code != checked.code
                    || before.receipt_steps != checked.receipt_steps
                    || before.delivery_steps != checked.delivery_steps
                    || !checked.is_active)
            {
                let refusal = app_inventory::warehouse::WarehouseError::DefaultIsFixed;
                return Ok(Submission::rejected(refusal.field(), refusal.message()));
            }

            let mut tx = pool.begin().await.map_err(DbError::Query)?;

            match store::update(&mut *tx, id, &checked, caller.user_id()).await {
                Ok(true) => {}
                Ok(false) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected("code", msg!("warehouses.gone")));
                }
                Err(DbError::CodeExists { code, .. }) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(code_taken(&code));
                }
                Err(err) => return Err(err.into()),
            }

            // The code is the first segment of every path in the building, so
            // renaming it renames the whole tree.
            if before.code != checked.code
                && let Err(err) = locations::rename_subtree(
                    &mut *tx,
                    before.view_location_id,
                    &before.code,
                    &checked.code,
                )
                .await
            {
                tx.rollback().await.map_err(DbError::Query)?;
                return match err {
                    DbError::CodeExists { code, .. } => Ok(code_taken(&code)),
                    err => Err(err.into()),
                };
            }

            // The view node's own path, which `rename_subtree` deliberately
            // leaves alone: it renames what is *beneath* a node.
            if before.code != checked.code {
                let view = LocationRename {
                    id: before.view_location_id,
                    name: checked.code.clone(),
                };
                if let Err(err) = view.apply(&mut tx).await {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Err(err);
                }
            }

            // Adds whatever a changed step count now needs. Never removes:
            // stock may be sitting in the location that is no longer on the
            // route, and a location that disappears takes its history with it.
            let after = Warehouse {
                code: checked.code.clone(),
                name: checked.name.clone(),
                receipt_steps: checked.receipt_steps,
                delivery_steps: checked.delivery_steps,
                is_active: checked.is_active,
                ..before.clone()
            };
            store::ensure_sublocations(&mut tx, &after, caller.user_id()).await?;

            tx.commit().await.map_err(DbError::Query)?;

            let stored = WarehouseInput {
                id: Some(id),
                // Read-only, and answered from the row rather than from the
                // draft: nothing makes a warehouse the default by sending this.
                is_default: before.is_default,
                ..checked
            };

            audit::updated(
                pool,
                caller,
                Target::new(kinds::WAREHOUSE, id).named(&stored.name),
                &WarehouseInput::from_warehouse(&before),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
    }
}

/// Switch a warehouse off.
///
/// There is no delete. A warehouse owns locations, the locations carry every
/// movement that ever crossed them, and `ON DELETE RESTRICT` would refuse it
/// anyway - so the honest operation is the one offered.
pub async fn set_active(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    active: bool,
) -> ServiceResult<Submission<WarehouseInput>> {
    let current = detail(pool, caller, id).await?;

    save(
        pool,
        caller,
        WarehouseInput {
            is_active: active,
            ..WarehouseInput::from_warehouse(&current)
        },
    )
    .await
}

/// Rename one location's own path segment.
///
/// A small struct rather than a store function, because it is the one write
/// that belongs to a warehouse rename and to nothing else.
struct LocationRename {
    id: Uuid,
    name: String,
}

impl LocationRename {
    async fn apply(&self, tx: &mut phonix_db::sqlx::Transaction<'_, phonix_db::sqlx::Postgres>) -> ServiceResult<()> {
        let Some(location) = locations::find(&mut **tx, self.id).await? else {
            return Ok(());
        };

        let draft = app_inventory::location::LocationInput {
            name: self.name.clone(),
            ..app_inventory::location::LocationInput::from_location(&location)
        };

        // The view node has no parent, so its path is its name.
        locations::update(&mut **tx, self.id, &self.name, &draft, None).await?;
        Ok(())
    }
}

fn code_taken(code: &str) -> Submission<WarehouseInput> {
    Submission::rejected("code", msg!("warehouses.error.code_taken", code = code))
}
