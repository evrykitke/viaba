//! The item category form.
//!
//! # This is an accounting screen wearing an inventory heading
//!
//! Three of the five fields decide what the workspace says its stock is worth.
//! The costing method in particular is frozen once anything is filed here,
//! because changing it is a revaluation - so the help text says that before
//! somebody tries, rather than the save refusing afterwards.

use app_inventory::category::{CategoryInput, CategorySummary, CostingMethod, RemovalStrategy, Valuation};
use phonix_core::permissions;
use uuid::Uuid;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::save_item_category;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

const NOT_SET: &str = "";

/// How a kind of stock is costed, valued and picked.
pub fn item_category_form(
    editing: Option<Uuid>,
    categories: Vec<CategorySummary>,
) -> FormConfig<CategoryInput> {
    FormConfig::new("item-category", |draft: CategoryInput| async move {
        save_item_category(draft).await
    })
    .field(
        Field::text("name", l!("field.name"), |m: &CategoryInput| {
            FieldValue::text(&m.name)
        })
        .writing(|m, value| m.name = value.as_input())
        .placeholder("Raw materials")
        .require(permissions::ITEM_CATEGORIES_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "parent_id",
            l!("categories.parent"),
            parent_choices(editing, &categories),
            |m: &CategoryInput| {
                FieldValue::choice(
                    m.parent_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| NOT_SET.to_owned()),
                )
            },
        )
        .writing(|m, value| {
            m.parent_id = value
                .as_choice()
                .filter(|raw| !raw.is_empty())
                .and_then(|raw| Uuid::parse_str(raw).ok());
        })
        .none_label(l!("categories.parent.none"))
        .require(permissions::ITEM_CATEGORIES_MANAGE),
    )
    .field(
        Field::select(
            "costing_method",
            l!("categories.costing"),
            costing_choices(),
            |m: &CategoryInput| FieldValue::choice(m.costing_method.as_str()),
        )
        .writing(|m, value| {
            m.costing_method = value
                .as_choice()
                .and_then(CostingMethod::parse)
                .unwrap_or(CostingMethod::Average);
        })
        // Said before somebody tries, not after the save refuses.
        .help(l!("categories.costing_help"))
        .require(permissions::ITEM_CATEGORIES_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "valuation",
            l!("categories.valuation"),
            valuation_choices(),
            |m: &CategoryInput| FieldValue::choice(m.valuation.as_str()),
        )
        .writing(|m, value| {
            m.valuation = value
                .as_choice()
                .and_then(Valuation::parse)
                .unwrap_or(Valuation::Automated);
        })
        .help(l!("categories.valuation_help"))
        .require(permissions::ITEM_CATEGORIES_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "removal_strategy",
            l!("categories.removal"),
            removal_choices(),
            |m: &CategoryInput| FieldValue::choice(m.removal_strategy.as_str()),
        )
        .writing(|m, value| {
            m.removal_strategy = value
                .as_choice()
                .and_then(RemovalStrategy::parse)
                .unwrap_or(RemovalStrategy::Fifo);
        })
        // FEFO needs lot numbers with dates on them, and an item without them
        // silently falls back to FIFO. Said here rather than discovered.
        .help(l!("categories.removal_help"))
        .require(permissions::ITEM_CATEGORIES_MANAGE),
    )
    .field(
        Field::toggle("is_active", l!("field.in_use"), |m: &CategoryInput| {
            FieldValue::Bool(m.is_active)
        })
        .writing(|m, value| m.is_active = value.as_bool())
        .help(l!("categories.active_help"))
        .require(permissions::ITEM_CATEGORIES_MANAGE),
    )
    .action(
        FormAction::submit(l!("common.save"))
            .icon(Icon::Save)
            .then(Then::Say("Category saved."))
            .require(permissions::ITEM_CATEGORIES_MANAGE),
    )
}

fn costing_choices() -> Vec<Choice> {
    CostingMethod::ALL
        .iter()
        .map(|method| Choice::new(method.as_str(), crate::i18n::t(&method.label())))
        .collect()
}

fn valuation_choices() -> Vec<Choice> {
    Valuation::ALL
        .iter()
        .map(|valuation| Choice::new(valuation.as_str(), crate::i18n::t(&valuation.label())))
        .collect()
}

fn removal_choices() -> Vec<Choice> {
    RemovalStrategy::ALL
        .iter()
        .map(|strategy| Choice::new(strategy.as_str(), crate::i18n::t(&strategy.label())))
        .collect()
}

fn parent_choices(editing: Option<Uuid>, categories: &[CategorySummary]) -> Vec<Choice> {
    let mut choices = Vec::new();

    choices.extend(
        categories
            .iter()
            .filter(|row| Some(row.id) != editing)
            .map(|row| {
                let indent = "\u{00a0}\u{00a0}".repeat(row.depth.min(4) as usize);
                Choice::new(row.id.to_string(), format!("{indent}{}", row.name))
            }),
    );

    choices
}
