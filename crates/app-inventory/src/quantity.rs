//! [`Quantity`]: an exact count of something, which is very often not a count.
//!
//! # Why not an integer
//!
//! Because 0.35 kg of flour, 2.5 metres of cable and a third of a drum are all
//! stock, and a system that stores quantities as integers has already decided
//! its customers sell boxes. Rounding a quantity where it is entered is how a
//! long production run comes out short.
//!
//! # Why not a float
//!
//! Because stock is reconciled. Three receipts of 0.1 have to make 0.3 exactly,
//! or a stock take reports a discrepancy that does not exist and somebody
//! spends an afternoon counting a shelf.
//!
//! Six decimal places, matching `NUMERIC(19, 6)`. Two more than [`Money`]'s
//! four on purpose: a unit price times a quantity is money, and the quantity is
//! the side that carries the small numbers - grams, millilitres, a
//! thousandth of a reel.
//!
//! [`Money`]: phonix_core::money::Money

use core::cmp::Ordering;
use core::fmt;

use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};

/// Decimal places every quantity is stored at.
pub const SCALE: u32 = 6;

/// `10^SCALE`. Public because valuation multiplies a money amount by a
/// quantity, and doing that exactly means knowing the quantity's scale.
pub const SCALE_FACTOR: i128 = 1_000_000;

/// The largest scaled value `NUMERIC(19, 6)` holds: 9999999999999.999999.
pub const MAX_SCALED: i128 = 9_999_999_999_999_999_999;

/// An exact quantity, in whatever unit the line it sits on names.
///
/// Unitless by construction. A quantity does not carry its unit for the reason
/// [`phonix_core::money::Money`] does carry its currency: two amounts in
/// different currencies must never be added, whereas a stock line's unit is
/// fixed by the item and the same on both sides of every comparison. Where a
/// document lets somebody pick a unit, [`crate::unit::Conversion`] converts to
/// the item's stock unit before anything reaches here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Quantity {
    scaled: i128,
}

impl Quantity {
    pub const ZERO: Self = Self { scaled: 0 };
    pub const ONE: Self = Self {
        scaled: SCALE_FACTOR,
    };

    /// Build from the scaled integer - the quantity times `10^SCALE`. What the
    /// database layer reads a `NUMERIC(19, 6)` back as.
    pub fn from_scaled(scaled: i128) -> Result<Self, QuantityError> {
        if !(-MAX_SCALED..=MAX_SCALED).contains(&scaled) {
            return Err(QuantityError::OutOfRange);
        }
        Ok(Self { scaled })
    }

    pub fn from_units(units: i64) -> Result<Self, QuantityError> {
        i128::from(units)
            .checked_mul(SCALE_FACTOR)
            .ok_or(QuantityError::OutOfRange)
            .and_then(Self::from_scaled)
    }

