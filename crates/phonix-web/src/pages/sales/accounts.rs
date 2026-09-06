//! The chart of accounts: the list, the tree, and adding one.
//!
//! Two views of the same chart on one address. The grid answers "where is
//! 6200"; the tree answers "what is in expenses". Which one is showing lives in
//! the URL, so a link opens the view somebody was talking about.

use leptos::prelude::*;
use leptos_meta::Title;

use app_books::account::AccountInput;

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

    let list_tab = Tab::new("list", "List", move || {
        view! { <DataGrid config=accounts_grid() /> }.into_any()
    })
    .icon(Icon::Table);

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

    view! {
        <Title text=format!("{} | Phonix", l!("accounts.title")) />

        <PageHeader
            title=l!("accounts.title")
            subtitle=l!("accounts.subtitle")
            icon=Icon::ListTree
        />

        <TabbedPanel id="accounts" tabs=vec![list_tab, tree_tab] />
    }
}

/// Adding an account.
#[component]
pub fn account_new_page() -> impl IntoView {
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
                <EntityForm config=account_form(false) value=AccountInput::blank() />
            </CollapsibleCard>
        </div>
    }
}
