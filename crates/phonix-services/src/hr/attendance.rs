//! Attendance: what was recorded, read against the calendar that was in force.
//!
//! The records and the calendar are separate facts and this is where they are
//! put together. [`timesheet`] is the reason the module exists: a list of
//! records answers "what was keyed", and a person asking about a month wants
//! "what did each day come to", which is a different question on every day
//! nobody keyed anything.

use app_hr::attendance::{
    Attendance, AttendanceError, AttendanceInput, AttendanceSummary, DayOutcome, TimesheetDay,
};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::permissions;
use phonix_db::error::DbError;
use phonix_db::hr::attendance as store;
use phonix_db::hr::holiday as calendar;
use phonix_db::hr::shift as shifts;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// The widest span a timesheet may ask for.
///
/// A year and a day. The store will happily walk a decade, so the cap lives
/// here rather than there: a screen asks for a month, an export asks for a
/// year, and anything wider is a report nobody has designed yet.
pub const MAX_SPAN_DAYS: i64 = 366;

/// The workspace's own zone, or UTC where the stored name is not one this
/// build can resolve.
///
/// The fallback is what `phonix_core::locale::timezone` says it should be:
/// the name is shape-checked when it is stored and the tables live only here,
/// so a name that passes there and fails here resolves to UTC with a warning
/// rather than failing the request. A timesheet that loses its verdicts is a
/// worse answer than one whose verdicts are an hour out, and the log says
/// which happened.
async fn workspace_zone(pool: &PgPool) -> ServiceResult<chrono_tz::Tz> {
    let profile = phonix_db::organization::load(pool).await?;
    let named = profile.profile.timezone.as_str().to_owned();

    match named.parse::<chrono_tz::Tz>() {
        Ok(zone) => Ok(zone),
        Err(_) => {
            tracing::warn!(
                timezone = %named,
                "the workspace time zone is not one this build can resolve; reading times as UTC",
            );
            Ok(chrono_tz::UTC)
        }
    }
}

/// Everybody's records on one date.
pub async fn on_date(
    pool: &PgPool,
    caller: &Caller,
    date: NaiveDate,
) -> ServiceResult<Vec<AttendanceSummary>> {
    caller.require(permissions::ATTENDANCE)?;
    Ok(store::on_date(pool, date).await?)
}

/// One person's month, resolved day by day.
///
/// Two statements and a merge rather than one join: the dates come from
/// `generate_series` on the calendar side and the records come from their own
/// table, and a join between them would still have to invent the days that have
/// neither - which are the days the answer is about.
pub async fn timesheet(
    pool: &PgPool,
    caller: &Caller,
    employee_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> ServiceResult<Vec<TimesheetDay>> {
    caller.require(permissions::ATTENDANCE)?;

    if to < from {
        return Ok(Vec::new());
    }

    if (to - from).num_days() > MAX_SPAN_DAYS {
        return Err(ServiceError::rejected(
            "from",
            phonix_core::msg!("attendance.error.span_too_long"),
        ));
    }

    let days = calendar::working_days(pool, employee_id, from, to).await?;
    let records = store::for_employee(pool, employee_id, from, to).await?;
    let rostered = shifts::for_span(pool, employee_id, from, to).await?;
    let zone = workspace_zone(pool).await?;

    Ok(days
        .into_iter()
        .map(|(on_date, working)| {
            let record = records
                .iter()
                .find(|record| record.on_date == on_date)
                .cloned();

            let shift = rostered
                .iter()
                .find(|(day, _)| *day == on_date)
                .and_then(|(_, shift)| shift.as_ref());

            // Both halves or nothing: a shift with no check-in has no minutes
            // to judge, and a check-in with no shift has nothing to judge them
            // against.
            let arrival = shift
                .zip(record.as_ref().and_then(|r| r.checked_in_at))
                .map(|(shift, at)| shift.arrival(at.with_timezone(&zone).time()));

            TimesheetDay {
                outcome: DayOutcome::resolve(record.as_ref(), &working),
                on_date,
                record,
                shift_name: shift.map(|shift| shift.name.clone()),
                arrival,
            }
        })
        .collect())
}

pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Attendance> {
    caller.require(permissions::ATTENDANCE)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("attendance", AttendanceError::Gone.message()))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<AttendanceInput> {
    Ok(AttendanceInput::from_record(
        &detail(pool, caller, id).await?,
    ))
}

/// A blank record for one person on one day.
pub fn blank(caller: &Caller, employee_id: Uuid, on: NaiveDate) -> ServiceResult<AttendanceInput> {
    caller.require(permissions::ATTENDANCE_RECORD)?;

    Ok(AttendanceInput {
        employee_id: Some(employee_id),
        ..AttendanceInput::blank(on)
    })
}

pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: AttendanceInput,
) -> ServiceResult<Submission<AttendanceInput>> {
    caller.require(permissions::ATTENDANCE_RECORD)?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let id = match store::save(pool, &checked, caller.user_id()).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Ok(Submission::rejected("id", AttendanceError::Gone.message()));
        }
        // The unique index on (employee, date). Caught as a conflict rather
        // than as a failure: two people keying the same day is ordinary, and
        // the second one should be told which day rather than shown a trace.
        Err(DbError::CodeExists { .. }) => {
            return Ok(Submission::rejected(
                "on_date",
                AttendanceError::DayRecordedTwice.message(),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    let stored = AttendanceInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::ATTENDANCE, id)
        .fact("status", stored.status.as_str())
        .fact("source", stored.source.as_str());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Remove a record keyed in error.
///
/// Deleting rather than marking: an attendance row is an assertion somebody
/// made, and a wrong one is best withdrawn. What was asserted and by whom is in
/// the audit trail either way.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Submission<()>> {
    caller.require(permissions::ATTENDANCE_RECORD)?;
    acting_user(caller)?;

    let before = detail(pool, caller, id).await?;
    let removed = store::delete(pool, id).await?;

    if removed {
        audit::deleted(pool, caller, Target::new(kinds::ATTENDANCE, id), &before).await;
    }

    Ok(Submission::Saved(()))
}
