//! The item form.
//!
//! # Why this one has tabs
//!
//! Nineteen fields. An item is what the workspace stocks, what it costs, what
//! it is bought in, what it sells for, how it is traced and what a catalogue
//! prints about it, and a person opening this screen has come for one of those
//! - not all six. One column of nineteen controls is a form somebody scrolls
//! past three quarters of every time.
//!
//! It is still one form: one draft, one save, one set of errors, and a tab
//! holding a field the server complained about is marked. See
//! [`Field::on_tab`](crate::ui::form::Field::on_tab).
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
//! # The category is a lookup, not a select
//!
//! Because it is a *record*, and because discovering halfway through a new item
//! that its category does not exist yet must not cost the form. The quick add
//! creates one in a dialog, selects it and closes - see [`crate::ui::lookup`].
//!
//! # Fields that appear and disappear
//!
//! A service has no stock, so nothing about counting it is shown. An untracked
//! item has no lot numbers, and an item with no lot numbers has nothing for an
//! expiry date to hang on. Each of those is a combination a form could put on
//! screen and none of them means anything, so the fields are hidden rather than
//! shown and quietly ignored.

use app_inventory::category::{Category, CategoryInput};
use app_inventory::item::{ItemInput, ItemKind, Tracking};
use app_inventory::unit::Unit;
use leptos::prelude::*;
use phonix_core::permissions;
use uuid::Uuid;

use super::FormConfig;
use crate::components::page::{Notice, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{save_item, save_item_category};
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};
use crate::ui::lookup::{Choices, QuickAdd};

const NOT_SET: &str = "";

/// The tabs, in the order somebody fills them in.
const GENERAL: &str = "general";
const STOCK: &str = "stock";
const BUYING: &str = "buying";
const SELLING: &str = "selling";
const NOTES: &str = "notes";

