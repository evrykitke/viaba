//! Sales orders: what was agreed, and what everything after it is measured
//! against.
//!
//! The mirror of [`crate::purchase`], and deliberately the same shape. A
//! purchase order is a promise this workspace made to a supplier; a sales order
//! is one a customer made to it. The arithmetic is identical and so are the
//! failure modes, so reading one after the other should not feel like reading
//! two systems.
//!
//! # A quotation and an order are one document in two states
//!
//! Odoo's model, and the one the purchase order already follows. ERPNext makes
//! them two doctypes and copies one into the other; the copy is where the two
//! stop agreeing - a price edited on the order after the quotation was accepted
//! leaves no trace of what was actually quoted.
//!
//! # The number is taken when it leaves the building
//!
//! A purchase order takes its number at *confirm*, because nothing before that
//! has been sent anywhere. A quotation is different: it is sent to somebody who
//! quotes it back on their own paperwork and on the telephone, and a quotation
//! with no number is one nobody can cite. So the number is allocated at the
//! first move out of [`SaleState::Draft`] - [`SaleState::Sent`], or a
//! confirmation straight from a draft, which is what an order taken over the
//! counter is.
//!
//! A quotation that is never accepted keeps its number. Somebody was quoted it,
//! and a series that hands numbers back is one nobody can cite either.
//!
//! # There is no tax here
//!
//! A sales order states quantities and prices. What tax is due on them is
//! decided by the *invoice*, against the group in force on the invoice's own
//! date, and a tax total here would be a number that disagrees with the invoice
//! the day a rate changes. The same rule the purchase order states, for the
//! same reason.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

pub const MAX_ORDER_NOTE_LEN: usize = 2000;
pub const MAX_LINE_DESCRIPTION_LEN: usize = 400;
pub const MAX_REFERENCE_LEN: usize = 120;

/// Where an order is in its life.
///
/// `Delivered` and `PartiallyDelivered` are *not* here: how much has gone is
/// arithmetic over the lines, and storing it as a state means two facts that
/// have to agree and eventually will not. See [`SalesOrder::delivery_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleState {
    /// Being written. Editable, deletable, and carrying no number.
    Draft,
    /// A quotation that has gone to the customer. Still editable - a quotation
    /// is an offer, not a commitment, and a customer who asks for a change gets
    /// a revised quotation rather than a second document.
    Sent,
    /// Agreed. From here the quantities on it are what deliveries and invoices
    /// are measured against.
    Confirmed,
    /// Closed. Either everything went and was billed, or somebody decided the
    /// remainder never will.
    Done,
    /// It will not happen. Nothing may be delivered against it.
    Cancelled,
}

impl SaleState {
    pub const ALL: &'static [Self] = &[
        Self::Draft,
        Self::Sent,
        Self::Confirmed,
        Self::Done,
        Self::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::Confirmed => "confirmed",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|state| state.as_str() == raw)
    }

    /// Whether the lines may still be changed.
    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft | Self::Sent)
    }

    /// Whether stock may leave against it.
    ///
    /// Only a confirmed order. Delivering against a quotation would mean goods
    /// leaving for something nobody has agreed to buy, and delivering against a
    /// cancelled one is the mistake this exists to catch.
    pub const fn accepts_deliveries(self) -> bool {
        matches!(self, Self::Confirmed)
    }

    /// Whether it has left the building, which is what decides whether it
    /// carries a number.
    pub const fn is_numbered(self) -> bool {
        matches!(self, Self::Sent | Self::Confirmed | Self::Done)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("sales_orders.state.draft"),
            Self::Sent => msg!("sales_orders.state.sent"),
            Self::Confirmed => msg!("sales_orders.state.confirmed"),
            Self::Done => msg!("sales_orders.state.done"),
            Self::Cancelled => msg!("sales_orders.state.cancelled"),
        }
    }
}

/// How much of an order has gone, or been billed. Worked out, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Progress {
    Nothing,
    Partly,
    Everything,
    /// More than was agreed. Not an error: a warehouse that shipped a spare
    /// carton has to be able to say so, and refusing to record it is asking
    /// somebody to write down a number they can see is wrong.
    Over,
}

