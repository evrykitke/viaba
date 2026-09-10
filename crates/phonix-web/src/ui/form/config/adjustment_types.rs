//! The adjustment type form.
//!
//! # The account is the whole form
//!
//! Everything else is a label and a flag. Naming the account is what turns a
//! growing "inventory adjustment" total into three answerable numbers, and the
//! help text says what happens when nobody names one - because leaving it blank
//! is a legitimate answer and has to read as one.
//!
//! The account choices are the chart as the `Ledger` port reports it, passed in
//! rather than fetched here: this stays a description of a form, and a workspace
//! with no accounting module gets a picker with nothing in it and every type on
//! the default, which is what it would have done anyway.

use app_inventory::adjustment::{AdjustmentTypeInput, Direction};
use phonix_core::permissions;
use phonix_ports::ledger::LedgerAccount;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::save_adjustment_type;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

/// Why a stock figure may be corrected by hand.
pub fn adjustment_type_form(chart: Vec<LedgerAccount>) -> FormConfig<AdjustmentTypeInput> {
    // Held for the writer below as well as for the choices: the draft stores
    // the number and the name beside the id, so that this screen and the
    // movements screen can show an account without either of them reaching
    // into another app's schema.
    let accounts = chart.clone();

    FormConfig::new("adjustment_type", |draft: AdjustmentTypeInput| async move {
        save_adjustment_type(draft).await
    })
    .field(
        Field::text("code", l!("field.code"), |m: &AdjustmentTypeInput| {
            FieldValue::text(&m.code)
        })
        .writing(|m, value| m.code = value.as_input())
        .placeholder("DAMAGE")
        .help(l!("adjustment_types.code_help"))
        // Frozen once the app seeded it. The code is what a report groups by
        // and what an integration sends; renaming the app's own is a way to
        // break both without anybody noticing until quarter end.
        .locked_when(|m: &AdjustmentTypeInput| m.is_system)
        .require(permissions::ADJUSTMENT_TYPES_MANAGE)
        .required(),
    )
    .field(
        Field::text("name", l!("field.name"), |m: &AdjustmentTypeInput| {
            FieldValue::text(&m.name)
        })
        .writing(|m, value| m.name = value.as_input())
        .placeholder("Damage")
        .require(permissions::ADJUSTMENT_TYPES_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "direction",
            l!("adjustment_types.direction"),
            direction_choices(),
            |m: &AdjustmentTypeInput| FieldValue::choice(m.direction.as_str()),
        )
        // Anything unparseable stays `Both`, which is the default and the one
        // answer that refuses nothing.
        .writing(|m, value| {
            m.direction = value
                .as_choice()
                .and_then(Direction::parse)
                .unwrap_or(Direction::Both);
        })
        .help(l!("adjustment_types.direction_help"))
        .require(permissions::ADJUSTMENT_TYPES_MANAGE)
        .required(),
    )
    .field(
        Field::select(
            "account_id",
            l!("adjustment_types.account"),
            account_choices(&chart),
            |m: &AdjustmentTypeInput| {
                FieldValue::choice(
                    m.account_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                )
            },
        )
        // All three fields or none. The number and the name are a snapshot the
        // grid and the movement screen read without crossing into `books` -
        // see ADR 0006 section 2 - so they travel with the id or the id does
        // not travel at all.
        .writing(move |m, value| {
            let chosen = value
                .as_choice()
                .and_then(|raw| raw.parse::<uuid::Uuid>().ok())
                .and_then(|id| accounts.iter().find(|account| account.id == id));

            match chosen {
                Some(account) => {
                    m.account_id = Some(account.id);
                    m.account_number = account.number.clone();
                    m.account_name = account.name.clone();
                }
                None => {
                    m.account_id = None;
                    m.account_number = String::new();
                    m.account_name = String::new();
                }
            }
        })
        .none_label(l!("adjustment_types.account.default"))
        .help(l!("adjustment_types.account_help"))
        .require(permissions::ADJUSTMENT_TYPES_MANAGE),
    )
    .field(
        Field::toggle(
            "needs_approval",
            l!("adjustment_types.approval"),
            |m: &AdjustmentTypeInput| FieldValue::Bool(m.needs_approval),
        )
        .writing(|m, value| m.needs_approval = value.as_bool())
        .help(l!("adjustment_types.approval_help"))
        .require(permissions::ADJUSTMENT_TYPES_MANAGE),
    )
    .field(
        Field::toggle(
            "is_active",
            l!("field.in_use"),
            |m: &AdjustmentTypeInput| FieldValue::Bool(m.is_active),
        )
        .writing(|m, value| m.is_active = value.as_bool())
        .help(l!("adjustment_types.active_help"))
        .require(permissions::ADJUSTMENT_TYPES_MANAGE),
    )
    .field(
        Field::multiline("note", l!("field.note"), 3, |m: &AdjustmentTypeInput| {
            FieldValue::text(&m.note)
        })
        .writing(|m, value| m.note = value.as_input())
        .help(l!("adjustment_types.note_help"))
        .require(permissions::ADJUSTMENT_TYPES_MANAGE),
    )
    .action(
        FormAction::submit(l!("common.save"))
            .icon(Icon::Save)
            .then(Then::Say("Adjustment type saved."))
            .require(permissions::ADJUSTMENT_TYPES_MANAGE),
    )
}

fn direction_choices() -> Vec<Choice> {
    Direction::ALL
        .iter()
        .map(|one| Choice::new(one.as_str(), crate::i18n::t(&one.label())))
        .collect()
}

fn account_choices(chart: &[LedgerAccount]) -> Vec<Choice> {
    chart
        .iter()
        .map(|account| {
            Choice::new(account.id.to_string(), account.name.clone()).detail(account.number.clone())
        })
        .collect()
}
