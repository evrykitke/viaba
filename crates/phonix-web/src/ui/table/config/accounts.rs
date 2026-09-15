//! The chart of accounts, as a grid.
//!
//! Read-only, and there is no "new account" button: the chart arrives seeded
//! from `config/defaults/books.toml` and editing it belongs with the ledger
//! that posts to it.
//!
//! Ordered by number and not sorted by default beyond that. An accountant reads
//! a chart in number order because the ranges *are* the classification, and a
//! grid that opened sorted by name would throw that away.
//!
//! Paged: a chart grows without anybody deciding to grow it. The class and
//! postable columns are derived from the account type rather than stored, so
//! the store sorts and filters them through expressions generated from the
//! enums - see `phonix_db::books::account`.

use app_books::account::{Account, AccountClass, AccountType};
use leptos::prelude::*;
use phonix_core::permissions;

use super::GridConfig;
use crate::components::account_class::{ClassChip, ClassDot};
use crate::components::page::{Badge, Tone};
use crate::i18n::t;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::books_fns::page_accounts;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// What this workspace posts to.
pub fn accounts_grid() -> GridConfig<Account> {
    GridConfig::new("accounts", Source::paged(page_accounts))
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
            // Twenty-eight choices, each prefixed with its class so the assets
            // sit together in the list. The class is the cut most people want
            // and has a filter of its own; the type is the sharper one.
            Filter::new("account_type", l!("field.type"), type_choices()),
        )
        .filter(
            Filter::new("class", l!("accounts.class"), class_choices()),
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
            ),
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
            ),
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

/// Every type, in class order, each named with the class it belongs to.
fn type_choices() -> Vec<FilterChoice> {
    let mut choices = vec![FilterChoice::all(l!("common.all"))];

    choices.extend(AccountType::ALL.iter().copied().map(|account_type| {
        let class = t(&account_type.class().label());
        let name = t(&account_type.label());

        FilterChoice::new(account_type.as_str(), format!("{class} · {name}"))
    }));

    choices
}

/// The five classes.
fn class_choices() -> Vec<FilterChoice> {
    let mut choices = vec![FilterChoice::all(l!("common.all"))];

    choices.extend(
        AccountClass::ALL
            .iter()
            .copied()
            .map(|class| FilterChoice::new(class.as_str(), t(&class.label()))),
    );

    choices
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

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> GridConfig<Account> {
        Owner::new().with(accounts_grid)
    }

    /// Literals rather than imports: `phonix-web` does not depend on
    /// `phonix-db`, and the point is that the two were written to agree. The
    /// source is `phonix_db::books::account::SORTABLE` plus the two the store
    /// orders by a generated expression, `class` and `postable`.
    const SERVER_SORTS: &[&str] = &["number", "name", "type", "is_active", "class", "postable"];

    /// The columns the `WHERE` looks inside. Same reasoning. `type` matches the
    /// stored value, not the word drawn for it.
    const SERVER_SEARCHES: &[&str] = &["number", "name", "type"];

    #[test]
    fn every_sortable_column_is_one_the_server_can_order_by() {
        for column in grid().columns.iter().filter(|column| column.sortable) {
            assert!(
                SERVER_SORTS.contains(&column.field()),
                "{} offers a sort the reader will ignore",
                column.field(),
            );
        }
    }

    #[test]
    fn every_searchable_column_is_one_the_server_looks_inside() {
        for column in grid().columns.iter().filter(|column| column.searchable) {
            assert!(
                SERVER_SEARCHES.contains(&column.field()),
                "{} is offered to the search box and never searched",
                column.field(),
            );
        }
    }

    #[test]
    fn it_opens_in_number_order_and_narrowed_by_nothing() {
        // No opening sort: the store falls back to number, which is the order
        // an accountant reads a chart in.
        let request = grid().initial_request();

        assert!(request.sort.is_none());
        assert!(request.filters.is_empty());

        for filter in &grid().filters {
            assert_eq!(filter.default_value(), "", "{}", filter.key());
            assert!(!filter.is_local(), "{}", filter.key());
        }
    }

    #[test]
    fn every_type_and_class_offered_is_one_the_reader_can_bind() {
        // The store binds the type as written and expands the class into the
        // types in it, so a choice that had drifted from the enum would match
        // no rows and read as an empty chart rather than a bug.
        let grid = grid();

        let types = grid.filters.iter().find(|f| f.key() == "account_type").unwrap();
        for choice in types.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                AccountType::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }

        let classes = grid.filters.iter().find(|f| f.key() == "class").unwrap();
        for choice in classes.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                AccountClass::ALL
                    .iter()
                    .any(|class| class.as_str() == choice.value),
                "{} is offered and names no class",
                choice.value,
            );
        }
    }
}
