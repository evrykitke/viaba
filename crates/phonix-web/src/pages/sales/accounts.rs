//! The chart of accounts: the list, the tree, and adding one.
//!
//! Two views of the same chart on one address. The tree answers "what is in
//! expenses" and the grid answers "where is 6200". Which one is showing lives
//! in the URL, so a link opens the view somebody was talking about.
//!
//! The tree comes first, and is what the screen opens on. A chart is a
//! structure before it is a list: somebody arriving here is usually looking for
//! the shape - what is under expenses, whether there is already an account for
//! this - and the flat grid is the answer to the narrower question that follows.

use leptos::prelude::*;
use leptos_meta::Title;

use app_books::account::{AccountInput, AccountType};

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::pages::sales::chart_tree::ChartTree;
use crate::server_fns::books_fns::list_accounts;
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::accounts::account_form;
use crate::ui::table::DataGrid;
use crate::ui::table::config::accounts::accounts_grid;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn accounts_page() -> impl IntoView {
    // Above the tab strip: a tab's render closure runs again every time it
    // comes back on screen, and the tree must not refetch on each switch.
    let accounts = Resource::new(|| (), |()| async move { list_accounts().await });

    let tree_tab = Tab::new("tree", "Tree", move || {
        view! {
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    // An unreadable chart draws an empty tree rather than a
                    // failed screen: the grid beside it reports the same fault.
                    view! { <ChartTree accounts=accounts.await.unwrap_or_default() /> }
                })}
            </Transition>
        }
        .into_any()
    })
    .icon(Icon::ListTree);

    let list_tab = Tab::new("list", "List", move || {
        view! { <DataGrid config=accounts_grid() /> }.into_any()
    })
    .icon(Icon::Table);

    view! {
        <Title text=format!("{} | Phonix", l!("accounts.title")) />

        <PageHeader
            title=l!("accounts.title")
            subtitle=l!("accounts.subtitle")
            icon=Icon::ListTree
        />

        <TabbedPanel id="accounts" tabs=vec![tree_tab, list_tab] />
    }
}

/// Adding an account.
///
/// `?type=` opens the form on a type, which is what the add button inside each
/// branch of the tree sends: the person has already said what kind of account
/// this is by choosing where to press, and being asked again is how one ends up
/// filed under the wrong type.
#[component]
pub fn account_new_page() -> impl IntoView {
    let query = leptos_router::hooks::use_query_map();

    let opening_type = move || {
        query
            .with(|query| query.get("type").and_then(|raw| AccountType::parse(&raw)))
    };

    // The chart, for the number suggestion. A failed fetch costs the suggestion
    // and not the form: somebody who knows the number they want should not be
    // stopped by a list that would not load.
    let chart = Resource::new(|| (), |()| async move { list_accounts().await });

    view! {
        <Title text=format!("{} | Phonix", l!("accounts.new")) />

        <PageHeader
            title=l!("accounts.new")
            subtitle=l!("accounts.new.subtitle")
            icon=Icon::ListTree
            back=("/sales/accounts", l!("accounts.title"))
        />

        <div class="max-w-3xl">
            <CollapsibleCard
                title=l!("accounts.new")
                detail=l!("accounts.new.subtitle")
                icon=Icon::ListTree
                open=true
            >
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        let chart = chart.await.unwrap_or_default();

                        let mut draft = AccountInput::blank();
                        if let Some(account_type) = opening_type() {
                            draft.account_type = account_type;
                        }

                        view! { <EntityForm config=account_form(false, chart) value=draft /> }
                    })}
                </Transition>
            </CollapsibleCard>
        </div>
    }
}
