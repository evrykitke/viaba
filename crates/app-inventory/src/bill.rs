//! The supplier's invoice, and the match that grades it.
//!
//! A tolerance decides who has to approve a difference, never whether it is
//! recorded: the variance always posts to purchase price variance. ADR 0006
//! section 7.

use chrono::{NaiveDate, NaiveDateTime};
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::purchase::SupplierSnapshot;
use crate::quantity::{Quantity, QuantityError};

/// Where a bill is. No `matched` state: the grade is derived from the
/// documents underneath rather than stored beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillState {
    Draft,
    Posted,
    Cancelled,
}

impl BillState {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Posted, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Posted => "posted",
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
        matches!(self, Self::Posted)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("bills.state.draft"),
            Self::Posted => msg!("bills.state.posted"),
            Self::Cancelled => msg!("bills.state.cancelled"),
        }
    }
}

/// How well this bill agrees with what was ordered and what arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchGrade {
    Clean,
    WithinTolerance,
    OverTolerance,
    /// Billed for more than was ever received.
    OverReceived,
    /// A line with no receipt behind it: a two-way match at best.
    NoReceipt,
    NoOrder,
    /// The order was confirmed after the goods arrived or the invoice was
    /// written, so the numbers agree by construction rather than by agreement.
    /// No tolerance catches this, because the arithmetic is perfect.
    Circular,
    /// One person confirmed the order and posted the receipt.
    SameHand,
}

impl MatchGrade {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::WithinTolerance => "within_tolerance",
            Self::OverTolerance => "over_tolerance",
            Self::OverReceived => "over_received",
            Self::NoReceipt => "no_receipt",
            Self::NoOrder => "no_order",
            Self::Circular => "circular",
            Self::SameHand => "same_hand",
        }
    }

    /// Whether posting needs somebody to say why in writing.
    pub const fn needs_override(self) -> bool {
        !matches!(self, Self::Clean | Self::WithinTolerance)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Clean => msg!("bills.match.clean"),
            Self::WithinTolerance => msg!("bills.match.within_tolerance"),
            Self::OverTolerance => msg!("bills.match.over_tolerance"),
            Self::OverReceived => msg!("bills.match.over_received"),
            Self::NoReceipt => msg!("bills.match.no_receipt"),
            Self::NoOrder => msg!("bills.match.no_order"),
            Self::Circular => msg!("bills.match.circular"),
            Self::SameHand => msg!("bills.match.same_hand"),
        }
    }

    /// The sentence shown beside the grade, saying what to do about it.
    pub fn detail(self) -> Message {
        match self {
            Self::Clean => msg!("bills.match.clean.detail"),
            Self::WithinTolerance => msg!("bills.match.within_tolerance.detail"),
            Self::OverTolerance => msg!("bills.match.over_tolerance.detail"),
            Self::OverReceived => msg!("bills.match.over_received.detail"),
            Self::NoReceipt => msg!("bills.match.no_receipt.detail"),
            Self::NoOrder => msg!("bills.match.no_order.detail"),
            Self::Circular => msg!("bills.match.circular.detail"),
            Self::SameHand => msg!("bills.match.same_hand.detail"),
        }
    }
}

/// How far a price may drift before somebody has to look. Seeded per ADR 0006
/// section 4, owned by the workspace afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tolerance {
    /// Scaled by 100, so `250` is 2.5%.
    pub price_percent: i64,
    pub price_cap: Money,
}

impl Tolerance {
    /// What a workspace gets before anybody changes it: 2.5%, capped at 50 of
    /// the currency's major unit. Small on purpose - a tolerance nothing ever
    /// trips is a control switched off without anybody deciding to.
    pub fn default_for(currency: phonix_core::locale::Currency) -> Self {
        Self {
            price_percent: 250,
            price_cap: Money::from_units(currency, 50).unwrap_or_else(|_| Money::zero(currency)),
        }
    }

