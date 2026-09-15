//! Deliveries: the goods going out, and the event with the accounting
//! consequence.
//!
//! The mirror of [`crate::receipt`], and the point at which a sales order stops
//! being a promise.
//!
//! # This is where value leaves the business
//!
//! Not the order, which is a promise, and not the invoice, which is paperwork.
//! Stock goes down and the cost of it lands in the profit and loss the moment
//! the van is loaded, and a system that waits for the invoice is wrong about
//! its own stock for however long the paperwork takes - the same argument ADR
//! 0006 section 6.5 makes about the buying side, in reverse.
//!
//! # Where the goods leave from depends on the warehouse
//!
//! One-step warehouses ship straight off the shelf. Two- and three-step ones
//! ship from `Output`, and the pick that puts goods there - through `Packing
//! Zone` where there is one - is an internal transfer. That is Odoo's model and
//! the reason [`crate::warehouse::required_sublocations`] exists. Stock in
//! `Output` is picked and not gone: still on hand, still on the balance sheet,
//! and no longer available to promise anybody else.
//!
//! # A short shipment leaves the rest on the order
//!
//! Agreed forty, shipped thirty: the delivery is for thirty and ten stay
//! outstanding on the order line. Nothing is stored about the remainder -
//! [`Outstanding`] works it out from the order's own lines, so a cancelled
//! delivery cannot leave a shortfall pointing at nothing.
//!
//! # A delivery does not have to have an order
//!
//! A sample, a replacement, a counter sale nobody quoted. `order_id` is
//! optional for that reason.
//!
//! # There is no price here
//!
//! A delivery moves goods and posts what those goods *cost*. What the customer
//! is charged is the invoice's business, against the tax group in force on the
//! invoice's own date. `value` is cost, in the workspace's own currency.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bill::AgeBucket;
use crate::quantity::{Quantity, QuantityError};

pub const MAX_DELIVERY_NOTE_LEN: usize = 2000;
pub const MAX_CARRIER_REFERENCE_LEN: usize = 120;

/// Where a delivery is.
///
/// Two states that matter and one that is an admission, exactly as a receipt
/// has. A draft is what somebody is keying while they load the van; `Done` is
/// the moment the stock moved and the journal posted, and nothing about it
/// changes afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Draft,
    /// Posted. The moves exist, the quants moved, the journal is filed, and
    /// this row is evidence.
    Done,
    Cancelled,
}

impl DeliveryState {
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
            Self::Draft => msg!("deliveries.state.draft"),
            Self::Done => msg!("deliveries.state.done"),
            Self::Cancelled => msg!("deliveries.state.cancelled"),
        }
    }
}

/// One delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: Uuid,
    /// `OUT-2026-00042`. Empty until it is posted.
    pub number: String,
    pub state: DeliveryState,
    /// The order this is against, where there is one.
    pub order_id: Option<Uuid>,
    pub order_number: Option<String>,
    pub customer: crate::sales_order::CustomerSnapshot,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    /// Where the goods leave from: `Output` for a two- or three-step warehouse,
    /// the stock location for a one-step one.
    pub from_location_id: Uuid,
    pub from_location_path: String,
    pub despatched_on: NaiveDate,
    /// The carrier's consignment number, typed off their paperwork.
    pub carrier_reference: Option<String>,
    pub note: Option<String>,
    /// What the goods **cost**, in the workspace's own currency. The figure the
    /// journal posted, and not what they sold for.
    pub value: Money,
    pub lines: Vec<DeliveryLine>,
}

impl Delivery {
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} \u{b7} {}", self.despatched_on, self.customer.name)
        } else {
            self.number.clone()
        }
    }

    /// Whether anything on it would actually move stock.
    pub fn has_quantity(&self) -> bool {
        self.lines.iter().any(|line| line.quantity.is_positive())
    }
}

/// One line of a delivery: this much of this thing, out of this lot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryLine {
    pub id: Uuid,
    pub line_no: i32,
    /// The order line this satisfies, where there is one.
    pub order_line_id: Option<Uuid>,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    /// How much went, in the item's **stock** unit.
    pub quantity: Quantity,
    pub unit_code: String,
    /// Which batch left. Chosen from what this workspace holds rather than
    /// typed - the opposite of a receipt, where the number is the supplier's
    /// and new to us.
    pub lot_id: Option<Uuid>,
    pub lot_number: Option<String>,
    /// What one stock unit **cost**, worked out by the costing method when the
    /// move was applied. Zero until the delivery is posted, because average and
    /// FIFO only know it then.
    pub unit_cost: Money,
    pub value: Money,
    /// The movement this line became. Set when the delivery is posted, and the
    /// thread back from the stock ledger to the paperwork.
    pub move_id: Option<Uuid>,
    /// How much of this line has been invoiced, in the stock unit. The sell-side
    /// mirror of a receipt line's `billed`.
    pub invoiced: Quantity,
}

