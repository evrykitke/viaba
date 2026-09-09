//! People: hiring, moving, leaving, rehiring, and the login some of them get.
//!
//! # Every act here is dated, and none of them overwrites the last
//!
//! [`save`] edits who somebody *is* - their name, how to reach them. It touches
//! nothing with a date on it, deliberately: correcting a misspelled surname
//! must not be able to rewrite which department somebody was in last March.
//!
//! Moving them is [`move_to`], which closes the open assignment and opens a new
//! one. Leaving is [`record_leaver`]. Coming back is [`rehire`], which starts a
//! *second* engagement and leaves the first exactly as it was. That is the
//! whole design - see the head of `migrations/apps/hr/0002_people.sql`.
//!
//! # An employee is not a user, and creating one creates no account
//!
//! Most people who work somewhere never sign in. [`create_login`] is a separate,
//! deliberate act with its own permission, and it goes through the ordinary
//! invitation flow rather than writing an account itself: the person sets their
//! own password, and nobody - not the administrator who invited them - ever
//! knows it.
//!
//! It also needs `Users.Create`, which `identity::invitation::invite` checks.
//! `Employees.Invite` on its own therefore grants nothing, which is the point:
//! adding somebody to the staff list is an HR act, and letting them into the
//! accounting system is a security one.
//!
//! # Cycles are walked, not constrained
//!
//! `assignments_not_own_manager` catches the one-row case. A reports to B who
//! reports to A cannot be seen from one row, so [`check_reporting_line`] walks
//! the chain before a manager is stored - the same shape
//! `department::check_placement` uses for the tree, and for the same reason: a
//! `CHECK` cannot follow an edge.

use app_hr::employee::{
    AssignmentInput, Employee, EmployeeError, EmployeeInput, EmployeeSummary, LeavingInput,
    MAX_REPORTING_DEPTH,
};
use chrono::{Duration, NaiveDate};
use phonix_core::form::Submission;
use phonix_core::identity::{InvitationIssued, UserInvite};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::employee as store;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};
use crate::identity::invitation::Inviting;

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<EmployeeSummary>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::list(pool).await?)
}

/// Everybody currently employed, for a manager picker.
///
/// Employees rather than users: most managers never sign in, and a reporting
/// line that only exists for people with accounts is an org chart with holes.
pub async fn employed(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<(Uuid, String, String)>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::employed(pool).await?)
}

/// One person, with every engagement and every assignment.
///
/// Personal details - date of birth, national identifier - are stripped for a
/// caller without `Employees.Personal`. Removed *here* rather than hidden by
/// the screen: a field the browser merely does not draw is one that was still
/// sent to it.
pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Employee> {
    caller.require(permissions::EMPLOYEES)?;

    let employee = store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("employee", msg!("employees.gone")))?;

    if caller.can(permissions::EMPLOYEES_PERSONAL) {
        return Ok(employee);
    }

    Ok(Employee {
        date_of_birth: None,
        national_id: None,
        ..employee
    })
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<EmployeeInput> {
    Ok(EmployeeInput::from_employee(
        &detail(pool, caller, id).await?,
        today(),
    ))
}

pub fn blank(caller: &Caller) -> ServiceResult<EmployeeInput> {
    caller.require(permissions::EMPLOYEES_MANAGE)?;
    Ok(EmployeeInput::blank(today()))
}

/// What each department currently costs in people.
pub async fn headcount(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<(Uuid, String, i64)>> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::headcount(pool).await?)
}

