//! Named price lists, and what a variant costs in one.
//!
//! # Why a list and not a price on the item
//!
//! `items.sale_price` is one number, so a wholesale customer and a walk-in
//! share it. ERPNext splits the same thing into Price List and Item Price and
//! Odoo into pricelists, for the reason both had to: what something costs is a
//! function of who is buying, how many, and when.
//!
//! # Which price wins
//!
//! A variant may have several rows in one list - a quantity break, a dated
//! offer, or both - and overlapping rows are allowed deliberately. The
//! migration says why: a constraint forbidding them would forbid stating an
//! ordinary pair of facts. [`resolve`] is the rule instead, and it is here
//! rather than in SQL so that the browser can answer it while somebody types.
//!
//! The rule is most specific wins: the highest quantity break the line
//! qualifies for, and among equal breaks the one whose window starts latest. A
//! price with no dates is the fallback that has always applied, so it loses to
//! any dated offer covering today - which is what makes a promotion a
//! promotion.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::locale::Currency;
use phonix_core::money::Money;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::Quantity;

pub const MAX_PRICE_LIST_CODE_LEN: usize = 40;
pub const MAX_PRICE_LIST_NAME_LEN: usize = 120;

/// A named set of selling prices, in one currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceList {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub currency: Currency,
    pub is_active: bool,
}

/// What one variant costs in one list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPrice {
    pub id: Uuid,
    pub price_list_id: Uuid,
    pub variant_id: Uuid,
    /// The break this applies from, in the item's stock unit. Zero is "any
    /// quantity".
    pub min_quantity: Quantity,
    /// Open at either end: `None` has always applied, or still does.
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
    /// Per stock unit, in the list's currency.
    pub unit_price: Money,
}

impl ItemPrice {
    /// Whether this row may price a line of `quantity` on `on`.
    pub fn applies(&self, quantity: Quantity, on: NaiveDate) -> bool {
        if quantity < self.min_quantity {
            return false;
        }

        let started = self.valid_from.is_none_or(|from| from <= on);
        let running = self.valid_to.is_none_or(|to| to >= on);

        started && running
    }

    /// How specific this row is, for choosing between two that both apply.
    ///
    /// The break first, then how recently the window opened. A row with no
    /// `valid_from` sorts below every dated one, which is what makes a standing
    /// price the fallback rather than the winner.
    fn specificity(&self) -> (Quantity, Option<NaiveDate>) {
        (self.min_quantity, self.valid_from)
    }
}

/// The price a line qualifies for, or `None` where the list prices nothing that
/// applies.
///
/// `None` is an answer rather than a failure: a list that does not carry an
/// item is ordinary, and the caller falls back or refuses as its own rules say.
pub fn resolve(prices: &[ItemPrice], quantity: Quantity, on: NaiveDate) -> Option<&ItemPrice> {
    prices
        .iter()
        .filter(|price| price.applies(quantity, on))
        .max_by(|a, b| a.specificity().cmp(&b.specificity()))
}

/// What can be wrong with a price list somebody typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PriceListError {
    #[error("a price list needs a code")]
    CodeRequired,
    #[error("that code is too long")]
    CodeTooLong,
    #[error("a price list needs a name")]
    NameRequired,
    #[error("that name is too long")]
    NameTooLong,
    #[error("a price cannot be negative")]
    PriceNegative,
    #[error("that window ends before it starts")]
    WindowBackwards,
}

impl PriceListError {
    pub const fn field(self) -> &'static str {
        match self {
            Self::CodeRequired | Self::CodeTooLong => "code",
            Self::NameRequired | Self::NameTooLong => "name",
            Self::PriceNegative => "unit_price",
            Self::WindowBackwards => "valid_to",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CodeRequired => msg!("price_lists.error.code_required"),
            Self::CodeTooLong => msg!("price_lists.error.code_too_long"),
            Self::NameRequired => msg!("price_lists.error.name_required"),
            Self::NameTooLong => msg!("price_lists.error.name_too_long"),
            Self::PriceNegative => msg!("price_lists.error.price_negative"),
            Self::WindowBackwards => msg!("price_lists.error.window_backwards"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, of: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, of).expect("a real date")
    }

