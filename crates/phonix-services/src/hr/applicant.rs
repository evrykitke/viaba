//! Recruitment: who has applied, and what happens when one is hired.
//!
//! # Hiring points at a person rather than making one
//!
//! [`hire`] looks for an existing employee before creating one, and where it
//! finds one it opens a *second engagement* through
//! [`super::employee::rehire`]. Somebody who left in 2019 and applies again in
//! 2026 is one human being with two periods of employment — the
//! duplicate-identity case the whole of `0002_people.sql` was written to
//! refuse, and the one a recruitment module is most likely to walk into,
//! because an applicant arrives looking exactly like a stranger.
//!
//! The match is on the work email, which is the cheapest honest signal
//! available here. It is deliberately not a guess at names: two people called
//! the same thing is ordinary, and merging them would be worse than the
//! duplicate.

use app_hr::applicant::{Applicant, ApplicantError, ApplicantInput, ApplicantSummary, Stage};
use app_hr::employee::{EmployeeInput, EmploymentType};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::permissions;
use phonix_db::hr::applicant as store;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(pool: &PgPool, caller: &Caller) -> ServiceResult<Vec<ApplicantSummary>> {
    caller.require(permissions::APPLICANTS)?;
    Ok(store::list(pool).await?)
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Applicant> {
    caller.require(permissions::APPLICANTS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("applicant", ApplicantError::Gone.message()))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<ApplicantInput> {
    let applicant = detail(pool, caller, id).await?;

    if !applicant.stage.is_movable() {
        return Err(ServiceError::rejected(
            "stage",
            ApplicantError::NotMovable.message(),
        ));
    }

    Ok(ApplicantInput::from_applicant(&applicant))
}

pub fn blank(caller: &Caller) -> ServiceResult<ApplicantInput> {
    caller.require(permissions::APPLICANTS_MANAGE)?;
    Ok(ApplicantInput::blank(chrono::Utc::now().date_naive()))
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: ApplicantInput,
) -> ServiceResult<Submission<ApplicantInput>> {
    caller.require(permissions::APPLICANTS_MANAGE)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::save(pool, &checked, caller.user_id()).await? {
        Some(id) => id,
        // The update names `stage <> 'hired'`, so nothing came back either
        // because the row is gone or because somebody hired them first.
        None => {
            return Ok(Submission::rejected(
                "stage",
                ApplicantError::NotMovable.message(),
            ));
        }
    };

    let stored = ApplicantInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::APPLICANT, id)
        .named(format!("{} {}", checked.given_name, checked.family_name))
        .fact("stage", checked.stage.as_str());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Hire them: open an engagement, and point the application at the person.
///
/// `started_on` is the day the employment begins, which is not the day the
/// application was made and not today either — an offer accepted in March for a
/// June start is ordinary.
///
/// Where the applicant's email matches an employee already on file, this opens
/// a second engagement on that record rather than creating another person. See
/// the head of this module.
pub async fn hire(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    started_on: NaiveDate,
) -> ServiceResult<Submission<Uuid>> {
    caller.require(permissions::APPLICANTS_HIRE)?;
    acting_user(caller)?;

    let applicant = detail(pool, caller, id).await?;

    if !applicant.stage.is_open() {
        return Ok(Submission::rejected(
            "stage",
            ApplicantError::AlreadyClosed.message(),
        ));
    }

    // The one question that decides whether this creates a person.
    let existing = match applicant.email.as_deref() {
        Some(email) => store::employee_with_email(pool, email).await?,
        None => None,
    };

    let draft = EmployeeInput {
        id: existing,
        given_name: applicant.given_name.clone(),
        family_name: applicant.family_name.clone(),
        work_email: applicant.email.clone().unwrap_or_default(),
        work_phone: applicant.phone.clone().unwrap_or_default(),
        started_on: Some(started_on),
        employment_type: EmploymentType::Permanent,
        // The job they applied for is the job they start in. Everything else
        // about the assignment is left for somebody to fill in afterwards:
        // guessing a department from a role would be inventing a fact.
        job_position_id: Some(applicant.job_position_id),
        ..EmployeeInput::blank(started_on)
    };

    // The two paths answer different shapes, so each is reduced to the
    // question this one asks: did the employee half refuse? Whatever it
    // refused with is the answer - it knows why, and this would be guessing.
    let refusal = match existing {
        // A second engagement on the record they already have. `rehire` owns
        // the rules: it refuses somebody still employed, and it opens the
        // engagement and its first assignment together.
        Some(employee_id) => {
            match super::employee::rehire(pool, caller, employee_id, draft).await? {
                Submission::Rejected(errors) => Some(errors),
                Submission::Saved(_) => None,
            }
        }
        None => match super::employee::save(pool, caller, draft).await? {
            Submission::Rejected(errors) => Some(errors),
            Submission::Saved(_) => None,
        },
    };

    if let Some(errors) = refusal {
        return Ok(Submission::Rejected(errors));
    }

    let Some(employee_id) = resolve_employee(pool, &applicant, existing).await? else {
        return Ok(Submission::rejected("id", ApplicantError::Gone.message()));
    };

    if !store::mark_hired(pool, id, employee_id, caller.user_id()).await? {
        return Ok(Submission::rejected(
            "stage",
            ApplicantError::AlreadyClosed.message(),
        ));
    }

    audit::updated(
        pool,
        caller,
        Target::new(kinds::APPLICANT, id)
            .named(applicant.display_name())
            .fact("stage", Stage::Hired.as_str())
            .fact("employee_id", employee_id.to_string()),
        &applicant,
        &detail(pool, caller, id).await?,
    )
    .await;

    Ok(Submission::Saved(employee_id))
}

/// Which employee the hire landed on.
///
/// The existing one where there was one; otherwise the record just created,
/// found by the address it was created with. Looked up rather than threaded
/// back, because `employee::save` answers with an `EmployeeInput` and the two
/// crates agree on the email rather than on a return shape.
async fn resolve_employee(
    pool: &PgPool,
    applicant: &Applicant,
    existing: Option<Uuid>,
) -> ServiceResult<Option<Uuid>> {
    if let Some(employee_id) = existing {
        return Ok(Some(employee_id));
    }

    match applicant.email.as_deref() {
        Some(email) => Ok(store::employee_with_email(pool, email).await?),
        None => Ok(None),
    }
}

/// Remove an application nobody was hired from.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::APPLICANTS_MANAGE)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;

    if !store::delete(pool, id).await? {
        return Ok(Submission::rejected(
            "stage",
            ApplicantError::NotMovable.message(),
        ));
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::APPLICANT, id).named(before.display_name()),
        &before,
    )
    .await;

    Ok(Submission::Saved(()))
}
