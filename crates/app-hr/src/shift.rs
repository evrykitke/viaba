//! What somebody was expected to work.
//!
//! The shift is the expectation and [`crate::attendance`] is the observation.
//! Lateness is the difference between them rather than a column either one
//! carries, which is how Frappe HR separates them and why an attendance record
//! alone can say somebody was present but not whether they were on time.
//!
//! Which shift applies to somebody is a field on their
//! [`crate::employee::Assignment`], like their department and their calendar.

use chrono::{NaiveTime, TimeDelta};
use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_SHIFT_CODE_LEN: usize = 40;
pub const MAX_SHIFT_NAME_LEN: usize = 120;

/// The longest grace either end may allow, in minutes.
///
/// A day. A grace window longer than the shift it forgives is not a grace
/// window, and the column refuses it too.
pub const MAX_GRACE_MINUTES: i64 = 1440;

/// One shift: when it runs, and how much lateness still counts as on time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftType {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    /// Local clock times where the person works, not instants. Turning one into
    /// an instant needs the workspace's zone and a date.
    pub starts_at: NaiveTime,
    pub ends_at: NaiveTime,
    pub late_grace_minutes: i64,
    pub early_exit_grace_minutes: i64,
    pub is_active: bool,
}

impl ShiftType {
    /// Whether the shift runs past midnight.
    ///
    /// A night shift ends before it starts, which is ordinary rather than an
    /// error - half the industries that would use this run one.
    pub fn crosses_midnight(&self) -> bool {
        self.ends_at <= self.starts_at
    }

    /// The last moment somebody may arrive and still be on time.
    ///
    /// Wraps past midnight where the grace window runs over it, which is what
    /// `overflowing_add_signed` is being asked for: a shift starting at 23:50
    /// with fifteen minutes of grace is on time until 00:05.
    pub fn on_time_until(&self) -> NaiveTime {
        let grace = TimeDelta::try_minutes(self.late_grace_minutes).unwrap_or_else(TimeDelta::zero);
        self.starts_at.overflowing_add_signed(grace).0
    }

    /// What an arrival at `at` came to, as a local clock time.
    ///
    /// Takes a clock time rather than an instant deliberately: the caller is
    /// the only one that knows the workspace's zone and the date, and freezing
    /// either into this crate would be wrong twice a year.
    pub fn arrival(&self, at: NaiveTime) -> Arrival {
        let late_by = minutes_from(self.starts_at, at);

        if late_by <= self.late_grace_minutes {
            Arrival::OnTime
        } else {
            Arrival::Late {
                by_minutes: late_by,
            }
        }
    }

    /// What a departure at `at` came to, as a local clock time.
    pub fn departure(&self, at: NaiveTime) -> Departure {
        let early_by = minutes_from(at, self.ends_at);

        if early_by <= self.early_exit_grace_minutes {
            Departure::Full
        } else {
            Departure::Early {
                by_minutes: early_by,
            }
        }
    }
}

/// Minutes from `from` to `to`, forwards round the clock.
///
/// Never negative: arriving before the shift starts is nought minutes late, not
/// minus twenty, and a caller comparing against a grace window should not have
/// to think about the sign. Times more than twelve hours apart are read as the
/// short way round, so an arrival at 08:55 for a 09:00 shift is five minutes
/// early rather than twenty-three hours and fifty-five minutes late.
fn minutes_from(from: NaiveTime, to: NaiveTime) -> i64 {
    let forwards = (to - from).num_minutes();

    if forwards < 0 {
        // `to` is earlier in the day: arrived before the shift, left after it.
        0
    } else if forwards > 12 * 60 {
        0
    } else {
        forwards
    }
}

/// What an arrival came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrival {
    OnTime,
    Late { by_minutes: i64 },
}

impl Arrival {
    pub const fn is_late(self) -> bool {
        matches!(self, Self::Late { .. })
    }

    pub fn label(self) -> Message {
        match self {
            Self::OnTime => msg!("shifts.arrival.on_time"),
            Self::Late { .. } => msg!("shifts.arrival.late"),
        }
    }
}

/// What a departure came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Departure {
    Full,
    Early { by_minutes: i64 },
}

impl Departure {
    pub const fn is_early(self) -> bool {
        matches!(self, Self::Early { .. })
    }

