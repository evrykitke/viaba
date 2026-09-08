//! What stock cost, and what leaves the stock account when it goes out.
//!
//! # A layer per receipt, whatever the costing method
//!
//! Every move that brings value in writes a layer; every move that takes value
//! out consumes one or more. Under FIFO the layers are consumed oldest first
//! and each carries its own cost. Under average and standard there is one cost
//! for everything, and the layers are still written - because a layer is also
//! the record of *what a receipt cost*, and a workspace that switches costing
//! method next year would otherwise have thrown that away.
//!
//! # Rounding happens once, on the value
//!
//! `quantity * unit_cost` is rounded to the money scale once, when the layer is
//! written, and the stored value is what the journal posts. Recomputing it from
//! a rounded unit cost at report time is how a stock account and a stock report
//! come to differ by pennies that nobody can explain.
//!
//! # The average is recomputed on receipt, never on issue
//!
//! [`weighted_average`] runs when stock comes in. An issue takes the average as
//! it stands, which is what makes the average stable across a day's picking:
//! recomputing on the way out would let the order two pickers happened to work
//! in change what the month cost.

use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError, Rounding};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::category::CostingMethod;
use crate::quantity::Quantity;

/// What a receipt put in, and how much of it is left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Uuid,
    /// The move that created it.
    pub move_id: Uuid,
    pub variant_id: Uuid,
    /// What came in.
    pub quantity: Quantity,
    /// What has not been consumed yet. Zero for a spent layer.
    pub remaining: Quantity,
    pub unit_cost: Money,
    /// `quantity * unit_cost`, rounded once when this was written.
    pub value: Money,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// One layer's share of an issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consumed {
    pub layer_id: Uuid,
    pub quantity: Quantity,
    pub unit_cost: Money,
    pub value: Money,
}

/// What an issue cost in total, and which layers paid for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub lines: Vec<Consumed>,
    pub value: Money,
}

impl Issue {
    /// The cost of one unit across the whole issue, for a move row that keeps
    /// one number. Zero quantity has no unit cost and answers zero.
    pub fn unit_cost(&self, quantity: Quantity) -> Result<Money, ValuationError> {
        if quantity.is_zero() {
            return Ok(Money::zero(self.value.currency()));
        }

        Ok(self
            .value
            .scale_by(crate::quantity::SCALE_FACTOR, quantity.scaled(), Rounding::HalfUp)?)
    }
}

/// `quantity * unit_cost`, rounded once.
pub fn value_of(quantity: Quantity, unit_cost: Money) -> Result<Money, ValuationError> {
    Ok(unit_cost.scale_by(
        quantity.scaled(),
        crate::quantity::SCALE_FACTOR,
        Rounding::HalfUp,
    )?)
}

/// What one unit of `value` spread over `quantity` costs.
pub fn unit_cost_of(quantity: Quantity, value: Money) -> Result<Money, ValuationError> {
    if quantity.is_zero() {
        return Err(ValuationError::NoQuantity);
    }

    Ok(value.scale_by(crate::quantity::SCALE_FACTOR, quantity.scaled(), Rounding::HalfUp)?)
}

/// The new average after a receipt.
///
/// `(on_hand * current + incoming * incoming_cost) / (on_hand + incoming)`, in
/// one division so the intermediate is never rounded. Receiving into nothing
/// answers the incoming cost, which is the only sensible reading of "the
/// average of one receipt".
pub fn weighted_average(
    on_hand: Quantity,
    current: Money,
    incoming: Quantity,
    incoming_cost: Money,
) -> Result<Money, ValuationError> {
    if incoming.is_zero() {
        return Ok(current);
    }

    let total = on_hand
        .checked_add(incoming)
        .map_err(|_| ValuationError::OutOfRange)?;

    // Stock that has run down to nothing - or into a negative that a virtual
    // location is allowed to be - has no average to blend with.
    if !total.is_positive() {
        return Ok(incoming_cost);
    }

    let held = value_of(on_hand, current)?;
    let arriving = value_of(incoming, incoming_cost)?;
    let combined = held.checked_add(arriving)?;

    unit_cost_of(total, combined)
}

