//! Freight, duty and handling, put where they belong.
//!
//! # The gap this closes
//!
//! ADR 0006 section 6.2. The carrier invoices three weeks after the lorry, the
//! invoice is coded to a freight account, and the goods sit on the balance
//! sheet at the price the supplier charged. Inventory is carried below what it
//! cost, every margin on every sale of those goods is overstated, and nothing
//! looks wrong: the freight bill is a real expense and the receipt was valued at
//! a real price.
//!
//! # This is not the freight bill
//!
//! The carrier's invoice is an ordinary bill. This document is the
//! *allocation* - it names a receipt, names the charges, and moves that money
//! out of expense and into the value of what is on the shelf. They are separate
//! because the two facts arrive weeks apart and from different people, and a
//! model that needed them together is one in which nothing is ever capitalised.
//!
//! # Each charge keeps its own basis
//!
//! Freight goes by weight, duty and insurance by value, a customs clearance fee
//! by the carton. One basis for a document holding all three would make two of
//! them wrong, and "why is this unit 4.12 and that one 4.09" is answerable a
//! year later only if each charge kept the basis it was actually spread on.
//!
//! # Capitalised is not all of it
//!
//! Freight arriving six weeks late is freight on stock that has partly been
//! sold. The share belonging to units still in the layer goes into the layer;
//! the share belonging to units already issued cannot, because those units are
//! gone and their cost went to cost of sales at a figure that was too low. That
//! share goes to cost of sales too - where it would have gone had the carrier
//! invoiced on time.
//!
//! Putting all of it into the layer would value the units that are left at the
//! freight for units that are not, which is section 6.2's error arrived at from
//! the other side.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError, Rounding};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

pub const MAX_LANDED_COST_NOTE_LEN: usize = 2000;
pub const MAX_CHARGE_DESCRIPTION_LEN: usize = 200;

/// How a charge is spread over the lines it landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationBasis {
    /// In proportion to what each line cost. Duty, insurance, and anything a
    /// third party charged as a percentage.
    Value,
    /// In proportion to how many units arrived. A per-carton handling fee.
    Quantity,
    /// In proportion to what each line weighs. Freight, which is what a carrier
    /// actually charges on.
    Weight,
}

impl AllocationBasis {
    pub const ALL: &'static [Self] = &[Self::Value, Self::Quantity, Self::Weight];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Quantity => "quantity",
            Self::Weight => "weight",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|basis| basis.as_str() == raw)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Value => msg!("landed_costs.basis.value"),
            Self::Quantity => msg!("landed_costs.basis.quantity"),
            Self::Weight => msg!("landed_costs.basis.weight"),
        }
    }

    /// What a person needs to know before choosing it.
    pub fn explain(self) -> Message {
        match self {
            Self::Value => msg!("landed_costs.basis.value_explained"),
            Self::Quantity => msg!("landed_costs.basis.quantity_explained"),
            Self::Weight => msg!("landed_costs.basis.weight_explained"),
        }
    }
}

/// Draft until somebody posts it, and then never editable again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandedCostState {
    Draft,
    Done,
    Cancelled,
}

impl LandedCostState {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Done, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|state| state.as_str() == raw)
    }

    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    pub const fn is_posted(self) -> bool {
        matches!(self, Self::Done)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("landed_costs.state.draft"),
            Self::Done => msg!("landed_costs.state.done"),
            Self::Cancelled => msg!("landed_costs.state.cancelled"),
        }
    }
}

/// One thing being spread, and what it is spread on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Charge {
    pub id: Uuid,
    pub line_no: i32,
    /// What the carrier called it.
    pub description: String,
    pub basis: AllocationBasis,
    /// In the workspace's base currency. Negative is a credit note from the
    /// carrier, and is how a landed cost is corrected.
    pub amount: Money,
}

