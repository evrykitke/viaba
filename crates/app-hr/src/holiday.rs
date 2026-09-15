//! The days nobody is expected to work.
//!
//! A named calendar covering a span, with a dated row per day off. Which one
//! applies to somebody is a field on their [`crate::employee::Assignment`], for
//! the reason every other fact about what somebody does is: a person who moves
//! office changes calendar on a date, and last year's attendance still has to
//! be read against last year's calendar.

use chrono::{Datelike, NaiveDate, Weekday};
use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_LIST_CODE_LEN: usize = 40;
pub const MAX_LIST_NAME_LEN: usize = 120;
pub const MAX_HOLIDAY_NAME_LEN: usize = 120;

/// The longest span one list may cover.
///
/// Five years. A list is a year's calendar in Frappe HR and in practice, and
/// the cap is here because [`HolidayListInput::weekly_offs`] walks the span a
/// day at a time: a typo of 2925 for 2025 would otherwise generate rows until
/// the request timed out.
pub const MAX_SPAN_DAYS: i64 = 1826;

/// One calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolidayList {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub valid_from: NaiveDate,
    pub valid_to: NaiveDate,
    pub is_active: bool,
    /// The days off, earliest first.
    pub holidays: Vec<Holiday>,
}

impl HolidayList {
    /// Whether this list has anything to say about `date`.
    ///
    /// Outside its span it does not, which is not the same as saying the day is
    /// worked: see [`WorkingDay`].
    pub fn covers(&self, date: NaiveDate) -> bool {
        date >= self.valid_from && date <= self.valid_to
    }

    /// What this calendar says about one date.
    pub fn on(&self, date: NaiveDate) -> WorkingDay {
        if !self.covers(date) {
            return WorkingDay::NotCovered;
        }

        match self
            .holidays
            .iter()
            .find(|holiday| holiday.observed_on == date)
        {
            Some(holiday) => WorkingDay::Off {
                name: holiday.name.clone(),
                is_weekly_off: holiday.is_weekly_off,
            },
            None => WorkingDay::Working,
        }
    }
}

/// One day off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holiday {
    pub id: Uuid,
    pub observed_on: NaiveDate,
    pub name: String,
    /// Generated from the weekly pattern rather than named by somebody.
    pub is_weekly_off: bool,
}

/// What a calendar says about a date.
///
/// Three answers rather than a boolean, because "this list does not cover that
/// date" is a different fact from "that date is worked", and a caller that
/// conflates them reports a missing calendar as a full working year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingDay {
    Working,
    Off { name: String, is_weekly_off: bool },
    NotCovered,
}

impl WorkingDay {
    /// Whether somebody is expected in.
    ///
    /// A date the list does not cover is not a working day by this answer: the
    /// calendar was asked and could not say, and counting it as worked is the
    /// assumption that quietly turns an empty list into a full year.
    pub const fn is_working(&self) -> bool {
        matches!(self, Self::Working)
    }

    pub fn label(&self) -> Message {
        match self {
            Self::Working => msg!("holidays.day.working"),
            Self::Off { .. } => msg!("holidays.day.off"),
            Self::NotCovered => msg!("holidays.day.not_covered"),
        }
    }
}

/// A list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolidayListSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub valid_from: NaiveDate,
    pub valid_to: NaiveDate,
    pub is_active: bool,
    pub holiday_count: i64,
    /// How many people are currently assigned to it.
    pub headcount: i64,
}

/// A calendar being written on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolidayListInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
    pub is_active: bool,
    pub holidays: Vec<HolidayInput>,
}