/// Spend `quantity` out of `layers`, oldest first.
///
/// The caller orders the layers by the category's removal strategy before
/// handing them over - FIFO by creation, FEFO by expiry - because this function
/// knows about cost and nothing about shelves.
///
/// A short run of layers is not clamped: under FIFO, issuing more than was ever
/// received means a unit with no cost behind it, and inventing one is exactly
/// the guesswork ADR 0006 section 6.6 refuses.
pub fn consume_fifo(layers: &[Layer], quantity: Quantity) -> Result<Issue, ValuationError> {
    if quantity.is_negative() {
        return Err(ValuationError::NegativeIssue);
    }

    let currency = layers
        .first()
        .map(|layer| layer.unit_cost.currency())
        .ok_or(ValuationError::NoLayers)?;

    let mut left = quantity;
    let mut lines: Vec<Consumed> = Vec::new();

    for layer in layers {
        if left.is_zero() {
            break;
        }
        if !layer.remaining.is_positive() {
            continue;
        }

        let taken = if layer.remaining.compare(left).is_le() {
            layer.remaining
        } else {
            left
        };

        lines.push(Consumed {
            layer_id: layer.id,
            quantity: taken,
            unit_cost: layer.unit_cost,
            value: value_of(taken, layer.unit_cost)?,
        });

        left = left.checked_sub(taken).map_err(|_| ValuationError::OutOfRange)?;
    }

    if !left.is_zero() {
        return Err(ValuationError::LayersExhausted { short: left });
    }

    let value = Money::total(currency, lines.iter().map(|line| line.value))?;

    Ok(Issue { lines, value })
}

/// What one unit leaving costs, given how the category says stock is costed.
///
/// Standard and average both answer one number and consume no layer; FIFO is
/// the one that needs [`consume_fifo`], and this returns `None` to say so.
pub fn issue_cost(method: CostingMethod, standing: Money) -> Option<Money> {
    match method {
        CostingMethod::Standard | CostingMethod::Average => Some(standing),
        CostingMethod::Fifo => None,
    }
}

