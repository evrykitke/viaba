//! Purchase orders: the commitment, and what everything else is measured
//! against.
//!
//! # A draft is a plan and a confirmed order is a promise
//!
//! Until it is confirmed a purchase order can be edited, re-priced and thrown
//! away, and it carries no number. Confirming is the act that makes it a
//! document: it takes a number from `core.number_sequences`, freezes the
//! supplier's name onto the record, and from that moment the quantities on it
//! are what receipts and bills are checked against. That is Books' rule for an
//! invoice, applied here for the same reason - ADR 0006 section 3, and the
//! reason the `receipt` series comment already gives: a draft somebody
//! abandoned must not leave a hole in a numbered series.
//!
//! This is a deliberate departure from Odoo, which numbers at create. The rest
//! of the model is Odoo's: lines in the *purchase* unit with the stock-unit
//! quantity carried beside them, a received quantity per line that a partial
//! delivery advances, and an order state that follows from those quantities
//! rather than from somebody ticking a box.
//!
//! # The supplier is an id with no foreign key
//!
//! `master.parties`, carried the way `books.invoices` carries one, with the
//! code and name snapshotted beside it. An app may not hold a key into another
//! app's schema - ADR 0001 - and a supplier who renames themselves next year
//! must not rewrite an order somebody already sent.
//!
//! # There is no tax here
//!
//! A purchase order states quantities and prices. What tax is due on them is
//! decided by the *bill*, against the tax group in force on the bill's own
//! date, and putting a tax total on the order would be a number that disagrees
//! with the bill the day a rate changes.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

pub const MAX_ORDER_NOTE_LEN: usize = 2000;
pub const MAX_LINE_DESCRIPTION_LEN: usize = 400;

/// Where an order is in its life.
///
/// `Received` and `PartiallyReceived` are *not* here: how much has arrived is
/// arithmetic over the lines, and storing it as a state means two facts that
/// have to agree and eventually will not. See [`PurchaseOrder::receipt_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    /// Being written. Editable, deletable, and carries no number.
    Draft,
    /// Sent to the supplier for a price. Still editable - a quotation is not a
    /// commitment, and this is the state Odoo calls `sent`.
    Sent,
    /// The commitment. Numbered, and from here the quantities are what
    /// receipts and bills are measured against.
    Confirmed,
    /// Closed. Either everything arrived and was billed, or somebody decided
    /// the remainder never will.
    Done,
    /// It will not happen. Nothing may be received against it.
    Cancelled,
}

impl OrderState {
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
        Self::ALL.iter().copied().find(|s| s.as_str() == raw)
    }

    /// Whether the lines may still be changed.
    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft | Self::Sent)
    }

    /// Whether goods may be received against it.
    ///
    /// Only a confirmed order. Receiving against a draft would mean stock
    /// arriving for something nobody committed to buy, and receiving against a
    /// cancelled one is the mistake this exists to catch.
    pub const fn accepts_receipts(self) -> bool {
        matches!(self, Self::Confirmed)
    }

    pub const fn is_numbered(self) -> bool {
        matches!(self, Self::Confirmed | Self::Done)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("purchase_orders.state.draft"),
            Self::Sent => msg!("purchase_orders.state.sent"),
            Self::Confirmed => msg!("purchase_orders.state.confirmed"),
            Self::Done => msg!("purchase_orders.state.done"),
            Self::Cancelled => msg!("purchase_orders.state.cancelled"),
        }
    }
}

/// How much of an order has arrived, worked out rather than stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    Nothing,
    Partly,
    Everything,
    /// More arrived than was ordered. Not an error - suppliers over-ship, and a
    /// system that refused to record it would be asking a warehouse to lie.
    Over,
}

impl ReceiptState {
    pub fn label(self) -> Message {
        match self {
            Self::Nothing => msg!("purchase_orders.received.nothing"),
            Self::Partly => msg!("purchase_orders.received.partly"),
            Self::Everything => msg!("purchase_orders.received.everything"),
            Self::Over => msg!("purchase_orders.received.over"),
        }
    }
}

/// What an order keeps about who it was sent to.
///
/// Snapshotted at confirm, for the reason an invoice snapshots a party: a
/// supplier who changes their name must not rewrite an order already sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierSnapshot {
    pub party_id: Uuid,
    pub code: String,
    pub name: String,
}

impl SupplierSnapshot {
    /// `ACME01 · Acme Fasteners`. One spelling for two screens.
    pub fn label(&self) -> String {
        format!("{} · {}", self.code, self.name)
    }
}

