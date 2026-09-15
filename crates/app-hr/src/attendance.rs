//! What was recorded, as opposed to what was expected.
//!
//! One record per person per day, and the day it describes is read against the
//! [`crate::holiday`] calendar that was in force then. The two are separate
//! facts and the interesting cases are where they disagree: somebody in on a
//! bank holiday, and somebody missing on a day the calendar cannot speak for.
//!
//! No coordinates. See the head of `migrations/apps/hr/0004_attendance.sql`.

use chrono::{DateTime, NaiveDate, Utc};
use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::holiday::WorkingDay;

pub const MAX_NOTE_LEN: usize = 500;

/// What a day was recorded as.
///
/// Three, and no leave: leave is deliberately not built - ADR 0006 section 9 -
/// and offering it here would be a promise the schema cannot keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceStatus {
    Present,
    HalfDay,
    Absent,
}

impl AttendanceStatus {
    pub const ALL: &'static [Self] = &[Self::Present, Self::HalfDay, Self::Absent];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::HalfDay => "half_day",
            Self::Absent => "absent",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    /// Whether any of the day was worked.
    pub const fn was_worked(self) -> bool {
        matches!(self, Self::Present | Self::HalfDay)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Present => msg!("attendance.status.present"),
            Self::HalfDay => msg!("attendance.status.half_day"),
            Self::Absent => msg!("attendance.status.absent"),
        }
    }
}

/// Who or what asserted a record.
///
/// A figure somebody will be paid on should say whether a clock recorded it or
/// a manager typed it afterwards; the two are not equally good evidence, and a
/// dispute is exactly when somebody asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceSource {
    /// A clock, a terminal, a reader.
    Device,
    /// Somebody keyed it.
    Manual,
    /// It arrived in a file.
    Import,
}

impl AttendanceSource {
    pub const ALL: &'static [Self] = &[Self::Device, Self::Manual, Self::Import];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::Manual => "manual",
            Self::Import => "import",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Device => msg!("attendance.source.device"),
            Self::Manual => msg!("attendance.source.manual"),
            Self::Import => msg!("attendance.source.import"),
        }
    }
}

/// One person, one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attendance {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub on_date: NaiveDate,
    pub status: AttendanceStatus,
    /// Where anybody recorded the minutes. A day marked present after the fact
    /// has neither.
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_out_at: Option<DateTime<Utc>>,
    pub source: AttendanceSource,
    pub note: Option<String>,
}

/// A list row: the record, with who it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttendanceSummary {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub on_date: NaiveDate,
    pub status: AttendanceStatus,
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_out_at: Option<DateTime<Utc>>,
    pub source: AttendanceSource,
}

/// What one date came to, once the record and the calendar are read together.
///
/// Six answers rather than three, because the calendar and the record are
/// separate facts and the cases where they disagree are the ones anybody asks
/// about. [`Self::Unknown`] carries the same discipline as
/// [`WorkingDay::NotCovered`]: a date no calendar covers is not an absence, and
/// reporting it as one would turn a workspace that has not set up a calendar
/// into one whose staff never came to work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayOutcome {
    /// Expected in, and was.
    Present,
    /// Expected in, and was, for half of it.
    HalfDay,
    /// Expected in, and a record says they were not.
    Absent,
    /// Expected in, and nobody recorded anything.
    NotRecorded,
    /// Not expected in, and did not come.
    DayOff { name: String },
    /// Not expected in, and came anyway. What overtime is worked out from.
    WorkedDayOff { name: String },
    /// No calendar covers the date, so nothing can be said about it.
    Unknown,
}

impl DayOutcome {
    /// Read a record and a calendar together.
    ///
    /// The record is `None` for a day nobody keyed, which is the ordinary state
    /// of every future date and of every day before attendance was kept.
    pub fn resolve(record: Option<&Attendance>, day: &WorkingDay) -> Self {
        match day {
            WorkingDay::NotCovered => Self::Unknown,

            WorkingDay::Off { name, .. } => match record {
                Some(record) if record.status.was_worked() => {
                    Self::WorkedDayOff { name: name.clone() }
                }
                // A day off with an absence recorded against it is still a day
                // off: nobody was expected, so nobody was missing.
                _ => Self::DayOff { name: name.clone() },
            },

            WorkingDay::Working => match record.map(|record| record.status) {
                Some(AttendanceStatus::Present) => Self::Present,
                Some(AttendanceStatus::HalfDay) => Self::HalfDay,
                Some(AttendanceStatus::Absent) => Self::Absent,
                None => Self::NotRecorded,
            },
        }
    }

    /// Whether any of the day was worked, however it was expected to go.
    pub const fn was_worked(&self) -> bool {
        matches!(
            self,
            Self::Present | Self::HalfDay | Self::WorkedDayOff { .. }
        )
    }

    /// Whether somebody was expected and did not appear.
    ///
    /// False for [`Self::NotRecorded`] and [`Self::Unknown`]: neither is
    /// evidence of anything, and counting them would invent absences out of a
    /// workspace that has simply not keyed the week yet.
    pub const fn is_absence(&self) -> bool {
        matches!(self, Self::Absent)
    }

