//! The unit of measure form.
//!
//! # The factor is the whole form
//!
//! Everything else is a label. The factor says how many of the class's base
//! unit one of these is, and it is what makes a conversion possible at all - so
//! the help text says which unit it is against rather than leaving somebody to
//! work out that grams are the base and kilograms are 1000.

use app_inventory::unit::{Unit, UnitClass, UnitInput};
use phonix_core::permissions;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::save_unit;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

/// What stock is counted in.
///
/// `existing` is the unit list as the screen has it, so the help text can name
/// the base of whichever class is chosen. Passed in rather than fetched here,
/// which keeps this a pure description of a form.
pub fn unit_form(existing: Vec<Unit>) -> FormConfig<UnitInput> {
    FormConfig::new("unit", |draft: UnitInput| async move { save_unit(draft).await })
        .field(
            Field::text("code", l!("field.code"), |m: &UnitInput| {
                FieldValue::text(&m.code)
            })
            .writing(|m, value| m.code = value.as_input())
            .placeholder("KG")
            .help(l!("units.code_help"))
            .require(permissions::UNITS_MANAGE)
            .required(),
        )
        .field(
            Field::text("name", l!("field.name"), |m: &UnitInput| {
                FieldValue::text(&m.name)
            })
            .writing(|m, value| m.name = value.as_input())
            .placeholder("Kilogram")
            .require(permissions::UNITS_MANAGE)
            .required(),
        )
        .field(
            Field::select(
                "class",
                l!("units.class"),
                class_choices(),
                |m: &UnitInput| FieldValue::choice(m.class.as_str()),
            )
            // Anything unparseable stays a count, which is the default and the
            // one class that needs no conversion to be useful.
            .writing(|m, value| {
                m.class = value
                    .as_choice()
                    .and_then(UnitClass::parse)
                    .unwrap_or(UnitClass::Count);
            })
            .help(l!("units.class_help"))
            .require(permissions::UNITS_MANAGE)
            .required(),
        )
        .field(
            Field::text("factor", l!("units.factor"), |m: &UnitInput| {
                FieldValue::text(&m.factor)
            })
            .writing(|m, value| m.factor = value.as_input())
            .placeholder("1000")
            .help(factor_help(&existing))
            .require(permissions::UNITS_MANAGE)
            .required(),
        )
        .field(
            Field::toggle("is_active", l!("field.in_use"), |m: &UnitInput| {
                FieldValue::Bool(m.is_active)
            })
            .writing(|m, value| m.is_active = value.as_bool())
            .help(l!("units.active_help"))
            .require(permissions::UNITS_MANAGE),
        )
        .action(
            FormAction::submit(l!("common.save"))
                .icon(Icon::Save)
                .then(Then::Say("Unit saved."))
                .require(permissions::UNITS_MANAGE),
        )
}

fn class_choices() -> Vec<Choice> {
    UnitClass::ALL
        .iter()
        .map(|class| Choice::new(class.as_str(), crate::i18n::t(&class.label())))
        .collect()
}

/// "How many of the base unit one of these is", with the bases named.
///
/// Naming them is the point. "Factor against the base unit" is true and useless
/// to somebody who does not know what the base is, and getting it wrong by a
/// thousand is a mistake that only surfaces on a delivery note.
fn factor_help(existing: &[Unit]) -> String {
    let bases: Vec<String> = UnitClass::ALL
        .iter()
        .filter_map(|class| {
            existing
                .iter()
                .find(|unit| unit.class == *class && unit.is_base)
                .map(|unit| format!("{} = {}", crate::i18n::t(&class.label()), unit.code))
        })
        .collect();

    if bases.is_empty() {
        // The first unit of a class defines it, and a factor of one is what
        // says so.
        return l!("units.factor_help.first");
    }

    format!("{} {}", l!("units.factor_help"), bases.join(", "))
}
