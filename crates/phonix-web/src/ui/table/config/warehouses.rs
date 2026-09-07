//! The warehouses grid.
//!
//! Small — most workspaces have one. The step columns are here rather than
//! behind a detail screen because they are what a workspace actually changes
//! about a warehouse after the day it was created, and seeing "one step" on
//! every row is how somebody notices that the depot doing quality checks is
//! not set up for them.

use leptos::prelude::*;
use phonix_core::permissions;

use app_inventory::warehouse::WarehouseSummary;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_warehouses;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// The buildings, and how each one works.
pub fn warehouses_grid() -> GridConfig<WarehouseSummary> {
    GridConfig::new("warehouses", Source::in_memory(list_warehouses))
        .searching(l!("warehouses.search"))
        .exports_as("warehouses")
        .min_width("sm:min-w-[44rem]")
        .empty(
            Icon::Warehouse,
            l!("warehouses.empty.title"),
            l!("warehouses.empty.detail"),
        )
        .column(
            Column::new("code", l!("field.code"), |row: &WarehouseSummary| {
                Cell::text(&row.code)
            })
            .findable()
            .pinned()
            .essential()
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &WarehouseSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .essential(),
        )
        .column(
            Column::new(
                "receipt_steps",
                l!("warehouses.receiving"),
                |row: &WarehouseSummary| Cell::text(crate::i18n::t(&row.receipt_steps.label())),
            )
            .sortable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "delivery_steps",
                l!("warehouses.shipping"),
                |row: &WarehouseSummary| Cell::text(crate::i18n::t(&row.delivery_steps.label())),
            )
            .sortable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "location_count",
                l!("warehouses.locations"),
                |row: &WarehouseSummary| Cell::number(row.location_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("is_active", l!("field.status"), |row: &WarehouseSummary| {
                Cell::bool(row.is_active)
            })
            .sortable()
            .render(|row| status_cell(row).into_any()),
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
            .matching(|row: &WarehouseSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(
                l!("warehouses.new"),
                Icon::Plus,
                "/inventory/warehouses/new",
            )
            .require(permissions::WAREHOUSES_MANAGE)
            .primary(),
        )
        .action(
            RowAction::link(l!("common.edit"), Icon::Pencil, |row: &WarehouseSummary| {
                format!("/inventory/warehouses/{}", row.id)
            })
            .require(permissions::WAREHOUSES_MANAGE),
        )
}

fn status_cell(row: &WarehouseSummary) -> impl IntoView {
    let active = row.is_active;

    view! {
        <Show
            when=move || active
            fallback=|| view! { <Badge tone=Tone::Neutral label=l!("common.inactive") /> }
        >
            <Badge tone=Tone::Success label=l!("common.active") />
        </Show>
    }
}