/// One delivery's worth of goods gone and not yet charged for.
///
/// At cost, not at price: this is what the goods-delivered-not-invoiced balance
/// carries. The sell-side mirror of [`crate::bill::UnbilledReceipt`], and it
/// buckets by the same ages because the two are read side by side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninvoicedDelivery {
    pub delivery_id: Uuid,
    pub number: String,
    pub despatched_on: NaiveDate,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub order_number: Option<String>,
    pub uninvoiced: Money,
    pub age_days: i32,
}

impl UninvoicedDelivery {
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

/// One row of the delivery grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverySummary {
    pub id: Uuid,
    pub number: String,
    pub state: DeliveryState,
    pub customer_name: String,
    pub order_number: Option<String>,
    pub warehouse_name: String,
    pub despatched_on: NaiveDate,
    pub carrier_reference: Option<String>,
    pub value: Money,
    pub line_count: i64,
}

/// What an order still owes after everything shipped against it.
///
/// Derived, never stored - the mirror of [`crate::receipt::Backorder`], and for
/// the same reason: a cancelled delivery must not leave a shortfall pointing at
/// a despatch that never happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outstanding {
    pub order_id: Uuid,
    pub order_number: String,
    /// One entry per line still owing something.
    pub lines: Vec<OutstandingLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutstandingLine {
    pub order_line_id: Uuid,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    /// In stock units.
    pub outstanding: Quantity,
}

impl Outstanding {
    /// What is left of an order after everything shipped against it.
    ///
    /// `None` where nothing is owing, which is the ordinary end of an order and
    /// is what a screen draws as "complete" rather than as an empty list.
    pub fn of(order: &crate::sales_order::SalesOrder) -> Option<Self> {
        let lines: Vec<OutstandingLine> = order
            .lines
            .iter()
            .filter(|line| line.is_outstanding())
            .map(|line| OutstandingLine {
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

/// The editable part of a delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryInput {
    pub id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub despatched_on: NaiveDate,
    pub carrier_reference: String,
    pub note: String,
    pub lines: Vec<DeliveryLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryLineInput {
    pub id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    /// As typed, in the item's stock unit.
    pub quantity: String,
    /// Which batch is going. Only meaningful where the item is tracked, and
    /// chosen from what is on hand rather than typed.
    pub lot_id: Option<Uuid>,
}

impl DeliveryLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            order_line_id: None,
            variant_id: None,
            description: String::new(),
            quantity: String::new(),
            lot_id: None,
        }
    }
}

impl DeliveryInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            order_id: None,
            customer_id: None,
            warehouse_id: None,
            despatched_on: today,
            carrier_reference: String::new(),
            note: String::new(),
            lines: vec![DeliveryLineInput::blank()],
        }
    }

    /// Reopen a draft for editing.
    pub fn from_delivery(delivery: &Delivery) -> Self {
        Self {
            id: Some(delivery.id),
            order_id: delivery.order_id,
            customer_id: Some(delivery.customer.party_id),
            warehouse_id: Some(delivery.warehouse_id),
            despatched_on: delivery.despatched_on,
            carrier_reference: delivery.carrier_reference.clone().unwrap_or_default(),
            note: delivery.note.clone().unwrap_or_default(),
            lines: delivery
                .lines
                .iter()
                .map(|line| DeliveryLineInput {
                    id: Some(line.id),
                    order_line_id: line.order_line_id,
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    lot_id: line.lot_id,
                })
                .collect(),
        }
    }

    /// A delivery prefilled with everything an order still owes.
    ///
    /// The screen somebody actually wants: the van is at the door and the
    /// question is which of these forty lines are going today, not which item
    /// this is. Quantities default to what is outstanding and are edited down
    /// where the shipment is short.
    pub fn against(order: &crate::sales_order::SalesOrder, today: NaiveDate) -> Self {
        Self {
            id: None,
            order_id: Some(order.id),
            customer_id: Some(order.customer.party_id),
            warehouse_id: Some(order.warehouse_id),
            despatched_on: today,
            carrier_reference: String::new(),
            note: String::new(),
            lines: order
                .lines
                .iter()
                .filter(|line| line.is_outstanding())
                .map(|line| DeliveryLineInput {
                    order_line_id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.outstanding().to_display_string(),
                    ..DeliveryLineInput::blank()
                })
                .collect(),
        }
    }

    /// Everything decidable without the database.
    pub fn check(&self) -> Result<CheckedDelivery, DeliveryError> {
        let warehouse_id = self.warehouse_id.ok_or(DeliveryError::WarehouseRequired)?;
        let customer_id = self.customer_id.ok_or(DeliveryError::CustomerRequired)?;

        if self.note.chars().count() > MAX_DELIVERY_NOTE_LEN {
            return Err(DeliveryError::NoteTooLong);
        }
        if self.carrier_reference.chars().count() > MAX_CARRIER_REFERENCE_LEN {
            return Err(DeliveryError::ReferenceTooLong);
        }

        let mut lines = Vec::new();

        for line in &self.lines {
            if is_blank(line) {
                continue;
            }

            let variant_id = line.variant_id.ok_or(DeliveryError::ItemRequired)?;
            let quantity = Quantity::parse(&line.quantity)?;

            // A line for nothing is a line somebody edited to zero because that
            // item is not going today. Dropped rather than refused, which is
            // what makes a short shipment one edit instead of a row deletion.
            if quantity.is_zero() {
                continue;
            }
            if quantity.is_negative() {
                return Err(DeliveryError::QuantityNegative);
            }

            lines.push(CheckedDeliveryLine {
                id: line.id,
                order_line_id: line.order_line_id,
                variant_id,
                description: line.description.trim().to_owned(),
                quantity,
                lot_id: line.lot_id,
            });
        }

        if lines.is_empty() {
            return Err(DeliveryError::NothingDespatched);
        }

        Ok(CheckedDelivery {
            id: self.id,
            order_id: self.order_id,
            customer_id,
            warehouse_id,
            despatched_on: self.despatched_on,
            carrier_reference: non_empty(&self.carrier_reference),
            note: non_empty(&self.note),
            lines,
        })
    }
}

