//! Buying: what was asked for, what was ordered, and what arrived.
//!
//! Three list screens, thin by design - each is a `PageHeader` over a grid
//! whose whole shape lives in `ui::table::config`. The documents behind them
//! are their own files, because those are where the decisions are.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::ui::table::DataGrid;
use crate::ui::table::config::purchase_orders::purchase_orders_grid;
use crate::ui::table::config::requisitions::requisitions_grid;
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

/// What has been asked for, before anybody committed to buying it.
#[component]
pub fn requisitions_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("requisitions.title")) />

        <PageHeader
            title=l!("requisitions.title")
            subtitle=l!("requisitions.subtitle")
            icon=Icon::ClipboardList
        />

        <DataGrid config=requisitions_grid() />
    }
}
