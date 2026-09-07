//! Quants: how much of one thing is in one place.
//!
//! # A cache with a proof behind it
//!
//! A quant is the running total of every [`StockMove`](crate::movement) across
//! one (variant, location, lot). It exists because "what is on hand" is asked
//! on every screen and summing the whole movement history per row is a table
//! scan per row - not because it is the truth. The moves are the truth, and a
//! quant that disagrees with them is a bug this design can *detect*, which is
//! the whole reason inventory is kept as double entry.
//!
//! # Reserved is not gone
//!
//! A reservation holds stock for a picking that has not happened. The quantity
//! is still there and still on the balance sheet; what has changed is that
//! somebody else may not promise it. [`Quant::available`] is the number a sales
//! line should be checked against, and `quantity` is the number a stock count
//! is checked against - conflating the two is how a warehouse is told it has
//! nothing while a full pallet sits in the aisle.
//!
//! # Negative stock is refused
//!
//! ADR 0006 section 6.6. A system that lets a location go below zero is a
//! system whose valuation is guesswork from that moment on, because there is no
//! cost for units that were never received. The refusal is [`take`], and it is
//! not configurable.

use phonix_core::i18n::Message;
use phonix_core::money::Money;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

/// What is in one place, of one thing, of one lot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quant {
    pub id: Uuid,
    pub variant_id: Uuid,
    pub location_id: Uuid,
    /// `None` for an item that keeps no lot numbers.
    pub lot_id: Option<Uuid>,
    pub quantity: Quantity,
    /// Held for a picking that has not happened yet. Never above `quantity`.
    pub reserved: Quantity,
}

impl Quant {
    /// What may still be promised to somebody.
    pub fn available(&self) -> Quantity {
        self.quantity
            .checked_sub(self.reserved)
            .unwrap_or(Quantity::ZERO)
    }

    pub fn is_empty(&self) -> bool {
        self.quantity.is_zero() && self.reserved.is_zero()
    }
}

/// Add stock to a running total.
///
/// A move *into* a location. Overflow is the only way this fails, and it is a
/// number no warehouse holds.
pub fn put(on_hand: Quantity, quantity: Quantity) -> Result<Quantity, QuantError> {
    on_hand.checked_add(quantity).map_err(QuantError::from)
}

/// Take stock away, refusing to go below zero.
///
/// The one rule that keeps valuation honest. `owned` says whether this end is
/// ours: a vendor's or a customer's side has no floor, because we do not keep
/// their books and a receipt would otherwise be refused for taking a supplier
/// below nothing.
pub fn take(on_hand: Quantity, quantity: Quantity, owned: bool) -> Result<Quantity, QuantError> {
    let left = on_hand.checked_sub(quantity)?;

    if owned && left.is_negative() {
        return Err(QuantError::WouldGoNegative {
            short: quantity.checked_sub(on_hand).unwrap_or(Quantity::ZERO),
        });
    }

    Ok(left)
}

/// Hold stock for a picking, refusing to promise the same units twice.
pub fn reserve(quant: &Quant, quantity: Quantity) -> Result<Quantity, QuantError> {
    let held = quant.reserved.checked_add(quantity)?;

    if held.compare(quant.quantity).is_gt() {
        return Err(QuantError::NotEnoughAvailable {
            available: quant.available(),
        });
    }

    Ok(held)
}

/// Give back what a cancelled picking was holding.
pub fn release(quant: &Quant, quantity: Quantity) -> Result<Quantity, QuantError> {
    let held = quant.reserved.checked_sub(quantity)?;

    // More released than was ever held is a caller that lost count, and
    // clamping it silently would hide the bug in a number nobody re-reads.
    if held.is_negative() {
        return Err(QuantError::NotReserved);
    }

    Ok(held)
}

/// One line of the stock-on-hand report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnHandRow {
    pub variant_id: Uuid,
    pub variant_code: String,
    pub item_id: Uuid,
    pub item_name: String,
    pub combination: Option<String>,
    pub location_id: Uuid,
    pub location_path: String,
    pub lot_id: Option<Uuid>,
    pub lot_number: Option<String>,
    pub expires_on: Option<chrono::NaiveDate>,
    pub quantity: Quantity,
    pub reserved: Quantity,
    pub unit_code: String,
    /// What the quantity here is worth, at what the layers say it cost.
    pub value: Money,
}