/// The difference between what a receipt was expected to cost and what it did.
///
/// Non-zero only under [`CostingMethod::Standard`], where it is the whole
/// point: the variance is a number somebody can look at rather than a drift in
/// the stock value that nobody can.
pub fn price_variance(
    method: CostingMethod,
    quantity: Quantity,
    standard: Money,
    actual: Money,
) -> Result<Money, ValuationError> {
    if !matches!(method, CostingMethod::Standard) {
        return Ok(Money::zero(standard.currency()));
    }

    let at_standard = value_of(quantity, standard)?;
    let at_actual = value_of(quantity, actual)?;

    Ok(at_actual.checked_sub(at_standard)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValuationError {
    #[error("there are no cost layers to take this from")]
    NoLayers,
    #[error("the cost layers do not cover that much")]
    LayersExhausted { short: Quantity },
    #[error("a cost cannot be worked out for no quantity")]
    NoQuantity,
    #[error("an issue is a positive quantity")]
    NegativeIssue,
    #[error("that value is too large")]
    OutOfRange,
    #[error("two currencies cannot be added")]
    CurrencyMismatch,
}

impl From<MoneyError> for ValuationError {
    fn from(err: MoneyError) -> Self {
        match err {
            MoneyError::CurrencyMismatch { .. } => Self::CurrencyMismatch,
            _ => Self::OutOfRange,
        }
    }
}

impl ValuationError {
    pub fn message(self) -> Message {
        match self {
            Self::NoLayers => msg!("valuation.error.no_layers"),
            Self::LayersExhausted { short } => {
                msg!("valuation.error.layers_exhausted", short = short.to_display_string())
            }
            Self::NoQuantity => msg!("valuation.error.no_quantity"),
            Self::NegativeIssue => msg!("valuation.error.negative_issue"),
            Self::OutOfRange => msg!("valuation.error.out_of_range"),
            Self::CurrencyMismatch => msg!("valuation.error.currency_mismatch"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use phonix_core::locale::Currency;

    fn gbp(amount: &str) -> Money {
        Money::parse(Currency::parse("GBP").unwrap(), amount).unwrap()
    }

    fn qty(amount: &str) -> Quantity {
        Quantity::parse(amount).unwrap()
    }

    fn layer(id: u128, remaining: &str, unit_cost: &str) -> Layer {
        let quantity = qty(remaining);
        let cost = gbp(unit_cost);

        Layer {
            id: Uuid::from_u128(id),
            move_id: Uuid::from_u128(id + 100),
            variant_id: Uuid::from_u128(1),
            quantity,
            remaining: quantity,
            unit_cost: cost,
            value: value_of(quantity, cost).unwrap(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn a_value_is_rounded_once_rather_than_per_report() {
        // 3 at 1.005 is 3.015, and the stored value is what the journal posts.
        assert_eq!(value_of(qty("3"), gbp("1.005")).unwrap(), gbp("3.015"));
    }

    #[test]
    fn fifo_takes_the_oldest_layer_first_and_splits_the_next() {
        let layers = [layer(1, "10", "2.00"), layer(2, "10", "3.00")];
        let issue = consume_fifo(&layers, qty("15")).unwrap();

        assert_eq!(issue.lines.len(), 2);
        assert_eq!(issue.lines[0].quantity, qty("10"));
        assert_eq!(issue.lines[1].quantity, qty("5"));
        // 10 at 2.00 and 5 at 3.00.
        assert_eq!(issue.value, gbp("35.00"));
        assert_eq!(issue.unit_cost(qty("15")).unwrap(), gbp("2.3333"));
    }

    #[test]
    fn issuing_more_than_was_ever_received_is_refused() {
        // Section 6.6 again, seen from the valuation side: a unit with no layer
        // behind it has no cost, and inventing one is the guesswork.
        let layers = [layer(1, "10", "2.00")];

        assert_eq!(
            consume_fifo(&layers, qty("12")),
            Err(ValuationError::LayersExhausted { short: qty("2") })
        );
    }

    #[test]
    fn a_spent_layer_is_stepped_over_rather_than_consumed_twice() {
        let mut spent = layer(1, "10", "2.00");
        spent.remaining = Quantity::ZERO;

        let layers = [spent, layer(2, "10", "3.00")];
        let issue = consume_fifo(&layers, qty("4")).unwrap();

        assert_eq!(issue.lines.len(), 1);
        assert_eq!(issue.lines[0].layer_id, Uuid::from_u128(2));
        assert_eq!(issue.value, gbp("12.00"));
    }

    #[test]
    fn the_average_moves_towards_what_just_arrived() {
        // 10 at 2.00 plus 10 at 3.00 is 20 at 2.50.
        let blended =
            weighted_average(qty("10"), gbp("2.00"), qty("10"), gbp("3.00")).unwrap();

        assert_eq!(blended, gbp("2.50"));
    }

    #[test]
    fn receiving_into_an_empty_shelf_takes_the_price_that_arrived() {
        let first = weighted_average(Quantity::ZERO, gbp("0"), qty("5"), gbp("4.20")).unwrap();

        assert_eq!(first, gbp("4.20"));
    }

    #[test]
    fn a_standard_cost_sends_the_difference_to_variance() {
        // The whole point of a standard: the difference is a number an
        // accountant looks at, not a drift in what the shelf is said to be
        // worth.
        let variance =
            price_variance(CostingMethod::Standard, qty("100"), gbp("2.00"), gbp("2.15")).unwrap();

        assert_eq!(variance, gbp("15.00"));
        // Under the other two the price simply becomes the cost.
        assert_eq!(
            price_variance(CostingMethod::Average, qty("100"), gbp("2.00"), gbp("2.15")).unwrap(),
            gbp("0")
        );
    }

    #[test]
    fn fifo_is_the_one_method_that_has_to_read_the_layers() {
        assert_eq!(issue_cost(CostingMethod::Fifo, gbp("2.00")), None);
        assert_eq!(
            issue_cost(CostingMethod::Average, gbp("2.00")),
            Some(gbp("2.00"))
        );
    }
}