impl HolidayListInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            valid_from: None,
            valid_to: None,
            is_active: true,
            holidays: Vec::new(),
        }
    }

    pub fn from_list(list: &HolidayList) -> Self {
        Self {
            id: Some(list.id),
            code: list.code.clone(),
            name: list.name.clone(),
            valid_from: Some(list.valid_from),
            valid_to: Some(list.valid_to),
            is_active: list.is_active,
            holidays: list
                .holidays
                .iter()
                .map(|holiday| HolidayInput {
                    observed_on: Some(holiday.observed_on),
                    name: holiday.name.clone(),
                    is_weekly_off: holiday.is_weekly_off,
                })
                .collect(),
        }
    }

    /// Every occurrence of `weekday` in the span, as rows ready to add.
    ///
    /// What Frappe HR's "add weekly holidays" button does. Generated rather
    /// than derived at read time, so that changing next year's pattern does not
    /// rewrite what last year's calendar said. Returns nothing where the span
    /// is not set or does not make sense: the caller is a button, and a button
    /// pressed too early should do nothing rather than refuse.
    pub fn weekly_offs(&self, weekday: Weekday, called: &str) -> Vec<HolidayInput> {
        let (Some(from), Some(to)) = (self.valid_from, self.valid_to) else {
            return Vec::new();
        };

        if to < from || (to - from).num_days() > MAX_SPAN_DAYS {
            return Vec::new();
        }

        from.iter_days()
            .take_while(|day| *day <= to)
            .filter(|day| day.weekday() == weekday)
            .map(|day| HolidayInput {
                observed_on: Some(day),
                name: called.to_owned(),
                is_weekly_off: true,
            })
            .collect()
    }

    pub fn check(&self) -> Result<CheckedHolidayList, HolidayError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(HolidayError::NameRequired);
        }
        if name.chars().count() > MAX_LIST_NAME_LEN {
            return Err(HolidayError::NameTooLong);
        }

        let code = self.code.trim();
        if code.is_empty() {
            return Err(HolidayError::CodeRequired);
        }
        if code.chars().count() > MAX_LIST_CODE_LEN {
            return Err(HolidayError::CodeTooLong);
        }
        if !crate::is_code_shaped(code) {
            return Err(HolidayError::CodeMalformed);
        }

        let (Some(valid_from), Some(valid_to)) = (self.valid_from, self.valid_to) else {
            return Err(HolidayError::SpanRequired);
        };

        if valid_to < valid_from {
            return Err(HolidayError::SpanBackwards);
        }
        if (valid_to - valid_from).num_days() > MAX_SPAN_DAYS {
            return Err(HolidayError::SpanTooLong);
        }

        let mut holidays: Vec<CheckedHoliday> = Vec::with_capacity(self.holidays.len());

        for holiday in &self.holidays {
            let Some(observed_on) = holiday.observed_on else {
                return Err(HolidayError::DateRequired);
            };

            if observed_on < valid_from || observed_on > valid_to {
                return Err(HolidayError::DateOutsideSpan);
            }

            let called = holiday.name.trim();
            if called.is_empty() {
                return Err(HolidayError::HolidayNameRequired);
            }
            if called.chars().count() > MAX_HOLIDAY_NAME_LEN {
                return Err(HolidayError::HolidayNameTooLong);
            }

            // The unique index refuses this too. Caught here so the screen can
            // say which day, rather than handing back a constraint violation.
            if holidays.iter().any(|kept| kept.observed_on == observed_on) {
                return Err(HolidayError::DayTwice);
            }

            holidays.push(CheckedHoliday {
                observed_on,
                name: called.to_owned(),
                is_weekly_off: holiday.is_weekly_off,
            });
        }

        holidays.sort_by_key(|holiday| holiday.observed_on);

        Ok(CheckedHolidayList {
            id: self.id,
            code: code.to_owned(),
            name: name.to_owned(),
            valid_from,
            valid_to,
            is_active: self.is_active,
            holidays,
        })
    }
}