impl OnHandRow {
    pub fn available(&self) -> Quantity {
        self.quantity
            .checked_sub(self.reserved)
            .unwrap_or(Quantity::ZERO)
    }
}

/// What the on-hand screen may be narrowed by.
///
/// `location_id` matches the whole subtree beneath it, because "how much is in
/// the Manchester warehouse" is one question rather than one per shelf.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnHandFilter {
    pub item_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub location_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    /// Rows whose quantity is zero, which a stock take wants and a picker does
    /// not.
    pub include_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QuantError {
    /// ADR 0006 section 6.6, and the reason it is not a setting.
    #[error("there is not that much there")]
    WouldGoNegative { short: Quantity },
    #[error("that much is already promised to somebody else")]
    NotEnoughAvailable { available: Quantity },
    #[error("that much was never reserved")]
    NotReserved,
    #[error("that quantity is too large")]
    OutOfRange,
}

impl From<QuantityError> for QuantError {
    fn from(_: QuantityError) -> Self {
        Self::OutOfRange
    }
}

impl QuantError {
    pub fn message(self) -> Message {
        match self {
            Self::WouldGoNegative { short } => {
                msg!("quants.error.would_go_negative", short = short.to_display_string())
            }
            Self::NotEnoughAvailable { available } => msg!(
                "quants.error.not_enough_available",
                available = available.to_display_string()
            ),
            Self::NotReserved => msg!("quants.error.not_reserved"),
            Self::OutOfRange => msg!("quantity.error.out_of_range"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quant(quantity: &str, reserved: &str) -> Quant {
        Quant {
            id: Uuid::from_u128(1),
            variant_id: Uuid::from_u128(2),
            location_id: Uuid::from_u128(3),
            lot_id: None,
            quantity: Quantity::parse(quantity).unwrap(),
            reserved: Quantity::parse(reserved).unwrap(),
        }
    }

    #[test]
    fn a_location_of_ours_never_goes_below_nothing() {
        // Section 6.6. Units that were never received have no cost, so every
        // valuation after a negative balance is guesswork.
        let ten = Quantity::from_units(10).unwrap();
        let twelve = Quantity::from_units(12).unwrap();

        assert_eq!(
            take(ten, twelve, true),
            Err(QuantError::WouldGoNegative {
                short: Quantity::from_units(2).unwrap()
            })
        );
        assert_eq!(take(ten, ten, true), Ok(Quantity::ZERO));
    }

    #[test]
    fn somebody_elses_side_has_no_floor() {
        // A receipt takes stock from the vendor's location, which has never
        // held anything. Refusing that would refuse every first receipt.
        let out = take(Quantity::ZERO, Quantity::from_units(5).unwrap(), false).unwrap();

        assert_eq!(out.to_display_string(), "-5");
    }

    #[test]
    fn reserved_stock_is_still_on_the_shelf() {
        let held = quant("10", "4");

        assert_eq!(held.quantity, Quantity::from_units(10).unwrap());
        assert_eq!(held.available(), Quantity::from_units(6).unwrap());
    }

    #[test]
    fn the_same_units_are_not_promised_twice() {
        let held = quant("10", "4");

        assert_eq!(
            reserve(&held, Quantity::from_units(7).unwrap()),
            Err(QuantError::NotEnoughAvailable {
                available: Quantity::from_units(6).unwrap()
            })
        );
        assert_eq!(
            reserve(&held, Quantity::from_units(6).unwrap()),
            Ok(Quantity::from_units(10).unwrap())
        );
    }

    #[test]
    fn releasing_more_than_was_held_is_a_bug_rather_than_a_zero() {
        let held = quant("10", "4");

        assert_eq!(
            release(&held, Quantity::from_units(5).unwrap()),
            Err(QuantError::NotReserved)
        );
        assert_eq!(
            release(&held, Quantity::from_units(4).unwrap()),
            Ok(Quantity::ZERO)
        );
    }

    #[test]
    fn a_fractional_quantity_survives_a_round_trip_through_a_quant() {
        // Three receipts of a tenth are three tenths, not 0.30000000000000004.
        let tenth = Quantity::parse("0.1").unwrap();
        let mut on_hand = Quantity::ZERO;

        for _ in 0..3 {
            on_hand = put(on_hand, tenth).unwrap();
        }

        assert_eq!(on_hand, Quantity::parse("0.3").unwrap());
        assert_eq!(take(on_hand, tenth, true).unwrap().to_display_string(), "0.2");
    }
}