/// Hire somebody, or correct who they are.
///
/// On create this writes three rows - the person, their first engagement, and
/// their first assignment - because hiring somebody is one act and asking a
/// user to perform it as three forms would be asking them to remember to.
///
/// On edit it writes exactly one. See the module docs.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: EmployeeInput,
) -> ServiceResult<Submission<EmployeeInput>> {
    caller.require(permissions::EMPLOYEES_MANAGE)?;
    acting_user(caller)?;

    let mut checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    if let Some(manager_id) = checked.manager_id {
        if let Err(err) = check_reporting_line(pool, checked.id, manager_id).await? {
            return Ok(Submission::rejected(err.field(), err.message()));
        }
    }

    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if checked.id.is_none() && checked.code.is_empty() {
        let key = SequenceKey::new(app_hr::APP_ID, app_hr::EMPLOYEE);
        match generator.next(&mut tx, key, today()).await {
            Ok(allocated) => checked.code = allocated.number,
            Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "code",
                    msg!("employees.error.no_series"),
                ));
            }
            Err(err) => return Err(err),
        }
    }

    let creating = checked.id.is_none();

    let id = match checked.id {
        None => match store::insert(&mut tx, &checked, caller.user_id()).await {
            Ok(id) => id,
            Err(err) => return rollback_with(tx, err).await,
        },
        Some(id) => match store::update(&mut tx, id, &checked, caller.user_id()).await {
            Ok(true) => id,
            Ok(false) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected("id", msg!("employees.gone")));
            }
            Err(err) => return rollback_with(tx, err).await,
        },
    };

    // The first engagement and the first assignment, on create only. An edit
    // reaches neither: moving somebody is `move_to`, and changing when they
    // started is not something a name-correction form may do.
    if creating {
        let Some(started_on) = checked.started_on else {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "started_on",
                EmployeeError::StartDateRequired.message(),
            ));
        };

        let engagement_id = store::start_engagement(
            &mut tx,
            id,
            started_on,
            checked.employment_type,
            None,
            caller.user_id(),
        )
        .await?;

        store::open_assignment(
            &mut tx,
            engagement_id,
            &app_hr::employee::CheckedAssignment {
                effective_from: started_on,
                department_id: checked.department_id,
                job_position_id: checked.job_position_id,
                work_location_id: checked.work_location_id,
                manager_id: checked.manager_id,
                reason: None,
            },
            caller.user_id(),
        )
        .await?;
    }

    tx.commit().await.map_err(DbError::Query)?;

    let stored = EmployeeInput {
        id: Some(id),
        code: checked.code.clone(),
        ..draft
    };

    let name = format!("{} {}", stored.given_name, stored.family_name);
    let target = Target::new(kinds::EMPLOYEE, id)
        .named(&name)
        .fact("code", &stored.code);

    if creating {
        audit::created(pool, caller, target, &stored).await;
    } else {
        audit::updated(pool, caller, target, &stored, &stored).await;
    }

    Ok(Submission::Saved(stored))
}