/// One purchase order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: Uuid,
    /// `PO-2026-00042`. Empty until it is confirmed.
    pub number: String,
    pub state: OrderState,
    pub supplier: SupplierSnapshot,
    /// Where the goods are going. A receipt lands at this warehouse's input or
    /// stock location depending on how many steps it receives in.
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    pub order_date: NaiveDate,
    /// When the supplier said it would arrive.
    pub expected_on: Option<NaiveDate>,
    /// What the supplier quotes in. Converted to the workspace's own currency
    /// at the *receipt's* date, because that is when the value arrives.
    pub currency: String,
    /// What the whole order comes to, before tax. Stored rather than
    /// recomputed, on the same terms as an invoice's totals.
    pub net: Money,
    /// The cost centre the goods are for, where the order came from a
    /// requisition. Resolved through the `CostCentres` port.
    pub cost_centre_id: Option<Uuid>,
    pub supplier_reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<OrderLine>,
}

impl PurchaseOrder {
    /// How much of this has arrived.
    ///
    /// Derived from the lines every time it is asked. A stored answer is a
    /// second fact about the same thing, and the two disagree the first time a
    /// receipt is cancelled.
    pub fn receipt_state(&self) -> ReceiptState {
        let orderable: Vec<&OrderLine> = self.lines.iter().filter(|line| !line.is_cancelled).collect();

        if orderable.is_empty() {
            return ReceiptState::Nothing;
        }

        let mut any = false;
        let mut all = true;
        let mut over = false;

        for line in orderable {
            let ordering = line.received.compare(line.quantity_stock);

            if line.received.is_positive() {
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
            (_, _, true) => ReceiptState::Over,
            (_, true, _) => ReceiptState::Everything,
            (true, _, _) => ReceiptState::Partly,
            _ => ReceiptState::Nothing,
        }
    }

    /// Whether anything is still outstanding, which is what decides whether a
    /// receipt screen has anything to offer.
    pub fn has_outstanding(&self) -> bool {
        self.lines.iter().any(OrderLine::is_outstanding)
    }

    pub fn can_be_received(&self) -> bool {
        self.state.accepts_receipts() && self.has_outstanding()
    }

    /// What a document calls this. The number once it has one, and something a
    /// person can still recognise before that.
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} · {}", self.order_date, self.supplier.name)
        } else {
            self.number.clone()
        }
    }
}

/// One line of an order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLine {
    pub id: Uuid,
    /// Position on the printed order, from one.
    pub line_no: i32,
    pub variant_id: Uuid,
    pub variant_code: String,
    /// What the item was called when the order was raised. A snapshot, because
    /// a printed order must not change wording next year.
    pub description: String,
    /// How many, in the **purchase** unit - cases, reels, whatever the supplier
    /// quotes in.
    pub quantity: Quantity,
    pub unit_id: Uuid,
    pub unit_code: String,
    /// The same quantity in the item's stock unit, converted once when the line
    /// was written.
    ///
    /// Stored rather than converted on read: a factor somebody edits next year
    /// would otherwise restate how much was ordered, and a receipt would be
    /// measured against a number the supplier never agreed to.
    pub quantity_stock: Quantity,
    /// Per purchase unit, in the order's currency.
    pub unit_price: Money,
    /// `quantity * unit_price`, rounded once.
    pub net: Money,
    /// How much has arrived, in stock units. Advanced by each receipt.
    pub received: Quantity,
    /// How much has been billed, in stock units. The third leg of the
    /// three-way match - ADR 0006 section 6.5.
    pub billed: Quantity,
    pub expected_on: Option<NaiveDate>,
    /// A line somebody struck out after confirming. Kept, because the order was
    /// sent with it on.
    pub is_cancelled: bool,
}

impl OrderLine {
    /// What is still to come, in stock units. Never negative: a supplier who
    /// over-shipped owes nothing more.
    pub fn outstanding(&self) -> Quantity {
        if self.is_cancelled {
            return Quantity::ZERO;
        }

        match self.quantity_stock.checked_sub(self.received) {
            Ok(left) if left.is_positive() => left,
            _ => Quantity::ZERO,
        }
    }

    pub fn is_outstanding(&self) -> bool {
        self.outstanding().is_positive()
    }

