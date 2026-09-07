//! An item: the thing a workspace buys, keeps, counts and sells.
//!
//! # Goods, services, and the flag between them
//!
//! [`ItemKind::Service`] is time, and time is not on a shelf: it has no stock,
//! no location and no valuation, and it is here rather than in a separate app
//! because it appears on the same purchase orders and the same bills.
//!
//! [`ItemKind::Goods`] splits again on [`Item::is_tracked`]. Tracked goods have
//! a quantity that is counted and valued. Untracked goods - stationery, screws
//! bought by the tub - are expensed on receipt and never counted. Making that a
//! flag rather than a third kind is deliberate: a workspace routinely decides
//! partway through that it does want to count the screws after all, and that
//! should be a tick rather than a new item and a data migration.
//!
//! # The code is generated and the barcode is typed
//!
//! ADR 0006 section 3. `ITM-00042` comes out of `core.number_sequences`,
//! because a code somebody invents is a code that collides with one somebody
//! else invented last Tuesday. The barcode does *not*: a UPC is printed on the
//! packet by whoever made it, and generating one would be inventing a fact
//! about the physical world.
//!
//! A workspace migrating from another system may still type a code, and what is
//! refused is a blank one on a row that already has one.
//!
//! # What is *not* here
//!
//! Costing method, valuation and removal strategy, which are on the
//! [category](crate::category) - a decision about a kind of stock rather than
//! about one item. Reordering rules, which are per item *and per location* and
//! so cannot be a column here.

use phonix_core::i18n::Message;
use phonix_core::money::Money;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::Quantity;

pub const MAX_ITEM_CODE_LEN: usize = 40;
pub const MAX_ITEM_NAME_LEN: usize = 200;
pub const MAX_BARCODE_LEN: usize = 64;
pub const MAX_ITEM_DESCRIPTION_LEN: usize = 2000;

/// What sort of thing this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Something physical. Whether it is counted is [`Item::is_tracked`].
    Goods,
    /// Time or work. No quantity on hand, ever.
    Service,
}

impl ItemKind {
    pub const ALL: &'static [Self] = &[Self::Goods, Self::Service];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Goods => "goods",
            Self::Service => "service",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == raw)
    }

    /// Whether it is capable of having a quantity at all.
    pub const fn can_be_stocked(self) -> bool {
        matches!(self, Self::Goods)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Goods => msg!("items.kind.goods"),
            Self::Service => msg!("items.kind.service"),
        }
    }
}

/// How closely individual units are followed.
///
/// # Why this cannot be changed once stock exists
///
/// Turning tracking on for an item that already has two hundred on hand leaves
/// two hundred units belonging to no lot, and every FEFO pick and every recall
/// afterwards silently skips them. The service refuses the change rather than
/// letting a workspace discover that during a recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tracking {
    /// A number on hand and nothing more.
    None,
    /// Batches. One lot covers many units, and an expiry date belongs to the
    /// lot rather than to each unit.
    Lot,
    /// One number per unit. What a warranty claim and a service history need.
    Serial,
}

impl Tracking {
    pub const ALL: &'static [Self] = &[Self::None, Self::Lot, Self::Serial];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lot => "lot",
            Self::Serial => "serial",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == raw)
    }

    /// Whether a movement of this item has to name a lot or serial number.
    pub const fn needs_a_number(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Whether one number covers exactly one unit, which is what makes a
    /// fractional quantity impossible.
    pub const fn is_one_per_unit(self) -> bool {
        matches!(self, Self::Serial)
    }

    pub fn label(self) -> Message {
        match self {
            Self::None => msg!("items.tracking.none"),
            Self::Lot => msg!("items.tracking.lot"),
            Self::Serial => msg!("items.tracking.serial"),
        }
    }
}

