//! The chart of accounts.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::ui::table::DataGrid;
use crate::ui::table::config::accounts::accounts_grid;

#[component]
pub fn accounts_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("accounts.title")) />

        <PageHeader
            title=l!("accounts.title")
            subtitle=l!("accounts.subtitle")
            icon=Icon::ListTree
        />

        <DataGrid config=accounts_grid() />
    }
}
