//! Lots and serial numbers: which particular units these are.
//!
//! # One type for both, because they differ in one number
//!
//! A lot covers many units; a serial covers exactly one. Everything else about
//! them - the number, the expiry, the way a pick reaches for the oldest first -
//! is the same, and two tables would mean every quant, every move and every
//! recall query written twice. [`Tracking`] says which of the two a row is.
//!
//! # Expiry is the reason a recall is a query
//!
//! A lot that expires is what makes FEFO possible and what makes "which
//! customers have this batch" answerable by reading the moves rather than by
//! ringing round. Only items whose `uses_expiry` is set carry one.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::item::Tracking;
use crate::quantity::Quantity;

pub const MAX_LOT_NUMBER_LEN: usize = 64;

/// One batch, or one unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lot {
    pub id: Uuid,
    /// Stock hangs off a variant, so a lot does too.
    pub variant_id: Uuid,
    /// Typed, never generated. It is the supplier's number, printed on the
    /// carton, and a number this system invented would match nothing.
    pub number: String,
    pub expires_on: Option<NaiveDate>,
    /// Snapshotted from the item, so a quant can be checked without reading it.
    pub tracking: Tracking,
}

impl Lot {
    pub fn is_expired(&self, today: NaiveDate) -> bool {
        self.expires_on.is_some_and(|date| date < today)
    }

    /// `LOT-8841 · 2027-01-04`. One spelling, so a picking list and a till tile
    /// cannot disagree.
    pub fn label(&self) -> String {
        match self.expires_on {
            None => self.number.clone(),
            Some(date) => format!("{} · {date}", self.number),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotInput {
    pub id: Option<Uuid>,
    pub variant_id: Uuid,
    pub number: String,
    pub expires_on: Option<NaiveDate>,
}

impl LotInput {
    pub const fn for_variant(variant_id: Uuid) -> Self {
        Self {
            id: None,
            variant_id,
            number: String::new(),
            expires_on: None,
        }
    }

    /// Trim, and refuse what the item's tracking mode does not allow.
    ///
    /// Takes the item's rules rather than reading them: this crate compiles
    /// into the browser, and the form has the item in hand.
    pub fn check(&self, tracking: Tracking, uses_expiry: bool) -> Result<Self, LotError> {
        if !tracking.needs_a_number() {
            return Err(LotError::ItemIsNotTracked);
        }

        let number = self.number.trim();
        if number.is_empty() {
            return Err(LotError::NumberRequired);
        }
        if number.chars().count() > MAX_LOT_NUMBER_LEN {
            return Err(LotError::NumberTooLong);
        }
        // A number with a space in it is a number a scanner reads as two.
        if number.chars().any(char::is_whitespace) {
            return Err(LotError::NumberHasSpace);
        }

        if self.expires_on.is_some() && !uses_expiry {
            return Err(LotError::ExpiryNotKept);
        }

        Ok(Self {
            id: self.id,
            variant_id: self.variant_id,
            number: number.to_owned(),
            expires_on: self.expires_on,
        })
    }
}

/// A lot with what is on hand of it, for a picking screen and a recall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotSummary {
    pub id: Uuid,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub item_name: String,
    pub number: String,
    pub expires_on: Option<NaiveDate>,
    pub tracking: Tracking,
    /// Across every internal location.
    pub on_hand: Quantity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LotError {
    #[error("this item does not keep lot or serial numbers")]
    ItemIsNotTracked,
    #[error("a lot needs a number")]
    NumberRequired,
    #[error("a lot number is at most 64 characters")]
    NumberTooLong,
    #[error("a lot number may not contain a space")]
    NumberHasSpace,
    #[error("this item does not keep expiry dates")]
    ExpiryNotKept,
    #[error("a serial number covers exactly one unit")]
    SerialIsNotOne,
    #[error("this item needs a lot or serial number on every movement")]
    NumberRequiredOnMove,
    #[error("that lot belongs to a different item")]
    WrongVariant,
}

impl LotError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NumberRequired | Self::NumberTooLong | Self::NumberHasSpace => "number",
            Self::ExpiryNotKept => "expires_on",
            Self::ItemIsNotTracked
            | Self::SerialIsNotOne
            | Self::NumberRequiredOnMove
            | Self::WrongVariant => "lot_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::ItemIsNotTracked => msg!("lots.error.item_is_not_tracked"),
            Self::NumberRequired => msg!("lots.error.number_required"),
            Self::NumberTooLong => msg!("lots.error.number_too_long"),
            Self::NumberHasSpace => msg!("lots.error.number_has_space"),
            Self::ExpiryNotKept => msg!("lots.error.expiry_not_kept"),
            Self::SerialIsNotOne => msg!("lots.error.serial_is_not_one"),
            Self::NumberRequiredOnMove => msg!("lots.error.number_required_on_move"),
            Self::WrongVariant => msg!("lots.error.wrong_variant"),
        }
    }
}