/// One item, as a screen and a document line see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: Uuid,
    /// Generated. `ITM-00042`.
    pub code: String,
    pub name: String,
    /// The UPC, EAN or whatever is printed on the packet. Typed, never
    /// generated, and unique where present.
    pub barcode: Option<String>,
    pub description: Option<String>,
    pub kind: ItemKind,
    /// Whether a quantity is kept for it. Always false for a service.
    pub is_tracked: bool,
    pub tracking: Tracking,
    /// Whether a lot of this expires, which is what makes FEFO possible.
    pub uses_expiry: bool,
    pub category_id: Uuid,
    pub category_name: String,
    /// The unit every stored quantity is in. Changing it after stock exists
    /// would restate every quantity, so the service refuses to.
    pub stock_unit_id: Uuid,
    pub stock_unit_code: String,
    /// The unit a supplier quotes in, where it differs - a case, a reel.
    /// Converted to the stock unit on receipt.
    pub purchase_unit_id: Uuid,
    pub purchase_unit_code: String,
    /// What one stock unit costs, in the workspace's base currency. The
    /// standard under `CostingMethod::Standard`; the running average or the
    /// latest layer's cost under the others.
    pub cost: Money,
    /// The list price, before whatever a customer's terms do to it.
    pub sale_price: Option<Money>,
    pub can_be_purchased: bool,
    pub can_be_sold: bool,
    /// Grams. For a shipping quote and for a lorry's weight limit.
    pub weight_grams: Option<i64>,
    /// Days between ordering and arrival, used to work out when a reordering
    /// rule has to fire rather than when the shelf is empty.
    pub purchase_lead_days: Option<i32>,
    pub is_active: bool,
}

impl Item {
    /// Whether a stock movement may name this item.
    pub const fn holds_stock(&self) -> bool {
        self.is_active && self.is_tracked && self.kind.can_be_stocked()
    }

    /// Whether a quantity of this item may be fractional.
    ///
    /// Not for a serial-tracked item: half a serial number is not a thing.
    pub const fn allows_fractional_quantity(&self) -> bool {
        !self.tracking.is_one_per_unit()
    }

    /// Check a quantity against what this item can actually be counted in.
    pub fn check_quantity(&self, quantity: Quantity) -> Result<(), ItemError> {
        if !self.allows_fractional_quantity() && quantity.scaled() % 1_000_000 != 0 {
            return Err(ItemError::SerialQuantityNotWhole);
        }
        Ok(())
    }
}

/// One row of the item grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub barcode: Option<String>,
    pub kind: ItemKind,
    pub is_tracked: bool,
    pub tracking: Tracking,
    pub category_name: String,
    pub stock_unit_code: String,
    pub cost: Money,
    pub is_active: bool,
    /// On hand across every internal location, in the stock unit. `None` for
    /// an item that is not tracked, which is different from zero.
    pub on_hand: Option<Quantity>,
}

/// The editable part of an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemInput {
    pub id: Option<Uuid>,
    /// Empty on create means "allocate one"; typed means use it as typed.
    pub code: String,
    pub name: String,
    pub barcode: String,
    pub description: String,
    pub kind: ItemKind,
    pub is_tracked: bool,
    pub tracking: Tracking,
    pub uses_expiry: bool,
    pub category_id: Option<Uuid>,
    pub stock_unit_id: Option<Uuid>,
    pub purchase_unit_id: Option<Uuid>,
    /// As typed, in the base currency.
    pub cost: String,
    pub sale_price: String,
    pub can_be_purchased: bool,
    pub can_be_sold: bool,
    pub weight_grams: Option<i64>,
    pub purchase_lead_days: Option<i32>,
    pub is_active: bool,
}