/// Move somebody: a new assignment, closing the one before it.
///
/// The old assignment ends the day *before* the new one begins, so the two do
/// not both cover the changeover date - a headcount report run that day would
/// otherwise count the person twice.
pub async fn move_to(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
    draft: AssignmentInput,
) -> ServiceResult<Submission<Employee>> {
    caller.require(permissions::EMPLOYEES_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let Some((engagement_id, started_on)) = store::open_engagement(pool, employee_id).await? else {
        return Ok(Submission::rejected(
            "id",
            EmployeeError::NotEmployed.message(),
        ));
    };

    if checked.effective_from < started_on {
        return Ok(Submission::rejected(
            "effective_from",
            EmployeeError::AssignmentBeforeEngagement.message(),
        ));
    }

    if let Some(manager_id) = checked.manager_id {
        if let Err(err) = check_reporting_line(pool, Some(employee_id), manager_id).await? {
            return Ok(Submission::rejected(err.field(), err.message()));
        }
    }

    let before = detail(pool, caller, employee_id).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    // Close first: `assignments_one_open_per_engagement` refuses a second open
    // row, which is the index doing its job rather than an obstacle.
    store::close_open_assignment(
        &mut tx,
        engagement_id,
        checked.effective_from - Duration::days(1),
        caller.user_id(),
    )
    .await?;

    store::open_assignment(&mut tx, engagement_id, &checked, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let after = detail(pool, caller, employee_id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::EMPLOYEE, employee_id)
            .named(&after.display_name())
            .fact("moved_on", &checked.effective_from.to_string()),
        &before,
        &after,
    )
    .await;

    Ok(Submission::Saved(after))
}

/// End somebody's employment.
///
/// The record is kept: this is what makes a rehire a second engagement rather
/// than a second person. The login, if there is one, is deliberately left
/// alone - closing an account is `identity`'s act, under its own permission,
/// and an HR screen quietly revoking access would be a security decision taken
/// by somebody recording a leaving date.
pub async fn record_leaver(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
    draft: LeavingInput,
) -> ServiceResult<Submission<Employee>> {
    caller.require(permissions::EMPLOYEES_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, employee_id).await?;

    let Some((engagement_id, started_on)) = store::open_engagement(pool, employee_id).await? else {
        return Ok(Submission::rejected(
            "id",
            EmployeeError::NotEmployed.message(),
        ));
    };

    let checked = match draft.check(started_on) {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    // The assignment closes with the engagement. An open assignment on a closed
    // engagement would keep the person in `current_headcount` for ever, because
    // the view joins on both being open.
    store::close_open_assignment(&mut tx, engagement_id, checked.ended_on, caller.user_id())
        .await?;

    if !store::end_engagement(&mut tx, engagement_id, &checked, caller.user_id()).await? {
        // Somebody else recorded it between the read and the write. Their
        // reason stands - overwriting it would replace one leaving reason with
        // another, silently.
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(Submission::rejected(
            "id",
            EmployeeError::NotEmployed.message(),
        ));
    }

    tx.commit().await.map_err(DbError::Query)?;

    let after = detail(pool, caller, employee_id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::EMPLOYEE, employee_id)
            .named(&after.display_name())
            .fact("left_on", &checked.ended_on.to_string())
            .fact("reason", checked.reason.as_str()),
        &before,
        &after,
    )
    .await;

    Ok(Submission::Saved(after))
}

/// Take somebody back: a second engagement, leaving every earlier one alone.
pub async fn rehire(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
    draft: EmployeeInput,
) -> ServiceResult<Submission<Employee>> {
    caller.require(permissions::EMPLOYEES_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, employee_id).await?;

    if before.is_employed() {
        return Ok(Submission::rejected(
            "id",
            EmployeeError::AlreadyEmployed.message(),
        ));
    }

    let Some(started_on) = draft.started_on else {
        return Ok(Submission::rejected(
            "started_on",
            EmployeeError::StartDateRequired.message(),
        ));
    };

    if let Some(manager_id) = draft.manager_id {
        if let Err(err) = check_reporting_line(pool, Some(employee_id), manager_id).await? {
            return Ok(Submission::rejected(err.field(), err.message()));
        }
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let engagement_id = store::start_engagement(
        &mut tx,
        employee_id,
        started_on,
        draft.employment_type,
        None,
        caller.user_id(),
    )
    .await?;

    store::open_assignment(
        &mut tx,
        engagement_id,
        &app_hr::employee::CheckedAssignment {
            effective_from: started_on,
            department_id: draft.department_id,
            job_position_id: draft.job_position_id,
            work_location_id: draft.work_location_id,
            manager_id: draft.manager_id,
            reason: None,
        },
        caller.user_id(),
    )
    .await?;

    tx.commit().await.map_err(DbError::Query)?;

    let after = detail(pool, caller, employee_id).await?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::EMPLOYEE, employee_id)
            .named(&after.display_name())
            .fact("rehired_on", &started_on.to_string()),
        &before,
        &after,
    )
    .await;

    Ok(Submission::Saved(after))
}