    /// Parse what somebody typed. Accepts a leading sign, thousands separators
    /// and up to [`SCALE`] decimal places; refuses anything else rather than
    /// reading the leading digits of it.
    pub fn parse(raw: &str) -> Result<Self, QuantityError> {
        let trimmed = raw.trim().replace([',', ' ', '\u{a0}', '_'], "");
        if trimmed.is_empty() {
            return Err(QuantityError::Empty);
        }

        let (negative, digits) = match trimmed.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, trimmed.strip_prefix('+').unwrap_or(&trimmed)),
        };

        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (digits, ""),
        };

        if whole.is_empty() && fraction.is_empty() {
            return Err(QuantityError::Malformed);
        }
        if !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(QuantityError::Malformed);
        }
        if fraction.len() > SCALE as usize {
            return Err(QuantityError::TooPrecise);
        }

        let mut scaled: i128 = 0;
        for byte in whole.bytes().chain(fraction.bytes()) {
            scaled = scaled
                .checked_mul(10)
                .and_then(|value| value.checked_add(i128::from(byte - b'0')))
                .ok_or(QuantityError::OutOfRange)?;
        }

        // Pad the fraction out to the stored scale: "1.5" is 1.500000.
        let padding = SCALE as usize - fraction.len();
        for _ in 0..padding {
            scaled = scaled.checked_mul(10).ok_or(QuantityError::OutOfRange)?;
        }

        Self::from_scaled(if negative { -scaled } else { scaled })
    }

    pub const fn scaled(self) -> i128 {
        self.scaled
    }

    pub const fn is_zero(self) -> bool {
        self.scaled == 0
    }

    pub const fn is_negative(self) -> bool {
        self.scaled < 0
    }

    pub const fn is_positive(self) -> bool {
        self.scaled > 0
    }

    pub const fn abs(self) -> Self {
        Self {
            scaled: self.scaled.abs(),
        }
    }

    pub const fn negate(self) -> Self {
        Self {
            scaled: -self.scaled,
        }
    }

    pub fn checked_add(self, other: Self) -> Result<Self, QuantityError> {
        self.scaled
            .checked_add(other.scaled)
            .ok_or(QuantityError::OutOfRange)
            .and_then(Self::from_scaled)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, QuantityError> {
        self.scaled
            .checked_sub(other.scaled)
            .ok_or(QuantityError::OutOfRange)
            .and_then(Self::from_scaled)
    }

    /// Add up a run of quantities. `ZERO` for an empty one.
    pub fn total(quantities: impl IntoIterator<Item = Self>) -> Result<Self, QuantityError> {
        quantities
            .into_iter()
            .try_fold(Self::ZERO, |running, next| running.checked_add(next))
    }

    /// Multiply by a decimal factor given as its own scaled integer at
    /// `factor_scale` decimal places, rounding half away from zero.
    ///
    /// This is what a unit conversion is: twelve per case, or 0.4536 kg per
    /// pound. The factor arrives scaled rather than as a float because a float
    /// factor is how 12 boxes of 12 becomes 143.99999999.
    pub fn scale_by(self, factor: i128, factor_scale: u32) -> Result<Self, QuantityError> {
        let divisor = 10_i128
            .checked_pow(factor_scale)
            .ok_or(QuantityError::OutOfRange)?;

        let product = self
            .scaled
            .checked_mul(factor)
            .ok_or(QuantityError::OutOfRange)?;

        let half = divisor / 2;
        let rounded = if product >= 0 {
            product
                .checked_add(half)
                .ok_or(QuantityError::OutOfRange)?
                .div_euclid(divisor)
        } else {
            -((-product)
                .checked_add(half)
                .ok_or(QuantityError::OutOfRange)?
                .div_euclid(divisor))
        };

        Self::from_scaled(rounded)
    }

    /// Multiply by `numerator` and divide by `denominator` in one step,
    /// rounding half away from zero.
    ///
    /// One step rather than two because a ratio of thirds has no exact
    /// reciprocal: dividing by 3 after multiplying is right, and multiplying by
    /// a rounded 0.333333 is not.
    pub fn scale_by_ratio(self, numerator: i128, denominator: i128) -> Result<Self, QuantityError> {
        if denominator == 0 {
            return Err(QuantityError::OutOfRange);
        }

        let product = self
            .scaled
            .checked_mul(numerator)
            .ok_or(QuantityError::OutOfRange)?;

        let magnitude = product.unsigned_abs();
        let divisor = denominator.unsigned_abs();
        let rounded = magnitude
            .checked_add(divisor / 2)
            .ok_or(QuantityError::OutOfRange)?
            / divisor;

        let negative = (product < 0) != (denominator < 0);
        let signed = i128::try_from(rounded).map_err(|_| QuantityError::OutOfRange)?;

        Self::from_scaled(if negative { -signed } else { signed })
    }

    pub fn compare(self, other: Self) -> Ordering {
        self.scaled.cmp(&other.scaled)
    }

    /// What goes in a `NUMERIC(19, 6)` bind, and what a decimal in a payload
    /// looks like. Always the full scale, never localised.
    pub fn to_storage_string(self) -> String {
        let negative = self.scaled < 0;
        let magnitude = self.scaled.unsigned_abs();
        let factor = SCALE_FACTOR.unsigned_abs();
        let whole = magnitude / factor;
        let fraction = magnitude % factor;
        let sign = if negative { "-" } else { "" };
        format!("{sign}{whole}.{fraction:0width$}", width = SCALE as usize)
    }

    /// What a grid shows: trailing zeros dropped, because "12" is a count and
    /// "12.000000" is a database column.
    pub fn to_display_string(self) -> String {
        let stored = self.to_storage_string();
        match stored.split_once('.') {
            None => stored,
            Some((whole, fraction)) => {
                let trimmed = fraction.trim_end_matches('0');
                if trimmed.is_empty() {
                    whole.to_owned()
                } else {
                    format!("{whole}.{trimmed}")
                }
            }
        }
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_display_string())
    }
}