    fn price(min: &str, from: Option<NaiveDate>, to: Option<NaiveDate>, amount: &str) -> ItemPrice {
        ItemPrice {
            id: Uuid::nil(),
            price_list_id: Uuid::nil(),
            variant_id: Uuid::nil(),
            min_quantity: Quantity::parse(min).expect("a quantity"),
            valid_from: from,
            valid_to: to,
            unit_price: Money::parse(Currency::USD, amount).expect("an amount"),
        }
    }

    fn qty(raw: &str) -> Quantity {
        Quantity::parse(raw).expect("a quantity")
    }

    #[test]
    fn an_empty_list_prices_nothing() {
        assert!(resolve(&[], qty("1"), day(2026, 3, 14)).is_none());
    }

    #[test]
    fn the_highest_break_the_line_qualifies_for_wins() {
        let prices = [
            price("0", None, None, "10.00"),
            price("10", None, None, "9.00"),
            price("100", None, None, "8.00"),
        ];

        let at = |quantity: &str| {
            resolve(&prices, qty(quantity), day(2026, 3, 14))
                .expect("a price")
                .unit_price
                .to_display_string()
        };

        assert_eq!(at("1"), "10.00");
        assert_eq!(at("10"), "9.00");
        assert_eq!(at("99"), "9.00");
        assert_eq!(at("100"), "8.00");
        assert_eq!(at("1000"), "8.00");
    }

    #[test]
    fn a_break_it_does_not_reach_does_not_apply() {
        let prices = [price("10", None, None, "9.00")];

        assert!(resolve(&prices, qty("9"), day(2026, 3, 14)).is_none());
        assert!(resolve(&prices, qty("10"), day(2026, 3, 14)).is_some());
    }

    #[test]
    fn a_dated_offer_beats_the_standing_price_while_it_runs() {
        // The whole point of a window: the standing price is the fallback, and
        // an offer covering today displaces it without being deleted after.
        let prices = [
            price("0", None, None, "10.00"),
            price("0", Some(day(2026, 3, 1)), Some(day(2026, 3, 31)), "7.50"),
        ];

        let at = |on: NaiveDate| {
            resolve(&prices, qty("1"), on)
                .expect("a price")
                .unit_price
                .to_display_string()
        };

        assert_eq!(at(day(2026, 2, 28)), "10.00");
        assert_eq!(at(day(2026, 3, 1)), "7.50");
        assert_eq!(at(day(2026, 3, 31)), "7.50");
        assert_eq!(at(day(2026, 4, 1)), "10.00");
    }

    #[test]
    fn a_window_is_closed_at_both_ends() {
        let offer = price("0", Some(day(2026, 3, 1)), Some(day(2026, 3, 31)), "7.50");

        assert!(!offer.applies(qty("1"), day(2026, 2, 28)));
        assert!(offer.applies(qty("1"), day(2026, 3, 1)));
        assert!(offer.applies(qty("1"), day(2026, 3, 31)));
        assert!(!offer.applies(qty("1"), day(2026, 4, 1)));
    }

    #[test]
    fn a_quantity_break_beats_a_dated_offer_at_a_lower_break() {
        // Specificity is the break first. Somebody buying a pallet during a
        // promotion gets the pallet price, which is the one that was negotiated
        // rather than advertised.
        let prices = [
            price("100", None, None, "8.00"),
            price("0", Some(day(2026, 3, 1)), Some(day(2026, 3, 31)), "7.50"),
        ];

        assert_eq!(
            resolve(&prices, qty("100"), day(2026, 3, 14))
                .expect("a price")
                .unit_price
                .to_display_string(),
            "8.00",
        );
    }

    #[test]
    fn the_later_window_wins_between_two_offers_at_one_break() {
        let prices = [
            price("0", Some(day(2026, 1, 1)), None, "9.00"),
            price("0", Some(day(2026, 3, 1)), None, "7.50"),
        ];

        assert_eq!(
            resolve(&prices, qty("1"), day(2026, 3, 14))
                .expect("a price")
                .unit_price
                .to_display_string(),
            "7.50",
        );
    }
}