/// What one charge did to one received line. Written at post and never again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    pub id: Uuid,
    pub charge_id: Uuid,
    /// Snapshot, so a row reads without a join.
    pub charge_description: String,
    pub receipt_line_id: Uuid,
    pub layer_id: Uuid,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    pub basis: AllocationBasis,
    /// What this line contributed to the basis: its value, its quantity, or its
    /// weight in grams.
    pub basis_amount: Quantity,
    pub amount: Money,
    /// The part that went into the value of stock still on the shelf.
    pub capitalised: Money,
    /// The part that belonged to units already issued.
    pub expensed: Money,
}

/// Where a journal for this document landed, on the three terms a stock move
/// takes - see [`crate::movement::JournalOutcome`], which this mirrors because
/// a landed cost posts through the same port for the same reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandedCost {
    pub id: Uuid,
    /// `LC-2026-00007`. Empty until posted.
    pub number: String,
    pub state: LandedCostState,
    pub receipt_id: Uuid,
    pub receipt_number: String,
    pub supplier_name: String,
    /// When the charge was incurred, which is not when the goods arrived.
    pub cost_date: NaiveDate,
    pub note: Option<String>,
    pub total: Money,
    pub capitalised: Money,
    pub expensed: Money,
    pub journal_number: Option<String>,
    pub charges: Vec<Charge>,
    /// Empty until posted.
    pub allocations: Vec<Allocation>,
}

impl LandedCost {
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} · {}", self.cost_date, self.supplier_name)
        } else {
            self.number.clone()
        }
    }

    /// Whether anything on it would actually move value.
    pub fn has_charges(&self) -> bool {
        !self.charges.is_empty()
    }

    /// Every basis this document actually uses, in declaration order. What the
    /// screen labels the basis column with.
    pub fn bases(&self) -> Vec<AllocationBasis> {
        AllocationBasis::ALL
            .iter()
            .copied()
            .filter(|basis| self.charges.iter().any(|charge| charge.basis == *basis))
            .collect()
    }
}

/// One row of the landed-cost grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandedCostSummary {
    pub id: Uuid,
    pub number: String,
    pub state: LandedCostState,
    pub receipt_id: Uuid,
    pub receipt_number: String,
    pub supplier_name: String,
    pub cost_date: NaiveDate,
    pub total: Money,
    pub capitalised: Money,
    pub charge_count: i64,
}

/// What one delivery has been landed with, in total.
///
/// The receipt screen's question, and the only figure on it that is not the
/// receipt's own: a delivery valued at 4,000 on Tuesday is worth 4,420 once the
/// freight lands, and a screen showing only the first is showing a number
/// nobody can reconcile the stock account to. Posted documents only - a draft
/// has changed nothing yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptLandedCost {
    pub receipt_id: Uuid,
    pub document_count: i64,
    pub total: Money,
    pub capitalised: Money,
    pub expensed: Money,
    pub latest_on: NaiveDate,
}

// --- The allocation -------------------------------------------------------

/// A received line the allocation can reach: one that carries value.
///
/// A receipt line whose item holds no stock has no valuation layer, and is not
/// here. It is dropped from the basis rather than given a share it has nowhere
/// to put.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landable {
    pub receipt_line_id: Uuid,
    pub layer_id: Uuid,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    /// What the layer took in.
    pub quantity: Quantity,
    /// What is left in it. The ratio that decides capitalised against expensed.
    pub remaining: Quantity,
    /// What the layer cost, before anything was landed on it.
    pub value: Money,
    /// Per stock unit, from the item. `None` for an item nobody weighed, which
    /// is why the weight basis can find nothing to spread on.
    pub weight_grams: Option<i64>,
}

impl Landable {
    /// What this line contributes to a basis.
    pub fn basis_amount(&self, basis: AllocationBasis) -> Result<Quantity, LandedCostError> {
        match basis {
            // Money is four decimal places and a quantity is six, so the value
            // moves up two to be compared against the others in one type.
            AllocationBasis::Value => Ok(Quantity::from_scaled(
                self.value
                    .scaled()
                    .checked_mul(MONEY_TO_QUANTITY)
                    .ok_or(LandedCostError::BasisTooLarge)?,
            )?),
            AllocationBasis::Quantity => Ok(self.quantity),
            AllocationBasis::Weight => match self.weight_grams {
                None => Ok(Quantity::ZERO),
                Some(grams) => Ok(self.quantity.scale_by(i128::from(grams), 0)?),
            },
        }
    }
}