/// Serialised as its decimal string, so a quantity survives JSON and a
/// JavaScript number never sees it.
impl Serialize for Quantity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_storage_string())
    }
}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QuantityError {
    #[error("a quantity is required")]
    Empty,
    #[error("that is not a number")]
    Malformed,
    #[error("a quantity carries at most six decimal places")]
    TooPrecise,
    #[error("that quantity is too large")]
    OutOfRange,
}

impl QuantityError {
    pub fn message(self) -> Message {
        match self {
            Self::Empty => msg!("quantity.error.empty"),
            Self::Malformed => msg!("quantity.error.malformed"),
            Self::TooPrecise => msg!("quantity.error.too_precise"),
            Self::OutOfRange => msg!("quantity.error.out_of_range"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_quantity_keeps_every_digit_it_was_given() {
        assert_eq!(Quantity::parse("12").unwrap().to_display_string(), "12");
        assert_eq!(Quantity::parse("0.35").unwrap().to_display_string(), "0.35");
        assert_eq!(Quantity::parse("1 200.5").unwrap().to_display_string(), "1200.5");
        assert_eq!(Quantity::parse("-2.5").unwrap().to_display_string(), "-2.5");
        assert_eq!(
            Quantity::parse(".5").unwrap().to_storage_string(),
            "0.500000"
        );
    }

    #[test]
    fn precision_beyond_the_column_is_refused_rather_than_rounded_away() {
        assert_eq!(Quantity::parse("1.1234567"), Err(QuantityError::TooPrecise));
        assert_eq!(Quantity::parse("1.2.3"), Err(QuantityError::Malformed));
        assert_eq!(Quantity::parse("ten"), Err(QuantityError::Malformed));
        assert_eq!(Quantity::parse("  "), Err(QuantityError::Empty));
    }

    #[test]
    fn three_tenths_add_up_to_exactly_nine_tenths() {
        // The reason this is not a float. A stock take that reports a
        // discrepancy of 0.0000000001 costs somebody an afternoon.
        let tenth = Quantity::parse("0.1").unwrap();
        let total = Quantity::total([tenth, tenth, tenth]).unwrap();

        assert_eq!(total, Quantity::parse("0.3").unwrap());
        assert_eq!(total.to_display_string(), "0.3");
    }

    #[test]
    fn a_conversion_factor_is_applied_exactly() {
        // Twelve per case, at six decimal places of factor.
        let cases = Quantity::parse("12").unwrap();
        let eaches = cases.scale_by(12_000_000, 6).unwrap();
        assert_eq!(eaches.to_display_string(), "144");

        // 0.4536 kg per pound, four decimal places.
        let pounds = Quantity::parse("10").unwrap();
        assert_eq!(
            pounds.scale_by(4536, 4).unwrap().to_display_string(),
            "4.536"
        );
    }

    #[test]
    fn rounding_a_conversion_goes_away_from_zero_on_both_sides() {
        let half = Quantity::from_scaled(1).unwrap();
        assert_eq!(half.scale_by(5, 1).unwrap().scaled(), 1);
        assert_eq!(half.negate().scale_by(5, 1).unwrap().scaled(), -1);
    }

    #[test]
    fn a_quantity_crosses_the_wire_as_a_decimal_string() {
        let quantity = Quantity::parse("0.000001").unwrap();
        let json = serde_json::to_string(&quantity).unwrap();

        assert_eq!(json, "\"0.000001\"");
        assert_eq!(serde_json::from_str::<Quantity>(&json).unwrap(), quantity);
    }
}
