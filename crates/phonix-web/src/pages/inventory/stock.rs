//! What is on hand, and everything that has moved.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::page::PageHeader;
use crate::icons::Icon;
use crate::l;
use crate::ui::table::DataGrid;
use crate::ui::table::config::stock::stock_grid;
use crate::ui::table::config::stock_moves::stock_moves_grid;

#[component]
pub fn stock_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("stock.title")) />

        <PageHeader title=l!("stock.title") subtitle=l!("stock.subtitle") icon=Icon::Boxes />

        <DataGrid config=stock_grid() />
    }
}

#[component]
pub fn stock_moves_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("moves.title")) />

        <PageHeader
            title=l!("moves.title")
            subtitle=l!("moves.subtitle")
            icon=Icon::ArrowRight
        />

        <DataGrid config=stock_moves_grid() />
    }
}