/// `10^(Quantity::SCALE - Money::SCALE)`. What lifts a money amount into the
/// scale a basis is compared in.
const MONEY_TO_QUANTITY: i128 = 100;

/// One line's share of one charge, before it has an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub charge_id: Uuid,
    pub receipt_line_id: Uuid,
    pub layer_id: Uuid,
    pub variant_id: Uuid,
    pub basis: AllocationBasis,
    pub basis_amount: Quantity,
    pub amount: Money,
    pub capitalised: Money,
    pub expensed: Money,
}

/// What a document adds up to once it has been spread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spread {
    pub shares: Vec<Share>,
    pub total: Money,
    pub capitalised: Money,
    pub expensed: Money,
}

/// Spread every charge over every line that can hold it.
///
/// Largest-remainder within each charge, so the shares add back up to the
/// charge exactly - a penny short here is a penny by which the stock account
/// and the stock ledger would disagree for ever, and reconciling that is what
/// ADR 0006 section 6.7 exists to avoid.
///
/// A line contributing nothing to a basis - an unweighed item under the weight
/// basis - gets no row at all rather than a row for zero.
pub fn spread(
    charges: &[Charge],
    lines: &[Landable],
    currency: phonix_core::locale::Currency,
) -> Result<Spread, LandedCostError> {
    if charges.is_empty() {
        return Err(LandedCostError::NothingToSpread);
    }
    if lines.is_empty() {
        return Err(LandedCostError::NothingToSpreadOver);
    }

    let mut shares: Vec<Share> = Vec::new();

    for charge in charges {
        let bases = lines
            .iter()
            .map(|line| line.basis_amount(charge.basis))
            .collect::<Result<Vec<_>, _>>()?;

        let weights = bases
            .iter()
            .map(|amount| i64::try_from(amount.scaled()).map_err(|_| LandedCostError::BasisTooLarge))
            .collect::<Result<Vec<_>, _>>()?;

        if weights.iter().all(|weight| *weight == 0) {
            return Err(LandedCostError::NoBasis { basis: charge.basis });
        }
        // A negative basis cannot happen - a layer's quantity and value are
        // both positive by CHECK - but `allocate` refuses one, and saying so
        // here names the line rather than the arithmetic.
        if weights.iter().any(|weight| *weight < 0) {
            return Err(LandedCostError::NoBasis { basis: charge.basis });
        }

        let portions = charge.amount.allocate(&weights)?;

        for ((line, basis_amount), amount) in lines.iter().zip(bases).zip(portions) {
            if amount.is_zero() {
                continue;
            }

            let capitalised = split_capitalised(amount, line)?;
            let expensed = amount.checked_sub(capitalised)?;

            shares.push(Share {
                charge_id: charge.id,
                receipt_line_id: line.receipt_line_id,
                layer_id: line.layer_id,
                variant_id: line.variant_id,
                basis: charge.basis,
                basis_amount,
                amount,
                capitalised,
                expensed,
            });
        }
    }

    Ok(Spread {
        total: Money::total(currency, charges.iter().map(|charge| charge.amount))?,
        capitalised: Money::total(currency, shares.iter().map(|share| share.capitalised))?,
        expensed: Money::total(currency, shares.iter().map(|share| share.expensed))?,
        shares,
    })
}

/// The part of a share that belongs to units still in the layer.
///
/// The remainder is not recomputed - it is subtracted - so the two halves add
/// back up to the share whatever the rounding did.
fn split_capitalised(amount: Money, line: &Landable) -> Result<Money, LandedCostError> {
    if !line.quantity.is_positive() {
        return Err(LandedCostError::LayerEmpty);
    }
    if line.remaining.compare(line.quantity).is_ge() {
        return Ok(amount);
    }
    if !line.remaining.is_positive() {
        return Ok(Money::zero(amount.currency()));
    }

    Ok(amount.scale_by(line.remaining.scaled(), line.quantity.scaled(), Rounding::HalfUp)?)
}

