//! Locations: adding one, moving one, retiring one.
//!
//! # The path is derived, so a rename is two writes
//!
//! `WH/Stock/Zone A` is built from the tree. Renaming `Zone A` or moving it
//! rewrites its own row and every row beneath it - [`save`] does both in one
//! transaction, because a subtree half-renamed is a set of paths that point at
//! a node that no longer has that name.

use app_inventory::location::{
    DeleteOutcome, Location, LocationError, LocationInput, LocationKind, LocationSummary,
    MAX_LOCATION_DEPTH, path_under,
};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::location as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// Every location, arranged into the tree.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<LocationSummary>> {
    caller.require(permissions::STOCK_LOCATIONS)?;
    Ok(store::list(pool).await?)
}

/// The locations a movement may name.
///
/// Gated on `ITEMS` rather than `STOCK_LOCATIONS`: this is what a document form
/// needs, and somebody receiving goods must be able to say where they went
/// without being allowed to redraw the warehouse.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Location>> {
    caller.require(permissions::ITEMS)?;
    Ok(store::selectable(pool).await?)
}

/// The one location of a counterpart kind - the vendor side of a receipt, the
/// inventory-loss side of a count difference.
///
/// Ungated: it is not a screen, it is what a posting needs, and the caller has
/// already checked whoever is posting.
pub async fn counterpart(pool: &PgPool, kind: LocationKind) -> ServiceResult<Location> {
    store::counterpart(pool, kind).await?.ok_or_else(|| {
        // The seed creates these, so reaching here means somebody deleted one.
        // Named rather than generic: "there is no vendor location" is a thing
        // an administrator can act on.
        ServiceError::rejected(
            "location",
            msg!("locations.error.no_counterpart", kind = kind.as_str()),
        )
    })
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Location> {
    caller.require(permissions::STOCK_LOCATIONS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("location", msg!("locations.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<LocationInput> {
    Ok(LocationInput::from_location(
        &detail(pool, caller, id).await?,
    ))
}

/// Add a location, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: LocationInput,
) -> ServiceResult<Submission<LocationInput>> {
    caller.require(permissions::STOCK_LOCATIONS_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    // Needs the rest of the tree, which is why it is not in `check`.
    if let Some(parent_id) = checked.parent_id
        && let Err(err) = check_placement(pool, checked.id, parent_id).await?
    {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    let parent_path = match checked.parent_id {
        None => None,
        Some(parent_id) => store::find(pool, parent_id).await?.map(|parent| parent.code),
    };
    let path = path_under(parent_path.as_deref(), &checked.name);

    match checked.id {
        None => {
            let id = match store::insert(pool, &path, &checked, caller.user_id()).await {
                Ok(id) => id,
                Err(DbError::CodeExists { code, .. }) => return Ok(path_taken(&code)),
                Err(err) => return Err(err.into()),
            };

            let stored = LocationInput {
                id: Some(id),
                ..checked
            };

            audit::created(
                pool,
                caller,
                Target::new(kinds::STOCK_LOCATION, id)
                    .named(&path)
                    .fact("kind", stored.kind.as_str()),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            let mut tx = pool.begin().await.map_err(DbError::Query)?;

            match store::update(&mut *tx, id, &path, &checked, caller.user_id()).await {
                Ok(true) => {}
                Ok(false) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected("name", msg!("locations.gone")));
                }
                Err(DbError::CodeExists { code, .. }) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(path_taken(&code));
                }
                Err(err) => return Err(err.into()),
            }

            // Its own row and every row beneath it, in one transaction. A
            // subtree half-renamed is a set of paths pointing at a name that no
            // longer exists.
            if let Err(err) = store::rename_subtree(&mut *tx, id, &before.code, &path).await {
                tx.rollback().await.map_err(DbError::Query)?;
                return match err {
                    DbError::CodeExists { code, .. } => Ok(path_taken(&code)),
                    err => Err(err.into()),
                };
            }

            tx.commit().await.map_err(DbError::Query)?;

            let stored = LocationInput {
                id: Some(id),
                ..checked
            };

            audit::updated(
                pool,
                caller,
                Target::new(kinds::STOCK_LOCATION, id).named(&path),
                &LocationInput::from_location(&before),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
    }
}

/// Remove a location nothing has been at.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DeleteOutcome> {
    caller.require(permissions::STOCK_LOCATIONS_MANAGE)?;
    acting_user(caller)?;

    let location = detail(pool, caller, id).await?;

    // Postgres would refuse this itself; asking first lets the screen say how
    // many are in the way.
    let children = store::descendants(pool, id).await?;
    if !children.is_empty() {
        return Ok(DeleteOutcome::HasChildren {
            count: children.len() as i64,
        });
    }

    // Once the stock tables exist this asks them. Until then there are no
    // movements to find, and the answer is the same either way.

    if !store::delete(pool, id).await? {
        return Ok(DeleteOutcome::Deleted);
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::STOCK_LOCATION, id).named(&location.code),
        &LocationInput::from_location(&location),
    )
    .await;

    Ok(DeleteOutcome::Deleted)
}

/// Whether `parent_id` is somewhere this may go.
///
/// Both refusals - a cycle, and exceeding the depth limit - need the rest of
/// the tree, which is why they are not in `LocationInput::check`.
async fn check_placement(
    pool: &PgPool,
    moving: Option<Uuid>,
    parent_id: Uuid,
) -> ServiceResult<Result<(), LocationError>> {
    let Some(parent) = store::find(pool, parent_id).await? else {
        // The parent is gone. Both this and a cycle mean "not somewhere this
        // can go", and the picker only offers it by being stale.
        return Ok(Err(LocationError::Cycle));
    };

    // A grouping is what a location tree hangs under. Anything else has no
    // business having children, because stock in a place *and* in the place
    // inside it is stock counted twice.
    if !matches!(parent.kind, LocationKind::View | LocationKind::Internal) {
        return Ok(Err(LocationError::NotStockable));
    }

    if let Some(moving) = moving {
        if moving == parent_id {
            return Ok(Err(LocationError::OwnParent));
        }
        if store::descendants(pool, moving).await?.contains(&parent_id) {
            return Ok(Err(LocationError::Cycle));
        }
    }

    // The path is the ancestry, so counting separators is counting levels -
    // no query per ancestor.
    if parent.code.matches('/').count() + 2 > MAX_LOCATION_DEPTH {
        return Ok(Err(LocationError::TooDeep));
    }

    Ok(Ok(()))
}

fn path_taken(path: &str) -> Submission<LocationInput> {
    Submission::rejected("name", msg!("locations.error.path_taken", path = path))
}