    pub fn label(self) -> Message {
        match self {
            Self::Full => msg!("shifts.departure.full"),
            Self::Early { .. } => msg!("shifts.departure.early"),
        }
    }
}

/// A list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftTypeSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub starts_at: NaiveTime,
    pub ends_at: NaiveTime,
    pub late_grace_minutes: i64,
    pub is_active: bool,
    /// How many people are currently on it.
    pub headcount: i64,
}

/// A shift being written on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShiftTypeInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub starts_at: Option<NaiveTime>,
    pub ends_at: Option<NaiveTime>,
    pub late_grace_minutes: i64,
    pub early_exit_grace_minutes: i64,
    pub is_active: bool,
}

impl ShiftTypeInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            starts_at: None,
            ends_at: None,
            late_grace_minutes: 0,
            early_exit_grace_minutes: 0,
            is_active: true,
        }
    }

    pub fn from_shift(shift: &ShiftType) -> Self {
        Self {
            id: Some(shift.id),
            code: shift.code.clone(),
            name: shift.name.clone(),
            starts_at: Some(shift.starts_at),
            ends_at: Some(shift.ends_at),
            late_grace_minutes: shift.late_grace_minutes,
            early_exit_grace_minutes: shift.early_exit_grace_minutes,
            is_active: shift.is_active,
        }
    }

    pub fn check(&self) -> Result<CheckedShiftType, ShiftError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ShiftError::NameRequired);
        }
        if name.chars().count() > MAX_SHIFT_NAME_LEN {
            return Err(ShiftError::NameTooLong);
        }

        let code = self.code.trim();
        if code.is_empty() {
            return Err(ShiftError::CodeRequired);
        }
        if code.chars().count() > MAX_SHIFT_CODE_LEN {
            return Err(ShiftError::CodeTooLong);
        }
        if !crate::is_code_shaped(code) {
            return Err(ShiftError::CodeMalformed);
        }

        let (Some(starts_at), Some(ends_at)) = (self.starts_at, self.ends_at) else {
            return Err(ShiftError::HoursRequired);
        };

        // No check that the end is after the start. A night shift ends before
        // it starts, and refusing that would refuse the case this is for.
        if starts_at == ends_at {
            return Err(ShiftError::ZeroLength);
        }

        for grace in [self.late_grace_minutes, self.early_exit_grace_minutes] {
            if !(0..=MAX_GRACE_MINUTES).contains(&grace) {
                return Err(ShiftError::GraceOutOfRange);
            }
        }

        Ok(CheckedShiftType {
            id: self.id,
            code: code.to_owned(),
            name: name.to_owned(),
            starts_at,
            ends_at,
            late_grace_minutes: self.late_grace_minutes,
            early_exit_grace_minutes: self.early_exit_grace_minutes,
            is_active: self.is_active,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedShiftType {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub starts_at: NaiveTime,
    pub ends_at: NaiveTime,
    pub late_grace_minutes: i64,
    pub early_exit_grace_minutes: i64,
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ShiftError {
    #[error("a shift needs a name")]
    NameRequired,
    #[error("a name is at most 120 characters")]
    NameTooLong,
    #[error("a shift needs a code")]
    CodeRequired,
    #[error("a code is at most 40 characters")]
    CodeTooLong,
    #[error("a code may hold only letters, digits, hyphens and underscores")]
    CodeMalformed,
    #[error("that code is already in use")]
    CodeTaken,
    #[error("a shift needs the hours it runs")]
    HoursRequired,
    #[error("a shift that ends when it starts is no shift")]
    ZeroLength,
    #[error("a grace window is between nothing and a day")]
    GraceOutOfRange,
    #[error("that shift is still assigned to somebody")]
    StillUsed,
    #[error("that shift is not here any more")]
    Gone,
}

impl ShiftError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NameRequired | Self::NameTooLong => "name",
            Self::CodeRequired | Self::CodeTooLong | Self::CodeMalformed | Self::CodeTaken => {
                "code"
            }
            Self::HoursRequired | Self::ZeroLength => "starts_at",
            Self::GraceOutOfRange => "late_grace_minutes",
            Self::StillUsed | Self::Gone => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NameRequired => msg!("shifts.error.name_required"),
            Self::NameTooLong => msg!("shifts.error.name_too_long"),
            Self::CodeRequired => msg!("shifts.error.code_required"),
            Self::CodeTooLong => msg!("shifts.error.code_too_long"),
            Self::CodeMalformed => msg!("shifts.error.code_malformed"),
            Self::CodeTaken => msg!("shifts.error.code_taken"),
            Self::HoursRequired => msg!("shifts.error.hours_required"),
            Self::ZeroLength => msg!("shifts.error.zero_length"),
            Self::GraceOutOfRange => msg!("shifts.error.grace_out_of_range"),
            Self::StillUsed => msg!("shifts.error.still_used"),
            Self::Gone => msg!("shifts.error.gone"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).expect("a real time")
    }

    fn day_shift(grace: i64) -> ShiftType {
        ShiftType {
            id: Uuid::nil(),
            code: "DAY".to_owned(),
            name: "Day shift".to_owned(),
            starts_at: at(9, 0),
            ends_at: at(17, 0),
            late_grace_minutes: grace,
            early_exit_grace_minutes: grace,
            is_active: true,
        }
    }

    #[test]
    fn within_the_grace_window_is_on_time_and_past_it_is_not() {
        let shift = day_shift(10);

        assert_eq!(shift.arrival(at(9, 0)), Arrival::OnTime);
        assert_eq!(shift.arrival(at(9, 10)), Arrival::OnTime);
        assert_eq!(shift.arrival(at(9, 11)), Arrival::Late { by_minutes: 11 });
        assert!(shift.arrival(at(9, 45)).is_late());
    }

    /// Arriving early is not lateness of a negative number of minutes, which is
    /// what a naive subtraction gives and what a grace comparison would then
    /// get wrong in the other direction.
    #[test]
    fn arriving_before_the_shift_is_never_late() {
        let shift = day_shift(0);

        assert_eq!(shift.arrival(at(8, 30)), Arrival::OnTime);
        assert_eq!(shift.arrival(at(6, 0)), Arrival::OnTime);
    }

    #[test]
    fn leaving_before_the_end_is_early_only_past_the_grace() {
        let shift = day_shift(5);

        assert_eq!(shift.departure(at(17, 0)), Departure::Full);
        assert_eq!(shift.departure(at(16, 55)), Departure::Full);
        assert_eq!(
            shift.departure(at(16, 30)),
            Departure::Early { by_minutes: 30 }
        );
        // Staying late is a full shift, not a negative early departure.
        assert_eq!(shift.departure(at(18, 0)), Departure::Full);
    }

    /// A night shift ends before it starts. Refusing that would refuse the case
    /// the grace window is most often used for.
    #[test]
    fn a_night_shift_is_allowed_and_knows_it_crosses_midnight() {
        let nights = ShiftType {
            starts_at: at(22, 0),
            ends_at: at(6, 0),
            ..day_shift(15)
        };

        assert!(nights.crosses_midnight());
        assert!(!day_shift(0).crosses_midnight());

        assert_eq!(nights.arrival(at(22, 10)), Arrival::OnTime);
        assert_eq!(nights.arrival(at(22, 30)), Arrival::Late { by_minutes: 30 });
    }

    /// A grace window that runs over midnight still names a clock time.
    #[test]
    fn a_grace_window_wraps_past_midnight() {
        let late_start = ShiftType {
            starts_at: at(23, 50),
            ends_at: at(7, 0),
            ..day_shift(15)
        };

        assert_eq!(late_start.on_time_until(), at(0, 5));
    }

    #[test]
    fn a_shift_that_ends_when_it_starts_is_refused() {
        let draft = ShiftTypeInput {
            code: "DAY".to_owned(),
            name: "Day shift".to_owned(),
            starts_at: Some(at(9, 0)),
            ends_at: Some(at(9, 0)),
            ..ShiftTypeInput::blank()
        };

        assert_eq!(draft.check(), Err(ShiftError::ZeroLength));
    }

    #[test]
    fn a_grace_window_longer_than_a_day_is_refused() {
        let draft = ShiftTypeInput {
            code: "DAY".to_owned(),
            name: "Day shift".to_owned(),
            starts_at: Some(at(9, 0)),
            ends_at: Some(at(17, 0)),
            late_grace_minutes: MAX_GRACE_MINUTES + 1,
            ..ShiftTypeInput::blank()
        };

        assert_eq!(draft.check(), Err(ShiftError::GraceOutOfRange));
    }
}