// --- Input ----------------------------------------------------------------

/// The editable part of a landed cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandedCostInput {
    pub id: Option<Uuid>,
    pub receipt_id: Option<Uuid>,
    pub cost_date: NaiveDate,
    pub note: String,
    pub charges: Vec<ChargeInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChargeInput {
    pub id: Option<Uuid>,
    pub description: String,
    pub basis: AllocationBasis,
    /// As typed. Parsing needs the workspace's currency.
    pub amount: String,
}

impl ChargeInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            description: String::new(),
            // Freight is the charge somebody is nearly always keying, and it is
            // charged by weight.
            basis: AllocationBasis::Weight,
            amount: String::new(),
        }
    }

    fn is_blank(&self) -> bool {
        self.description.trim().is_empty() && self.amount.trim().is_empty()
    }
}

impl LandedCostInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            receipt_id: None,
            cost_date: today,
            note: String::new(),
            charges: vec![ChargeInput::blank()],
        }
    }

    /// Against one delivery, which is how this screen is nearly always reached.
    pub fn against(receipt_id: Uuid, today: NaiveDate) -> Self {
        Self {
            receipt_id: Some(receipt_id),
            ..Self::blank(today)
        }
    }

    /// Reopen a draft for editing.
    pub fn from_document(document: &LandedCost) -> Self {
        Self {
            id: Some(document.id),
            receipt_id: Some(document.receipt_id),
            cost_date: document.cost_date,
            note: document.note.clone().unwrap_or_default(),
            charges: document
                .charges
                .iter()
                .map(|charge| ChargeInput {
                    id: Some(charge.id),
                    description: charge.description.clone(),
                    basis: charge.basis,
                    amount: charge.amount.to_storage_string(),
                })
                .collect(),
        }
    }

    /// Everything decidable without the database.
    pub fn check(&self) -> Result<CheckedLandedCost, LandedCostError> {
        let receipt_id = self.receipt_id.ok_or(LandedCostError::ReceiptRequired)?;

        if self.note.chars().count() > MAX_LANDED_COST_NOTE_LEN {
            return Err(LandedCostError::NoteTooLong);
        }

        let mut charges = Vec::new();

        for charge in &self.charges {
            if charge.is_blank() {
                continue;
            }

            let description = charge.description.trim();
            if description.is_empty() {
                return Err(LandedCostError::DescriptionRequired);
            }
            if description.chars().count() > MAX_CHARGE_DESCRIPTION_LEN {
                return Err(LandedCostError::DescriptionTooLong);
            }
            if charge.amount.trim().is_empty() {
                return Err(LandedCostError::AmountRequired);
            }

            charges.push(CheckedCharge {
                id: charge.id,
                description: description.to_owned(),
                basis: charge.basis,
                amount: charge.amount.trim().to_owned(),
            });
        }

        if charges.is_empty() {
            return Err(LandedCostError::NothingToSpread);
        }

        Ok(CheckedLandedCost {
            id: self.id,
            receipt_id,
            cost_date: self.cost_date,
            note: non_empty(&self.note),
            charges,
        })
    }
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedLandedCost {
    pub id: Option<Uuid>,
    pub receipt_id: Uuid,
    pub cost_date: NaiveDate,
    pub note: Option<String>,
    pub charges: Vec<CheckedCharge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedCharge {
    pub id: Option<Uuid>,
    pub description: String,
    pub basis: AllocationBasis,
    /// Still text: parsing needs the workspace's currency.
    pub amount: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LandedCostError {
    #[error("a landed cost needs a delivery to land on")]
    ReceiptRequired,
    #[error("a landed cost needs at least one charge")]
    NothingToSpread,
    #[error("there is nothing on this delivery that can carry a cost")]
    NothingToSpreadOver,
    #[error("a charge needs a description")]
    DescriptionRequired,
    #[error("a description is at most 200 characters")]
    DescriptionTooLong,
    #[error("a charge needs an amount")]
    AmountRequired,
    #[error("a charge of nothing spreads nothing")]
    AmountZero,
    #[error("nothing on this delivery has that to spread on")]
    NoBasis { basis: AllocationBasis },
    #[error("that basis is too large to spread")]
    BasisTooLarge,
    #[error("a cost layer holding nothing cannot be landed on")]
    LayerEmpty,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("only a posted delivery can be landed on")]
    ReceiptNotPosted,
    #[error("a posted landed cost cannot be changed")]
    NotEditable,
    #[error("that amount is not an amount")]
    Money(#[from] MoneyError),
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
}

impl LandedCostError {
    pub fn field(self) -> &'static str {
        match self {
            Self::ReceiptRequired | Self::ReceiptNotPosted | Self::NothingToSpreadOver => {
                "receipt_id"
            }
            Self::NothingToSpread | Self::DescriptionRequired | Self::DescriptionTooLong => {
                "charges"
            }
            Self::AmountRequired | Self::AmountZero | Self::Money(_) => "amount",
            Self::NoBasis { .. } => "basis",
            Self::BasisTooLarge | Self::LayerEmpty | Self::Quantity(_) => "charges",
            Self::NoteTooLong => "note",
            Self::NotEditable => "state",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::ReceiptRequired => msg!("landed_costs.error.receipt_required"),
            Self::NothingToSpread => msg!("landed_costs.error.nothing_to_spread"),
            Self::NothingToSpreadOver => msg!("landed_costs.error.nothing_to_spread_over"),
            Self::DescriptionRequired => msg!("landed_costs.error.description_required"),
            Self::DescriptionTooLong => msg!("landed_costs.error.description_too_long"),
            Self::AmountRequired => msg!("landed_costs.error.amount_required"),
            Self::AmountZero => msg!("landed_costs.error.amount_zero"),
            Self::NoBasis { basis } => msg!("landed_costs.error.no_basis", basis = basis.as_str()),
            Self::BasisTooLarge => msg!("landed_costs.error.basis_too_large"),
            Self::LayerEmpty => msg!("landed_costs.error.layer_empty"),
            Self::NoteTooLong => msg!("landed_costs.error.note_too_long"),
            Self::ReceiptNotPosted => msg!("landed_costs.error.receipt_not_posted"),
            Self::NotEditable => msg!("landed_costs.error.not_editable"),
            Self::Money(err) => err.message(),
            Self::Quantity(err) => err.message(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use phonix_core::locale::Currency;

    fn currency() -> Currency {
        Currency::parse("GBP").expect("GBP")
    }

    fn gbp(amount: &str) -> Money {
        Money::parse(currency(), amount).expect("amount")
    }

    fn qty(amount: &str) -> Quantity {
        Quantity::parse(amount).expect("quantity")
    }

    fn charge(id: u128, basis: AllocationBasis, amount: &str) -> Charge {
        Charge {
            id: Uuid::from_u128(id),
            line_no: 1,
            description: "Sea freight".to_owned(),
            basis,
            amount: gbp(amount),
        }
    }

    /// A line whose layer is untouched: everything is still on the shelf.
    fn line(id: u128, quantity: &str, unit_cost: &str, grams: Option<i64>) -> Landable {
        let quantity = qty(quantity);
        let value = Money::parse(currency(), unit_cost)
            .expect("cost")
            .scale_by(quantity.scaled(), crate::quantity::SCALE_FACTOR, Rounding::HalfUp)
            .expect("value");

        Landable {
            receipt_line_id: Uuid::from_u128(id),
            layer_id: Uuid::from_u128(id + 1000),
            variant_id: Uuid::from_u128(id + 2000),
            variant_code: format!("ITM-{id:05}"),
            description: "A thing".to_owned(),
            quantity,
            remaining: quantity,
            value,
            weight_grams: grams,
        }
    }

    #[test]
    fn freight_follows_the_weight_and_not_the_price() {
        // The case that makes the basis matter. Two lines of equal value, one
        // of which weighs nine times the other: a carrier charges on the
        // second, and spreading by value would put the same freight on both.
        let lines = [
            line(1, "10", "10.00", Some(100)),
            line(2, "10", "10.00", Some(900)),
        ];

        let by_weight = spread(&[charge(9, AllocationBasis::Weight, "100.00")], &lines, currency())
            .expect("spread");

        assert_eq!(by_weight.shares[0].amount, gbp("10.00"));
        assert_eq!(by_weight.shares[1].amount, gbp("90.00"));

        let by_value = spread(&[charge(9, AllocationBasis::Value, "100.00")], &lines, currency())
            .expect("spread");

        assert_eq!(by_value.shares[0].amount, gbp("50.00"));
        assert_eq!(by_value.shares[1].amount, gbp("50.00"));
    }

    #[test]
    fn the_shares_add_back_up_to_the_charge_exactly() {
        // A hundred pounds three ways is where a naive proportion loses a
        // penny, and the penny it loses is one the stock account and the stock
        // ledger then disagree by for ever.
        let lines = [
            line(1, "1", "1.00", Some(1)),
            line(2, "1", "1.00", Some(1)),
            line(3, "1", "1.00", Some(1)),
        ];

        let spread = spread(&[charge(9, AllocationBasis::Quantity, "100.00")], &lines, currency())
            .expect("spread");

        assert_eq!(
            Money::total(currency(), spread.shares.iter().map(|share| share.amount)).expect("total"),
            gbp("100.00")
        );
        assert_eq!(spread.total, gbp("100.00"));
        // Largest remainder, ties by position: the odd penny goes to the first.
        assert_eq!(spread.shares[0].amount, gbp("33.34"));
        assert_eq!(spread.shares[2].amount, gbp("33.33"));
    }

    #[test]
    fn freight_on_stock_already_sold_goes_to_cost_of_sales() {
        // Six weeks late, and three quarters of the delivery has gone out. The
        // share on what is left is capitalised; the rest cannot be, because
        // those units are not there to carry it.
        let mut sold = line(1, "100", "5.00", Some(1));
        sold.remaining = qty("25");

        let spread =
            spread(&[charge(9, AllocationBasis::Quantity, "400.00")], &[sold], currency())
                .expect("spread");

        assert_eq!(spread.capitalised, gbp("100.00"));
        assert_eq!(spread.expensed, gbp("300.00"));
        // And the two halves are the whole, which is the CHECK on the row.
        assert_eq!(
            spread.capitalised.checked_add(spread.expensed).expect("sum"),
            spread.total
        );
    }

    #[test]
    fn a_layer_that_has_run_out_capitalises_nothing() {
        let mut gone = line(1, "40", "2.00", Some(1));
        gone.remaining = Quantity::ZERO;

        let spread = spread(&[charge(9, AllocationBasis::Value, "80.00")], &[gone], currency())
            .expect("spread");

        assert_eq!(spread.capitalised, gbp("0"));
        assert_eq!(spread.expensed, gbp("80.00"));
    }

    #[test]
    fn an_unweighed_item_gets_no_row_rather_than_a_row_for_nothing() {
        let lines = [
            line(1, "10", "10.00", Some(500)),
            line(2, "10", "10.00", None),
        ];

        let spread = spread(&[charge(9, AllocationBasis::Weight, "60.00")], &lines, currency())
            .expect("spread");

        assert_eq!(spread.shares.len(), 1);
        assert_eq!(spread.shares[0].receipt_line_id, Uuid::from_u128(1));
        assert_eq!(spread.shares[0].amount, gbp("60.00"));
    }

    #[test]
    fn a_charge_with_nothing_to_spread_on_is_refused_rather_than_dropped() {
        // Nobody weighed anything on this delivery. Silently spreading it by
        // value instead would be the system deciding an accounting question.
        let lines = [line(1, "10", "10.00", None)];

        assert_eq!(
            spread(&[charge(9, AllocationBasis::Weight, "60.00")], &lines, currency()),
            Err(LandedCostError::NoBasis {
                basis: AllocationBasis::Weight
            })
        );
    }

    #[test]
    fn each_charge_keeps_its_own_basis() {
        // The reason the basis is on the charge. Freight by weight and duty by
        // value, on one invoice, land differently on the same two lines.
        let lines = [
            line(1, "10", "1.00", Some(1000)),
            line(2, "10", "9.00", Some(1000)),
        ];

        let spread = spread(
            &[
                charge(1, AllocationBasis::Weight, "20.00"),
                charge(2, AllocationBasis::Value, "20.00"),
            ],
            &lines,
            currency(),
        )
        .expect("spread");

        assert_eq!(spread.shares.len(), 4);
        // Freight: equal weights, equal shares.
        assert_eq!(spread.shares[0].amount, gbp("10.00"));
        assert_eq!(spread.shares[1].amount, gbp("10.00"));
        // Duty: one line is nine times the other's value.
        assert_eq!(spread.shares[2].amount, gbp("2.00"));
        assert_eq!(spread.shares[3].amount, gbp("18.00"));
        assert_eq!(spread.total, gbp("40.00"));
    }

    #[test]
    fn a_credit_from_the_carrier_is_a_negative_charge() {
        // Section 6.3: corrections are reversing entries, never edits. An
        // overcharge refunded is a second document with a negative on it.
        let lines = [line(1, "10", "10.00", Some(100))];

        let spread = spread(&[charge(9, AllocationBasis::Value, "-25.00")], &lines, currency())
            .expect("spread");

        assert_eq!(spread.total, gbp("-25.00"));
        assert_eq!(spread.capitalised, gbp("-25.00"));
    }

    #[test]
    fn a_document_with_no_charges_is_refused_before_it_reaches_the_layers() {
        assert_eq!(
            spread(&[], &[line(1, "10", "10.00", Some(1))], currency()),
            Err(LandedCostError::NothingToSpread)
        );
        assert_eq!(
            spread(&[charge(9, AllocationBasis::Value, "10.00")], &[], currency()),
            Err(LandedCostError::NothingToSpreadOver)
        );
    }

    #[test]
    fn a_blank_charge_row_is_dropped_and_a_half_typed_one_is_not() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 9).expect("date");

        let input = LandedCostInput {
            id: None,
            receipt_id: Some(Uuid::from_u128(1)),
            cost_date: today,
            note: String::new(),
            charges: vec![
                ChargeInput {
                    id: None,
                    description: "Sea freight".to_owned(),
                    basis: AllocationBasis::Weight,
                    amount: "500.00".to_owned(),
                },
                ChargeInput::blank(),
            ],
        };

        let checked = input.check().expect("checked");
        assert_eq!(checked.charges.len(), 1);

        // A description with no amount is somebody who was interrupted, not an
        // empty row.
        let half = LandedCostInput {
            charges: vec![ChargeInput {
                id: None,
                description: "Import duty".to_owned(),
                basis: AllocationBasis::Value,
                amount: String::new(),
            }],
            ..input
        };

        assert_eq!(half.check(), Err(LandedCostError::AmountRequired));
    }

    #[test]
    fn a_document_needs_a_delivery_and_a_charge() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 9).expect("date");

        assert_eq!(
            LandedCostInput::blank(today).check(),
            Err(LandedCostError::ReceiptRequired)
        );

        let empty = LandedCostInput::against(Uuid::from_u128(1), today);
        assert_eq!(empty.check(), Err(LandedCostError::NothingToSpread));
    }

    #[test]
    fn every_basis_round_trips_through_its_stored_word() {
        for basis in AllocationBasis::ALL {
            assert_eq!(AllocationBasis::parse(basis.as_str()), Some(*basis));
        }
        for state in LandedCostState::ALL {
            assert_eq!(LandedCostState::parse(state.as_str()), Some(*state));
        }
    }
}
