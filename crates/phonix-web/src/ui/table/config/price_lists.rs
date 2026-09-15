//! The price lists grid.
//!
//! Reference data: a workspace has a handful of lists and adds one rarely, so
//! the whole set is fetched once and filtered in the browser.

use app_inventory::price_list::PriceList;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_price_lists;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// What this workspace charges, and to whom.
pub fn price_lists_grid() -> GridConfig<PriceList> {
    GridConfig::new("price-lists", Source::in_memory(list_price_lists))
        .searching(l!("price_lists.search"))
        .exports_as("price-lists")
        .sorted_by(Sort::ascending("code"))
        .min_width("sm:min-w-[36rem]")
        .empty(
            Icon::Receipt,
            l!("price_lists.empty.title"),
            l!("price_lists.empty.detail"),
        )
        .column(
            Column::new("code", l!("field.code"), |row: &PriceList| {
                Cell::text(&row.code)
            })
            .findable()
            .pinned()
            .essential()
            .class("font-mono tabular-nums text-xs"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &PriceList| {
                Cell::text(&row.name)
            })
            .findable()
            .essential(),
        )
        .column(
            Column::new("currency", l!("field.currency"), |row: &PriceList| {
                Cell::text(row.currency.code())
            })
            .findable()
            .essential(),
        )
        .column(
            Column::new("is_active", l!("field.status"), |row: &PriceList| {
                Cell::bool(row.is_active)
            })
            .sortable()
            .render(|row| {
                if row.is_active {
                    view! { <Badge label=l!("common.active") tone=Tone::Success /> }.into_any()
                } else {
                    view! { <Badge label=l!("common.inactive") /> }.into_any()
                }
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
            .matching(|row: &PriceList, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(
                l!("price_lists.new"),
                Icon::Plus,
                "/inventory/price-lists/new",
            )
            .require(permissions::ITEMS_EDIT)
            .primary(),
        )
        .action(
            RowAction::link(l!("common.edit"), Icon::Pencil, |row: &PriceList| {
                format!("/inventory/price-lists/{}", row.id)
            })
            .require(permissions::ITEMS_EDIT),
        )
}