impl ItemInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            barcode: String::new(),
            description: String::new(),
            kind: ItemKind::Goods,
            // Counted by default. A workspace that has opened an inventory app
            // wants to count things; the ones that do not are the exception and
            // can untick it.
            is_tracked: true,
            tracking: Tracking::None,
            uses_expiry: false,
            category_id: None,
            stock_unit_id: None,
            purchase_unit_id: None,
            cost: String::new(),
            sale_price: String::new(),
            can_be_purchased: true,
            can_be_sold: true,
            weight_grams: None,
            purchase_lead_days: None,
            is_active: true,
        }
    }

    pub fn from_item(item: &Item) -> Self {
        Self {
            id: Some(item.id),
            code: item.code.clone(),
            name: item.name.clone(),
            barcode: item.barcode.clone().unwrap_or_default(),
            description: item.description.clone().unwrap_or_default(),
            kind: item.kind,
            is_tracked: item.is_tracked,
            tracking: item.tracking,
            uses_expiry: item.uses_expiry,
            category_id: Some(item.category_id),
            stock_unit_id: Some(item.stock_unit_id),
            purchase_unit_id: Some(item.purchase_unit_id),
            cost: item.cost.to_storage_string(),
            sale_price: item
                .sale_price
                .map(Money::to_storage_string)
                .unwrap_or_default(),
            can_be_purchased: item.can_be_purchased,
            can_be_sold: item.can_be_sold,
            weight_grams: item.weight_grams,
            purchase_lead_days: item.purchase_lead_days,
            is_active: item.is_active,
        }
    }

    /// Trim, settle what one field implies about another, and say what is still
    /// wrong.
    ///
    /// The settling is the interesting part. A service cannot be tracked, an
    /// untracked item cannot carry lot numbers, and an item with no lot numbers
    /// cannot have expiry dates - each of those is a combination a form can put
    /// on screen and none of them means anything, so they are corrected here
    /// rather than stored and worked around downstream.
    pub fn check(&self) -> Result<Checked, ItemError> {
        let code = self.code.trim().to_uppercase();
        let name = self.name.trim();
        let barcode = self.barcode.trim();
        let description = self.description.trim();

        if code.is_empty() && self.id.is_some() {
            return Err(ItemError::CodeRequired);
        }
        if !code.is_empty() {
            if code.chars().count() > MAX_ITEM_CODE_LEN {
                return Err(ItemError::CodeTooLong);
            }
            if !code.starts_with(|c: char| c.is_ascii_alphanumeric())
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            {
                return Err(ItemError::CodeShape);
            }
        }

        if name.is_empty() {
            return Err(ItemError::NameRequired);
        }
        if name.chars().count() > MAX_ITEM_NAME_LEN {
            return Err(ItemError::NameTooLong);
        }

        if !barcode.is_empty() {
            if barcode.chars().count() > MAX_BARCODE_LEN {
                return Err(ItemError::BarcodeTooLong);
            }
            // Printed on a packet by a machine. Whitespace inside one is a
            // scan that went wrong or a paste that picked up a line break.
            if barcode.chars().any(char::is_whitespace) {
                return Err(ItemError::BarcodeShape);
            }
        }

        if description.chars().count() > MAX_ITEM_DESCRIPTION_LEN {
            return Err(ItemError::DescriptionTooLong);
        }

        let category_id = self.category_id.ok_or(ItemError::CategoryRequired)?;
        let stock_unit_id = self.stock_unit_id.ok_or(ItemError::StockUnitRequired)?;
        let purchase_unit_id = self.purchase_unit_id.unwrap_or(stock_unit_id);

        if self.weight_grams.is_some_and(|grams| grams < 0) {
            return Err(ItemError::WeightNegative);
        }
        if self.purchase_lead_days.is_some_and(|days| days < 0) {
            return Err(ItemError::LeadTimeNegative);
        }

        let is_tracked = self.is_tracked && self.kind.can_be_stocked();
        let tracking = if is_tracked { self.tracking } else { Tracking::None };
        let uses_expiry = self.uses_expiry && tracking.needs_a_number();

        Ok(Checked {
            id: self.id,
            code,
            name: name.to_owned(),
            barcode: (!barcode.is_empty()).then(|| barcode.to_owned()),
            description: (!description.is_empty()).then(|| description.to_owned()),
            kind: self.kind,
            is_tracked,
            tracking,
            uses_expiry,
            category_id,
            stock_unit_id,
            purchase_unit_id,
            cost: self.cost.trim().to_owned(),
            sale_price: self.sale_price.trim().to_owned(),
            can_be_purchased: self.can_be_purchased,
            can_be_sold: self.can_be_sold,
            weight_grams: self.weight_grams,
            purchase_lead_days: self.purchase_lead_days,
            is_active: self.is_active,
        })
    }
}

