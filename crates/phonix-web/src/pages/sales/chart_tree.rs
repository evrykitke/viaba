//! The chart of accounts as a tree.
//!
//! # What the levels are
//!
//! Class, then type, then the accounts. That is the chart's real hierarchy:
//! an account's *type* decides its normal balance and which statement it lands
//! on, and its class is what a balance sheet and a profit and loss account are
//! divided into. The numbers are a convention a workspace may change - see
//! `migrations/apps/books/0002_accounts.sql` - so nesting by number range would
//! rearrange the tree the day somebody renumbered.
//!
//! There is no parent account here. Sub-accounts are a second, workspace-drawn
//! hierarchy on top of this one, and `books.accounts` has no column for them;
//! adding one is a decision for the ledger, which is what would have to total
//! across it.
//!
//! # Why a second view rather than a second screen
//!
//! The grid answers "where is 6200" and this answers "what is in expenses".
//! Both are the chart, so they share an address and a fetch, and the toggle is
//! remembered in the URL so a link sends somebody to the view being discussed.

use std::collections::BTreeMap;

use app_books::account::{Account, AccountClass, AccountType};
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::account_class::{ClassChip, ClassDot, swatch};
use crate::i18n::t;
use crate::icons::{Icon, IconSize};
use crate::l;

/// Every account, grouped the way an accountant reads a chart.
#[component]
pub fn chart_tree(accounts: Vec<Account>) -> impl IntoView {
    let grouped = group(accounts);

    view! {
        <div class="space-y-3">
            {grouped
                .into_iter()
                .map(|(class, types)| view! { <ClassBranch class=class types=types /> })
                .collect::<Vec<_>>()}
        </div>
    }
}

/// One class, with the types under it.
///
/// Open by default. A tree that arrives collapsed is a screen showing five
/// words, and somebody who opened the tree view asked to see the shape.
#[component]
fn class_branch(class: AccountClass, types: Vec<(AccountType, Vec<Account>)>) -> impl IntoView {
    let total: usize = types.iter().map(|(_, accounts)| accounts.len()).sum();
    let colour = swatch(class);

    view! {
        <details open=true class="group rounded-card border border-edge bg-surface-raised">
            <summary class="flex cursor-pointer list-none items-center gap-2 px-4 py-2.5 hover:bg-surface-hover">
                <span class="text-content-subtle transition-transform group-open:rotate-90">
                    <Icon icon=Icon::ChevronRight size=IconSize::Xs />
                </span>
                <ClassChip class=class />
                <span class="flex-1" />
                <span class="text-xs tabular-nums text-content-subtle">{total}</span>
            </summary>

            <div
                class="ml-6 border-l pl-2"
                style=format!("border-color:color-mix(in oklch, {colour} 30%, transparent)")
            >
                {types
                    .into_iter()
                    .map(|(account_type, accounts)| {
                        view! { <TypeBranch class=class account_type=account_type accounts=accounts /> }
                    })
                    .collect::<Vec<_>>()}
            </div>
        </details>
    }
}

/// One type, with its accounts.
#[component]
fn type_branch(
    class: AccountClass,
    account_type: AccountType,
    accounts: Vec<Account>,
) -> impl IntoView {
    let label = t(&account_type.label());
    let count = accounts.len();

    view! {
        <details open=true class="group/type">
            <summary class="flex cursor-pointer list-none items-center gap-2 rounded-control px-2 py-1.5 hover:bg-surface-hover">
                <span class="text-content-subtle transition-transform group-open/type:rotate-90">
                    <Icon icon=Icon::ChevronRight size=IconSize::Xs />
                </span>
                <ClassDot class=class />
                <span class="text-xs font-medium text-content-muted">{label}</span>
                <span class="flex-1" />
                <span class="text-2xs tabular-nums text-content-subtle">{count}</span>
            </summary>

            <ul class="ml-5">
                {accounts
                    .into_iter()
                    .map(|account| view! { <AccountLeaf account=account /> })
                    .collect::<Vec<_>>()}
            </ul>
        </details>
    }
}

/// One account.
#[component]
fn account_leaf(account: Account) -> impl IntoView {
    let href = format!("/sales/accounts/{}", account.id);
    let number = account.number.clone();
    let name = account.name.clone();
    let retired = !account.is_active;
    // A control account is owned by a sub-ledger; posting to one by hand is how
    // a reconciliation breaks, so the tree says which are which.
    let control = !account.account_type.allows_manual_posting();

    view! {
        <li>
            <A
                href=href
                attr:class="flex items-center gap-2 rounded-control px-2 py-1 hover:bg-surface-hover"
            >
                <code class="w-14 shrink-0 font-mono text-2xs tabular-nums text-content-subtle">
                    {number}
                </code>
                <span class=move || {
                    if retired {
                        "truncate-fade text-sm text-content-subtle line-through"
                    } else {
                        "truncate-fade text-sm text-content"
                    }
                }>{name}</span>
                {control
                    .then(|| {
                        view! {
                            <span class="shrink-0 text-2xs text-content-subtle">
                                {l!("accounts.postable.no")}
                            </span>
                        }
                    })}
            </A>
        </li>
    }
}

/// Class, then type, then number - each level in its declared order rather than
/// alphabetically, because that order is the one a report is laid out in.
fn group(accounts: Vec<Account>) -> Vec<(AccountClass, Vec<(AccountType, Vec<Account>)>)> {
    // Keyed by position in ALL so the map sorts into declaration order without
    // a second pass. Accounts arrive in number order and stay in it.
    let mut buckets: BTreeMap<usize, BTreeMap<usize, Vec<Account>>> = BTreeMap::new();

    for account in accounts {
        let class = account.class();
        let account_type = account.account_type;

        let class_at = AccountClass::ALL
            .iter()
            .position(|it| *it == class)
            .unwrap_or(usize::MAX);
        let type_at = AccountType::ALL
            .iter()
            .position(|it| *it == account_type)
            .unwrap_or(usize::MAX);

        buckets
            .entry(class_at)
            .or_default()
            .entry(type_at)
            .or_default()
            .push(account);
    }

    buckets
        .into_iter()
        .filter_map(|(class_at, types)| {
            let class = *AccountClass::ALL.get(class_at)?;

            let types = types
                .into_iter()
                .filter_map(|(type_at, accounts)| {
                    Some((*AccountType::ALL.get(type_at)?, accounts))
                })
                .collect();

            Some((class, types))
        })
        .collect()
}
