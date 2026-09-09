//! Freight, duty and handling, and how much of each stayed on the shelf.
//!
//! Capitalised is its own column beside the total, because the difference
//! between them is the part that went to cost of sales - the number somebody
//! asks about when a month's margin moves and nobody knows why.

use app_inventory::landed_cost::{LandedCostState, LandedCostSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_landed_costs;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn landed_costs_grid() -> GridConfig<LandedCostSummary> {
    GridConfig::new("landed_costs", Source::in_memory(list_landed_costs))
        .searching(l!("landed_costs.search"))
        .exports_as("landed-costs")
        .sorted_by(Sort::descending("cost_date"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Truck,
            l!("landed_costs.empty.title"),
            l!("landed_costs.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &LandedCostSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "receipt",
                l!("landed_costs.receipt"),
                |row: &LandedCostSummary| Cell::text(&row.receipt_number),
            )
            .searchable()
            .findable()
            .essential()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "supplier",
                l!("purchase_orders.supplier"),
                |row: &LandedCostSummary| Cell::text(&row.supplier_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "cost_date",
                l!("landed_costs.dated"),
                |row: &LandedCostSummary| Cell::text(row.cost_date.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &LandedCostSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new("total", l!("landed_costs.total"), |row: &LandedCostSummary| {
                Cell::number(row.total.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = row.total.to_display_string();
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new(
                "capitalised",
                l!("landed_costs.capitalised"),
                |row: &LandedCostSummary| Cell::number(row.capitalised.scaled() as f64),
            )
            .sortable()
            .align(Align::End)
            .render(|row| {
                let text = row.capitalised.to_display_string();
                view! { <span class="tabular-nums text-content-muted">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new(
                "charges",
                l!("landed_costs.charges"),
                |row: &LandedCostSummary| Cell::number(row.charge_count as f64),
            )
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::new("draft", l!("landed_costs.state.draft")),
                    FilterChoice::new("done", l!("landed_costs.state.done")),
                    FilterChoice::new("cancelled", l!("landed_costs.state.cancelled")),
                ],
            )
            .matching(|row: &LandedCostSummary, wanted| match wanted {
                "draft" => matches!(row.state, LandedCostState::Draft),
                "done" => matches!(row.state, LandedCostState::Done),
                "cancelled" => matches!(row.state, LandedCostState::Cancelled),
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/landed-costs/new")
                .require(permissions::LANDED_COSTS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &LandedCostSummary| format!("/inventory/landed-costs/{}", row.id),
            )
            .require(permissions::LANDED_COSTS),
        )
}

fn number_cell(row: &LandedCostSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("landed_costs.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

fn state_label(state: LandedCostState) -> String {
    crate::i18n::t(&state.label())
}

fn state_tone(state: LandedCostState) -> Tone {
    match state {
        LandedCostState::Draft => Tone::Neutral,
        LandedCostState::Done => Tone::Success,
        LandedCostState::Cancelled => Tone::Warning,
    }
}