/// An item somebody typed, after checking: the ids are present and the flags
/// have been made to agree with each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub barcode: Option<String>,
    pub description: Option<String>,
    pub kind: ItemKind,
    pub is_tracked: bool,
    pub tracking: Tracking,
    pub uses_expiry: bool,
    pub category_id: Uuid,
    pub stock_unit_id: Uuid,
    pub purchase_unit_id: Uuid,
    pub cost: String,
    pub sale_price: String,
    pub can_be_purchased: bool,
    pub can_be_sold: bool,
    pub weight_grams: Option<i64>,
    pub purchase_lead_days: Option<i32>,
    pub is_active: bool,
}

/// What a delete answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    Deleted,
    /// Stock has moved. The moves are the audit trail and the item is named on
    /// them; retiring it is the answer, not deleting it.
    HasMovements,
    /// Some is on hand. Deleting it would lose the value with the row.
    HasStock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ItemError {
    #[error("an item needs a code")]
    CodeRequired,
    #[error("an item code is at most 40 characters")]
    CodeTooLong,
    #[error("an item code may contain only letters, digits, hyphens and underscores")]
    CodeShape,
    #[error("an item needs a name")]
    NameRequired,
    #[error("an item name is at most 200 characters")]
    NameTooLong,
    #[error("a barcode is at most 64 characters")]
    BarcodeTooLong,
    #[error("a barcode may not contain spaces")]
    BarcodeShape,
    #[error("that barcode is already on another item")]
    BarcodeTaken,
    #[error("a description is at most 2000 characters")]
    DescriptionTooLong,
    #[error("an item needs a category")]
    CategoryRequired,
    #[error("an item needs a stock unit")]
    StockUnitRequired,
    #[error("the purchase unit has to measure the same thing as the stock unit")]
    PurchaseUnitMismatch,
    #[error("a weight cannot be negative")]
    WeightNegative,
    #[error("a lead time cannot be negative")]
    LeadTimeNegative,
    #[error("a serial-tracked item is counted in whole units")]
    SerialQuantityNotWhole,
    #[error("stock already exists, so the unit it is counted in cannot change")]
    StockUnitLocked,
    #[error("stock already exists, so how it is tracked cannot change")]
    TrackingLocked,
}