impl Progress {
    /// Over a set of quantities: how far `done` has got towards `agreed`.
    fn of(lines: &[SaleLine], done: fn(&SaleLine) -> Quantity) -> Self {
        let live: Vec<&SaleLine> = lines.iter().filter(|line| !line.is_cancelled).collect();

        if live.is_empty() {
            return Self::Nothing;
        }

        let mut any = false;
        let mut all = true;
        let mut over = false;

        for line in live {
            let ordering = done(line).compare(line.quantity_stock);

            if done(line).is_positive() {
                any = true;
            }
            if ordering.is_lt() {
                all = false;
            }
            if ordering.is_gt() {
                over = true;
            }
        }

        match (any, all, over) {
            (_, _, true) => Self::Over,
            (_, true, _) => Self::Everything,
            (true, _, _) => Self::Partly,
            _ => Self::Nothing,
        }
    }

    pub fn delivered_label(self) -> Message {
        match self {
            Self::Nothing => msg!("sales_orders.delivered.nothing"),
            Self::Partly => msg!("sales_orders.delivered.partly"),
            Self::Everything => msg!("sales_orders.delivered.everything"),
            Self::Over => msg!("sales_orders.delivered.over"),
        }
    }

    pub fn invoiced_label(self) -> Message {
        match self {
            Self::Nothing => msg!("sales_orders.invoiced.nothing"),
            Self::Partly => msg!("sales_orders.invoiced.partly"),
            Self::Everything => msg!("sales_orders.invoiced.everything"),
            Self::Over => msg!("sales_orders.invoiced.over"),
        }
    }
}

/// What an order keeps about who it was agreed with.
///
/// Snapshotted when the document is first issued, for the reason an invoice
/// snapshots a party: a customer who changes their name must not rewrite a
/// quotation already sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerSnapshot {
    pub party_id: Uuid,
    pub code: String,
    pub name: String,
}

impl CustomerSnapshot {
    /// `ACME01 · Acme Fasteners`. One spelling for two screens.
    pub fn label(&self) -> String {
        format!("{} \u{b7} {}", self.code, self.name)
    }
}

/// One sales order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesOrder {
    pub id: Uuid,
    /// `SO-2026-00042`. Empty only while it is a draft.
    pub number: String,
    pub state: SaleState,
    pub customer: CustomerSnapshot,
    /// Which warehouse ships it. A delivery leaves this warehouse's output or
    /// stock location depending on how many steps it delivers in.
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    pub order_date: NaiveDate,
    /// What was promised, where anything was.
    pub promised_on: Option<NaiveDate>,
    /// How long the quotation stands.
    pub valid_until: Option<NaiveDate>,
    /// What the customer is quoted in. Converted to the workspace's own
    /// currency by the *invoice*, at the invoice's date.
    pub currency: String,
    /// What the whole order comes to, before tax.
    pub net: Money,
    /// Their purchase order number, off their paperwork.
    pub customer_reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<SaleLine>,
}

impl SalesOrder {
    /// How much of this has gone out.
    ///
    /// Derived from the lines every time it is asked. A stored answer is a
    /// second fact about the same thing, and the two disagree the first time a
    /// delivery is cancelled.
    pub fn delivery_state(&self) -> Progress {
        Progress::of(&self.lines, |line| line.delivered)
    }

    /// How much of this has been billed.
    pub fn invoice_state(&self) -> Progress {
        Progress::of(&self.lines, |line| line.invoiced)
    }

    /// Whether anything is still to ship, which is what decides whether a
    /// delivery screen has anything to offer.
    pub fn has_outstanding(&self) -> bool {
        self.lines.iter().any(SaleLine::is_outstanding)
    }

    pub fn can_be_delivered(&self) -> bool {
        self.state.accepts_deliveries() && self.has_outstanding()
    }

    /// Whether anything shipped is still unbilled.
    pub fn has_unbilled(&self) -> bool {
        self.lines.iter().any(SaleLine::is_unbilled)
    }

    /// Whether the quotation has passed the date it was good until.
    ///
    /// `today` is a parameter rather than a call to the clock, so a row renders
    /// the same on the server and in the browser - one that says "expired" on
    /// one side and not the other is a hydration mismatch.
    pub fn is_expired(&self, today: NaiveDate) -> bool {
        matches!(self.state, SaleState::Draft | SaleState::Sent)
            && self.valid_until.is_some_and(|until| until < today)
    }

    /// What a document calls this. The number once it has one, and something a
    /// person can still recognise before that.
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} \u{b7} {}", self.order_date, self.customer.name)
        } else {
            self.number.clone()
        }
    }
}

