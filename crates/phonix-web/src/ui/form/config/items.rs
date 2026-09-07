//! The item form.
//!
//! # The code is offered empty and the barcode is not
//!
//! Blank means the service allocates one; typed means it is used as typed. It
//! is not previewed - promising `ITM-00042` before the row exists promises a
//! number somebody else may take first.
//!
//! The barcode is the opposite and deliberately so: a UPC is printed on the
//! packet by whoever made it, and generating one would be inventing a fact
//! about the physical world. ADR 0006 section 3.
//!
//! # Fields that appear and disappear
//!
//! A service has no stock, so nothing about counting it is shown. An untracked
//! item has no lot numbers, and an item with no lot numbers has nothing for an
//! expiry date to hang on. Each of those is a combination a form could put on
//! screen and none of them means anything, so the fields are hidden rather than
//! shown and quietly ignored.

use app_inventory::category::Category;
use app_inventory::item::{ItemInput, ItemKind, Tracking};
use app_inventory::unit::Unit;
use phonix_core::permissions;
use uuid::Uuid;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::save_item;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

const NOT_SET: &str = "";

/// What the workspace stocks, buys and sells.
pub fn item_form(categories: Vec<Category>, units: Vec<Unit>) -> FormConfig<ItemInput> {
    FormConfig::new("item", |draft: ItemInput| async move { save_item(draft).await })
        .field(
            Field::text("name", l!("field.name"), |m: &ItemInput| {
                FieldValue::text(&m.name)
            })
            .writing(|m, value| m.name = value.as_input())
            .placeholder("Widget, 12mm")
            .require(permissions::ITEMS_EDIT)
            .required(),
        )
        .field(
            Field::text("code", l!("field.code"), |m: &ItemInput| {
                FieldValue::text(&m.code)
            })
            .writing(|m, value| m.code = value.as_input())
            // No placeholder: grey `ITM-00042` reads as a promise.
            .help(l!("items.code_help"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::text("barcode", l!("items.barcode"), |m: &ItemInput| {
                FieldValue::text(&m.barcode)
            })
            .writing(|m, value| m.barcode = value.as_input())
            .placeholder("5012345678900")
            .help(l!("items.barcode_help"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::select(
                "kind",
                l!("items.kind"),
                kind_choices(),
                |m: &ItemInput| FieldValue::choice(m.kind.as_str()),
            )
            .writing(|m, value| {
                m.kind = value
                    .as_choice()
                    .and_then(ItemKind::parse)
                    .unwrap_or(ItemKind::Goods);
            })
            .require(permissions::ITEMS_EDIT)
            .required(),
        )
        .field(
            Field::select(
                "category_id",
                l!("items.category"),
                category_choices(&categories),
                |m: &ItemInput| {
                    FieldValue::choice(
                        m.category_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| NOT_SET.to_owned()),
                    )
                },
            )
            .writing(|m, value| {
                m.category_id = value
                    .as_choice()
                    .filter(|raw| !raw.is_empty())
                    .and_then(|raw| Uuid::parse_str(raw).ok());
            })
            // What decides how it is costed, so it is not a filing decision.
            .help(l!("items.category_help"))
            .require(permissions::ITEMS_EDIT)
            .required(),
        )
        .field(
            Field::toggle("is_tracked", l!("items.tracked"), |m: &ItemInput| {
                FieldValue::Bool(m.is_tracked)
            })
            .writing(|m, value| m.is_tracked = value.as_bool())
            .help(l!("items.tracked_help"))
            // A service has no quantity, ever.
            .when(|m: &ItemInput| m.kind.can_be_stocked())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::select(
                "tracking",
                l!("items.tracking"),
                tracking_choices(),
                |m: &ItemInput| FieldValue::choice(m.tracking.as_str()),
            )
            .writing(|m, value| {
                m.tracking = value
                    .as_choice()
                    .and_then(Tracking::parse)
                    .unwrap_or(Tracking::None);
            })
            // Frozen once stock exists, and the reason is worth saying: the
            // units already on the shelf would belong to no lot.
            .help(l!("items.tracking_help"))
            .when(|m: &ItemInput| m.is_tracked && m.kind.can_be_stocked())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle("uses_expiry", l!("items.expiry"), |m: &ItemInput| {
                FieldValue::Bool(m.uses_expiry)
            })
            .writing(|m, value| m.uses_expiry = value.as_bool())
            .help(l!("items.expiry_help"))
            // An expiry date belongs to a lot; without lot numbers there is
            // nothing to date.
            .when(|m: &ItemInput| m.tracking.needs_a_number())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::select(
                "stock_unit_id",
                l!("items.stock_unit"),
                unit_choices(&units, false),
                |m: &ItemInput| {
                    FieldValue::choice(
                        m.stock_unit_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| NOT_SET.to_owned()),
                    )
                },
            )
            .writing(|m, value| {
                m.stock_unit_id = value
                    .as_choice()
                    .filter(|raw| !raw.is_empty())
                    .and_then(|raw| Uuid::parse_str(raw).ok());
            })
            .help(l!("items.stock_unit_help"))
            .require(permissions::ITEMS_EDIT)
            .required(),
        )
        .field(
            Field::select(
                "purchase_unit_id",
                l!("items.purchase_unit"),
                unit_choices(&units, true),
                |m: &ItemInput| {
                    FieldValue::choice(
                        m.purchase_unit_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| NOT_SET.to_owned()),
                    )
                },
            )
            .writing(|m, value| {
                m.purchase_unit_id = value
                    .as_choice()
                    .filter(|raw| !raw.is_empty())
                    .and_then(|raw| Uuid::parse_str(raw).ok());
            })
            .help(l!("items.purchase_unit_help"))
            .when(|m: &ItemInput| m.can_be_purchased)
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::text("cost", l!("items.cost"), |m: &ItemInput| {
                FieldValue::text(&m.cost)
            })
            .writing(|m, value| m.cost = value.as_input())
            .placeholder("0.00")
            .help(l!("items.cost_help"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::text("sale_price", l!("items.sale_price"), |m: &ItemInput| {
                FieldValue::text(&m.sale_price)
            })
            .writing(|m, value| m.sale_price = value.as_input())
            // No placeholder of 0.00: free and unpriced are different, and an
            // empty box is what "not priced yet" looks like.
            .help(l!("items.sale_price_help"))
            .when(|m: &ItemInput| m.can_be_sold)
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle(
                "can_be_purchased",
                l!("items.purchasable"),
                |m: &ItemInput| FieldValue::Bool(m.can_be_purchased),
            )
            .writing(|m, value| m.can_be_purchased = value.as_bool())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle("can_be_sold", l!("items.sellable"), |m: &ItemInput| {
                FieldValue::Bool(m.can_be_sold)
            })
            .writing(|m, value| m.can_be_sold = value.as_bool())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::number(
                "purchase_lead_days",
                l!("items.lead_days"),
                |m: &ItemInput| FieldValue::Number(m.purchase_lead_days.map(f64::from)),
            )
            .writing(|m, value| {
                m.purchase_lead_days = value
                    .as_number()
                    .filter(|days| *days >= 0.0)
                    .map(|days| days as i32);
            })
            // What a reordering rule needs to fire before the shelf is empty
            // rather than when it is.
            .help(l!("items.lead_days_help"))
            .when(|m: &ItemInput| m.can_be_purchased)
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::number("weight_grams", l!("items.weight"), |m: &ItemInput| {
                FieldValue::Number(m.weight_grams.map(|grams| grams as f64))
            })
            .writing(|m, value| {
                m.weight_grams = value
                    .as_number()
                    .filter(|grams| *grams >= 0.0)
                    .map(|grams| grams as i64);
            })
            .help(l!("items.weight_help"))
            .when(|m: &ItemInput| m.kind.can_be_stocked())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::multiline("description", l!("field.description"), 3, |m: &ItemInput| {
                FieldValue::text(&m.description)
            })
            .writing(|m, value| m.description = value.as_input())
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle("is_active", l!("field.status"), |m: &ItemInput| {
                FieldValue::Bool(m.is_active)
            })
            .writing(|m, value| m.is_active = value.as_bool())
            .require(permissions::ITEMS_EDIT),
        )
        .action(
            FormAction::submit(l!("common.save"))
                .icon(Icon::Save)
                .then(Then::Say("Item saved."))
                .require(permissions::ITEMS_EDIT),
        )
}

fn kind_choices() -> Vec<Choice> {
    ItemKind::ALL
        .iter()
        .map(|kind| Choice::new(kind.as_str(), crate::i18n::t(&kind.label())))
        .collect()
}

fn tracking_choices() -> Vec<Choice> {
    Tracking::ALL
        .iter()
        .map(|tracking| Choice::new(tracking.as_str(), crate::i18n::t(&tracking.label())))
        .collect()
}

fn category_choices(categories: &[Category]) -> Vec<Choice> {
    let mut choices = vec![Choice::new(NOT_SET, l!("items.category.none"))];

    // The full path rather than the name: two categories called "Consumables"
    // under different parents are two different costing methods, and a picker
    // that showed only the leaf would make them indistinguishable.
    choices.extend(
        categories
            .iter()
            .map(|row| Choice::new(row.id.to_string(), row.code.clone())),
    );

    choices
}

/// The units, labelled with what each one measures.
///
/// `optional` adds an empty choice, for the purchase unit: leaving it blank
/// means "the same as the stock unit", which is the ordinary case.
fn unit_choices(units: &[Unit], optional: bool) -> Vec<Choice> {
    let mut choices = Vec::new();

    if optional {
        choices.push(Choice::new(NOT_SET, l!("items.purchase_unit.same")));
    }

    choices.extend(units.iter().map(|unit| {
        Choice::new(unit.id.to_string(), format!("{} · {}", unit.code, unit.name))
            .detail(crate::i18n::t(&unit.class.label()))
    }));

    choices
}
