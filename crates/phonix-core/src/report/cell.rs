//! One value, in the shape that says what it is.
//!
//! # Why this is here and not in the crate that draws it
//!
//! A grid built it for sorting: `9` before `10`, and last March before this
//! January, neither of which survives being compared as text. A writer needs
//! the same distinction for a different reason - a spreadsheet given a column
//! of strings cannot add it up, and a date written as text does not sort in
//! the thing somebody opens it in.
//!
//! So the type moved here when the spreadsheet writer arrived, exactly as
//! [`super::rendered`] said it would: one cell type, read by the grid, by the
//! report and by every writer, rather than a second one growing beside it.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::money::Money;

/// A value read out of a row, in the shape that says how to compare it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Cell {
    /// Nothing to show. Sorts before every value and exports as blank.
    Empty,
    Text(String),
    Number(f64),
    /// An amount, which a spreadsheet must be able to add up and a screen has
    /// to show with its currency on it.
    Money(Money),
    Bool(bool),
    Timestamp(DateTime<Utc>),
    /// Several short values - roles, tags, labels.
    List(Vec<String>),
}

impl Cell {
    pub fn text(value: impl Into<String>) -> Self {
        let value: String = value.into();

        if value.is_empty() {
            Self::Empty
        } else {
            Self::Text(value)
        }
    }

    pub fn number(value: impl Into<f64>) -> Self {
        Self::Number(value.into())
    }

    /// An amount, kept as one.
    ///
    /// `Cell::text(money.to_display_string())` is the same cell with its type
    /// thrown away, and a spreadsheet cannot sum what it is given as a word.
    pub const fn money(amount: Money) -> Self {
        Self::Money(amount)
    }

    pub const fn bool(value: bool) -> Self {
        Self::Bool(value)
    }

    pub const fn timestamp(at: DateTime<Utc>) -> Self {
        Self::Timestamp(at)
    }

    pub fn list(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let values: Vec<String> = values.into_iter().map(Into::into).collect();

        if values.is_empty() {
            Self::Empty
        } else {
            Self::List(values)
        }
    }

    /// `Empty` when `None`, so an absent value never renders as "None" by
    /// accident.
    pub fn maybe(value: Option<impl Into<String>>) -> Self {
        value.map_or(Self::Empty, Self::text)
    }

    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// The number a writer should put in the cell, where the value is one.
    ///
    /// `None` for everything else, which a writer puts down as text - the
    /// difference between a column a spreadsheet can total and one it cannot.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Money(amount) => amount.to_display_string().parse().ok(),
            _ => None,
        }
    }

    /// The instant in the cell, where there is one.
    pub const fn as_timestamp(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Timestamp(at) => Some(*at),
            _ => None,
        }
    }

    /// The value as one line of text: what the cell shows when the column has
    /// no renderer, what a CSV writes, and what a search looks inside.
    ///
    /// The date format is fixed and sortable rather than friendly. A grid is
    /// scanned down a column, where `2026-03-04 09:15` lines up and
    /// "4 March 2026, 9:15 am" does not.
    pub fn to_text(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(value) => value.clone(),
            Self::Number(value) => format_number(*value),
            Self::Money(amount) => amount.to_display_string(),
            Self::Bool(true) => "Yes".to_owned(),
            Self::Bool(false) => "No".to_owned(),
            Self::Timestamp(at) => at.format("%Y-%m-%d %H:%M").to_string(),
            Self::List(values) => values.join(", "),
        }
    }

    /// Whether this cell contains `needle`, which is already lowercased.
    pub fn contains(&self, needle: &str) -> bool {
        self.to_text().to_lowercase().contains(needle)
    }

    /// Ascending order within a column.
    ///
    /// `Empty` sorts first, so "never signed in" collects at one end rather
    /// than being scattered by whatever its text happens to be. Mixed variants
    /// in one column would be a mistake in the configuration; they fall back to
    /// comparing text so that a mistake still produces a stable order.
    pub fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Empty, Self::Empty) => Ordering::Equal,
            (Self::Empty, _) => Ordering::Less,
            (_, Self::Empty) => Ordering::Greater,
            (Self::Number(a), Self::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Self::Money(a), Self::Money(b)) => {
                a.compare(*b).unwrap_or_else(|_| {
                    // Two currencies in one column is a configuration mistake;
                    // a stable order is what it gets rather than a panic.
                    a.to_display_string().cmp(&b.to_display_string())
                })
            }
            (Self::Timestamp(a), Self::Timestamp(b)) => a.cmp(b),
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            // Case-insensitive, because a column sorted A, B, a, b reads as
            // broken to everyone who is not a computer.
            (a, b) => a.to_text().to_lowercase().cmp(&b.to_text().to_lowercase()),
        }
    }
}

/// Trailing zeroes dropped: `4`, not `4.0000000`.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.4}");

        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::Currency;

    #[test]
    fn an_amount_is_a_number_a_spreadsheet_can_add() {
        let cell = Cell::money(Money::from_units(Currency::USD, 1_204).expect("an amount"));

        assert_eq!(cell.as_number(), Some(1204.0));
        assert!(Cell::text("1204.00").as_number().is_none());
    }

    #[test]
    fn nothing_sorts_before_everything() {
        assert_eq!(Cell::Empty.compare(&Cell::number(0)), Ordering::Less);
    }

    #[test]
    fn numbers_sort_as_numbers() {
        assert_eq!(Cell::number(9).compare(&Cell::number(10)), Ordering::Less);
        assert_eq!(
            Cell::text("9").compare(&Cell::text("10")),
            Ordering::Greater,
            "text is still text",
        );
    }

    #[test]
    fn text_sorts_without_regard_to_case() {
        assert_eq!(
            Cell::text("apple").compare(&Cell::text("Banana")),
            Ordering::Less
        );
    }

    #[test]
    fn an_empty_string_is_an_empty_cell() {
        assert!(Cell::text("").is_empty());
        assert!(Cell::list(Vec::<String>::new()).is_empty());
        assert!(Cell::maybe(None::<String>).is_empty());
    }

    #[test]
    fn a_list_reads_and_searches_as_its_members() {
        let cell = Cell::list(["Admin", "Buyer"]);

        assert_eq!(cell.to_text(), "Admin, Buyer");
        assert!(cell.contains("buyer"));
    }

    #[test]
    fn a_flag_reads_as_a_word_so_the_export_says_something() {
        assert_eq!(Cell::bool(true).to_text(), "Yes");
        assert_eq!(Cell::bool(false).to_text(), "No");
    }

    #[test]
    fn whole_numbers_do_not_grow_a_decimal_point() {
        assert_eq!(Cell::number(4).to_text(), "4");
        assert_eq!(Cell::number(4.5).to_text(), "4.5");
    }

    #[test]
    fn an_amount_keeps_its_currency_on_screen_and_loses_it_in_a_spreadsheet() {
        let cell = Cell::money(Money::from_units(Currency::USD, 5).expect("an amount"));

        assert_eq!(cell.to_text(), "5.00");
        assert_eq!(cell.as_number(), Some(5.0));
    }
}
