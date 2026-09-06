//! Creating, changing and retiring a department.
//!
//! A create with a blank code allocates one inside the insert's transaction, so
//! a failed insert returns the number. A typed code is used as typed.
//!
//! [`delete`] refuses any cost centre and offers deactivation instead: nothing
//! holds a foreign key into `hr`, so the database cannot answer whether
//! anything has been charged here. Same call `master::party::delete` makes.

use app_hr::department::{
    Department, DepartmentError, DepartmentInput, DepartmentSummary, MAX_DEPARTMENT_DEPTH,
};
use phonix_core::form::Submission;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::department as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub use app_hr::department::DeleteOutcome;

/// Every department, arranged into the tree.
pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<DepartmentSummary>> {
    caller.require(permissions::DEPARTMENTS)?;
    Ok(store::list(pool).await?)
}

/// One department.
pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Department> {
    caller.require(permissions::DEPARTMENTS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("department", msg!("departments.gone")))
}

/// The editable part of one, for the form to open on.
pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DepartmentInput> {
    Ok(DepartmentInput::from_department(
        &detail(pool, caller, id).await?,
    ))
}

/// Create a department, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: DepartmentInput,
) -> ServiceResult<Submission<DepartmentInput>> {
    match draft.id {
        None => create(pool, caller, draft).await,
        Some(id) => update(pool, caller, id, draft).await,
    }
}

async fn create(
    pool: &PgPool,
    caller: &Caller,
    draft: DepartmentInput,
) -> ServiceResult<Submission<DepartmentInput>> {
    caller.require(permissions::DEPARTMENTS_CREATE)?;
    acting_user(caller)?;

    // The browser's check is a courtesy; this one is the control.
    let mut checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    if let Some(parent_id) = checked.parent_id
        && let Err(err) = check_placement(pool, None, parent_id).await?
    {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    // Outside the transaction: every department queues through the sequence's
    // one row, so anything that can happen before the lock should.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if checked.code.is_empty() {
        // A department has no date. `hr.toml` says `reset = "never"`, so this
        // reaches only an unused period key.
        let key = SequenceKey::new(app_hr::APP_ID, app_hr::DEPARTMENT);
        match generator
            .next(&mut tx, key, chrono::Utc::now().date_naive())
            .await
        {
            Ok(allocated) => checked.code = allocated.number,
            // The series is missing or off. Reported on the code field,
            // because typing one by hand is the other way out.
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "code",
                    msg!("department.error.code_required"),
                ));
            }
            Err(err) => return Err(err),
        }
    }

    let id = match store::insert(&mut *tx, &checked, caller.user_id()).await {
        Ok(id) => id,
        Err(DbError::CodeExists { code, .. }) => {
            // Rolling back returns the number.
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(code_taken(&code));
        }
        Err(err) => return Err(err.into()),
    };

    tx.commit().await.map_err(DbError::Query)?;

    let stored = DepartmentInput {
        id: Some(id),
        ..checked
    };

    audit::created(
        pool,
        caller,
        Target::new(kinds::DEPARTMENT, id)
            .named(&stored.name)
            .fact("code", &stored.code),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

async fn update(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    draft: DepartmentInput,
) -> ServiceResult<Submission<DepartmentInput>> {
    caller.require(permissions::DEPARTMENTS_EDIT)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let before = detail(pool, caller, id).await?;

    // Only when it moved: re-checking would refuse a rename in a tree that is
    // already at the limit.
    if checked.parent_id != before.parent_id
        && let Some(parent_id) = checked.parent_id
        && let Err(err) = check_placement(pool, Some(id), parent_id).await?
    {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    let stored = match store::update(pool, id, &checked, caller.user_id()).await {
        Ok(true) => DepartmentInput {
            id: Some(id),
            ..checked
        },
        Ok(false) => return Ok(Submission::rejected("name", msg!("departments.gone"))),
        Err(DbError::CodeExists { code, .. }) => return Ok(code_taken(&code)),
        Err(err) => return Err(err.into()),
    };

    audit::updated(
        pool,
        caller,
        Target::new(kinds::DEPARTMENT, id).named(&stored.name),
        &DepartmentInput::from_department(&before),
        &stored,
    )
    .await;

    Ok(Submission::Saved(stored))
}

/// Remove a department, where removing one is the honest thing to do.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<DeleteOutcome> {
    caller.require(permissions::DEPARTMENTS_DELETE)?;
    acting_user(caller)?;

    let department = detail(pool, caller, id).await?;

    // Before the children check: emptying a cost centre would not make it
    // deletable anyway.
    if department.is_cost_centre {
        return Ok(DeleteOutcome::MayBeInUse);
    }

    // Postgres would refuse this itself; asking first lets the screen say how
    // many are in the way.
    let children = store::descendants_of(pool, id).await?;
    if !children.is_empty() {
        return Ok(DeleteOutcome::HasChildren {
            count: children.len() as i64,
        });
    }

    if !store::delete(pool, id).await? {
        // Somebody else removed it. "Make it gone" is about the end state.
        return Ok(DeleteOutcome::Deleted);
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::DEPARTMENT, id)
            .named(&department.name)
            .fact("code", &department.code),
        &DepartmentInput::from_department(&department),
    )
    .await;

    Ok(DeleteOutcome::Deleted)
}