impl ItemError {
    pub fn field(self) -> &'static str {
        match self {
            Self::CodeRequired | Self::CodeTooLong | Self::CodeShape => "code",
            Self::NameRequired | Self::NameTooLong => "name",
            Self::BarcodeTooLong | Self::BarcodeShape | Self::BarcodeTaken => "barcode",
            Self::DescriptionTooLong => "description",
            Self::CategoryRequired => "category_id",
            Self::StockUnitRequired | Self::StockUnitLocked => "stock_unit_id",
            Self::PurchaseUnitMismatch => "purchase_unit_id",
            Self::WeightNegative => "weight_grams",
            Self::LeadTimeNegative => "purchase_lead_days",
            Self::SerialQuantityNotWhole => "quantity",
            Self::TrackingLocked => "tracking",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CodeRequired => msg!("items.error.code_required"),
            Self::CodeTooLong => msg!("items.error.code_too_long"),
            Self::CodeShape => msg!("items.error.code_shape"),
            Self::NameRequired => msg!("items.error.name_required"),
            Self::NameTooLong => msg!("items.error.name_too_long"),
            Self::BarcodeTooLong => msg!("items.error.barcode_too_long"),
            Self::BarcodeShape => msg!("items.error.barcode_shape"),
            Self::BarcodeTaken => msg!("items.error.barcode_taken"),
            Self::DescriptionTooLong => msg!("items.error.description_too_long"),
            Self::CategoryRequired => msg!("items.error.category_required"),
            Self::StockUnitRequired => msg!("items.error.stock_unit_required"),
            Self::PurchaseUnitMismatch => msg!("items.error.purchase_unit_mismatch"),
            Self::WeightNegative => msg!("items.error.weight_negative"),
            Self::LeadTimeNegative => msg!("items.error.lead_time_negative"),
            Self::SerialQuantityNotWhole => msg!("items.error.serial_quantity_not_whole"),
            Self::StockUnitLocked => msg!("items.error.stock_unit_locked"),
            Self::TrackingLocked => msg!("items.error.tracking_locked"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ItemInput {
        ItemInput {
            name: "Widget".to_owned(),
            category_id: Some(Uuid::from_u128(1)),
            stock_unit_id: Some(Uuid::from_u128(2)),
            ..ItemInput::blank()
        }
    }

    #[test]
    fn a_new_item_may_arrive_without_a_code_but_an_existing_one_may_not() {
        assert_eq!(input().check().unwrap().code, "");

        let existing = ItemInput {
            id: Some(Uuid::from_u128(9)),
            ..input()
        };
        assert_eq!(existing.check(), Err(ItemError::CodeRequired));
    }

    #[test]
    fn a_barcode_is_taken_exactly_as_it_was_scanned() {
        // Typed, never generated: it is a fact about a packet somebody else
        // printed. See ADR 0006 section 3.
        let scanned = ItemInput {
            barcode: "  5012345678900  ".to_owned(),
            ..input()
        };
        assert_eq!(
            scanned.check().unwrap().barcode.as_deref(),
            Some("5012345678900")
        );

        let broken = ItemInput {
            barcode: "5012345 678900".to_owned(),
            ..input()
        };
        assert_eq!(broken.check(), Err(ItemError::BarcodeShape));
    }

    #[test]
    fn a_service_is_never_tracked_however_the_form_was_left() {
        let service = ItemInput {
            kind: ItemKind::Service,
            is_tracked: true,
            tracking: Tracking::Serial,
            uses_expiry: true,
            ..input()
        };

        let checked = service.check().unwrap();
        assert!(!checked.is_tracked);
        assert_eq!(checked.tracking, Tracking::None);
        assert!(!checked.uses_expiry);
    }

    #[test]
    fn expiry_dates_need_something_to_hang_on() {
        // An expiry date belongs to a lot. Without lot numbers there is nothing
        // to date, and FEFO would have nothing to sort by.
        let untracked = ItemInput {
            tracking: Tracking::None,
            uses_expiry: true,
            ..input()
        };
        assert!(!untracked.check().unwrap().uses_expiry);

        let lots = ItemInput {
            tracking: Tracking::Lot,
            uses_expiry: true,
            ..input()
        };
        assert!(lots.check().unwrap().uses_expiry);
    }

    #[test]
    fn the_purchase_unit_falls_back_to_the_stock_unit() {
        let checked = input().check().unwrap();
        assert_eq!(checked.purchase_unit_id, checked.stock_unit_id);
    }

    #[test]
    fn an_item_needs_somewhere_to_be_filed_and_something_to_be_counted_in() {
        let uncategorised = ItemInput {
            category_id: None,
            ..input()
        };
        assert_eq!(uncategorised.check(), Err(ItemError::CategoryRequired));

        let unitless = ItemInput {
            stock_unit_id: None,
            ..input()
        };
        assert_eq!(unitless.check(), Err(ItemError::StockUnitRequired));
    }

    #[test]
    fn half_a_serial_number_is_not_a_quantity() {
        let item = Item {
            id: Uuid::from_u128(1),
            code: "ITM-00001".to_owned(),
            name: "Lathe".to_owned(),
            barcode: None,
            description: None,
            kind: ItemKind::Goods,
            is_tracked: true,
            tracking: Tracking::Serial,
            uses_expiry: false,
            category_id: Uuid::from_u128(2),
            category_name: "All".to_owned(),
            stock_unit_id: Uuid::from_u128(3),
            stock_unit_code: "EA".to_owned(),
            purchase_unit_id: Uuid::from_u128(3),
            purchase_unit_code: "EA".to_owned(),
            cost: Money::zero(phonix_core::locale::Currency::USD),
            sale_price: None,
            can_be_purchased: true,
            can_be_sold: true,
            weight_grams: None,
            purchase_lead_days: None,
            is_active: true,
        };

        assert!(!item.allows_fractional_quantity());
        assert_eq!(
            item.check_quantity(Quantity::parse("1.5").unwrap()),
            Err(ItemError::SerialQuantityNotWhole)
        );
        assert_eq!(item.check_quantity(Quantity::parse("2").unwrap()), Ok(()));
        assert!(item.holds_stock());
    }
}