/// Remove a record created in error.
///
/// Only somebody with no employment at all, which in practice means a row saved
/// by mistake and caught immediately. Anybody who has ever worked here is
/// recorded as a leaver instead: deleting them would take their assignment
/// history - and every cost report that history explains - with them.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::EMPLOYEES_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if store::engagement_count(pool, id).await? > 0 {
        return Ok(Submission::rejected(
            "id",
            EmployeeError::HasHistory.message(),
        ));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let removed = store::delete(&mut tx, id).await?;
    tx.commit().await.map_err(DbError::Query)?;

    if removed {
        audit::deleted(
            pool,
            caller,
            Target::new(kinds::EMPLOYEE, id).named(&before.display_name()),
            &before,
        )
        .await;
    }

    Ok(Submission::Saved(()))
}

// ---------------------------------------------------------------------------
// The login
// ---------------------------------------------------------------------------

/// Create a login for an employee and send them its invitation.
///
/// Not everybody gets one - see the module docs. This is the only path from an
/// employee to an account, and it goes through the ordinary invitation flow
/// rather than writing a user row itself, which buys three things:
///
///   * the person sets their own password, so nobody else ever knows it;
///   * the address is proved by the fact that the link was opened;
///   * `identity::invitation::invite` checks `Users.Create` and the role rules,
///     so an HR permission alone cannot let somebody into the system.
///
/// The account is linked to the employee in the same request. If the link fails
/// - the only way being that the account was attached to somebody else in
/// between - the invitation still stands, and the screen says so rather than
/// pretending nothing happened.
pub async fn create_login(
    pool: &PgPool,
    caller: &Caller,
    ctx: &Inviting<'_>,
    employee_id: Uuid,
    roles: Vec<String>,
) -> ServiceResult<Submission<InvitationIssued>> {
    caller.require(permissions::EMPLOYEES_INVITE)?;
    acting_user(caller)?;

    let employee = detail(pool, caller, employee_id).await?;

    // Asked as one question so the screen gets the specific reason rather than
    // a generic refusal: already has one, no longer here, or nowhere to send it.
    if let Some(blocker) = employee.invitation_blocker() {
        return Ok(Submission::rejected(blocker.field(), blocker.message()));
    }

    let Some(email) = employee.work_email.clone() else {
        return Ok(Submission::rejected(
            "work_email",
            EmployeeError::WorkEmailRequiredForLogin.message(),
        ));
    };

    let issued = match crate::identity::invitation::invite(
        pool,
        caller,
        ctx,
        UserInvite {
            email,
            // The legal name, not the preferred one: this is the account's
            // identity, and it is what appears on an audit trail.
            first_name: employee.given_name.clone(),
            last_name: employee.family_name.clone(),
            roles,
        },
    )
    .await?
    {
        Submission::Saved(issued) => issued,
        // An address already taken, an unknown role. Both belong on the field
        // the invitation form would have put them on.
        rejection => return Ok(rejection),
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    let linked = store::set_login(&mut tx, employee_id, Some(issued.user_id), caller.user_id())
        .await;

    match linked {
        Ok(true) => tx.commit().await.map_err(DbError::Query)?,
        Ok(false) | Err(_) => {
            // The account exists and the invitation has been sent; only the
            // link failed. Rolling that back is right - a half-written link is
            // worse than none - but the account is not undone, and saying so is
            // more use than a bare error.
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(Submission::rejected(
                "user_id",
                EmployeeError::AlreadyHasLogin.message(),
            ));
        }
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::EMPLOYEE, employee_id)
            .named(&employee.display_name())
            .fact("login_created", &issued.email),
        &employee,
        &Employee {
            user_id: Some(issued.user_id),
            ..employee.clone()
        },
    )
    .await;

    Ok(Submission::Saved(issued))
}