/// One line of an order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleLine {
    pub id: Uuid,
    /// Position on the printed order, from one.
    pub line_no: i32,
    pub variant_id: Uuid,
    pub variant_code: String,
    /// What the item was called when the order was raised. A snapshot, because
    /// a quotation already sent must not change its wording next year.
    pub description: String,
    /// How many, in the unit the customer was quoted in.
    pub quantity: Quantity,
    pub unit_id: Uuid,
    pub unit_code: String,
    /// The same quantity in the item's stock unit, converted once when the line
    /// was written.
    pub quantity_stock: Quantity,
    /// Per quoted unit, in the order's currency.
    pub unit_price: Money,
    /// `quantity * unit_price`, rounded once.
    pub net: Money,
    /// How much has gone, in stock units. Advanced by each delivery.
    pub delivered: Quantity,
    /// How much has been billed, in stock units.
    pub invoiced: Quantity,
    pub promised_on: Option<NaiveDate>,
    /// A line somebody struck out after confirming. Kept, because the order was
    /// agreed with it on.
    pub is_cancelled: bool,
}

impl SaleLine {
    /// What is still to ship, in stock units. Never negative: over-shipping
    /// does not put the customer in credit for units.
    pub fn outstanding(&self) -> Quantity {
        if self.is_cancelled {
            return Quantity::ZERO;
        }

        match self.quantity_stock.checked_sub(self.delivered) {
            Ok(left) if left.is_positive() => left,
            _ => Quantity::ZERO,
        }
    }

    pub fn is_outstanding(&self) -> bool {
        self.outstanding().is_positive()
    }

    /// What has gone and has not been billed, in stock units.
    ///
    /// The sell side of the three-way match. Goods out and no invoice is a real
    /// state - a despatch on the thirtieth and its invoice on the second - and
    /// it is what the goods-delivered-not-invoiced accrual exists to carry.
    pub fn unbilled(&self) -> Quantity {
        match self.delivered.checked_sub(self.invoiced) {
            Ok(left) if left.is_positive() => left,
            _ => Quantity::ZERO,
        }
    }

    pub fn is_unbilled(&self) -> bool {
        self.unbilled().is_positive()
    }

    /// What one *stock* unit sells for, which is what a delivery values its
    /// accrual at.
    ///
    /// The line is priced per quoted unit - a case of twelve at £24 is £2 an
    /// each - and getting this backwards prices a delivery at twelve times what
    /// it was agreed for.
    pub fn stock_unit_price(&self) -> Result<Money, SaleError> {
        if !self.quantity_stock.is_positive() {
            return Err(SaleError::QuantityRequired);
        }

        Ok(self.net.scale_by(
            crate::quantity::SCALE_FACTOR,
            self.quantity_stock.scaled(),
            phonix_core::money::Rounding::HalfUp,
        )?)
    }
}

/// One row of the order grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleSummary {
    pub id: Uuid,
    pub number: String,
    pub state: SaleState,
    pub delivery_state: Progress,
    pub invoice_state: Progress,
    pub customer_name: String,
    pub warehouse_name: String,
    pub order_date: NaiveDate,
    pub promised_on: Option<NaiveDate>,
    pub valid_until: Option<NaiveDate>,
    pub currency: String,
    pub net: Money,
    pub line_count: i64,
}

/// The editable part of an order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleInput {
    pub id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub order_date: NaiveDate,
    pub promised_on: Option<NaiveDate>,
    pub valid_until: Option<NaiveDate>,
    pub currency: String,
    pub customer_reference: String,
    pub note: String,
    pub lines: Vec<SaleLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleLineInput {
    pub id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    /// As typed, in the quoted unit.
    pub quantity: String,
    pub unit_id: Option<Uuid>,
    pub unit_price: String,
    pub promised_on: Option<NaiveDate>,
}

impl SaleLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            variant_id: None,
            description: String::new(),
            quantity: "1".to_owned(),
            unit_id: None,
            unit_price: String::new(),
            promised_on: None,
        }
    }
}

impl SaleInput {
    pub fn blank(today: NaiveDate, currency: &str) -> Self {
        Self {
            id: None,
            customer_id: None,
            warehouse_id: None,
            order_date: today,
            promised_on: None,
            valid_until: None,
            currency: currency.to_owned(),
            customer_reference: String::new(),
            note: String::new(),
            lines: vec![SaleLineInput::blank()],
        }
    }

