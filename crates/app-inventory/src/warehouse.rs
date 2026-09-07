//! A warehouse: a building, and the locations it is made of.
//!
//! # A warehouse is not a location, it owns several
//!
//! Creating one creates a small tree: a [`View`] node named after it, a
//! `Stock` location inside that where things actually sit, and - where the
//! workspace receives or ships in more than one step - an `Input`, a `Quality`,
//! a `Packing` and an `Output` location between the door and the shelf.
//!
//! That is what multi-step receiving *is*. A two-step receipt is not a flag
//! that changes how a receipt behaves; it is a receipt into `WH/Input` followed
//! by an internal move from `WH/Input` to `WH/Stock`. Both are ordinary moves,
//! both are visible, and the goods are findable in between - which is the whole
//! point, because "received but not yet put away" is a real place a pallet
//! spends a real afternoon.
//!
//! # Why the number of steps is stored and the locations are derived
//!
//! Storing the steps lets a screen offer the three familiar choices. Deriving
//! the locations from them means changing from one step to two cannot leave a
//! warehouse whose stored `input_location_id` points at nothing.
//!
//! [`View`]: crate::location::LocationKind::View

use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_WAREHOUSE_CODE_LEN: usize = 8;
pub const MAX_WAREHOUSE_NAME_LEN: usize = 120;

/// How many moves stand between the supplier's lorry and the shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptSteps {
    /// Straight to stock. What almost every workspace wants and the default.
    One,
    /// Receive into an input area, then put away. The step that makes "arrived
    /// but not yet on the shelf" a place rather than a guess.
    TwoInputThenStock,
    /// Receive, inspect, then put away. The inspection is a real location, so
    /// stock failing it is somewhere rather than nowhere.
    ThreeInputQualityStock,
}

impl ReceiptSteps {
    pub const ALL: &'static [Self] = &[
        Self::One,
        Self::TwoInputThenStock,
        Self::ThreeInputQualityStock,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "one_step",
            Self::TwoInputThenStock => "two_steps",
            Self::ThreeInputQualityStock => "three_steps",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|steps| steps.as_str() == raw)
    }

    pub const fn count(self) -> u8 {
        match self {
            Self::One => 1,
            Self::TwoInputThenStock => 2,
            Self::ThreeInputQualityStock => 3,
        }
    }

    pub fn label(self) -> Message {
        match self {
            Self::One => msg!("warehouses.receipt.one_step"),
            Self::TwoInputThenStock => msg!("warehouses.receipt.two_steps"),
            Self::ThreeInputQualityStock => msg!("warehouses.receipt.three_steps"),
        }
    }
}

/// How many moves stand between the shelf and the customer's van.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverySteps {
    /// Pick and ship in one.
    One,
    /// Pick, then ship. The picked goods wait in an output area.
    TwoPickThenShip,
    /// Pick, pack, then ship.
    ThreePickPackShip,
}

impl DeliverySteps {
    pub const ALL: &'static [Self] = &[Self::One, Self::TwoPickThenShip, Self::ThreePickPackShip];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "one_step",
            Self::TwoPickThenShip => "two_steps",
            Self::ThreePickPackShip => "three_steps",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|steps| steps.as_str() == raw)
    }

    pub const fn count(self) -> u8 {
        match self {
            Self::One => 1,
            Self::TwoPickThenShip => 2,
            Self::ThreePickPackShip => 3,
        }
    }

    pub fn label(self) -> Message {
        match self {
            Self::One => msg!("warehouses.delivery.one_step"),
            Self::TwoPickThenShip => msg!("warehouses.delivery.two_steps"),
            Self::ThreePickPackShip => msg!("warehouses.delivery.three_steps"),
        }
    }
}

/// The locations a warehouse needs, given how many steps it works in.
///
/// A pure function of the two step counts, listed as the path segment each one
/// takes under the warehouse's view node. Provisioning creates exactly these
/// and no others, so a warehouse switched from three steps to one is left with
/// an unused `Input` rather than a dangling reference.
pub fn required_sublocations(receipt: ReceiptSteps, delivery: DeliverySteps) -> Vec<&'static str> {
    let mut wanted = vec!["Stock"];

    match receipt {
        ReceiptSteps::One => {}
        ReceiptSteps::TwoInputThenStock => wanted.push("Input"),
        ReceiptSteps::ThreeInputQualityStock => {
            wanted.push("Input");
            wanted.push("Quality Control");
        }
    }

    match delivery {
        DeliverySteps::One => {}
        DeliverySteps::TwoPickThenShip => wanted.push("Output"),
        DeliverySteps::ThreePickPackShip => {
            wanted.push("Packing Zone");
            wanted.push("Output");
        }
    }

    wanted
}

/// One warehouse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warehouse {
    pub id: Uuid,
    /// Short, upper case, and the prefix of every location path inside it:
    /// `WH`, `MAN`, `LDS`.
    pub code: String,
    pub name: String,
    /// The `View` node this warehouse's locations hang under.
    pub view_location_id: Uuid,
    /// Where stock sits by default. What a one-step receipt receives into.
    pub stock_location_id: Uuid,
    pub receipt_steps: ReceiptSteps,
    pub delivery_steps: DeliverySteps,
    pub is_active: bool,
}

