//! What is on hand, and what is promised.
//!
//! # Three numbers, and they are not the same number
//!
//! `quantity` is what is on the shelf and what a stock count is checked
//! against. `reserved` is what somebody has already promised. `available` is
//! the difference, and it is what a sales line may draw on. A screen that
//! showed only one of them would be telling a warehouse it has nothing while a
//! full pallet sits in the aisle.
//!
//! # There is no edit action, and there is no delete
//!
//! A quant is a running total of the movements, not a number anybody types.
//! Changing what is on a shelf is an adjustment - a movement with a reason on
//! it, which posts - and that is the only door in.

use app_inventory::quant::OnHandRow;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::stock_on_hand;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, Source, ToolbarAction};

pub fn stock_grid() -> GridConfig<OnHandRow> {
    GridConfig::new(
        "stock",
        Source::in_memory(|| stock_on_hand(app_inventory::quant::OnHandFilter::default())),
    )
    .searching(l!("stock.search"))
    .exports_as("stock-on-hand")
    .sorted_by(Sort::ascending("item"))
    .min_width("sm:min-w-[56rem]")
    .empty(Icon::Boxes, l!("stock.empty.title"), l!("stock.empty.detail"))
    .column(
        Column::new("item", l!("entity.item.singular"), |row: &OnHandRow| {
            Cell::text(&row.item_name)
        })
        .searchable()
        .sortable()
        .pinned()
        .essential(),
    )
    .column(
        Column::new("variant", l!("field.code"), |row: &OnHandRow| {
            Cell::text(&row.variant_code)
        })
        .findable()
        .essential()
        .class("font-mono text-xs tabular-nums")
        .render(|row| variant_cell(row).into_any()),
    )
    .column(
        Column::new("location", l!("stock.location"), |row: &OnHandRow| {
            Cell::text(&row.location_path)
        })
        .searchable()
        .sortable()
        .essential()
        .class("font-mono text-xs"),
    )
    .column(
        Column::new("lot", l!("stock.lot"), |row: &OnHandRow| {
            Cell::text(row.lot_number.clone().unwrap_or_default())
        })
        .searchable()
        .render(|row| lot_cell(row).into_any()),
    )
    .column(
        Column::new("quantity", l!("stock.quantity"), |row: &OnHandRow| {
            Cell::number(row.quantity.scaled() as f64)
        })
        .sortable()
        .essential()
        .align(Align::End)
        .render(|row| {
            let text = format!(
                "{} {}",
                row.quantity.to_display_string(),
                row.unit_code
            );
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .column(
        // Still on the shelf, and not anybody's to promise twice.
        Column::new("reserved", l!("stock.reserved"), |row: &OnHandRow| {
            Cell::number(row.reserved.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .class("tabular-nums text-content-muted"),
    )
    .column(
        Column::new("available", l!("stock.available"), |row: &OnHandRow| {
            Cell::number(row.available().scaled() as f64)
        })
        .sortable()
        .essential()
        .align(Align::End)
        .render(|row| {
            let text = row.available().to_display_string();
            view! { <span class="tabular-nums font-medium">{text}</span> }.into_any()
        }),
    )
    .column(
        Column::new("value", l!("stock.value"), |row: &OnHandRow| {
            Cell::number(row.value.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .render(|row| {
            let text = row.value.to_display_string();
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .filter(
        Filter::new(
            "held",
            l!("stock.held"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("reserved", l!("stock.only_reserved")),
                FilterChoice::new("free", l!("stock.only_free")),
            ],
        )
        .matching(|row: &OnHandRow, wanted| match wanted {
            "reserved" => !row.reserved.is_zero(),
            "free" => row.reserved.is_zero(),
            _ => true,
        }),
    )
    .toolbar(
        ToolbarAction::link(l!("stock.moves"), Icon::ArrowRight, "/inventory/moves")
            .require(permissions::STOCK),
    )
}

/// The code, with the combination under it. A grid of forty rows that all say
/// `ITM-00042` is a grid nobody can pick from.
fn variant_cell(row: &OnHandRow) -> impl IntoView {
    let code = row.variant_code.clone();
    let combination = row.combination.clone();

    view! {
        <div class="flex min-w-0 flex-col">
            <span class="truncate-fade font-mono text-xs">{code}</span>
            {combination
                .map(|text| view! { <span class="text-xs text-content-muted">{text}</span> })}
        </div>
    }
}

/// The lot, and a badge when it is past its date. An expired batch on a shelf
/// is the one row on this screen somebody has to act on today.
fn lot_cell(row: &OnHandRow) -> impl IntoView {
    let Some(number) = row.lot_number.clone() else {
        return view! { <span class="text-content-muted">{l!("common.none")}</span> }.into_any();
    };

    let expired = row
        .expires_on
        .is_some_and(|date| date < chrono::Utc::now().date_naive());
    let expires = row.expires_on.map(|date| date.to_string());

    view! {
        <div class="flex min-w-0 items-center gap-2">
            <span class="truncate-fade font-mono text-xs">{number}</span>
            {expires.map(|date| view! { <span class="text-xs text-content-muted">{date}</span> })}
            {expired.then(|| view! { <Badge label=l!("stock.expired") tone=Tone::Danger /> })}
        </div>
    }
    .into_any()
}
