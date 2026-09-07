//! Units of measure: adding one, changing one, retiring one.
//!
//! # Why a factor is not freely editable
//!
//! Every quantity ever recorded in a unit was recorded against the factor of
//! the day. Changing a kilogram from 1000 grams to 1 would restate every
//! weight in the workspace without touching a single stored number.
//!
//! It is allowed anyway, and only while nothing counts in the unit - which is
//! the honest line, because a workspace that typed 100 instead of 1000 this
//! morning has to be able to fix it.

use app_inventory::unit::{DeleteOutcome, Unit, UnitInput};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::inventory::unit as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// Every unit, grouped by what it measures.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Unit>> {
    caller.require(permissions::UNITS)?;
    Ok(store::list(pool).await?)
}

/// The units a picker offers.
///
/// Gated on `ITEMS` rather than `UNITS`: this is what the item form needs, and
/// somebody who may edit an item must be able to choose what it is counted in
/// without also being allowed to redraw the unit list.
pub async fn selectable(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<Unit>> {
    caller.require(permissions::ITEMS)?;
    Ok(store::selectable(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Unit> {
    caller.require(permissions::UNITS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("unit", msg!("units.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<UnitInput> {
    Ok(UnitInput::from_unit(&detail(pool, caller, id).await?))
}

/// Add a unit, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: UnitInput,
) -> ServiceResult<Submission<UnitInput>> {
    caller.require(permissions::UNITS_MANAGE)?;
    acting_user(caller)?;

    // The browser's check is a courtesy; this one is the control.
    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    match draft.id {
        None => {
            let id = match store::insert(pool, &checked, caller.user_id()).await {
                Ok(id) => id,
                Err(DbError::CodeExists { entity, code }) => return Ok(taken(entity, &code)),
                Err(err) => return Err(err.into()),
            };

            let stored = UnitInput {
                id: Some(id),
                ..draft
            };

            audit::created(
                pool,
                caller,
                Target::new(kinds::UNIT_OF_MEASURE, id)
                    .named(&checked.name)
                    .fact("code", &checked.code)
                    .fact("class", checked.class.as_str()),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
        Some(id) => {
            let before = detail(pool, caller, id).await?;

            // The one thing that cannot be undone by editing it back: every
            // quantity already recorded was recorded against the old factor.
            if before.factor_scaled != checked.factor_scaled
                && store::item_count(pool, id).await? > 0
            {
                return Ok(Submission::rejected(
                    "factor",
                    msg!("units.error.factor_locked"),
                ));
            }

            // Same argument, sharper: a kilogram reclassified as a volume makes
            // every conversion it was ever part of meaningless.
            if before.class != checked.class && store::item_count(pool, id).await? > 0 {
                return Ok(Submission::rejected(
                    "class",
                    msg!("units.error.class_locked"),
                ));
            }

            match store::update(pool, id, &checked, caller.user_id()).await {
                Ok(true) => {}
                Ok(false) => return Ok(Submission::rejected("code", msg!("units.gone"))),
                Err(DbError::CodeExists { entity, code }) => return Ok(taken(entity, &code)),
                Err(err) => return Err(err.into()),
            }

            let stored = UnitInput {
                id: Some(id),
                ..draft
            };

            audit::updated(
                pool,
                caller,
                Target::new(kinds::UNIT_OF_MEASURE, id).named(&checked.name),
                &UnitInput::from_unit(&before),
                &stored,
            )
            .await;

            Ok(Submission::Saved(stored))
        }
    }
}

/// Remove a unit nothing counts in.
///
/// Answers how many items are in the way rather than letting Postgres refuse
/// it, so the screen can say what to do about them.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DeleteOutcome> {
    caller.require(permissions::UNITS_MANAGE)?;
    acting_user(caller)?;

    let unit = detail(pool, caller, id).await?;

    let in_use = store::item_count(pool, id).await?;
    if in_use > 0 {
        return Ok(DeleteOutcome::InUse { count: in_use });
    }

    // A class with no reference unit has nothing for its factors to be
    // against, so the last one standing is retired rather than removed.
    if unit.is_base
        && store::list(pool)
            .await?
            .iter()
            .any(|other| other.class == unit.class && other.id != id)
    {
        return Ok(DeleteOutcome::IsTheReference);
    }

    if !store::delete(pool, id).await? {
        // Somebody else removed it. "Make it gone" is about the end state.
        return Ok(DeleteOutcome::Deleted);
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::UNIT_OF_MEASURE, id)
            .named(&unit.name)
            .fact("code", &unit.code),
        &UnitInput::from_unit(&unit),
    )
    .await;

    Ok(DeleteOutcome::Deleted)
}

/// A duplicate, on whichever field it was actually about. Two constraints reach
/// here and they are different mistakes.
fn taken(entity: &'static str, code: &str) -> Submission<UnitInput> {
    if entity == "unit_base" {
        Submission::rejected("factor", msg!("units.error.base_already_set"))
    } else {
        Submission::rejected("code", msg!("units.error.code_taken", code = code))
    }
}
