//! The chart of accounts, as a grid.
//!
//! Read-only, and there is no "new account" button: the chart arrives seeded
//! from `config/defaults/books.toml` and editing it belongs with the ledger
//! that posts to it.
//!
//! Ordered by number and not sorted by default beyond that. An accountant reads
//! a chart in number order because the ranges *are* the classification, and a
//! grid that opened sorted by name would throw that away.

use app_books::account::Account;
use leptos::prelude::*;
use phonix_core::permissions;

use super::GridConfig;
use crate::components::account_class::{ClassChip, ClassDot};
use crate::components::page::{Badge, Tone};
use crate::i18n::t;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::list_accounts;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// What this workspace posts to.
pub fn accounts_grid() -> GridConfig<Account> {
    GridConfig::new("accounts", Source::in_memory(list_accounts))
        .searching(l!("accounts.search"))
        .exports_as("accounts")
        .min_width("sm:min-w-[48rem]")
        .empty(
            Icon::ListTree,
            l!("accounts.empty.title"),
            l!("accounts.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &Account| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &Account| {
                Cell::text(&row.name)
            })
            .findable()
            .essential()
            .render(|row| name_cell(row).into_any()),
        )
        .column(
            Column::new("class", l!("accounts.class"), |row: &Account| {
                Cell::text(t(&row.class().label()))
            })
            .sortable()
            .render(|row| view! { <ClassChip class=row.class() /> }.into_any()),
        )
        .column(
            Column::new("type", l!("field.type"), |row: &Account| {
                Cell::text(t(&row.account_type.label()))
            })
            .searchable()
            .sortable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("postable", l!("accounts.postable"), |row: &Account| {
                Cell::bool(row.is_postable())
            })
            .sortable()
            .render(|row| postable_cell(row).into_any()),
        )
        .column(
            Column::new("is_active", l!("field.status"), |row: &Account| {
                Cell::bool(row.is_active)
            })
            .sortable()
            .render(|row| status_cell(row).into_any()),
        )
        .filter(
            Filter::new(
                "postable",
                l!("accounts.postable"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("yes", l!("accounts.only_postable")),
                    FilterChoice::new("no", l!("accounts.only_control")),
                ],
            )
            .matching(|row: &Account, wanted| match wanted {
                "yes" => row.is_postable(),
                "no" => !row.is_postable(),
                _ => true,
            }),
        )
        .filter(
            Filter::new(
                "status",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("active", l!("common.active")),
                    FilterChoice::new("inactive", l!("common.inactive")),
                ],
            )
            .matching(|row: &Account, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("accounts.new"), Icon::Plus, "/sales/accounts/new")
                .require(permissions::ACCOUNTS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &Account| {
                format!("/sales/accounts/{}", row.id)
            })
            .require(permissions::ACCOUNTS),
        )
}

/// The number, with the class colour in front of it. The dot rather than the
/// chip: a chip on every row of a three-hundred-row chart is a wall of colour,
/// and the class already has a column of its own.
fn number_cell(row: &Account) -> impl IntoView {
    let number = row.number.clone();
    let class = row.class();

    view! {
        <span class="flex items-center gap-2">
            <ClassDot class=class />
            <code class="font-mono tabular-nums text-content">{number}</code>
        </span>
    }
}

/// The name, with what the account is for underneath. The description is the
/// reason somebody can tell 5100 from 5200 without asking.
fn name_cell(row: &Account) -> impl IntoView {
    let name = row.name.clone();
    let description = row.description.clone();

    view! {
        <div class="min-w-0">
            <span class="truncate-fade font-medium text-content">{name}</span>
            {description
                .map(|description| {
                    view! {
                        <p class="truncate-fade text-2xs text-content-subtle">{description}</p>
                    }
                })}
        </div>
    }
}

/// Whether a person may post to it directly. A control account is owned by a
/// sub-ledger, and posting to one by hand is how a reconciliation breaks.
fn postable_cell(row: &Account) -> impl IntoView {
    if row.is_postable() {
        view! { <Badge label=l!("accounts.postable.yes") tone=Tone::Success /> }.into_any()
    } else {
        view! { <Badge label=l!("accounts.postable.no") /> }.into_any()
    }
}

fn status_cell(row: &Account) -> impl IntoView {
    if row.is_active {
        view! { <Badge label=l!("common.active") tone=Tone::Success /> }.into_any()
    } else {
        view! { <Badge label=l!("common.inactive") /> }.into_any()
    }
}
