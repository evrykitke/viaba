//! What has been posted to the ledger.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::ui::table::DataGrid;
use crate::ui::table::config::journals::journals_grid;

#[component]
pub fn journals_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("journals.title")) />

        <PageHeader
            title=l!("journals.title")
            subtitle=l!("journals.subtitle")
            icon=Icon::ScrollText
        />

        <DataGrid config=journals_grid() />
    }
}
