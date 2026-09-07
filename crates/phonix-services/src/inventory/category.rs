//! Item categories: where costing, valuation and picking policy are decided.
//!
//! # The one refusal here is an accounting refusal
//!
//! Changing a category's **costing method** restates what the workspace says
//! its stock is worth. Standard to average is not a setting change; it is a
//! revaluation, and it is one that would happen silently on a screen an
//! inventory clerk has open.
//!
//! It is refused once anything is filed here. A workspace that means to change
//! it moves the items to a category that already has the method they want,
//! which is the same act done visibly.

use app_inventory::category::{Category, CategoryError, CategoryInput, CategorySummary, DeleteOutcome, MAX_CATEGORY_DEPTH};
use app_inventory::location::path_under;
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::category as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<CategorySummary>> {
    caller.require(permissions::ITEM_CATEGORIES)?;
    Ok(store::list(pool).await?)
}

/// The categories an item form offers.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Category>> {
    caller.require(permissions::ITEMS)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Category> {
    caller.require(permissions::ITEM_CATEGORIES)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("category", msg!("categories.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<CategoryInput> {
    Ok(CategoryInput::from_category(
        &detail(pool, caller, id).await?,
    ))
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: CategoryInput,
) -> ServiceResult<Submission<CategoryInput>> {
    caller.require(permissions::ITEM_CATEGORIES_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

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

            let stored = CategoryInput {
                id: Some(id),
                ..checked
            };

            audit::created(
                pool,
                caller,
                Target::new(kinds::ITEM_CATEGORY, id)
                    .named(&path)
                    .fact("costing_method", stored.costing_method.as_str())
                    .fact("valuation", stored.valuation.as_str()),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            // A revaluation wearing a settings change. Refused while anything
            // is filed here; moving the items to a category that already has
            // the wanted method is the same act done visibly.
            if before.costing_method != checked.costing_method
                && store::item_count(pool, id).await? > 0
            {
                return Ok(Submission::rejected(
                    "costing_method",
                    CategoryError::HasValuedStock.message(),
                ));
            }

            let mut tx = pool.begin().await.map_err(DbError::Query)?;

            match store::update(&mut *tx, id, &path, &checked, caller.user_id()).await {
                Ok(true) => {}
                Ok(false) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(Submission::rejected("name", msg!("categories.gone")));
                }
                Err(DbError::CodeExists { code, .. }) => {
                    tx.rollback().await.map_err(DbError::Query)?;
                    return Ok(path_taken(&code));
                }
                Err(err) => return Err(err.into()),
            }

            if let Err(err) = store::rename_subtree(&mut *tx, id, &before.code, &path).await {
                tx.rollback().await.map_err(DbError::Query)?;
                return match err {
                    DbError::CodeExists { code, .. } => Ok(path_taken(&code)),
                    err => Err(err.into()),
                };
            }

            tx.commit().await.map_err(DbError::Query)?;

            let stored = CategoryInput {
                id: Some(id),
                ..checked
            };

            audit::updated(
                pool,
                caller,
                Target::new(kinds::ITEM_CATEGORY, id).named(&path),
                &CategoryInput::from_category(&before),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
    }
}

pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DeleteOutcome> {
    caller.require(permissions::ITEM_CATEGORIES_MANAGE)?;
    acting_user(caller)?;

    let category = detail(pool, caller, id).await?;

    let children = store::descendants(pool, id).await?;
    if !children.is_empty() {
        return Ok(DeleteOutcome::HasChildren {
            count: children.len() as i64,
        });
    }

    // Moving them elsewhere would change how they are costed, which is not a
    // thing a delete may do quietly.
    let items = store::item_count(pool, id).await?;
    if items > 0 {
        return Ok(DeleteOutcome::HasItems { count: items });
    }

    if !store::delete(pool, id).await? {
        return Ok(DeleteOutcome::Deleted);
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::ITEM_CATEGORY, id).named(&category.code),
        &CategoryInput::from_category(&category),
    )
    .await;

    Ok(DeleteOutcome::Deleted)
}

async fn check_placement(
    pool: &PgPool,
    moving: Option<Uuid>,
    parent_id: Uuid,
) -> ServiceResult<Result<(), CategoryError>> {
    let Some(parent) = store::find(pool, parent_id).await? else {
        return Ok(Err(CategoryError::Cycle));
    };

    if let Some(moving) = moving {
        if moving == parent_id {
            return Ok(Err(CategoryError::OwnParent));
        }
        if store::descendants(pool, moving).await?.contains(&parent_id) {
            return Ok(Err(CategoryError::Cycle));
        }
    }

    // The path is the ancestry, so counting separators is counting levels.
    if parent.code.matches('/').count() + 2 > MAX_CATEGORY_DEPTH {
        return Ok(Err(CategoryError::TooDeep));
    }

    Ok(Ok(()))
}

fn path_taken(path: &str) -> Submission<CategoryInput> {
    Submission::rejected("name", msg!("categories.error.path_taken", path = path))
}