/// One row of the warehouse grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarehouseSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub receipt_steps: ReceiptSteps,
    pub delivery_steps: DeliverySteps,
    pub is_active: bool,
    /// Internal locations beneath it, so a screen can say how divided up it is.
    pub location_count: i64,
}

/// The editable part of a warehouse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarehouseInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub receipt_steps: ReceiptSteps,
    pub delivery_steps: DeliverySteps,
    pub is_active: bool,
}

impl WarehouseInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            receipt_steps: ReceiptSteps::One,
            delivery_steps: DeliverySteps::One,
            is_active: true,
        }
    }

    pub fn from_warehouse(warehouse: &Warehouse) -> Self {
        Self {
            id: Some(warehouse.id),
            code: warehouse.code.clone(),
            name: warehouse.name.clone(),
            receipt_steps: warehouse.receipt_steps,
            delivery_steps: warehouse.delivery_steps,
            is_active: warehouse.is_active,
        }
    }

    pub fn check(&self) -> Result<Self, WarehouseError> {
        let code = self.code.trim().to_uppercase();
        let name = self.name.trim();

        if code.is_empty() {
            return Err(WarehouseError::CodeRequired);
        }
        if code.chars().count() > MAX_WAREHOUSE_CODE_LEN {
            return Err(WarehouseError::CodeTooLong);
        }
        // The code is the first segment of every location path beneath it, so
        // it may hold neither a separator nor a space.
        if !code.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(WarehouseError::CodeShape);
        }

        if name.is_empty() {
            return Err(WarehouseError::NameRequired);
        }
        if name.chars().count() > MAX_WAREHOUSE_NAME_LEN {
            return Err(WarehouseError::NameTooLong);
        }

        Ok(Self {
            id: self.id,
            code,
            name: name.to_owned(),
            receipt_steps: self.receipt_steps,
            delivery_steps: self.delivery_steps,
            is_active: self.is_active,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WarehouseError {
    #[error("a warehouse needs a short code")]
    CodeRequired,
    #[error("a warehouse code is at most eight characters")]
    CodeTooLong,
    #[error("a warehouse code may contain only letters and digits")]
    CodeShape,
    #[error("a warehouse needs a name")]
    NameRequired,
    #[error("a warehouse name is at most 120 characters")]
    NameTooLong,
    #[error("stock is still held in this warehouse")]
    HoldsStock,
}

impl WarehouseError {
    pub fn field(self) -> &'static str {
        match self {
            Self::CodeRequired | Self::CodeTooLong | Self::CodeShape => "code",
            Self::NameRequired | Self::NameTooLong => "name",
            Self::HoldsStock => "is_active",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CodeRequired => msg!("warehouses.error.code_required"),
            Self::CodeTooLong => msg!("warehouses.error.code_too_long"),
            Self::CodeShape => msg!("warehouses.error.code_shape"),
            Self::NameRequired => msg!("warehouses.error.name_required"),
            Self::NameTooLong => msg!("warehouses.error.name_too_long"),
            Self::HoldsStock => msg!("warehouses.error.holds_stock"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_step_in_and_out_needs_only_a_stock_location() {
        assert_eq!(
            required_sublocations(ReceiptSteps::One, DeliverySteps::One),
            vec!["Stock"]
        );
    }

    #[test]
    fn every_extra_step_is_a_real_place_goods_wait_in() {
        // The point of multi-step: "received but not put away" is somewhere a
        // pallet actually is, not a status on a document.
        assert_eq!(
            required_sublocations(
                ReceiptSteps::ThreeInputQualityStock,
                DeliverySteps::ThreePickPackShip
            ),
            vec!["Stock", "Input", "Quality Control", "Packing Zone", "Output"]
        );
    }

    #[test]
    fn a_warehouse_code_becomes_a_path_prefix_so_it_takes_no_punctuation() {
        let spaced = WarehouseInput {
            code: "WH 1".to_owned(),
            name: "Main".to_owned(),
            ..WarehouseInput::blank()
        };
        assert_eq!(spaced.check(), Err(WarehouseError::CodeShape));

        let lower = WarehouseInput {
            code: " wh ".to_owned(),
            name: " Main ".to_owned(),
            ..WarehouseInput::blank()
        };
        let checked = lower.check().unwrap();
        assert_eq!(checked.code, "WH");
        assert_eq!(checked.name, "Main");
    }

    #[test]
    fn the_step_counts_round_trip() {
        for steps in ReceiptSteps::ALL {
            assert_eq!(ReceiptSteps::parse(steps.as_str()), Some(*steps));
        }
        for steps in DeliverySteps::ALL {
            assert_eq!(DeliverySteps::parse(steps.as_str()), Some(*steps));
        }
        assert_eq!(ReceiptSteps::parse("four_steps"), None);
    }
}