    pub fn from_order(order: &SalesOrder) -> Self {
        Self {
            id: Some(order.id),
            customer_id: Some(order.customer.party_id),
            warehouse_id: Some(order.warehouse_id),
            order_date: order.order_date,
            promised_on: order.promised_on,
            valid_until: order.valid_until,
            currency: order.currency.clone(),
            customer_reference: order.customer_reference.clone().unwrap_or_default(),
            note: order.note.clone().unwrap_or_default(),
            lines: order
                .lines
                .iter()
                .filter(|line| !line.is_cancelled)
                .map(|line| SaleLineInput {
                    id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    unit_id: Some(line.unit_id),
                    unit_price: line.unit_price.to_storage_string(),
                    promised_on: line.promised_on,
                })
                .collect(),
        }
    }

    /// Everything that can be decided without the database.
    ///
    /// Blank lines are dropped rather than refused: a form that always shows one
    /// empty row at the bottom would otherwise refuse every save.
    pub fn check(&self) -> Result<Checked, SaleError> {
        let customer_id = self.customer_id.ok_or(SaleError::CustomerRequired)?;
        let warehouse_id = self.warehouse_id.ok_or(SaleError::WarehouseRequired)?;

        if self.currency.trim().is_empty() {
            return Err(SaleError::CurrencyRequired);
        }

        if self
            .promised_on
            .is_some_and(|promised| promised < self.order_date)
        {
            return Err(SaleError::PromisedBeforeOrdered);
        }

        if self
            .valid_until
            .is_some_and(|until| until < self.order_date)
        {
            return Err(SaleError::ValidBeforeOrdered);
        }

        if self.note.chars().count() > MAX_ORDER_NOTE_LEN {
            return Err(SaleError::NoteTooLong);
        }

        if self.customer_reference.chars().count() > MAX_REFERENCE_LEN {
            return Err(SaleError::ReferenceTooLong);
        }

        let mut lines = Vec::new();

        for line in &self.lines {
            if is_blank(line) {
                continue;
            }

            let variant_id = line.variant_id.ok_or(SaleError::ItemRequired)?;
            let unit_id = line.unit_id.ok_or(SaleError::UnitRequired)?;

            let quantity = Quantity::parse(&line.quantity)?;
            if !quantity.is_positive() {
                return Err(SaleError::QuantityRequired);
            }

            let description = line.description.trim();
            if description.chars().count() > MAX_LINE_DESCRIPTION_LEN {
                return Err(SaleError::DescriptionTooLong);
            }

            lines.push(CheckedLine {
                id: line.id,
                variant_id,
                description: description.to_owned(),
                quantity,
                unit_id,
                // Parsed against the order's currency by the service, which is
                // the only place that knows the currency is a real one.
                unit_price: line.unit_price.trim().to_owned(),
                promised_on: line.promised_on.or(self.promised_on),
            });
        }

        if lines.is_empty() {
            return Err(SaleError::NoLines);
        }

        Ok(Checked {
            id: self.id,
            customer_id,
            warehouse_id,
            order_date: self.order_date,
            promised_on: self.promised_on,
            valid_until: self.valid_until,
            currency: self.currency.trim().to_uppercase(),
            customer_reference: non_empty(&self.customer_reference),
            note: non_empty(&self.note),
            lines,
        })
    }
}