    /// What one *stock* unit costs, which is what a valuation layer needs.
    ///
    /// The line is priced per purchase unit - a case of twelve at £24 is £2 an
    /// each - and getting this backwards values a receipt at twelve times what
    /// it cost.
    pub fn stock_unit_price(&self) -> Result<Money, OrderError> {
        if !self.quantity_stock.is_positive() {
            return Err(OrderError::QuantityRequired);
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
pub struct OrderSummary {
    pub id: Uuid,
    pub number: String,
    pub state: OrderState,
    pub receipt_state: ReceiptState,
    pub supplier_name: String,
    pub warehouse_name: String,
    pub order_date: NaiveDate,
    pub expected_on: Option<NaiveDate>,
    pub currency: String,
    pub net: Money,
    pub line_count: i64,
}

/// The editable part of an order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderInput {
    pub id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub order_date: NaiveDate,
    pub expected_on: Option<NaiveDate>,
    pub currency: String,
    pub cost_centre_id: Option<Uuid>,
    pub supplier_reference: String,
    pub note: String,
    pub lines: Vec<OrderLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLineInput {
    pub id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    /// As typed, in the purchase unit.
    pub quantity: String,
    pub unit_id: Option<Uuid>,
    pub unit_price: String,
    pub expected_on: Option<NaiveDate>,
}

impl OrderLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            variant_id: None,
            description: String::new(),
            quantity: "1".to_owned(),
            unit_id: None,
            unit_price: String::new(),
            expected_on: None,
        }
    }
}

impl OrderInput {
    pub fn blank(today: NaiveDate, currency: &str) -> Self {
        Self {
            id: None,
            supplier_id: None,
            warehouse_id: None,
            order_date: today,
            expected_on: None,
            currency: currency.to_owned(),
            cost_centre_id: None,
            supplier_reference: String::new(),
            note: String::new(),
            lines: vec![OrderLineInput::blank()],
        }
    }

    pub fn from_order(order: &PurchaseOrder) -> Self {
        Self {
            id: Some(order.id),
            supplier_id: Some(order.supplier.party_id),
            warehouse_id: Some(order.warehouse_id),
            order_date: order.order_date,
            expected_on: order.expected_on,
            currency: order.currency.clone(),
            cost_centre_id: order.cost_centre_id,
            supplier_reference: order.supplier_reference.clone().unwrap_or_default(),
            note: order.note.clone().unwrap_or_default(),
            lines: order
                .lines
                .iter()
                .filter(|line| !line.is_cancelled)
                .map(|line| OrderLineInput {
                    id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    unit_id: Some(line.unit_id),
                    unit_price: line.unit_price.to_storage_string(),
                    expected_on: line.expected_on,
                })
                .collect(),
        }
    }

    /// Everything that can be decided without the database.
    ///
    /// Blank lines are dropped rather than refused: a form that always shows one
    /// empty row at the bottom would otherwise refuse every save.
    pub fn check(&self) -> Result<Checked, OrderError> {
        let supplier_id = self.supplier_id.ok_or(OrderError::SupplierRequired)?;
        let warehouse_id = self.warehouse_id.ok_or(OrderError::WarehouseRequired)?;

        if self.currency.trim().is_empty() {
            return Err(OrderError::CurrencyRequired);
        }

        if self
            .expected_on
            .is_some_and(|expected| expected < self.order_date)
        {
            return Err(OrderError::ExpectedBeforeOrdered);
        }

        if self.note.chars().count() > MAX_ORDER_NOTE_LEN {
            return Err(OrderError::NoteTooLong);
        }

        let mut lines = Vec::new();

        for line in &self.lines {
            if is_blank(line) {
                continue;
            }

            let variant_id = line.variant_id.ok_or(OrderError::ItemRequired)?;
            let unit_id = line.unit_id.ok_or(OrderError::UnitRequired)?;

            let quantity = Quantity::parse(&line.quantity)?;
            if !quantity.is_positive() {
                return Err(OrderError::QuantityRequired);
            }

            let description = line.description.trim();
            if description.chars().count() > MAX_LINE_DESCRIPTION_LEN {
                return Err(OrderError::DescriptionTooLong);
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
                expected_on: line.expected_on.or(self.expected_on),
            });
        }

        if lines.is_empty() {
            return Err(OrderError::NoLines);
        }

        Ok(Checked {
            id: self.id,
            supplier_id,
            warehouse_id,
            order_date: self.order_date,
            expected_on: self.expected_on,
            currency: self.currency.trim().to_uppercase(),
            cost_centre_id: self.cost_centre_id,
            supplier_reference: non_empty(&self.supplier_reference),
            note: non_empty(&self.note),
            lines,
        })
    }
}