/// Whether a movement of this item may name this lot, and whether it must.
///
/// One place, so a receipt and an adjustment cannot disagree about it.
pub fn check_on_move(
    tracking: Tracking,
    lot: Option<&Lot>,
    variant_id: Uuid,
    quantity: Quantity,
) -> Result<(), LotError> {
    match (tracking.needs_a_number(), lot) {
        (false, None) => Ok(()),
        (false, Some(_)) => Err(LotError::ItemIsNotTracked),
        (true, None) => Err(LotError::NumberRequiredOnMove),
        (true, Some(lot)) => {
            if lot.variant_id != variant_id {
                return Err(LotError::WrongVariant);
            }
            if tracking.is_one_per_unit() && quantity != Quantity::ONE {
                return Err(LotError::SerialIsNotOne);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lot(variant: u128) -> Lot {
        Lot {
            id: Uuid::from_u128(9),
            variant_id: Uuid::from_u128(variant),
            number: "LOT-8841".to_owned(),
            expires_on: None,
            tracking: Tracking::Lot,
        }
    }

    #[test]
    fn a_tracked_item_may_not_move_without_a_number() {
        // The rule that makes a recall a query. Two hundred units belonging to
        // no lot are two hundred a recall silently skips.
        assert_eq!(
            check_on_move(Tracking::Lot, None, Uuid::nil(), Quantity::ONE),
            Err(LotError::NumberRequiredOnMove)
        );
    }

    #[test]
    fn an_untracked_item_may_not_move_with_one() {
        let one = Uuid::from_u128(1);

        assert_eq!(
            check_on_move(Tracking::None, Some(&lot(1)), one, Quantity::ONE),
            Err(LotError::ItemIsNotTracked)
        );
        assert_eq!(
            check_on_move(Tracking::None, None, one, Quantity::ONE),
            Ok(())
        );
    }

    #[test]
    fn a_serial_number_covers_exactly_one_unit() {
        let one = Uuid::from_u128(1);
        let two = Quantity::from_units(2).unwrap();

        assert_eq!(
            check_on_move(Tracking::Serial, Some(&lot(1)), one, two),
            Err(LotError::SerialIsNotOne)
        );
        assert_eq!(
            check_on_move(Tracking::Serial, Some(&lot(1)), one, Quantity::ONE),
            Ok(())
        );
        // A lot covers as many as it covers.
        assert_eq!(check_on_move(Tracking::Lot, Some(&lot(1)), one, two), Ok(()));
    }

    #[test]
    fn a_lot_belongs_to_the_variant_it_was_made_for() {
        assert_eq!(
            check_on_move(Tracking::Lot, Some(&lot(1)), Uuid::from_u128(2), Quantity::ONE),
            Err(LotError::WrongVariant)
        );
    }

    #[test]
    fn a_number_is_trimmed_and_refused_where_it_would_not_scan() {
        let input = LotInput {
            number: "  LOT-8841  ".to_owned(),
            ..LotInput::for_variant(Uuid::nil())
        };
        assert_eq!(input.check(Tracking::Lot, false).unwrap().number, "LOT-8841");

        let spaced = LotInput {
            number: "LOT 8841".to_owned(),
            ..LotInput::for_variant(Uuid::nil())
        };
        assert_eq!(
            spaced.check(Tracking::Lot, false),
            Err(LotError::NumberHasSpace)
        );
    }

    #[test]
    fn an_expiry_is_refused_for_an_item_that_keeps_none() {
        let dated = LotInput {
            number: "LOT-1".to_owned(),
            expires_on: NaiveDate::from_ymd_opt(2027, 1, 4),
            ..LotInput::for_variant(Uuid::nil())
        };

        assert_eq!(
            dated.check(Tracking::Lot, false),
            Err(LotError::ExpiryNotKept)
        );
        assert!(dated.check(Tracking::Lot, true).is_ok());
    }

    #[test]
    fn a_lot_is_expired_the_day_after_its_date() {
        let expiring = Lot {
            expires_on: NaiveDate::from_ymd_opt(2026, 9, 7),
            ..lot(1)
        };

        let day = |d| NaiveDate::from_ymd_opt(2026, 9, d).unwrap();
        assert!(!expiring.is_expired(day(7)));
        assert!(expiring.is_expired(day(8)));
    }
}
