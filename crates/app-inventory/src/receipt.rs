//! Goods receipts: the event with the accounting consequence.
//!
//! # This is where value enters the business
//!
//! Not the purchase order, which is a promise, and not the bill, which is
//! paperwork that arrives later. Stock goes up and goods-received-not-invoiced
//! goes up the moment the lorry is unloaded, and ADR 0006 section 6.5 is about
//! systems that wait for the invoice and are therefore wrong about their own
//! liabilities for however long the supplier's post takes.
//!
//! # Where the goods land depends on the warehouse
//!
//! One-step warehouses receive straight onto the shelf. Two- and three-step
//! ones receive into `Input`, and a put-away - an internal transfer - moves it
//! on, through `Quality Control` where there is one. That is Odoo's model and
//! the reason [`crate::warehouse::required_sublocations`] exists. Until it is
//! transferred, stock in `Input` is on hand and on the balance sheet but has
//! not been put away, which is a distinction a two-step warehouse exists to
//! make.
//!
//! # A short delivery leaves a backorder
//!
//! Ordered forty, received thirty: the receipt is for thirty, and what is left
//! is still outstanding on the order. Odoo asks whether to raise a backorder
//! document; this derives it - [`Backorder`] is what the order still owes after
//! this receipt, worked out from the lines rather than stored, so cancelling a
//! receipt cannot leave a backorder pointing at nothing.
//!
//! # A receipt does not have to have an order
//!
//! Samples, returns from a customer, and the first stock a workspace ever
//! counts all arrive without a purchase order behind them. `order_id` is
//! optional for that reason, and a receipt with none is priced at the item's
//! own cost.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

pub const MAX_RECEIPT_NOTE_LEN: usize = 2000;

/// Where a receipt is.
///
/// Two states that matter and one that is an admission. A draft is what
/// somebody is keying while they walk the pallet; `Done` is the moment the
/// stock moved and the journal posted, and nothing about it changes afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    Draft,
    /// Posted. The moves exist, the quants moved, the journal is filed, and
    /// this row is evidence.
    Done,
    Cancelled,
}

impl ReceiptState {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Done, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == raw)
    }

    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    pub const fn is_posted(self) -> bool {
        matches!(self, Self::Done)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("receipts.state.draft"),
            Self::Done => msg!("receipts.state.done"),
            Self::Cancelled => msg!("receipts.state.cancelled"),
        }
    }
}

/// One goods receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub id: Uuid,
    /// `IN-2026-00042`. Empty until it is posted, on the same terms as an
    /// order's - a draft somebody abandoned must not leave a hole.
    pub number: String,
    pub state: ReceiptState,
    /// The order this is against, where there is one.
    pub order_id: Option<Uuid>,
    pub order_number: Option<String>,
    pub supplier: crate::purchase::SupplierSnapshot,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    /// Where the goods land: `Input` for a two- or three-step warehouse, the
    /// stock location for a one-step one.
    pub to_location_id: Uuid,
    pub to_location_path: String,
    pub received_on: NaiveDate,
    /// The supplier's own delivery note number, typed off the paperwork. What
    /// somebody searches for when the supplier rings.
    pub delivery_note: Option<String>,
    pub note: Option<String>,
    /// What the goods were worth, in the workspace's own currency. The figure
    /// the journal posted.
    pub value: Money,
    pub lines: Vec<ReceiptLine>,
}

impl Receipt {
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} · {}", self.received_on, self.supplier.name)
        } else {
            self.number.clone()
        }
    }

    /// Whether anything on it would actually move stock.
    pub fn has_quantity(&self) -> bool {
        self.lines.iter().any(|line| line.quantity.is_positive())
    }
}

/// One line of a receipt: this much of this thing, of this lot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptLine {
    pub id: Uuid,
    pub line_no: i32,
    /// The order line this satisfies, where there is one.
    pub order_line_id: Option<Uuid>,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    /// How much arrived, in the item's **stock** unit. Converted from whatever
    /// the order was placed in before it reaches here, because a quant is in
    /// stock units and nothing downstream should have to ask.
    pub quantity: Quantity,
    pub unit_code: String,
    /// The batch on the carton. Typed - it is the supplier's number, and one
    /// this system invented would match nothing on the box.
    pub lot_number: Option<String>,
    pub expires_on: Option<NaiveDate>,
    /// What one stock unit cost, in the workspace's own currency, at the rate
    /// on the receipt's date.
    pub unit_cost: Money,
    pub value: Money,
    /// The movement this line became. Set when the receipt is posted, and the
    /// thread back from the stock ledger to the paperwork.
    pub move_id: Option<Uuid>,
}