    pub fn label(&self) -> Message {
        match self {
            Self::Present => msg!("attendance.day.present"),
            Self::HalfDay => msg!("attendance.day.half_day"),
            Self::Absent => msg!("attendance.day.absent"),
            Self::NotRecorded => msg!("attendance.day.not_recorded"),
            Self::DayOff { .. } => msg!("attendance.day.off"),
            Self::WorkedDayOff { .. } => msg!("attendance.day.worked_day_off"),
            Self::Unknown => msg!("attendance.day.unknown"),
        }
    }
}

/// One day on a timesheet: what it came to, and the record behind it.
///
/// Assembled by the service, which is the only place that has both halves.
/// It lives here rather than there because it crosses to the browser, and
/// `phonix-services` does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimesheetDay {
    pub on_date: NaiveDate,
    pub outcome: DayOutcome,
    /// The record behind it, where somebody keyed one.
    pub record: Option<Attendance>,
    /// The shift they were expected on, where the assignment names one.
    pub shift_name: Option<String>,
    /// Whether the check-in was inside the grace window.
    ///
    /// `None` where either half is missing - no shift on the assignment, or
    /// no check-in time on the record. A day marked present after the fact
    /// has no minutes to judge, and saying "on time" about it would be an
    /// answer nobody can support.
    pub arrival: Option<crate::shift::Arrival>,
}

/// A record being written on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttendanceInput {
    pub id: Option<Uuid>,
    pub employee_id: Option<Uuid>,
    pub on_date: Option<NaiveDate>,
    pub status: AttendanceStatus,
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_out_at: Option<DateTime<Utc>>,
    pub source: AttendanceSource,
    pub note: String,
}

impl AttendanceInput {
    pub fn blank(on: NaiveDate) -> Self {
        Self {
            id: None,
            employee_id: None,
            on_date: Some(on),
            status: AttendanceStatus::Present,
            checked_in_at: None,
            checked_out_at: None,
            // Somebody is looking at a form, so somebody is keying it.
            source: AttendanceSource::Manual,
            note: String::new(),
        }
    }

    pub fn from_record(record: &Attendance) -> Self {
        Self {
            id: Some(record.id),
            employee_id: Some(record.employee_id),
            on_date: Some(record.on_date),
            status: record.status,
            checked_in_at: record.checked_in_at,
            checked_out_at: record.checked_out_at,
            source: record.source,
            note: record.note.clone().unwrap_or_default(),
        }
    }

    pub fn check(&self) -> Result<CheckedAttendance, AttendanceError> {
        let Some(employee_id) = self.employee_id else {
            return Err(AttendanceError::EmployeeRequired);
        };

        let Some(on_date) = self.on_date else {
            return Err(AttendanceError::DateRequired);
        };

        if let (Some(in_at), Some(out_at)) = (self.checked_in_at, self.checked_out_at)
            && out_at < in_at
        {
            return Err(AttendanceError::OutBeforeIn);
        }

        // An absence with a clock time on it is two statements that disagree,
        // and the times are the ones somebody would believe.
        if self.status == AttendanceStatus::Absent
            && (self.checked_in_at.is_some() || self.checked_out_at.is_some())
        {
            return Err(AttendanceError::AbsentWithTimes);
        }

        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(AttendanceError::NoteTooLong);
        }

