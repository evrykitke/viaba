//! The accounting calendar, and what closing one means.
//!
//! A period is a row rather than a month derived from a year-end setting,
//! because closing is an act somebody performs on a date and "which periods are
//! shut" is a question every posting asks. See
//! `migrations/apps/books/0003_ledger.sql`.
//!
//! Periods here are calendar months. A financial year that starts in April is
//! twelve of them beginning with April, each still labelled by its own
//! year and month - which is what an accountant writes on a working paper, and
//! what makes two systems' exports line up.

use chrono::{Datelike, NaiveDate};
use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How many months a financial year has.
pub const MONTHS_IN_YEAR: u32 = 12;

/// One period of the accounting calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    pub id: Uuid,
    /// `2026-03`.
    pub label: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub is_closed: bool,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Period {
    /// Whether a journal dated here belongs to this period. Inclusive at both
    /// ends: the last day of March is March's.
    pub fn covers(&self, date: NaiveDate) -> bool {
        date >= self.starts_on && date <= self.ends_on
    }

    /// Whether a journal dated here may be posted.
    pub fn accepts(&self, date: NaiveDate) -> bool {
        self.covers(date) && !self.is_closed
    }
}

/// A period about to be created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPeriod {
    pub label: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
}

/// The twelve months of a financial year beginning on `first_day`.
///
/// Returns empty for a date this calendar cannot express, rather than
/// panicking: this crate compiles to wasm, where a panic ends the session.
pub fn year_from(first_day: NaiveDate) -> Vec<NewPeriod> {
    // Anchored to the first of the month whatever day was given. A financial
    // year that began on the 6th of April is a tax year, not an accounting
    // calendar, and a period that straddles two months makes every monthly
    // report a special case.
    let Some(anchor) = first_day.with_day(1) else {
        return Vec::new();
    };

    (0..MONTHS_IN_YEAR).filter_map(|offset| month_at(anchor, offset)).collect()
}

/// The month `offset` months after `anchor`, which must be a first of a month.
fn month_at(anchor: NaiveDate, offset: u32) -> Option<NewPeriod> {
    let month_index = anchor.month0() + offset;
    let year = anchor.year() + i32::try_from(month_index / MONTHS_IN_YEAR).ok()?;
    let month = (month_index % MONTHS_IN_YEAR) + 1;

    let starts_on = NaiveDate::from_ymd_opt(year, month, 1)?;
    let ends_on = last_day_of(year, month)?;

    Some(NewPeriod {
        label: format!("{year:04}-{month:02}"),
        starts_on,
        ends_on,
    })
}

/// The last day of a month, found by stepping back from the first of the next.
/// Shorter than a table of lengths and correct in a leap year for free.
fn last_day_of(year: i32, month: u32) -> Option<NaiveDate> {
    let (next_year, next_month) = if month == MONTHS_IN_YEAR {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    NaiveDate::from_ymd_opt(next_year, next_month, 1)?.pred_opt()
}

/// Why a journal cannot be posted where it was aimed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PeriodError {
    #[error("no accounting period covers {0}")]
    NoPeriod(NaiveDate),
    #[error("{0} is closed")]
    Closed(String),
}

impl PeriodError {
    pub fn message(&self) -> Message {
        match self {
            Self::NoPeriod(date) => msg!("periods.error.no_period", date = date),
            Self::Closed(label) => msg!("periods.error.closed", period = label),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn period(starts: NaiveDate, ends: NaiveDate, is_closed: bool) -> Period {
        Period {
            id: Uuid::nil(),
            label: "2026-03".to_owned(),
            starts_on: starts,
            ends_on: ends,
            is_closed,
            closed_at: None,
        }
    }

    #[test]
    fn a_calendar_year_is_twelve_months_ending_on_the_right_days() {
        let year = year_from(day(2026, 1, 1));

        assert_eq!(year.len(), 12);
        assert_eq!(year[0].label, "2026-01");
        assert_eq!(year[0].starts_on, day(2026, 1, 1));
        assert_eq!(year[0].ends_on, day(2026, 1, 31));
        assert_eq!(year[11].label, "2026-12");
        assert_eq!(year[11].ends_on, day(2026, 12, 31));
    }

    #[test]
    fn a_financial_year_may_start_anywhere_and_runs_into_the_next_year() {
        // April to March, which is the commonest non-calendar year there is.
        let year = year_from(day(2026, 4, 1));

        assert_eq!(year.len(), 12);
        assert_eq!(year[0].label, "2026-04");
        assert_eq!(year[8].label, "2026-12");
        assert_eq!(year[9].label, "2027-01");
        assert_eq!(year[11].label, "2027-03");
        assert_eq!(year[11].ends_on, day(2027, 3, 31));
    }

    #[test]
    fn february_is_right_in_a_leap_year_and_out_of_one() {
        assert_eq!(year_from(day(2024, 1, 1))[1].ends_on, day(2024, 2, 29));
        assert_eq!(year_from(day(2026, 1, 1))[1].ends_on, day(2026, 2, 28));
    }

    #[test]
    fn a_year_is_anchored_to_the_first_of_the_month() {
        // A tax year starting on the 6th of April is not an accounting
        // calendar; a period straddling two months makes every monthly report
        // a special case.
        let year = year_from(day(2026, 4, 6));

        assert_eq!(year[0].starts_on, day(2026, 4, 1));
        assert_eq!(year[0].ends_on, day(2026, 4, 30));
    }

    #[test]
    fn periods_from_one_year_do_not_overlap_and_leave_no_gap() {
        let year = year_from(day(2026, 4, 1));

        for pair in year.windows(2) {
            let (earlier, later) = (&pair[0], &pair[1]);

            assert!(earlier.ends_on < later.starts_on);
            assert_eq!(earlier.ends_on.succ_opt(), Some(later.starts_on));
        }
    }

    #[test]
    fn a_period_covers_both_of_its_end_days() {
        let march = period(day(2026, 3, 1), day(2026, 3, 31), false);

        assert!(march.covers(day(2026, 3, 1)));
        assert!(march.covers(day(2026, 3, 31)));
        assert!(!march.covers(day(2026, 2, 28)));
        assert!(!march.covers(day(2026, 4, 1)));
    }

    #[test]
    fn a_closed_period_covers_a_date_and_refuses_it() {
        // The distinction the posting path depends on: the journal belongs to
        // March, and March will not take it.
        let march = period(day(2026, 3, 1), day(2026, 3, 31), true);

        assert!(march.covers(day(2026, 3, 14)));
        assert!(!march.accepts(day(2026, 3, 14)));
    }
}