/// A line nobody filled in. Dropped rather than refused.
fn is_blank(line: &OrderLineInput) -> bool {
    line.variant_id.is_none()
        && line.description.trim().is_empty()
        && line.unit_price.trim().is_empty()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// An order that passed [`OrderInput::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub id: Option<Uuid>,
    pub supplier_id: Uuid,
    pub warehouse_id: Uuid,
    pub order_date: NaiveDate,
    pub expected_on: Option<NaiveDate>,
    pub currency: String,
    pub cost_centre_id: Option<Uuid>,
    pub supplier_reference: Option<String>,
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
    pub expected_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OrderError {
    #[error("an order needs a supplier")]
    SupplierRequired,
    #[error("that party is not a supplier")]
    NotASupplier,
    #[error("an order needs a warehouse to deliver to")]
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
    #[error("goods cannot be expected before they were ordered")]
    ExpectedBeforeOrdered,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that price is not an amount")]
    Money(#[from] MoneyError),
    #[error("a confirmed order cannot be edited")]
    NotEditable,
    #[error("only a confirmed order can receive goods")]
    NotReceivable,
    #[error("that item cannot be purchased")]
    NotPurchasable,
    #[error("the purchase unit has to measure what the stock unit measures")]
    UnitMismatch,
}

impl OrderError {
    pub fn field(self) -> &'static str {
        match self {
            Self::SupplierRequired | Self::NotASupplier => "supplier_id",
            Self::WarehouseRequired => "warehouse_id",
            Self::CurrencyRequired => "currency",
            Self::NoLines | Self::ItemRequired | Self::NotPurchasable => "lines",
            Self::UnitRequired | Self::UnitMismatch => "unit_id",
            Self::QuantityRequired | Self::Quantity(_) => "quantity",
            Self::PriceRequired | Self::Money(_) => "unit_price",
            Self::DescriptionTooLong => "description",
            Self::NoteTooLong => "note",
            Self::ExpectedBeforeOrdered => "expected_on",
            Self::NotEditable | Self::NotReceivable => "state",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::SupplierRequired => msg!("purchase_orders.error.supplier_required"),
            Self::NotASupplier => msg!("purchase_orders.error.not_a_supplier"),
            Self::WarehouseRequired => msg!("purchase_orders.error.warehouse_required"),
            Self::CurrencyRequired => msg!("purchase_orders.error.currency_required"),
            Self::NoLines => msg!("purchase_orders.error.no_lines"),
            Self::ItemRequired => msg!("purchase_orders.error.item_required"),
            Self::UnitRequired => msg!("purchase_orders.error.unit_required"),
            Self::QuantityRequired => msg!("purchase_orders.error.quantity_required"),
            Self::PriceRequired => msg!("purchase_orders.error.price_required"),
            Self::DescriptionTooLong => msg!("purchase_orders.error.description_too_long"),
            Self::NoteTooLong => msg!("purchase_orders.error.note_too_long"),
            Self::ExpectedBeforeOrdered => msg!("purchase_orders.error.expected_before_ordered"),
            Self::Quantity(err) => err.message(),
            Self::Money(err) => err.message(),
            Self::NotEditable => msg!("purchase_orders.error.not_editable"),
            Self::NotReceivable => msg!("purchase_orders.error.not_receivable"),
            Self::NotPurchasable => msg!("purchase_orders.error.not_purchasable"),
            Self::UnitMismatch => msg!("items.error.purchase_unit_mismatch"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use phonix_core::locale::Currency;

    fn gbp(amount: &str) -> Money {
        Money::parse(Currency::Gbp, amount).unwrap()
    }

    fn qty(amount: &str) -> Quantity {
        Quantity::parse(amount).unwrap()
    }

    fn line(ordered: &str, received: &str) -> OrderLine {
        OrderLine {
            id: Uuid::from_u128(1),
            line_no: 1,
            variant_id: Uuid::from_u128(2),
            variant_code: "ITM-00042".to_owned(),
            description: "Hex bolt".to_owned(),
            quantity: qty(ordered),
            unit_id: Uuid::from_u128(3),
            unit_code: "EA".to_owned(),
            quantity_stock: qty(ordered),
            unit_price: gbp("2.00"),
            net: gbp("2.00"),
            received: qty(received),
            billed: Quantity::ZERO,
            expected_on: None,
            is_cancelled: false,
        }
    }

    fn order(lines: Vec<OrderLine>) -> PurchaseOrder {
        PurchaseOrder {
            id: Uuid::from_u128(9),
            number: "PO-2026-00042".to_owned(),
            state: OrderState::Confirmed,
            supplier: SupplierSnapshot {
                party_id: Uuid::from_u128(4),
                code: "ACME01".to_owned(),
                name: "Acme Fasteners".to_owned(),
            },
            warehouse_id: Uuid::from_u128(5),
            warehouse_name: "Main warehouse".to_owned(),
            order_date: NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
            expected_on: None,
            currency: "GBP".to_owned(),
            net: gbp("100.00"),
            cost_centre_id: None,
            supplier_reference: None,
            note: None,
            lines,
        }
    }

    fn input() -> OrderInput {
        OrderInput {
            supplier_id: Some(Uuid::from_u128(4)),
            warehouse_id: Some(Uuid::from_u128(5)),
            lines: vec![OrderLineInput {
                variant_id: Some(Uuid::from_u128(2)),
                unit_id: Some(Uuid::from_u128(3)),
                quantity: "10".to_owned(),
                unit_price: "2.00".to_owned(),
                ..OrderLineInput::blank()
            }],
            ..OrderInput::blank(NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(), "GBP")
        }
    }

    #[test]
    fn how_much_has_arrived_is_arithmetic_over_the_lines() {
        // Stored as a state it would be a second fact about the same thing,
        // and the two disagree the first time a receipt is cancelled.
        assert_eq!(
            order(vec![line("10", "0")]).receipt_state(),
            ReceiptState::Nothing
        );
        assert_eq!(
            order(vec![line("10", "4")]).receipt_state(),
            ReceiptState::Partly
        );
        assert_eq!(
            order(vec![line("10", "10")]).receipt_state(),
            ReceiptState::Everything
        );
        // One line short is a partly received order, however many are full.
        assert_eq!(
            order(vec![line("10", "10"), line("10", "3")]).receipt_state(),
            ReceiptState::Partly
        );
    }

    #[test]
    fn a_supplier_who_over_ships_is_recorded_rather_than_refused() {
        // Suppliers over-ship. A system that would not record it is asking a
        // warehouse to write down a number it can see is wrong.
        assert_eq!(
            order(vec![line("10", "12")]).receipt_state(),
            ReceiptState::Over
        );
        // And nothing is outstanding against them.
        assert_eq!(line("10", "12").outstanding(), Quantity::ZERO);
    }

    #[test]
    fn a_cancelled_line_is_owed_nothing() {
        let mut struck = line("10", "0");
        struck.is_cancelled = true;

        assert_eq!(struck.outstanding(), Quantity::ZERO);
        assert!(!struck.is_outstanding());
    }

    #[test]
    fn a_case_price_becomes_a_price_per_each() {
        // Twelve to the case at 24.00 is 2.00 an each. Getting this backwards
        // values a receipt at twelve times what it cost.
        let mut cased = line("2", "0");
        cased.quantity = qty("2");
        cased.quantity_stock = qty("24");
        cased.unit_price = gbp("24.00");
        cased.net = gbp("48.00");

        assert_eq!(cased.stock_unit_price().unwrap(), gbp("2.00"));
    }

    #[test]
    fn only_a_confirmed_order_receives_goods() {
        // Receiving against a draft is stock arriving for something nobody
        // committed to buy.
        assert!(OrderState::Confirmed.accepts_receipts());
        for state in [
            OrderState::Draft,
            OrderState::Sent,
            OrderState::Done,
            OrderState::Cancelled,
        ] {
            assert!(!state.accepts_receipts(), "{state:?}");
        }
    }

    #[test]
    fn a_quotation_is_still_editable_and_a_commitment_is_not() {
        assert!(OrderState::Draft.is_editable());
        assert!(OrderState::Sent.is_editable());
        assert!(!OrderState::Confirmed.is_editable());
    }

    #[test]
    fn the_empty_row_a_form_always_shows_is_dropped_rather_than_refused() {
        let mut typed = input();
        typed.lines.push(OrderLineInput::blank());
        // `blank()` carries a quantity of one and nothing else, which is what a
        // form's trailing row looks like.
        typed.lines.last_mut().unwrap().quantity = "1".to_owned();

        assert_eq!(typed.check().unwrap().lines.len(), 1);
    }

    #[test]
    fn an_order_with_nothing_on_it_is_refused() {
        let empty = OrderInput {
            lines: vec![OrderLineInput::blank()],
            ..input()
        };

        assert_eq!(empty.check(), Err(OrderError::NoLines));
    }

    #[test]
    fn goods_cannot_arrive_before_they_were_ordered() {
        let backwards = OrderInput {
            expected_on: NaiveDate::from_ymd_opt(2026, 9, 1),
            ..input()
        };

        assert_eq!(
            backwards.check(),
            Err(OrderError::ExpectedBeforeOrdered)
        );
    }

    #[test]
    fn a_currency_is_stored_the_way_iso_spells_it() {
        let lowercase = OrderInput {
            currency: " gbp ".to_owned(),
            ..input()
        };

        assert_eq!(lowercase.check().unwrap().currency, "GBP");
    }
}
