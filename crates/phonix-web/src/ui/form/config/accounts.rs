//! The account form.
//!
//! The number is typed, never generated, and blank is refused even on create:
//! the default chart supplies a number and an accountant adding an account has
//! one in mind before they have a name. That is the opposite of a department
//! code, and the reason is in ADR 0006 section 3 - a number an accountant reads
//! in a report is theirs to choose.
//!
//! The type picker is grouped by class, because the class is what a reader is
//! actually choosing between and twenty-eight flat options is a list nobody
//! reads to the end of.

use app_books::account::{AccountInput, AccountType};

use phonix_core::permissions;

use super::FormConfig;
use crate::i18n::t;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::save_account;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

/// What an account is, and whether it is still in use.
pub fn account_form(editing: bool) -> FormConfig<AccountInput> {
    let gate = if editing {
        permissions::ACCOUNTS_EDIT
    } else {
        permissions::ACCOUNTS_CREATE
    };

    FormConfig::new("account", |draft: AccountInput| async move {
        save_account(draft).await
    })
    .field(
        Field::text("number", l!("field.number"), |m: &AccountInput| {
            FieldValue::text(&m.number)
        })
        .writing(|m, value| m.number = value.as_input())
        .placeholder("6200")
        .help(l!("accounts.number_help"))
        .require(gate)
        .required(),
    )
    .field(
        Field::text("name", l!("field.name"), |m: &AccountInput| {
            FieldValue::text(&m.name)
        })
        .writing(|m, value| m.name = value.as_input())
        .placeholder("Office rent")
        .require(gate)
        .required(),
    )
    .field(
        Field::select(
            "account_type",
            l!("field.type"),
            type_choices(),
            |m: &AccountInput| FieldValue::choice(m.account_type.as_str()),
        )
        // An unparseable value keeps what the draft already had rather than
        // silently becoming an operating expense: the type decides which side
        // of a report every balance on this account lands on.
        .writing(|m, value| {
            if let Some(parsed) = value.as_choice().and_then(AccountType::parse) {
                m.account_type = parsed;
            }
        })
        .help(l!("accounts.type_help"))
        .require(gate)
        .required(),
    )
    .field(
        Field::multiline("description", l!("field.description"), 3, |m: &AccountInput| {
            FieldValue::text(m.description.clone().unwrap_or_default())
        })
        .writing(|m, value| {
            let text = value.as_input();
            m.description = (!text.is_empty()).then_some(text);
        })
        .help(l!("accounts.description_help"))
        .require(gate),
    )
    .field(
        Field::toggle("is_active", l!("field.status"), |m: &AccountInput| {
            FieldValue::Bool(m.is_active)
        })
        .writing(|m, value| m.is_active = value.as_bool())
        .help(l!("accounts.active_help"))
        .require(gate),
    )
    .action(
        FormAction::submit(l!("common.save"))
            .icon(Icon::Save)
            .then(Then::Say("Account saved."))
            .require(gate),
    )
}

/// Every type, with its class in front of it, in class order.
///
/// A `<select>` cannot be nested, so the class is a prefix rather than an
/// `<optgroup>`: the sort still puts the assets together, which is what the
/// grouping was for.
fn type_choices() -> Vec<Choice> {
    AccountType::ALL
        .iter()
        .copied()
        .map(|account_type| {
            let class = t(&account_type.class().label());
            let name = t(&account_type.label());

            Choice::new(account_type.as_str(), format!("{class} · {name}"))
        })
        .collect()
}