/// One row of the receipt grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptSummary {
    pub id: Uuid,
    pub number: String,
    pub state: ReceiptState,
    pub supplier_name: String,
    pub order_number: Option<String>,
    pub warehouse_name: String,
    pub received_on: NaiveDate,
    pub delivery_note: Option<String>,
    pub value: Money,
    pub line_count: i64,
}

/// What an order still owes after a receipt is posted.
///
/// Derived, never stored. Odoo raises a backorder *document*; this works the
/// same answer out of the order's own lines, so a cancelled receipt cannot
/// leave a backorder pointing at a delivery that never happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backorder {
    pub order_id: Uuid,
    pub order_number: String,
    /// One entry per line still owing something.
    pub lines: Vec<BackorderLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackorderLine {
    pub order_line_id: Uuid,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    /// In stock units.
    pub outstanding: Quantity,
}

impl Backorder {
    /// What is left of an order after everything received against it.
    ///
    /// `None` where nothing is owing, which is the ordinary end of an order and
    /// is what a screen draws as "complete" rather than as an empty list.
    pub fn of(order: &crate::purchase::PurchaseOrder) -> Option<Self> {
        let lines: Vec<BackorderLine> = order
            .lines
            .iter()
            .filter(|line| line.is_outstanding())
            .map(|line| BackorderLine {
                order_line_id: line.id,
                variant_id: line.variant_id,
                variant_code: line.variant_code.clone(),
                description: line.description.clone(),
                outstanding: line.outstanding(),
            })
            .collect();

        (!lines.is_empty()).then(|| Self {
            order_id: order.id,
            order_number: order.number.clone(),
            lines,
        })
    }
}

/// The editable part of a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptInput {
    pub id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub received_on: NaiveDate,
    pub delivery_note: String,
    pub note: String,
    pub lines: Vec<ReceiptLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptLineInput {
    pub id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    /// As typed, in the item's stock unit.
    pub quantity: String,
    pub lot_number: String,
    pub expires_on: Option<NaiveDate>,
    /// Empty means "whatever the order said", and for a receipt with no order,
    /// the item's own cost.
    pub unit_cost: String,
}

impl ReceiptLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            order_line_id: None,
            variant_id: None,
            description: String::new(),
            quantity: String::new(),
            lot_number: String::new(),
            expires_on: None,
            unit_cost: String::new(),
        }
    }
}

impl ReceiptInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            order_id: None,
            supplier_id: None,
            warehouse_id: None,
            received_on: today,
            delivery_note: String::new(),
            note: String::new(),
            lines: vec![ReceiptLineInput::blank()],
        }
    }

    /// Reopen a draft for editing.
    pub fn from_receipt(receipt: &Receipt) -> Self {
        Self {
            id: Some(receipt.id),
            order_id: receipt.order_id,
            supplier_id: Some(receipt.supplier.party_id),
            warehouse_id: Some(receipt.warehouse_id),
            received_on: receipt.received_on,
            delivery_note: receipt.delivery_note.clone().unwrap_or_default(),
            note: receipt.note.clone().unwrap_or_default(),
            lines: receipt
                .lines
                .iter()
                .map(|line| ReceiptLineInput {
                    id: Some(line.id),
                    order_line_id: line.order_line_id,
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    lot_number: line.lot_number.clone().unwrap_or_default(),
                    expires_on: line.expires_on,
                    unit_cost: line.unit_cost.to_storage_string(),
                })
                .collect(),
        }
    }

    /// A receipt prefilled with everything an order still owes.
    ///
    /// The screen somebody actually wants: the lorry is at the door and the
    /// question is which of these forty lines arrived, not which item this is.
    /// Quantities default to what is outstanding and are edited down where the
    /// delivery is short.
    pub fn against(order: &crate::purchase::PurchaseOrder, today: NaiveDate) -> Self {
        Self {
            id: None,
            order_id: Some(order.id),
            supplier_id: Some(order.supplier.party_id),
            warehouse_id: Some(order.warehouse_id),
            received_on: today,
            delivery_note: String::new(),
            note: String::new(),
            lines: order
                .lines
                .iter()
                .filter(|line| line.is_outstanding())
                .map(|line| ReceiptLineInput {
                    order_line_id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.outstanding().to_display_string(),
                    ..ReceiptLineInput::blank()
                })
                .collect(),
        }
    }

    /// Everything decidable without the database.
    pub fn check(&self) -> Result<CheckedReceipt, ReceiptError> {
        let warehouse_id = self.warehouse_id.ok_or(ReceiptError::WarehouseRequired)?;
        let supplier_id = self.supplier_id.ok_or(ReceiptError::SupplierRequired)?;

        if self.note.chars().count() > MAX_RECEIPT_NOTE_LEN {
            return Err(ReceiptError::NoteTooLong);
        }

        let mut lines = Vec::new();

        for line in &self.lines {
            if is_blank(line) {
                continue;
            }

            let variant_id = line.variant_id.ok_or(ReceiptError::ItemRequired)?;
            let quantity = Quantity::parse(&line.quantity)?;

            // A line for nothing is a line somebody edited to zero because that
            // item did not come. Dropped rather than refused, which is what
            // makes a short delivery one edit instead of a row deletion.
            if quantity.is_zero() {
                continue;
            }
            if quantity.is_negative() {
                return Err(ReceiptError::QuantityNegative);
            }

            lines.push(CheckedReceiptLine {
                id: line.id,
                order_line_id: line.order_line_id,
                variant_id,
                description: line.description.trim().to_owned(),
                quantity,
                lot_number: non_empty(&line.lot_number),
                expires_on: line.expires_on,
                unit_cost: non_empty(&line.unit_cost),
            });
        }

        if lines.is_empty() {
            return Err(ReceiptError::NothingReceived);
        }

        Ok(CheckedReceipt {
            id: self.id,
            order_id: self.order_id,
            supplier_id,
            warehouse_id,
            received_on: self.received_on,
            delivery_note: non_empty(&self.delivery_note),
            note: non_empty(&self.note),
            lines,
        })
    }
}

