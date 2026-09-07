//! The items grid.
//!
//! The **tracked** column separates what is counted from what is merely bought,
//! and it is the first thing to check when a stock report is missing something.
//! The **barcode** column is hidden by default and searchable, which is what
//! makes a scanner work here: the search box takes what the scanner types and
//! the row comes back, without anybody choosing a column first.

use leptos::prelude::*;
use phonix_core::permissions;

use app_inventory::item::{ItemKind, ItemSummary, Tracking};

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_items;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// What this workspace stocks, buys and sells.
pub fn items_grid() -> GridConfig<ItemSummary> {
    GridConfig::new("items", Source::in_memory(list_items))
        .searching(l!("items.search"))
        .exports_as("items")
        .min_width("sm:min-w-[52rem]")
        .empty(Icon::Package, l!("items.empty.title"), l!("items.empty.detail"))
        .column(
            Column::new("code", l!("field.code"), |row: &ItemSummary| {
                Cell::text(&row.code)
            })
            .findable()
            .pinned()
            .essential()
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &ItemSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .essential(),
        )
        .column(
            // Hidden and findable: this is what makes the search box a scanner
            // input. Nothing has to be configured for a scan to find its row.
            Column::new("barcode", l!("items.barcode"), |row: &ItemSummary| {
                Cell::maybe(row.barcode.clone())
            })
            .findable()
            .hidden()
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("category", l!("items.category"), |row: &ItemSummary| {
                Cell::text(&row.category_name)
            })
            .searchable()
            .sortable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("tracking", l!("items.tracked"), |row: &ItemSummary| {
                Cell::bool(row.is_tracked)
            })
            .sortable()
            .essential()
            .render(|row| tracking_cell(row).into_any()),
        )
        .column(
            Column::new("unit", l!("items.unit"), |row: &ItemSummary| {
                Cell::text(&row.stock_unit_code)
            })
            .sortable()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new("cost", l!("items.cost"), |row: &ItemSummary| {
                Cell::text(row.cost.to_display_string())
            })
            .sortable()
            .align(Align::End)
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("is_active", l!("field.status"), |row: &ItemSummary| {
                Cell::bool(row.is_active)
            })
            .sortable()
            .render(|row| status_cell(row).into_any()),
        )
        .filter(
            Filter::new(
                "kind",
                l!("items.kind"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("goods", l!("items.kind.goods")),
                    FilterChoice::new("service", l!("items.kind.service")),
                ],
            )
            .matching(|row: &ItemSummary, wanted| row.kind.as_str() == wanted),
        )
        .filter(
            Filter::new(
                "tracking",
                l!("items.tracked"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("tracked", l!("items.only_tracked")),
                    FilterChoice::new("untracked", l!("items.only_untracked")),
                    FilterChoice::new("lot", l!("items.tracking.lot")),
                    FilterChoice::new("serial", l!("items.tracking.serial")),
                ],
            )
            .matching(|row: &ItemSummary, wanted| match wanted {
                "tracked" => row.is_tracked,
                "untracked" => !row.is_tracked,
                other => row.tracking.as_str() == other,
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
            .matching(|row: &ItemSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("items.new"), Icon::Plus, "/inventory/items/new")
                .require(permissions::ITEMS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &ItemSummary| {
                format!("/inventory/items/{}", row.id)
            })
            .require(permissions::ITEMS),
        )
}

/// Whether it is counted, and how closely.
///
/// Three states, not two: not counted at all, counted as a number, counted with
/// lot or serial numbers behind it. The middle one is the ordinary case and
/// reads as plain "tracked".
fn tracking_cell(row: &ItemSummary) -> impl IntoView {
    let is_tracked = row.is_tracked;
    let tracking = row.tracking;
    let is_service = row.kind == ItemKind::Service;

    let label = if !is_tracked {
        if is_service {
            crate::i18n::t(&ItemKind::Service.label())
        } else {
            l!("items.not_counted")
        }
    } else {
        crate::i18n::t(&tracking.label())
    };

    let tone = match (is_tracked, tracking) {
        (false, _) => Tone::Neutral,
        (true, Tracking::None) => Tone::Success,
        // Lots and serials are stronger promises, and a screen that shows them
        // the same way as a plain count hides the thing a recall depends on.
        (true, _) => Tone::Brand,
    };

    view! { <Badge tone=tone label=label /> }
}

fn status_cell(row: &ItemSummary) -> impl IntoView {
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
