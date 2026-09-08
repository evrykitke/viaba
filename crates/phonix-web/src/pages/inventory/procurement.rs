//! Buying: what was ordered, and what arrived.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::ui::table::DataGrid;
use crate::ui::table::config::purchase_orders::purchase_orders_grid;
use crate::ui::table::config::receipts::receipts_grid;

#[component]
pub fn purchase_orders_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("purchase_orders.title")) />

        <PageHeader
            title=l!("purchase_orders.title")
            subtitle=l!("purchase_orders.subtitle")
            icon=Icon::ScrollText
        />

        <DataGrid config=purchase_orders_grid() />
    }
}

#[component]
pub fn receipts_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("receipts.title")) />

        <PageHeader
            title=l!("receipts.title")
            subtitle=l!("receipts.subtitle")
            icon=Icon::Package
        />

        <DataGrid config=receipts_grid() />
    }
}