/// One day off, being written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolidayInput {
    pub observed_on: Option<NaiveDate>,
    pub name: String,
    pub is_weekly_off: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedHolidayList {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub valid_from: NaiveDate,
    pub valid_to: NaiveDate,
    pub is_active: bool,
    /// Earliest first, and no date twice.
    pub holidays: Vec<CheckedHoliday>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedHoliday {
    pub observed_on: NaiveDate,
    pub name: String,
    pub is_weekly_off: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HolidayError {
    #[error("a calendar needs a name")]
    NameRequired,
    #[error("a name is at most 120 characters")]
    NameTooLong,
    #[error("a calendar needs a code")]
    CodeRequired,
    #[error("a code is at most 40 characters")]
    CodeTooLong,
    #[error("a code may hold only letters, digits, hyphens and underscores")]
    CodeMalformed,
    #[error("that code is already in use")]
    CodeTaken,
    #[error("a calendar needs the span it covers")]
    SpanRequired,
    #[error("that span ends before it starts")]
    SpanBackwards,
    #[error("a calendar covers at most five years")]
    SpanTooLong,
    #[error("a day off needs a date")]
    DateRequired,
    #[error("that day is outside the span this calendar covers")]
    DateOutsideSpan,
    #[error("a day off needs a name")]
    HolidayNameRequired,
    #[error("a name is at most 120 characters")]
    HolidayNameTooLong,
    #[error("that day is on the calendar twice")]
    DayTwice,
    #[error("that calendar is still assigned to somebody")]
    StillUsed,
}

impl HolidayError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NameRequired | Self::NameTooLong => "name",
            Self::CodeRequired | Self::CodeTooLong | Self::CodeMalformed | Self::CodeTaken => {
                "code"
            }
            Self::SpanRequired | Self::SpanBackwards | Self::SpanTooLong => "valid_from",
            Self::DateRequired
            | Self::DateOutsideSpan
            | Self::HolidayNameRequired
            | Self::HolidayNameTooLong
            | Self::DayTwice => "holidays",
            Self::StillUsed => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NameRequired => msg!("holidays.error.name_required"),
            Self::NameTooLong => msg!("holidays.error.name_too_long"),
            Self::CodeRequired => msg!("holidays.error.code_required"),
            Self::CodeTooLong => msg!("holidays.error.code_too_long"),
            Self::CodeMalformed => msg!("holidays.error.code_malformed"),
            Self::CodeTaken => msg!("holidays.error.code_taken"),
            Self::SpanRequired => msg!("holidays.error.span_required"),
            Self::SpanBackwards => msg!("holidays.error.span_backwards"),
            Self::SpanTooLong => msg!("holidays.error.span_too_long"),
            Self::DateRequired => msg!("holidays.error.date_required"),
            Self::DateOutsideSpan => msg!("holidays.error.date_outside_span"),
            Self::HolidayNameRequired => msg!("holidays.error.holiday_name_required"),
            Self::HolidayNameTooLong => msg!("holidays.error.holiday_name_too_long"),
            Self::DayTwice => msg!("holidays.error.day_twice"),
            Self::StillUsed => msg!("holidays.error.still_used"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).expect("a real date")
    }

    fn list(holidays: Vec<Holiday>) -> HolidayList {
        HolidayList {
            id: Uuid::nil(),
            code: "UK-2026".to_owned(),
            name: "United Kingdom 2026".to_owned(),
            valid_from: day(1, 1),
            valid_to: day(12, 31),
            is_active: true,
            holidays,
        }
    }

    fn holiday(month: u32, day_of: u32, name: &str) -> Holiday {
        Holiday {
            id: Uuid::nil(),
            observed_on: day(month, day_of),
            name: name.to_owned(),
            is_weekly_off: false,
        }
    }

    /// The three answers are three, and the caller can tell them apart.
    #[test]
    fn a_date_outside_the_span_is_not_a_working_day_and_not_a_holiday_either() {
        let list = list(vec![holiday(1, 1, "New Year")]);

        assert_eq!(list.on(day(1, 2)), WorkingDay::Working);
        assert!(list.on(day(1, 2)).is_working());

        let outside = NaiveDate::from_ymd_opt(2027, 3, 1).expect("a real date");
        assert_eq!(list.on(outside), WorkingDay::NotCovered);

        // The one that matters: not covered is not worked. A caller counting
        // working days against a calendar that has run out must not be told
        // every day of the missing year was worked.
        assert!(!list.on(outside).is_working());
    }

    #[test]
    fn a_named_day_comes_back_with_its_name() {
        let list = list(vec![holiday(12, 25, "Christmas Day")]);

        assert_eq!(
            list.on(day(12, 25)),
            WorkingDay::Off {
                name: "Christmas Day".to_owned(),
                is_weekly_off: false,
            }
        );
    }

    #[test]
    fn weekly_offs_are_every_one_of_that_weekday_in_the_span() {
        let draft = HolidayListInput {
            valid_from: Some(day(1, 1)),
            valid_to: Some(day(1, 31)),
            ..HolidayListInput::blank()
        };

        let sundays = draft.weekly_offs(Weekday::Sun, "Sunday");

        // January 2026 opens on a Thursday, so its Sundays are the 4th, 11th,
        // 18th and 25th.
        assert_eq!(sundays.len(), 4);
        assert_eq!(
            sundays.first().and_then(|row| row.observed_on),
            Some(day(1, 4))
        );
        assert_eq!(
            sundays.last().and_then(|row| row.observed_on),
            Some(day(1, 25))
        );
        assert!(sundays.iter().all(|row| row.is_weekly_off));
    }

    #[test]
    fn a_button_pressed_before_the_span_is_set_generates_nothing() {
        assert!(
            HolidayListInput::blank()
                .weekly_offs(Weekday::Sat, "Saturday")
                .is_empty()
        );
    }

    #[test]
    fn a_day_outside_the_span_is_refused_rather_than_stored() {
        let draft = HolidayListInput {
            code: "UK-2026".to_owned(),
            name: "United Kingdom 2026".to_owned(),
            valid_from: Some(day(1, 1)),
            valid_to: Some(day(6, 30)),
            holidays: vec![HolidayInput {
                observed_on: Some(day(12, 25)),
                name: "Christmas Day".to_owned(),
                is_weekly_off: false,
            }],
            ..HolidayListInput::blank()
        };

        assert_eq!(draft.check(), Err(HolidayError::DateOutsideSpan));
    }

    #[test]
    fn one_day_is_off_once() {
        let twice = HolidayInput {
            observed_on: Some(day(1, 1)),
            name: "New Year".to_owned(),
            is_weekly_off: false,
        };

        let draft = HolidayListInput {
            code: "UK-2026".to_owned(),
            name: "United Kingdom 2026".to_owned(),
            valid_from: Some(day(1, 1)),
            valid_to: Some(day(12, 31)),
            holidays: vec![twice.clone(), twice],
            ..HolidayListInput::blank()
        };

        assert_eq!(draft.check(), Err(HolidayError::DayTwice));
    }

    #[test]
    fn the_days_come_out_in_date_order_whatever_order_they_went_in() {
        let named = |month, day_of, name: &str| HolidayInput {
            observed_on: Some(day(month, day_of)),
            name: name.to_owned(),
            is_weekly_off: false,
        };

        let draft = HolidayListInput {
            code: "UK-2026".to_owned(),
            name: "United Kingdom 2026".to_owned(),
            valid_from: Some(day(1, 1)),
            valid_to: Some(day(12, 31)),
            holidays: vec![
                named(12, 25, "Christmas Day"),
                named(1, 1, "New Year"),
                named(5, 4, "Early May"),
            ],
            ..HolidayListInput::blank()
        };

        let checked = draft.check().expect("valid");
        let dates: Vec<NaiveDate> = checked
            .holidays
            .iter()
            .map(|holiday| holiday.observed_on)
            .collect();

        assert_eq!(dates, vec![day(1, 1), day(5, 4), day(12, 25)]);
    }
}