        Ok(CheckedAttendance {
            id: self.id,
            employee_id,
            on_date,
            status: self.status,
            checked_in_at: self.checked_in_at,
            checked_out_at: self.checked_out_at,
            source: self.source,
            note: crate::non_empty(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedAttendance {
    pub id: Option<Uuid>,
    pub employee_id: Uuid,
    pub on_date: NaiveDate,
    pub status: AttendanceStatus,
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_out_at: Option<DateTime<Utc>>,
    pub source: AttendanceSource,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AttendanceError {
    #[error("a record needs somebody it is about")]
    EmployeeRequired,
    #[error("a record needs a date")]
    DateRequired,
    #[error("that day is already recorded for this person")]
    DayRecordedTwice,
    #[error("checking out cannot come before checking in")]
    OutBeforeIn,
    #[error("an absence cannot carry clock times")]
    AbsentWithTimes,
    #[error("a note is at most 500 characters")]
    NoteTooLong,
    #[error("that record is not here any more")]
    Gone,
}

impl AttendanceError {
    pub fn field(self) -> &'static str {
        match self {
            Self::EmployeeRequired => "employee_id",
            Self::DateRequired | Self::DayRecordedTwice => "on_date",
            Self::OutBeforeIn | Self::AbsentWithTimes => "checked_in_at",
            Self::NoteTooLong => "note",
            Self::Gone => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::EmployeeRequired => msg!("attendance.error.employee_required"),
            Self::DateRequired => msg!("attendance.error.date_required"),
            Self::DayRecordedTwice => msg!("attendance.error.day_recorded_twice"),
            Self::OutBeforeIn => msg!("attendance.error.out_before_in"),
            Self::AbsentWithTimes => msg!("attendance.error.absent_with_times"),
            Self::NoteTooLong => msg!("attendance.error.note_too_long"),
            Self::Gone => msg!("attendance.error.gone"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(month: u32, day_of: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day_of).expect("a real date")
    }

    fn record(status: AttendanceStatus) -> Attendance {
        Attendance {
            id: Uuid::nil(),
            employee_id: Uuid::nil(),
            on_date: day(1, 5),
            status,
            checked_in_at: None,
            checked_out_at: None,
            source: AttendanceSource::Manual,
            note: None,
        }
    }

    fn off() -> WorkingDay {
        WorkingDay::Off {
            name: "Christmas Day".to_owned(),
            is_weekly_off: false,
        }
    }

    /// The whole point of resolving against the calendar: a day nobody was
    /// expected on is not an absence, however the record reads.
    #[test]
    fn nobody_is_absent_on_a_day_off() {
        assert_eq!(
            DayOutcome::resolve(None, &off()),
            DayOutcome::DayOff {
                name: "Christmas Day".to_owned()
            }
        );

        let marked = record(AttendanceStatus::Absent);
        assert!(!DayOutcome::resolve(Some(&marked), &off()).is_absence());
    }

    /// Coming in on a day off is its own answer, because it is what overtime is
    /// worked out from and it must not read as an ordinary day.
    #[test]
    fn working_a_day_off_is_neither_a_normal_day_nor_a_day_off() {
        let present = record(AttendanceStatus::Present);
        let outcome = DayOutcome::resolve(Some(&present), &off());

        assert_eq!(
            outcome,
            DayOutcome::WorkedDayOff {
                name: "Christmas Day".to_owned()
            }
        );
        assert!(outcome.was_worked());
        assert!(!outcome.is_absence());
    }

    /// A missing record is not an absence, and a date the calendar cannot speak
    /// for is not one either. Both would otherwise invent absences from a
    /// workspace that has simply not keyed the week.
    #[test]
    fn neither_silence_nor_a_missing_calendar_is_an_absence() {
        assert_eq!(
            DayOutcome::resolve(None, &WorkingDay::Working),
            DayOutcome::NotRecorded
        );
        assert!(!DayOutcome::resolve(None, &WorkingDay::Working).is_absence());

        assert_eq!(
            DayOutcome::resolve(None, &WorkingDay::NotCovered),
            DayOutcome::Unknown
        );
        assert!(!DayOutcome::resolve(None, &WorkingDay::NotCovered).is_absence());

        // Even a recorded absence says nothing on a date no calendar covers.
        let marked = record(AttendanceStatus::Absent);
        assert_eq!(
            DayOutcome::resolve(Some(&marked), &WorkingDay::NotCovered),
            DayOutcome::Unknown
        );
    }

    #[test]
    fn a_recorded_absence_on_a_working_day_is_the_one_that_counts() {
        let marked = record(AttendanceStatus::Absent);
        let outcome = DayOutcome::resolve(Some(&marked), &WorkingDay::Working);

        assert_eq!(outcome, DayOutcome::Absent);
        assert!(outcome.is_absence());
        assert!(!outcome.was_worked());
    }

    #[test]
    fn half_a_day_is_worked_and_is_not_an_absence() {
        let half = record(AttendanceStatus::HalfDay);
        let outcome = DayOutcome::resolve(Some(&half), &WorkingDay::Working);

        assert_eq!(outcome, DayOutcome::HalfDay);
        assert!(outcome.was_worked());
        assert!(!outcome.is_absence());
    }

    #[test]
    fn an_absence_cannot_carry_clock_times() {
        let draft = AttendanceInput {
            employee_id: Some(Uuid::nil()),
            status: AttendanceStatus::Absent,
            checked_in_at: Some(DateTime::<Utc>::MIN_UTC),
            ..AttendanceInput::blank(day(1, 5))
        };

        assert_eq!(draft.check(), Err(AttendanceError::AbsentWithTimes));
    }

    #[test]
    fn checking_out_before_checking_in_is_refused() {
        let draft = AttendanceInput {
            employee_id: Some(Uuid::nil()),
            checked_in_at: Some(DateTime::<Utc>::MAX_UTC),
            checked_out_at: Some(DateTime::<Utc>::MIN_UTC),
            ..AttendanceInput::blank(day(1, 5))
        };

        assert_eq!(draft.check(), Err(AttendanceError::OutBeforeIn));
    }

    #[test]
    fn every_status_and_source_survives_a_round_trip_through_its_column() {
        for status in AttendanceStatus::ALL {
            assert_eq!(AttendanceStatus::parse(status.as_str()), Some(*status));
        }
        for source in AttendanceSource::ALL {
            assert_eq!(AttendanceSource::parse(source.as_str()), Some(*source));
        }

        assert_eq!(AttendanceStatus::parse("on_leave"), None);
        assert_eq!(AttendanceSource::parse("telepathy"), None);
    }
}