fn is_blank(line: &DeliveryLineInput) -> bool {
    line.variant_id.is_none() && line.quantity.trim().is_empty()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedDelivery {
    pub id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub customer_id: Uuid,
    pub warehouse_id: Uuid,
    pub despatched_on: NaiveDate,
    pub carrier_reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<CheckedDeliveryLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedDeliveryLine {
    pub id: Option<Uuid>,
    pub order_line_id: Option<Uuid>,
    pub variant_id: Uuid,
    pub description: String,
    /// In stock units.
    pub quantity: Quantity,
    pub lot_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeliveryError {
    #[error("a delivery needs a customer")]
    CustomerRequired,
    #[error("that party is not a customer")]
    NotACustomer,
    #[error("a delivery needs a warehouse")]
    WarehouseRequired,
    #[error("a delivery needs somewhere for the goods to leave from")]
    LocationRequired,
    #[error("a delivery needs at least one line with a quantity on it")]
    NothingDespatched,
    #[error("a line needs an item")]
    ItemRequired,
    #[error("a quantity is positive - a delivery is goods leaving")]
    QuantityNegative,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("a reference is at most 120 characters")]
    ReferenceTooLong,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that cost is not an amount")]
    Money(#[from] MoneyError),
    #[error("a posted delivery cannot be changed")]
    NotEditable,
    #[error("this order cannot be delivered against")]
    OrderNotDeliverable,
    #[error("that line belongs to a different order")]
    WrongOrder,
    #[error("a tracked item needs the batch it is going from")]
    LotRequired,
}

impl DeliveryError {
    pub fn field(self) -> &'static str {
        match self {
            Self::CustomerRequired | Self::NotACustomer => "customer_id",
            Self::WarehouseRequired | Self::LocationRequired => "warehouse_id",
            Self::NothingDespatched | Self::ItemRequired | Self::WrongOrder => "lines",
            Self::QuantityNegative | Self::Quantity(_) => "quantity",
            Self::Money(_) => "unit_cost",
            Self::NoteTooLong => "note",
            Self::ReferenceTooLong => "carrier_reference",
            Self::NotEditable => "state",
            Self::OrderNotDeliverable => "order_id",
            Self::LotRequired => "lot_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CustomerRequired => msg!("deliveries.error.customer_required"),
            Self::NotACustomer => msg!("sales_orders.error.not_a_customer"),
            Self::WarehouseRequired => msg!("deliveries.error.warehouse_required"),
            Self::LocationRequired => msg!("deliveries.error.location_required"),
            Self::NothingDespatched => msg!("deliveries.error.nothing_despatched"),
            Self::ItemRequired => msg!("deliveries.error.item_required"),
            Self::QuantityNegative => msg!("deliveries.error.quantity_negative"),
            Self::NoteTooLong => msg!("deliveries.error.note_too_long"),
            Self::ReferenceTooLong => msg!("deliveries.error.reference_too_long"),
            Self::Quantity(err) => err.message(),
            Self::Money(err) => err.message(),
            Self::NotEditable => msg!("deliveries.error.not_editable"),
            Self::OrderNotDeliverable => msg!("sales_orders.error.not_deliverable"),
            Self::WrongOrder => msg!("deliveries.error.wrong_order"),
            Self::LotRequired => msg!("deliveries.error.lot_required"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::sales_order::{CustomerSnapshot, SaleLine, SaleState, SalesOrder};
    use phonix_core::locale::Currency;

    fn gbp(amount: &str) -> Money {
        Money::parse(Currency::parse("GBP").unwrap(), amount).unwrap()
    }

    fn qty(amount: &str) -> Quantity {
        Quantity::parse(amount).unwrap()
    }

    fn order(agreed: &str, delivered: &str) -> SalesOrder {
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
            lines: vec![SaleLine {
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
                invoiced: Quantity::ZERO,
                promised_on: None,
                is_cancelled: false,
            }],
        }
    }

    /// The van is at the door: the form opens on what is still owed, not on
    /// what was originally agreed.
    #[test]
    fn a_delivery_against_an_order_opens_on_what_is_still_owed() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let input = DeliveryInput::against(&order("40", "30"), today);

        assert_eq!(input.lines.len(), 1);
        assert_eq!(input.lines[0].quantity, "10");
        assert_eq!(input.order_id, Some(Uuid::from_u128(9)));
    }

    /// A line fully shipped is not offered again.
    #[test]
    fn a_finished_line_is_left_off() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let input = DeliveryInput::against(&order("40", "40"), today);

        assert!(input.lines.is_empty());
        assert!(Outstanding::of(&order("40", "40")).is_none());
    }

    /// What is left is worked out, never stored.
    #[test]
    fn the_remainder_comes_from_the_order_rather_than_a_stored_number() {
        let outstanding = Outstanding::of(&order("40", "30")).unwrap();

        assert_eq!(outstanding.order_number, "SO-2026-00042");
        assert_eq!(outstanding.lines.len(), 1);
        assert_eq!(outstanding.lines[0].outstanding, qty("10"));
    }

    /// A short shipment is one edit: type zero on the lines that are not going
    /// and they drop out rather than being refused.
    #[test]
    fn a_line_edited_to_nothing_is_dropped_rather_than_refused() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let mut input = DeliveryInput::against(&order("40", "0"), today);

        input.lines.push(DeliveryLineInput {
            variant_id: Some(Uuid::from_u128(7)),
            quantity: "0".to_owned(),
            ..DeliveryLineInput::blank()
        });

        let checked = input.check().unwrap();

        assert_eq!(checked.lines.len(), 1);
        assert_eq!(checked.lines[0].quantity, qty("40"));
    }

    /// A delivery with nothing on it is not a delivery.
    #[test]
    fn a_delivery_of_nothing_is_refused() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let mut input = DeliveryInput::blank(today);

        input.customer_id = Some(Uuid::from_u128(4));
        input.warehouse_id = Some(Uuid::from_u128(5));

        assert_eq!(input.check(), Err(DeliveryError::NothingDespatched));
    }

    /// Goods leaving is a positive quantity. A negative one is a return, which
    /// is a receipt.
    #[test]
    fn a_negative_quantity_is_refused() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let mut input = DeliveryInput::blank(today);

        input.customer_id = Some(Uuid::from_u128(4));
        input.warehouse_id = Some(Uuid::from_u128(5));
        input.lines = vec![DeliveryLineInput {
            variant_id: Some(Uuid::from_u128(2)),
            quantity: "-5".to_owned(),
            ..DeliveryLineInput::blank()
        }];

        assert_eq!(input.check(), Err(DeliveryError::QuantityNegative));
    }
}