/// A line nobody filled in. Dropped rather than refused.
fn is_blank(line: &SaleLineInput) -> bool {
    line.variant_id.is_none()
        && line.description.trim().is_empty()
        && line.unit_price.trim().is_empty()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// An order that passed [`SaleInput::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub id: Option<Uuid>,
    pub customer_id: Uuid,
    pub warehouse_id: Uuid,
    pub order_date: NaiveDate,
    pub promised_on: Option<NaiveDate>,
    pub valid_until: Option<NaiveDate>,
    pub currency: String,
    pub customer_reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<CheckedLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedLine {
    pub id: Option<Uuid>,
    pub variant_id: Uuid,
    pub description: String,
    pub quantity: Quantity,
    pub unit_id: Uuid,
    /// Still text: parsing it needs the order's currency, which is the
    /// service's to establish.
    pub unit_price: String,
    pub promised_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SaleError {
    #[error("an order needs a customer")]
    CustomerRequired,
    #[error("that party is not a customer")]
    NotACustomer,
    #[error("an order needs a warehouse to ship from")]
    WarehouseRequired,
    #[error("an order needs a currency")]
    CurrencyRequired,
    #[error("an order needs at least one line")]
    NoLines,
    #[error("a line needs an item")]
    ItemRequired,
    #[error("a line needs a unit")]
    UnitRequired,
    #[error("a line needs a quantity above nothing")]
    QuantityRequired,
    #[error("a line needs a price")]
    PriceRequired,
    #[error("a description is at most 400 characters")]
    DescriptionTooLong,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("a reference is at most 120 characters")]
    ReferenceTooLong,
    #[error("goods cannot be promised before they were ordered")]
    PromisedBeforeOrdered,
    #[error("a quotation cannot expire before it was written")]
    ValidBeforeOrdered,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that price is not an amount")]
    Money(#[from] MoneyError),
    #[error("a confirmed order cannot be edited")]
    NotEditable,
    #[error("only a confirmed order can be delivered against")]
    NotDeliverable,
    #[error("that item cannot be sold")]
    NotSellable,
    #[error("the quoted unit has to measure what the stock unit measures")]
    UnitMismatch,
}

impl SaleError {
    pub fn field(self) -> &'static str {
        match self {
            Self::CustomerRequired | Self::NotACustomer => "customer_id",
            Self::WarehouseRequired => "warehouse_id",
            Self::CurrencyRequired => "currency",
            Self::NoLines | Self::ItemRequired | Self::NotSellable => "lines",
            Self::UnitRequired | Self::UnitMismatch => "unit_id",
            Self::QuantityRequired | Self::Quantity(_) => "quantity",
            Self::PriceRequired | Self::Money(_) => "unit_price",
            Self::DescriptionTooLong => "description",
            Self::NoteTooLong => "note",
            Self::ReferenceTooLong => "customer_reference",
            Self::PromisedBeforeOrdered => "promised_on",
            Self::ValidBeforeOrdered => "valid_until",
            Self::NotEditable | Self::NotDeliverable => "state",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CustomerRequired => msg!("sales_orders.error.customer_required"),
            Self::NotACustomer => msg!("sales_orders.error.not_a_customer"),
            Self::WarehouseRequired => msg!("sales_orders.error.warehouse_required"),
            Self::CurrencyRequired => msg!("sales_orders.error.currency_required"),
            Self::NoLines => msg!("sales_orders.error.no_lines"),
            Self::ItemRequired => msg!("sales_orders.error.item_required"),
            Self::UnitRequired => msg!("sales_orders.error.unit_required"),
            Self::QuantityRequired => msg!("sales_orders.error.quantity_required"),
            Self::PriceRequired => msg!("sales_orders.error.price_required"),
            Self::DescriptionTooLong => msg!("sales_orders.error.description_too_long"),
            Self::NoteTooLong => msg!("sales_orders.error.note_too_long"),
            Self::ReferenceTooLong => msg!("sales_orders.error.reference_too_long"),
            Self::PromisedBeforeOrdered => msg!("sales_orders.error.promised_before_ordered"),
            Self::ValidBeforeOrdered => msg!("sales_orders.error.valid_before_ordered"),
            Self::Quantity(err) => err.message(),
            Self::Money(err) => err.message(),
            Self::NotEditable => msg!("sales_orders.error.not_editable"),
            Self::NotDeliverable => msg!("sales_orders.error.not_deliverable"),
            Self::NotSellable => msg!("sales_orders.error.not_sellable"),
            Self::UnitMismatch => msg!("sales_orders.error.unit_mismatch"),
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

    fn line(agreed: &str, delivered: &str, invoiced: &str) -> SaleLine {
        SaleLine {
            id: Uuid::from_u128(1),
            line_no: 1,
            variant_id: Uuid::from_u128(2),
            variant_code: "ITM-00001-A".to_owned(),
            description: "Widget".to_owned(),
            quantity: qty(agreed),
            unit_id: Uuid::from_u128(3),
            unit_code: "each".to_owned(),
            quantity_stock: qty(agreed),
            unit_price: gbp("10.00"),
            net: gbp("100.00"),
            delivered: qty(delivered),
            invoiced: qty(invoiced),
            promised_on: None,
            is_cancelled: false,
        }
    }

    fn order(lines: Vec<SaleLine>) -> SalesOrder {
        SalesOrder {
            id: Uuid::from_u128(9),
            number: "SO-2026-00042".to_owned(),
            state: SaleState::Confirmed,
            customer: CustomerSnapshot {
                party_id: Uuid::from_u128(4),
                code: "ACME01".to_owned(),
                name: "Acme Fasteners".to_owned(),
            },
            warehouse_id: Uuid::from_u128(5),
            warehouse_name: "Main".to_owned(),
            order_date: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            promised_on: None,
            valid_until: None,
            currency: "GBP".to_owned(),
            net: gbp("100.00"),
            customer_reference: None,
            note: None,
            lines,
        }
    }

    /// The two progress figures are independent: goods can be out of the door
    /// with nothing billed, which is exactly the state the accrual exists for.
    #[test]
    fn delivered_and_invoiced_are_counted_apart() {
        let order = order(vec![line("10", "10", "0")]);

        assert_eq!(order.delivery_state(), Progress::Everything);
        assert_eq!(order.invoice_state(), Progress::Nothing);
        assert!(order.has_unbilled());
    }

    #[test]
    fn a_part_shipment_is_partly_and_leaves_the_rest_outstanding() {
        let order = order(vec![line("10", "4", "0")]);

        assert_eq!(order.delivery_state(), Progress::Partly);
        assert_eq!(order.lines[0].outstanding(), qty("6"));
        assert!(order.can_be_delivered());
    }

    /// A warehouse that shipped a spare carton has to be able to say so, and
    /// the customer is not owed units for it.
    #[test]
    fn over_delivery_is_recorded_and_owes_nothing_more() {
        let order = order(vec![line("10", "12", "0")]);

        assert_eq!(order.delivery_state(), Progress::Over);
        assert_eq!(order.lines[0].outstanding(), Quantity::ZERO);
        assert!(!order.has_outstanding());
    }

    /// A struck-out line is not a shortfall. It was agreed and then withdrawn,
    /// and an order made entirely of them is complete rather than pending.
    #[test]
    fn a_cancelled_line_is_not_outstanding() {
        let mut lines = vec![line("10", "0", "0")];
        lines[0].is_cancelled = true;

        let order = order(lines);

        assert!(!order.has_outstanding());
        assert_eq!(order.delivery_state(), Progress::Nothing);
    }

    /// Priced per quoted unit. A case of twelve at £120 is £10 an each, and
    /// getting it backwards prices a delivery at twelve times what was agreed.
    #[test]
    fn the_stock_unit_price_is_the_line_divided_by_stock_units() {
        let mut line = line("1", "0", "0");
        line.quantity = qty("1");
        line.quantity_stock = qty("12");
        line.unit_price = gbp("120.00");
        line.net = gbp("120.00");

        assert_eq!(line.stock_unit_price().unwrap(), gbp("10.00"));
    }

    /// Only before it is agreed. A confirmed order is not a stale offer.
    #[test]
    fn a_quotation_expires_and_an_order_does_not() {
        let day = |d| NaiveDate::from_ymd_opt(2026, 3, d).unwrap();

        let mut quotation = order(vec![line("10", "0", "0")]);
        quotation.state = SaleState::Sent;
        quotation.valid_until = Some(day(10));

        assert!(quotation.is_expired(day(11)));
        assert!(!quotation.is_expired(day(10)));

        let mut confirmed = quotation.clone();
        confirmed.state = SaleState::Confirmed;

        assert!(!confirmed.is_expired(day(11)));
    }

    /// A draft carries no number and everything that has been sent does.
    #[test]
    fn a_number_belongs_to_a_document_that_has_left_the_building() {
        assert!(!SaleState::Draft.is_numbered());
        assert!(SaleState::Sent.is_numbered());
        assert!(SaleState::Confirmed.is_numbered());
        assert!(SaleState::Done.is_numbered());
    }

    /// A form always offers one empty row, and it is not a line.
    #[test]
    fn an_untouched_row_is_dropped_rather_than_refused() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let mut input = SaleInput::blank(today, "GBP");

        input.customer_id = Some(Uuid::from_u128(4));
        input.warehouse_id = Some(Uuid::from_u128(5));
        input.lines = vec![
            SaleLineInput {
                id: None,
                variant_id: Some(Uuid::from_u128(2)),
                description: "Widget".to_owned(),
                quantity: "10".to_owned(),
                unit_id: Some(Uuid::from_u128(3)),
                unit_price: "10.00".to_owned(),
                promised_on: None,
            },
            SaleLineInput::blank(),
        ];

        let checked = input.check().unwrap();

        assert_eq!(checked.lines.len(), 1);
    }
}