    /// Both halves must agree: a percentage alone lets a large order hide a
    /// large variance, a cap alone punishes small ones.
    pub fn accepts(&self, accrued: Money, variance: Money) -> bool {
        let drift = variance.abs();

        if !matches!(drift.compare(self.price_cap), Ok(order) if !order.is_gt()) {
            return false;
        }

        let allowed = accrued.abs().scale_by(
            self.price_percent as i128,
            10_000,
            phonix_core::money::Rounding::HalfUp,
        );

        match allowed {
            Ok(allowed) => matches!(drift.compare(allowed), Ok(order) if !order.is_gt()),
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bill {
    pub id: Uuid,
    pub number: String,
    pub state: BillState,

    pub order_id: Option<Uuid>,
    pub order_number: Option<String>,

    pub supplier: SupplierSnapshot,
    pub supplier_reference: String,

    pub bill_date: NaiveDate,
    pub due_on: Option<NaiveDate>,

    pub currency: String,
    pub net: Money,
    /// What the receipts accrued for the lines this clears.
    pub accrued: Money,
    pub variance: Money,

    pub note: Option<String>,

    /// Why somebody posted this over a grade that did not clear.
    pub match_note: Option<String>,
    pub overridden_by: Option<Uuid>,
    pub overridden_at: Option<NaiveDateTime>,

    pub journal_id: Option<Uuid>,

    pub lines: Vec<BillLine>,
}

impl Bill {
    /// What to call it on a screen before it has a number.
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            return format!("{} {}", self.supplier.name, self.supplier_reference);
        }
        self.number.clone()
    }

    /// Whether they charged more than was accrued.
    pub fn is_overcharge(&self) -> bool {
        !self.variance.is_negative() && !self.variance.is_zero()
    }

    pub fn has_lines(&self) -> bool {
        !self.lines.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillLine {
    pub id: Uuid,
    pub line_no: i32,

    pub receipt_line_id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,

    /// `None` on a charge line - freight, a deposit, a rebate.
    pub variant_id: Option<Uuid>,
    pub variant_code: Option<String>,
    pub description: String,

    pub quantity: Quantity,
    pub unit_id: Option<Uuid>,
    pub unit_code: Option<String>,

    pub unit_price: Money,
    pub net: Money,
    pub accrued: Money,
}

impl BillLine {
    pub fn is_goods(&self) -> bool {
        self.variant_id.is_some()
    }

    pub fn variance(&self) -> Money {
        self.net
            .checked_sub(self.accrued)
            .unwrap_or_else(|_| Money::zero(self.net.currency()))
    }
}

/// One row of the bills grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillSummary {
    pub id: Uuid,
    pub number: String,
    pub state: BillState,
    pub supplier_name: String,
    pub supplier_reference: String,
    pub order_number: Option<String>,
    pub bill_date: NaiveDate,
    pub due_on: Option<NaiveDate>,
    pub net: Money,
    pub variance: Money,
    pub line_count: i64,
    pub was_overridden: bool,
}

/// The aged GRNI balance, from the `unbilled_receipts` view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnbilledReceipt {
    pub receipt_id: Uuid,
    pub number: String,
    pub received_on: NaiveDate,
    pub supplier_id: Uuid,
    pub supplier_name: String,
    pub order_number: Option<String>,
    pub unbilled: Money,
    pub age_days: i32,
}

impl UnbilledReceipt {
    /// Thirty-day buckets, the last open-ended.
    pub fn bucket(&self) -> AgeBucket {
        match self.age_days {
            ..=30 => AgeBucket::Current,
            31..=60 => AgeBucket::ThirtyDays,
            61..=90 => AgeBucket::SixtyDays,
            _ => AgeBucket::Older,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBucket {
    Current,
    ThirtyDays,
    SixtyDays,
    Older,
}

impl AgeBucket {
    pub fn label(self) -> Message {
        match self {
            Self::Current => msg!("bills.age.current"),
            Self::ThirtyDays => msg!("bills.age.thirty"),
            Self::SixtyDays => msg!("bills.age.sixty"),
            Self::Older => msg!("bills.age.older"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillInput {
    pub id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub supplier_reference: String,
    pub bill_date: NaiveDate,
    pub due_on: Option<NaiveDate>,
    pub currency: String,
    pub note: String,
    pub lines: Vec<BillLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillLineInput {
    pub id: Option<Uuid>,
    pub receipt_line_id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    pub quantity: String,
    pub unit_id: Option<Uuid>,
    pub unit_price: String,
}

impl BillLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            receipt_line_id: None,
            order_line_id: None,
            variant_id: None,
            description: String::new(),
            quantity: String::new(),
            unit_id: None,
            unit_price: String::new(),
        }
    }
}

impl BillInput {
    pub fn blank(today: NaiveDate, currency: &str) -> Self {
        Self {
            id: None,
            order_id: None,
            supplier_id: None,
            supplier_reference: String::new(),
            bill_date: today,
            due_on: None,
            currency: currency.to_owned(),
            note: String::new(),
            lines: vec![BillLineInput::blank()],
        }
    }

    pub fn from_bill(bill: &Bill) -> Self {
        Self {
            id: Some(bill.id),
            order_id: bill.order_id,
            supplier_id: Some(bill.supplier.party_id),
            supplier_reference: bill.supplier_reference.clone(),
            bill_date: bill.bill_date,
            due_on: bill.due_on,
            currency: bill.currency.clone(),
            note: bill.note.clone().unwrap_or_default(),
            lines: bill
                .lines
                .iter()
                .map(|line| BillLineInput {
                    id: Some(line.id),
                    receipt_line_id: line.receipt_line_id,
                    order_line_id: line.order_line_id,
                    variant_id: line.variant_id,
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    unit_id: line.unit_id,
                    unit_price: line.unit_price.to_storage_string(),
                })
                .collect(),
        }
    }

    /// Everything decidable without the database. Blank lines are dropped, so a
    /// form's trailing empty row does not refuse every save.
    pub fn check(&self) -> Result<CheckedBill, BillError> {
        let supplier_id = self.supplier_id.ok_or(BillError::SupplierRequired)?;

        let reference = self.supplier_reference.trim();
        if reference.is_empty() {
            return Err(BillError::ReferenceRequired);
        }
        if reference.chars().count() > 120 {
            return Err(BillError::ReferenceTooLong);
        }

        if self.currency.trim().is_empty() {
            return Err(BillError::CurrencyRequired);
        }

        if let Some(due) = self.due_on {
            if due < self.bill_date {
                return Err(BillError::DueBeforeDated);
            }
        }

        let note = self.note.trim();
        if note.chars().count() > 2000 {
            return Err(BillError::NoteTooLong);
        }

        let mut lines = Vec::with_capacity(self.lines.len());

        for line in &self.lines {
            if line.variant_id.is_none()
                && line.quantity.trim().is_empty()
                && line.description.trim().is_empty()
            {
                continue;
            }

            let quantity = Quantity::parse(line.quantity.trim())?;
            if !quantity.is_positive() {
                return Err(BillError::QuantityRequired);
            }

            let description = line.description.trim();
            if description.chars().count() > 400 {
                return Err(BillError::DescriptionTooLong);
            }

            if line.variant_id.is_some() && line.unit_id.is_none() {
                return Err(BillError::UnitRequired);
            }

            if line.unit_price.trim().is_empty() {
                return Err(BillError::PriceRequired);
            }

            lines.push(CheckedBillLine {
                id: line.id,
                receipt_line_id: line.receipt_line_id,
                order_line_id: line.order_line_id,
                variant_id: line.variant_id,
                description: description.to_owned(),
                quantity,
                unit_id: line.unit_id,
                unit_price: line.unit_price.trim().to_owned(),
            });
        }

        if lines.is_empty() {
            return Err(BillError::NoLines);
        }

        Ok(CheckedBill {
            id: self.id,
            order_id: self.order_id,
            supplier_id,
            supplier_reference: reference.to_owned(),
            bill_date: self.bill_date,
            due_on: self.due_on,
            currency: self.currency.trim().to_uppercase(),
            note: (!note.is_empty()).then(|| note.to_owned()),
            lines,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedBill {
    pub id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub supplier_id: Uuid,
    pub supplier_reference: String,
    pub bill_date: NaiveDate,
    pub due_on: Option<NaiveDate>,
    pub currency: String,
    pub note: Option<String>,
    pub lines: Vec<CheckedBillLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedBillLine {
    pub id: Option<Uuid>,
    pub receipt_line_id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    pub quantity: Quantity,
    pub unit_id: Option<Uuid>,
    pub unit_price: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BillError {
    #[error("a bill needs a supplier")]
    SupplierRequired,
    #[error("that party is not a supplier")]
    NotASupplier,
    #[error("a bill needs the supplier's own invoice number")]
    ReferenceRequired,
    #[error("an invoice number is at most 120 characters")]
    ReferenceTooLong,
    #[error("that invoice number is already on another bill from this supplier")]
    DuplicateReference,
    #[error("a bill needs a currency")]
    CurrencyRequired,
    #[error("a bill needs at least one line")]
    NoLines,
    #[error("a line needs a quantity above nothing")]
    QuantityRequired,
    #[error("a line needs a price")]
    PriceRequired,
    #[error("a goods line needs a unit")]
    UnitRequired,
    #[error("a description is at most 400 characters")]
    DescriptionTooLong,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("a bill cannot fall due before it is dated")]
    DueBeforeDated,
    #[error("that receipt line has already been billed in full")]
    AlreadyBilled,
    #[error("that line belongs to a different order")]
    WrongOrder,
    #[error("a posted bill cannot be changed")]
    NotEditable,
    #[error("this match needs a reason before it can be posted")]
    OverrideRequired,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that price is not an amount")]
    Money(#[from] MoneyError),
}

impl BillError {
    pub fn field(self) -> &'static str {
        match self {
            Self::SupplierRequired | Self::NotASupplier => "supplier_id",
            Self::ReferenceRequired | Self::ReferenceTooLong | Self::DuplicateReference => {
                "supplier_reference"
            }
            Self::CurrencyRequired => "currency",
            Self::NoLines | Self::AlreadyBilled | Self::WrongOrder => "lines",
            Self::QuantityRequired | Self::Quantity(_) => "quantity",
            Self::PriceRequired | Self::Money(_) => "unit_price",
            Self::UnitRequired => "unit_id",
            Self::DescriptionTooLong => "description",
            Self::NoteTooLong => "note",
            Self::DueBeforeDated => "due_on",
            Self::NotEditable => "state",
            Self::OverrideRequired => "match_note",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::SupplierRequired => msg!("bills.error.supplier_required"),
            Self::NotASupplier => msg!("purchase_orders.error.not_a_supplier"),
            Self::ReferenceRequired => msg!("bills.error.reference_required"),
            Self::ReferenceTooLong => msg!("bills.error.reference_too_long"),
            Self::DuplicateReference => msg!("bills.error.duplicate_reference"),
            Self::CurrencyRequired => msg!("purchase_orders.error.currency_required"),
            Self::NoLines => msg!("bills.error.no_lines"),
            Self::QuantityRequired => msg!("purchase_orders.error.quantity_required"),
            Self::PriceRequired => msg!("purchase_orders.error.price_required"),
            Self::UnitRequired => msg!("purchase_orders.error.unit_required"),
            Self::DescriptionTooLong => msg!("purchase_orders.error.description_too_long"),
            Self::NoteTooLong => msg!("purchase_orders.error.note_too_long"),
            Self::DueBeforeDated => msg!("bills.error.due_before_dated"),
            Self::AlreadyBilled => msg!("bills.error.already_billed"),
            Self::WrongOrder => msg!("receipts.error.wrong_order"),
            Self::NotEditable => msg!("bills.error.not_editable"),
            Self::OverrideRequired => msg!("bills.error.override_required"),
            Self::Quantity(err) => err.message(),
            Self::Money(err) => err.message(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonix_core::locale::Currency;

    fn tolerance() -> Tolerance {
        Tolerance {
            price_percent: 250,
            price_cap: Money::parse(Currency::USD, "50.00").unwrap(),
        }
    }

    fn money(raw: &str) -> Money {
        Money::parse(Currency::USD, raw).unwrap()
    }

    #[test]
    fn a_variance_inside_both_halves_is_accepted() {
        assert!(tolerance().accepts(money("1000.00"), money("20.00")));
    }

    #[test]
    fn the_cap_binds_even_when_the_percentage_does_not() {
        assert!(!tolerance().accepts(money("100000.00"), money("2000.00")));
    }

    #[test]
    fn the_percentage_binds_on_a_small_order() {
        assert!(!tolerance().accepts(money("100.00"), money("10.00")));
    }

    #[test]
    fn an_undercharge_is_measured_the_same_way() {
        assert!(tolerance().accepts(money("1000.00"), money("-20.00")));
        assert!(!tolerance().accepts(money("1000.00"), money("-90.00")));
    }

    #[test]
    fn only_a_clean_or_tolerated_match_posts_unaided() {
        assert!(!MatchGrade::Clean.needs_override());
        assert!(!MatchGrade::WithinTolerance.needs_override());

        for grade in [
            MatchGrade::OverTolerance,
            MatchGrade::OverReceived,
            MatchGrade::NoReceipt,
            MatchGrade::NoOrder,
            MatchGrade::Circular,
            MatchGrade::SameHand,
        ] {
            assert!(grade.needs_override(), "{} should need one", grade.as_str());
        }
    }

    #[test]
    fn ageing_buckets_split_at_thirty_sixty_and_ninety() {
        let at = |age| {
            UnbilledReceipt {
                receipt_id: Uuid::nil(),
                number: String::new(),
                received_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                supplier_id: Uuid::nil(),
                supplier_name: String::new(),
                order_number: None,
                unbilled: money("1.00"),
                age_days: age,
            }
            .bucket()
        };

        assert_eq!(at(0), AgeBucket::Current);
        assert_eq!(at(30), AgeBucket::Current);
        assert_eq!(at(31), AgeBucket::ThirtyDays);
        assert_eq!(at(60), AgeBucket::ThirtyDays);
        assert_eq!(at(61), AgeBucket::SixtyDays);
        assert_eq!(at(90), AgeBucket::SixtyDays);
        assert_eq!(at(91), AgeBucket::Older);
    }

    #[test]
    fn a_bill_needs_the_suppliers_own_number() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        let mut draft = BillInput::blank(today, "USD");
        draft.supplier_id = Some(Uuid::nil());
        draft.lines = vec![BillLineInput {
            description: "Gloves".to_owned(),
            quantity: "10".to_owned(),
            unit_price: "1.00".to_owned(),
            ..BillLineInput::blank()
        }];

        assert_eq!(draft.check().unwrap_err(), BillError::ReferenceRequired);

        draft.supplier_reference = "INV-99".to_owned();
        assert!(draft.check().is_ok());
    }
}