/// Whether `parent_id` is somewhere `moving` may go. Both refusals — a cycle,
/// and exceeding the depth limit — need the rest of the tree, which is why they
/// are not in `DepartmentInput::check`.
///
/// Depth is measured from the deepest row being moved. `moving` is `None` on a
/// create.
async fn check_placement(
    pool: &PgPool,
    moving: Option<Uuid>,
    parent_id: Uuid,
) -> ServiceResult<Result<(), DepartmentError>> {
    let Some(parent_depth) = store::depth_of(pool, parent_id).await? else {
        // The parent is gone. Both this and a cycle mean "not somewhere this
        // can go", and the picker only offers it by being stale.
        return Ok(Err(DepartmentError::Cycle));
    };

    let mut subtree_height = 1;

    if let Some(moving) = moving {
        if moving == parent_id {
            return Ok(Err(DepartmentError::OwnParent));
        }

        let below = store::descendants_of(pool, moving).await?;
        if below.contains(&parent_id) {
            return Ok(Err(DepartmentError::Cycle));
        }

        // The tallest thing being carried. One query per descendant is fine
        // at this table's size.
        let moving_depth = store::depth_of(pool, moving).await?.unwrap_or(1);
        for descendant in below {
            if let Some(depth) = store::depth_of(pool, descendant).await? {
                subtree_height = subtree_height.max(depth - moving_depth + 1);
            }
        }
    }

    if parent_depth + subtree_height > MAX_DEPARTMENT_DEPTH as i64 {
        return Ok(Err(DepartmentError::TooDeep));
    }

    Ok(Ok(()))
}

/// The code is already on another department — typed, or handed out by a
/// sequence whose `start_at` was moved backwards.
fn code_taken(code: &str) -> Submission<DepartmentInput> {
    Submission::rejected("code", msg!("error.department.code_taken", code = code))
}

/// Who the manager picker may offer: a name and an id.
///
/// Gated on `Departments.Edit`, not `Users` — requiring user administration to
/// fill in a dropdown would hand out password changes with it.
pub async fn manager_candidates(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<(phonix_core::identity::UserId, String)>> {
    caller.require(permissions::DEPARTMENTS_EDIT)?;
    Ok(store::manager_candidates(pool).await?)
}

/// How many departments there are, and how many are chargeable.
pub async fn counts(pool: &PgPool, caller: &Caller) -> ServiceResult<(i64, i64)> {
    caller.require(permissions::DEPARTMENTS)?;
    Ok(store::counts(pool).await?)
}
