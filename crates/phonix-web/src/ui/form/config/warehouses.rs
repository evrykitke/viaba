//! The warehouse form.
//!
//! # The step counts are not settings, they are locations
//!
//! Choosing two-step receiving creates a `WH/Input` location and routes goods
//! through it. That is why the help text under each says what it *makes*: a
//! screen that presented them as preferences would leave somebody wondering
//! where the extra location came from.
//!
//! Switching back to one step does not remove the location. Stock may be
//! sitting in it, and a location that disappears takes its history with it.
//!
//! # The default warehouse can be renamed and nothing else
//!
//! A workspace always has one - the app seeds it, because a receipt with no
//! building to receive into is not an awkward screen but a document that
//! cannot exist. Its code is the first segment of every location path inside
//! it, its step counts decide which of those locations exist, and retiring it
//! would leave nowhere for stock to be. So those three are locked and the name
//! is not: "Main warehouse" is our word for it, and a workspace should call its
//! building whatever it calls it.
//!
//! Locked rather than hidden, which is the rule the permission gate follows
//! too: the value on screen is the answer to "why can I not edit this".

use app_inventory::warehouse::{DeliverySteps, ReceiptSteps, WarehouseInput};
use phonix_core::permissions;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::save_warehouse;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

/// A building, and how goods move through it.
pub fn warehouse_form() -> FormConfig<WarehouseInput> {
    FormConfig::new("warehouse", |draft: WarehouseInput| async move {
        save_warehouse(draft).await
    })
    .field(
        Field::text("code", l!("field.code"), |m: &WarehouseInput| {
            FieldValue::text(&m.code)
        })
        .writing(|m, value| m.code = value.as_input())
        .placeholder("WH")
        // It becomes the first segment of every location path in the building,
        // which is why it takes no punctuation and why renaming it is a bigger
        // act than it looks.
        .help(l!("warehouses.code_help"))
        .locked_when(WarehouseInput::locked)
        .require(permissions::WAREHOUSES_MANAGE)
        .required(),
    )
    .field(
        Field::text("name", l!("field.name"), |m: &WarehouseInput| {
            FieldValue::text(&m.name)
        })
        .writing(|m, value| m.name = value.as_input())
        .placeholder("Main warehouse")
        .help(l!("warehouses.name_help"))
        .require(permissions::WAREHOUSES_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "receipt_steps",
            l!("warehouses.receiving"),
            receipt_choices(),
            |m: &WarehouseInput| FieldValue::choice(m.receipt_steps.as_str()),
        )
        .writing(|m, value| {
            m.receipt_steps = value
                .as_choice()
                .and_then(ReceiptSteps::parse)
                .unwrap_or(ReceiptSteps::One);
        })
        .help(l!("warehouses.receiving_help"))
        .locked_when(WarehouseInput::locked)
        .require(permissions::WAREHOUSES_MANAGE),
    )
    .field(
        Field::select(
            "delivery_steps",
            l!("warehouses.shipping"),
            delivery_choices(),
            |m: &WarehouseInput| FieldValue::choice(m.delivery_steps.as_str()),
        )
        .writing(|m, value| {
            m.delivery_steps = value
                .as_choice()
                .and_then(DeliverySteps::parse)
                .unwrap_or(DeliverySteps::One);
        })
        .help(l!("warehouses.shipping_help"))
        .locked_when(WarehouseInput::locked)
        .require(permissions::WAREHOUSES_MANAGE),
    )
    .field(
        Field::toggle("is_active", l!("field.in_use"), |m: &WarehouseInput| {
            FieldValue::Bool(m.is_active)
        })
        .writing(|m, value| m.is_active = value.as_bool())
        // There is no delete: a warehouse owns the locations that carry every
        // movement that ever crossed them.
        .help(l!("warehouses.active_help"))
        .locked_when(WarehouseInput::locked)
        .require(permissions::WAREHOUSES_MANAGE),
    )
    .action(
        FormAction::submit(l!("common.save"))
            .icon(Icon::Save)
            .then(Then::Say("Warehouse saved."))
            .require(permissions::WAREHOUSES_MANAGE),
    )
}

fn receipt_choices() -> Vec<Choice> {
    ReceiptSteps::ALL
        .iter()
        .map(|steps| Choice::new(steps.as_str(), crate::i18n::t(&steps.label())))
        .collect()
}

fn delivery_choices() -> Vec<Choice> {
    DeliverySteps::ALL
        .iter()
        .map(|steps| Choice::new(steps.as_str(), crate::i18n::t(&steps.label())))
        .collect()
}