/// Detach a login from a person, leaving the account itself alone.
///
/// For the link made against the wrong employee. Closing the account is a
/// different act under a different permission, and doing both from here would
/// mean somebody correcting a mis-click silently locked a real person out.
pub async fn unlink_login(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
) -> ServiceResult<Submission<()>> {
    caller.require(permissions::EMPLOYEES_INVITE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, employee_id).await?;

    let mut tx = pool.begin().await.map_err(DbError::Query)?;
    store::set_login(&mut tx, employee_id, None, caller.user_id()).await?;
    tx.commit().await.map_err(DbError::Query)?;

    audit::updated(
        pool,
        caller,
        Target::new(kinds::EMPLOYEE, employee_id).named(&before.display_name()),
        &before,
        &Employee {
            user_id: None,
            ..before.clone()
        },
    )
    .await;

    Ok(Submission::Saved(()))
}

// ---------------------------------------------------------------------------
// Working out
// ---------------------------------------------------------------------------

/// Refuse a reporting line that loops.
///
/// `assignments_not_own_manager` catches A reporting to A. This catches A → B →
/// A, and every longer ring, by walking up from the proposed manager until it
/// runs out of managers or finds the person being assigned.
///
/// [`MAX_REPORTING_DEPTH`] stops the walk regardless. A ring that already
/// exists in the data - written before this check did, or by a manager being
/// deleted and reassigned - would otherwise spin here for ever, and a hang is a
/// worse failure than a refusal.
async fn check_reporting_line(
    pool: &PgPool,
    employee_id: Option<Uuid>,
    manager_id: Uuid,
) -> ServiceResult<Result<(), EmployeeError>> {
    // A manager who does not work here cannot be reported to. Checked before
    // the walk, because the walk reads open assignments and a leaver has none -
    // which would otherwise read as "no cycle" rather than as the real problem.
    if store::open_engagement(pool, manager_id).await?.is_none() {
        return Ok(Err(EmployeeError::ManagerNotEmployed));
    }

    let Some(employee_id) = employee_id else {
        // Creating somebody: they have no id yet, so nothing can point back at
        // them and there is no ring to find.
        return Ok(Ok(()));
    };

    if manager_id == employee_id {
        return Ok(Err(EmployeeError::ReportingCycle));
    }

    let mut seen = manager_id;

    for _ in 0..MAX_REPORTING_DEPTH {
        let Some(next) = store::current_manager(pool, seen).await? else {
            return Ok(Ok(()));
        };

        if next == employee_id {
            return Ok(Err(EmployeeError::ReportingCycle));
        }

        seen = next;
    }

    // Deeper than anybody defends, or a ring that predates this check. Either
    // way the honest answer is to refuse rather than to keep walking.
    Ok(Err(EmployeeError::ReportingCycle))
}

/// A code clash and a national-identifier clash both arrive as `CodeExists`,
/// and they belong on different fields.
async fn rollback_with<T>(
    tx: phonix_db::sqlx::Transaction<'_, phonix_db::sqlx::Postgres>,
    err: DbError,
) -> ServiceResult<Submission<T>> {
    tx.rollback().await.map_err(DbError::Query)?;

    match err {
        DbError::CodeExists { entity, .. } if entity == "employee_national_id" => Ok(
            Submission::rejected("national_id", EmployeeError::NationalIdTaken.message()),
        ),
        DbError::CodeExists { .. } => Ok(Submission::rejected(
            "code",
            EmployeeError::CodeTaken.message(),
        )),
        other => Err(other.into()),
    }
}

/// Somebody's current assignment, for a move form to open on.
pub async fn current_assignment(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
) -> ServiceResult<AssignmentInput> {
    let employee = detail(pool, caller, employee_id).await?;

    Ok(AssignmentInput::next(
        employee.current_assignment(),
        today(),
    ))
}

/// Who reports to somebody now. Shown before a leaver is recorded, so the
/// screen can say who has to be reassigned rather than leaving it to be
/// discovered.
pub async fn direct_reports(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
) -> ServiceResult<i64> {
    caller.require(permissions::EMPLOYEES)?;
    Ok(store::direct_reports(pool, employee_id).await?)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