fn is_blank(line: &ReceiptLineInput) -> bool {
    line.variant_id.is_none() && line.quantity.trim().is_empty()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedReceipt {
    pub id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub supplier_id: Uuid,
    pub warehouse_id: Uuid,
    pub received_on: NaiveDate,
    pub delivery_note: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<CheckedReceiptLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedReceiptLine {
    pub id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Uuid,
    pub description: String,
    /// In stock units.
    pub quantity: Quantity,
    pub lot_number: Option<String>,
    pub expires_on: Option<NaiveDate>,
    /// Still text: parsing needs the workspace's currency.
    pub unit_cost: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReceiptError {
    #[error("a receipt needs a supplier")]
    SupplierRequired,
    #[error("a receipt needs a warehouse")]
    WarehouseRequired,
    #[error("a receipt needs somewhere for the goods to land")]
    LocationRequired,
    #[error("a receipt needs at least one line with a quantity on it")]
    NothingReceived,
    #[error("a line needs an item")]
    ItemRequired,
    #[error("a quantity is positive - a receipt is goods arriving")]
    QuantityNegative,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that cost is not an amount")]
    Money(#[from] MoneyError),
    #[error("a posted receipt cannot be changed")]
    NotEditable,
    #[error("this order cannot receive goods")]
    OrderNotReceivable,
    #[error("that line belongs to a different order")]
    WrongOrder,
}

impl ReceiptError {
    pub fn field(self) -> &'static str {
        match self {
            Self::SupplierRequired => "supplier_id",
            Self::WarehouseRequired | Self::LocationRequired => "warehouse_id",
            Self::NothingReceived | Self::ItemRequired => "lines",
            Self::QuantityNegative | Self::Quantity(_) => "quantity",
            Self::Money(_) => "unit_cost",
            Self::NoteTooLong => "note",
            Self::NotEditable => "state",
            Self::OrderNotReceivable | Self::WrongOrder => "order_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::SupplierRequired => msg!("receipts.error.supplier_required"),
            Self::WarehouseRequired => msg!("receipts.error.warehouse_required"),
            Self::LocationRequired => msg!("receipts.error.location_required"),
            Self::NothingReceived => msg!("receipts.error.nothing_received"),
            Self::ItemRequired => msg!("receipts.error.item_required"),
            Self::QuantityNegative => msg!("receipts.error.quantity_negative"),
            Self::NoteTooLong => msg!("receipts.error.note_too_long"),
            Self::Quantity(err) => err.message(),
            Self::Money(err) => err.message(),
            Self::NotEditable => msg!("receipts.error.not_editable"),
            Self::OrderNotReceivable => msg!("purchase_orders.error.not_receivable"),
            Self::WrongOrder => msg!("receipts.error.wrong_order"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::purchase::{OrderLine, OrderState, PurchaseOrder, SupplierSnapshot};
    use phonix_core::locale::Currency;

    fn gbp(amount: &str) -> Money {
        Money::parse(Currency::Gbp, amount).unwrap()
    }

    fn qty(amount: &str) -> Quantity {
        Quantity::parse(amount).unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 7).unwrap()
    }

    fn order_line(id: u128, ordered: &str, received: &str) -> OrderLine {
        OrderLine {
            id: Uuid::from_u128(id),
            line_no: 1,
            variant_id: Uuid::from_u128(id + 50),
            variant_code: format!("ITM-{id:05}"),
            description: "Hex bolt".to_owned(),
            quantity: qty(ordered),
            unit_id: Uuid::from_u128(3),
            unit_code: "EA".to_owned(),
            quantity_stock: qty(ordered),
            unit_price: gbp("2.00"),
            net: gbp("20.00"),
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
            order_date: today(),
            expected_on: None,
            currency: "GBP".to_owned(),
            net: gbp("40.00"),
            cost_centre_id: None,
            supplier_reference: None,
            note: None,
            lines,
        }
    }

    #[test]
    fn a_receipt_opens_on_what_the_order_still_owes() {
        // The lorry is at the door. The question is which of these lines
        // arrived, not which item this is.
        let outstanding = order(vec![order_line(1, "10", "4"), order_line(2, "10", "10")]);
        let draft = ReceiptInput::against(&outstanding, today());

        assert_eq!(draft.lines.len(), 1);
        assert_eq!(draft.lines[0].quantity, "6");
        assert_eq!(draft.order_id, Some(outstanding.id));
    }

    #[test]
    fn a_short_delivery_is_one_edit_rather_than_a_deleted_row() {
        // Somebody types 0 against the line that did not come. That line is
        // dropped, and what is left of it stays outstanding on the order.
        let mut draft = ReceiptInput::against(&order(vec![order_line(1, "10", "0")]), today());
        draft.supplier_id = Some(Uuid::from_u128(4));
        draft.warehouse_id = Some(Uuid::from_u128(5));
        draft.lines.push(ReceiptLineInput {
            variant_id: Some(Uuid::from_u128(60)),
            quantity: "0".to_owned(),
            ..ReceiptLineInput::blank()
        });

        assert_eq!(draft.check().unwrap().lines.len(), 1);
    }

    #[test]
    fn a_receipt_of_nothing_at_all_is_refused() {
        let mut empty = ReceiptInput::blank(today());
        empty.supplier_id = Some(Uuid::from_u128(4));
        empty.warehouse_id = Some(Uuid::from_u128(5));

        assert_eq!(empty.check(), Err(ReceiptError::NothingReceived));
    }

    #[test]
    fn a_negative_receipt_is_refused_rather_than_read_as_a_return() {
        // A return is a delivery to a vendor location, which is its own
        // document. Minus three arriving is somebody in the wrong screen.
        let mut backwards = ReceiptInput::blank(today());
        backwards.supplier_id = Some(Uuid::from_u128(4));
        backwards.warehouse_id = Some(Uuid::from_u128(5));
        backwards.lines = vec![ReceiptLineInput {
            variant_id: Some(Uuid::from_u128(60)),
            quantity: "-3".to_owned(),
            ..ReceiptLineInput::blank()
        }];

        assert_eq!(backwards.check(), Err(ReceiptError::QuantityNegative));
    }

    #[test]
    fn a_backorder_is_what_the_order_still_owes() {
        let partly = order(vec![order_line(1, "10", "4"), order_line(2, "10", "10")]);
        let left = Backorder::of(&partly).unwrap();

        assert_eq!(left.lines.len(), 1);
        assert_eq!(left.lines[0].outstanding, qty("6"));
        assert_eq!(left.order_number, "PO-2026-00042");
    }

    #[test]
    fn a_complete_order_leaves_no_backorder_rather_than_an_empty_one() {
        let done = order(vec![order_line(1, "10", "10")]);

        assert_eq!(Backorder::of(&done), None);
    }

    #[test]
    fn a_receipt_needs_no_order_behind_it() {
        // Samples, customer returns and the first stock a workspace ever
        // counts all arrive without one.
        let mut walk_in = ReceiptInput::blank(today());
        walk_in.supplier_id = Some(Uuid::from_u128(4));
        walk_in.warehouse_id = Some(Uuid::from_u128(5));
        walk_in.lines = vec![ReceiptLineInput {
            variant_id: Some(Uuid::from_u128(60)),
            quantity: "5".to_owned(),
            ..ReceiptLineInput::blank()
        }];

        let checked = walk_in.check().unwrap();
        assert_eq!(checked.order_id, None);
        assert_eq!(checked.lines.len(), 1);
    }

    #[test]
    fn a_posted_receipt_is_not_editable() {
        assert!(ReceiptState::Draft.is_editable());
        assert!(!ReceiptState::Done.is_editable());
        assert!(ReceiptState::Done.is_posted());
    }
}