/// What the workspace stocks, buys and sells.
pub fn item_form(categories: Vec<Category>, units: Vec<Unit>) -> FormConfig<ItemInput> {
    // The lookup reads a *record* - an id and the label to draw beside it - and
    // `ItemInput` carries only the id. Rather than widen the draft to hold a
    // name it never sends, the reader closes over the list the screen already
    // fetched and finds the label there.
    let named = categories.clone();

    FormConfig::new("item", |draft: ItemInput| async move { save_item(draft).await })
        // A workbench rather than a questionnaire: five tabs of a handful of
        // fields each, where the reading measure would leave two thirds of a
        // wide screen empty and the person scrolling the third that is not.
        .full_width()
        .field(
            Field::text("name", l!("field.name"), |m: &ItemInput| {
                FieldValue::text(&m.name)
            })
            .writing(|m, value| m.name = value.as_input())
            .placeholder("Widget, 12mm")
            .on_tab_with(GENERAL, l!("items.tab.general"), Icon::Package)
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
            .on_tab(GENERAL, l!("items.tab.general"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::text("barcode", l!("items.barcode"), |m: &ItemInput| {
                FieldValue::text(&m.barcode)
            })
            .writing(|m, value| m.barcode = value.as_input())
            .placeholder("5012345678900")
            .help(l!("items.barcode_help"))
            .on_tab(GENERAL, l!("items.tab.general"))
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
            .on_tab(GENERAL, l!("items.tab.general"))
            .require(permissions::ITEMS_EDIT)
            .required(),
        )
        .field(
            Field::lookup(
                "category_id",
                l!("items.category"),
                Choices::List(category_choices(&categories)),
                move |m: &ItemInput| {
                    FieldValue::record(m.category_id.and_then(|id| {
                        named
                            .iter()
                            .find(|row| row.id == id)
                            .map(|row| Choice::new(row.id.to_string(), &row.code))
                    }))
                },
            )
            .writing(|m, value| {
                m.category_id = value
                    .as_records()
                    .first()
                    .and_then(|chosen| chosen.value.parse().ok());
            })
            // Discovering the category is missing must not cost the form.
            .adding(QuickAdd::form(
                l!("categories.new"),
                l!("categories.new"),
                |answer| view! { <AddCategory answer=answer /> }.into_any(),
            ))
            // What decides how it is costed, so it is not a filing decision.
            .help(l!("items.category_help"))
            .on_tab(GENERAL, l!("items.tab.general"))
            .require(permissions::ITEMS_EDIT)
            .required(),
        )
        .field(
            Field::toggle("is_active", l!("field.in_use"), |m: &ItemInput| {
                FieldValue::Bool(m.is_active)
            })
            .writing(|m, value| m.is_active = value.as_bool())
            .help(l!("items.active_help"))
            .on_tab(GENERAL, l!("items.tab.general"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle("is_tracked", l!("items.tracked"), |m: &ItemInput| {
                FieldValue::Bool(m.is_tracked)
            })
            .writing(|m, value| m.is_tracked = value.as_bool())
            .help(l!("items.tracked_help"))
            // A service has no quantity, ever.
            .when(|m: &ItemInput| m.kind.can_be_stocked())
            .on_tab_with(STOCK, l!("items.tab.stock"), Icon::Boxes)
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::select(
                "stock_unit_id",
                l!("items.stock_unit"),
                unit_choices(&units),
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
            .on_tab(STOCK, l!("items.tab.stock"))
            .require(permissions::ITEMS_EDIT)
            .required(),
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
            .on_tab(STOCK, l!("items.tab.stock"))
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
            .on_tab(STOCK, l!("items.tab.stock"))
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
            .on_tab(STOCK, l!("items.tab.stock"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle(
                "can_be_purchased",
                l!("items.purchasable"),
                |m: &ItemInput| FieldValue::Bool(m.can_be_purchased),
            )
            .writing(|m, value| m.can_be_purchased = value.as_bool())
            .on_tab_with(BUYING, l!("items.tab.buying"), Icon::Truck)
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::select(
                "purchase_unit_id",
                l!("items.purchase_unit"),
                unit_choices(&units),
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
            .none_label(l!("items.purchase_unit.same"))
            .help(l!("items.purchase_unit_help"))
            .when(|m: &ItemInput| m.can_be_purchased)
            .on_tab(BUYING, l!("items.tab.buying"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::text("cost", l!("items.cost"), |m: &ItemInput| {
                FieldValue::text(&m.cost)
            })
            .writing(|m, value| m.cost = value.as_input())
            .placeholder("0.00")
            .help(l!("items.cost_help"))
            .on_tab(BUYING, l!("items.tab.buying"))
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
            .on_tab(BUYING, l!("items.tab.buying"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            Field::toggle("can_be_sold", l!("items.sellable"), |m: &ItemInput| {
                FieldValue::Bool(m.can_be_sold)
            })
            .writing(|m, value| m.can_be_sold = value.as_bool())
            .on_tab_with(SELLING, l!("items.tab.selling"), Icon::ShoppingCart)
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
            .on_tab(SELLING, l!("items.tab.selling"))
            .require(permissions::ITEMS_EDIT),
        )
        .field(
            // The editor rather than a textarea: what goes here is what a
            // catalogue prints and what a quotation pastes, and a specification
            // is a list and a table more often than it is a paragraph.
            Field::rich_text("description", l!("field.description"), |m: &ItemInput| {
                FieldValue::text(&m.description)
            })
            .writing(|m, value| m.description = value.as_input())
            .on_tab_with(NOTES, l!("items.tab.notes"), Icon::FileText)
            .require(permissions::ITEMS_EDIT),
        )
        .action(
            FormAction::submit(l!("common.save"))
                .icon(Icon::Save)
                .then(Then::Say("Item saved."))
                .require(permissions::ITEMS_EDIT),
        )
}

/// A category, without leaving the item.
///
/// The name and nothing else. Everything a category actually decides - the
/// costing method, the valuation, the removal strategy - takes the defaults
/// here, because a person halfway through adding an item is not in a position
/// to answer them and a dialog that asked would be a second form. The category
/// screen is where those are set, and this one says so.
#[component]
fn add_category(answer: Callback<Choice>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let failed = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();

        let typed = name.get_untracked().trim().to_owned();
        if typed.is_empty() {
            return;
        }

        saving.set(true);
        failed.set(None);

        leptos::task::spawn_local(async move {
            let draft = CategoryInput {
                name: typed.clone(),
                ..CategoryInput::blank()
            };
            let result = save_item_category(draft).await;
            let _ = saving.try_set(false);

            match result {
                Ok(phonix_core::form::Submission::Saved(stored)) => match stored.id {
                    // Answered with the id the service allocated, and with the
                    // name as typed: the full path is what the picker's other
                    // rows show, and this row is a top-level category whose
                    // path is its name.
                    Some(id) => answer.run(Choice::new(id.to_string(), typed)),
                    None => {
                        let _ = failed.try_set(Some(l!("categories.gone")));
                    }
                },
                Ok(phonix_core::form::Submission::Rejected(errors)) => {
                    let said = errors
                        .into_iter()
                        .map(|error| crate::i18n::t(&error.message))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let _ = failed.try_set(Some(said));
                }
                Err(err) => {
                    let _ = failed.try_set(Some(err.to_string()));
                }
            }
        });
    };

    view! {
        <form class="space-y-3" on:submit=submit>
            <Notice message=Signal::derive(move || failed.get()) tone=Tone::Danger />

            <div>
                <label for="quick-category-name" class="text-sm font-medium text-content">
                    {l!("field.name")}
                </label>
                <input
                    id="quick-category-name"
                    class="mt-1"
                    placeholder="Raw materials"
                    prop:value=move || name.get()
                    on:input=move |event| name.set(event_target_value(&event))
                />
                <p class="mt-1 text-xs text-content-subtle">{l!("categories.quick_add_help")}</p>
            </div>

            <PrimaryButton
                label=l!("common.save")
                icon=Icon::Save
                button_type="submit"
                pending=saving
            />
        </form>
    }
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
    // The full path rather than the name: two categories called "Consumables"
    // under different parents are two different costing methods, and a picker
    // that showed only the leaf would make them indistinguishable.
    categories
        .iter()
        .map(|row| Choice::new(row.id.to_string(), row.code.clone()))
        .collect()
}

/// The units, labelled with what each one measures.
///
/// No empty entry: the renderer adds one to every select that is not required,
/// and the purchase unit names it through `Field::none_label` - "Same as the
/// stock unit", which is what leaving it blank actually means.
fn unit_choices(units: &[Unit]) -> Vec<Choice> {
    units
        .iter()
        .map(|unit| {
            Choice::new(unit.id.to_string(), format!("{} \u{b7} {}", unit.code, unit.name))
                .detail(crate::i18n::t(&unit.class.label()))
        })
        .collect()
}
