//! Every change to every quantity, newest first.
//!
//! # There is no edit and no delete, and that is the screen saying what this is
//!
//! The same statement the journal list makes. A movement that has happened is
//! evidence; the correction for one is a movement the other way, and this grid
//! offers neither a pencil nor a bin because neither exists.
//!
//! # The journal column is the one worth reading
//!
//! It says whether the stock account was told. `Posted` names the journal;
//! `no ledger` is a workspace that never bought the accounting module, and is
//! not a failure; `not needed` is a pallet that crossed an aisle. A row that
//! said nothing here would be the gap ADR 0006 section 6.1 is about.

use app_inventory::location::MoveKind;
use app_inventory::movement::{JournalOutcome, MoveState, MoveSummary};
use leptos::prelude::*;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::stock_moves;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, Source};

pub fn stock_moves_grid() -> GridConfig<MoveSummary> {
    GridConfig::new(
        "stock-moves",
        Source::in_memory(|| stock_moves(app_inventory::movement::MoveFilter::default())),
    )
    .searching(l!("moves.search"))
    .exports_as("stock-moves")
    .sorted_by(Sort::descending("moved_on"))
    .min_width("sm:min-w-[60rem]")
    .empty(
        Icon::ArrowRight,
        l!("moves.empty.title"),
        l!("moves.empty.detail"),
    )
    .column(
        Column::new("moved_on", l!("moves.date"), |row: &MoveSummary| {
            Cell::text(row.moved_on.to_string())
        })
        .sortable()
        .pinned()
        .essential()
        .class("tabular-nums"),
    )
    .column(
        Column::new("item", l!("entity.item.singular"), |row: &MoveSummary| {
            Cell::text(&row.item_name)
        })
        .searchable()
        .sortable()
        .essential()
        .render(|row| item_cell(row).into_any()),
    )
    .column(
        Column::new("kind", l!("moves.kind"), |row: &MoveSummary| {
            Cell::text(kind_label(row.kind()))
        })
        .essential()
        .render(|row| view! { <Badge label=kind_label(row.kind()) tone=kind_tone(row.kind()) /> }
            .into_any()),
    )
    .column(
        Column::new("from", l!("field.from"), |row: &MoveSummary| {
            Cell::text(&row.from_path)
        })
        .searchable()
        .class("font-mono text-xs text-content-muted"),
    )
    .column(
        Column::new("to", l!("moves.to"), |row: &MoveSummary| {
            Cell::text(&row.to_path)
        })
        .searchable()
        .essential()
        .class("font-mono text-xs"),
    )
    .column(
        Column::new("lot", l!("stock.lot"), |row: &MoveSummary| {
            Cell::text(row.lot_number.clone().unwrap_or_default())
        })
        .searchable()
        .class("font-mono text-xs text-content-muted"),
    )
    .column(
        Column::new("quantity", l!("stock.quantity"), |row: &MoveSummary| {
            Cell::number(row.quantity.scaled() as f64)
        })
        .sortable()
        .essential()
        .align(Align::End)
        .render(|row| {
            let text = format!("{} {}", row.quantity.to_display_string(), row.unit_code);
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .column(
        Column::new("value", l!("stock.value"), |row: &MoveSummary| {
            Cell::number(row.value.scaled() as f64)
        })
        .sortable()
        .align(Align::End)
        .render(|row| {
            let text = row.value.to_display_string();
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }),
    )
    .column(
        Column::new("journal", l!("moves.journal"), |row: &MoveSummary| {
            Cell::text(row.journal.number().unwrap_or_default())
        })
        .findable()
        .render(|row| journal_cell(&row.journal).into_any()),
    )
    .column(
        Column::new("reference", l!("moves.reference"), |row: &MoveSummary| {
            Cell::text(row.reference.clone().unwrap_or_default())
        })
        .searchable()
        .class("text-xs text-content-muted"),
    )
    .filter(
        Filter::new(
            "kind",
            l!("moves.kind"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("receipt", l!("moves.kind.receipt")),
                FilterChoice::new("delivery", l!("moves.kind.delivery")),
                FilterChoice::new("internal", l!("moves.kind.internal")),
                FilterChoice::new("adjustment", l!("moves.kind.adjustment")),
            ],
        )
        .matching(|row: &MoveSummary, wanted| match wanted {
            "receipt" => matches!(row.kind(), MoveKind::Receipt),
            "delivery" => matches!(row.kind(), MoveKind::Delivery),
            "internal" => matches!(row.kind(), MoveKind::Internal),
            "adjustment" => matches!(row.kind(), MoveKind::Adjustment),
            _ => true,
        }),
    )
    .filter(
        Filter::new(
            "state",
            l!("field.status"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("done", l!("moves.state.done")),
                FilterChoice::new("draft", l!("moves.state.draft")),
                FilterChoice::new("cancelled", l!("moves.state.cancelled")),
            ],
        )
        .matching(|row: &MoveSummary, wanted| match wanted {
            "done" => matches!(row.state, MoveState::Done),
            "draft" => matches!(row.state, MoveState::Draft),
            "cancelled" => matches!(row.state, MoveState::Cancelled),
            _ => true,
        }),
    )
}

fn item_cell(row: &MoveSummary) -> impl IntoView {
    let name = row.item_name.clone();
    let code = row.variant_code.clone();

    view! {
        <div class="flex min-w-0 flex-col">
            <span class="truncate-fade text-content">{name}</span>
            <span class="font-mono text-xs text-content-muted">{code}</span>
        </div>
    }
}

fn journal_cell(outcome: &JournalOutcome) -> impl IntoView {
    match outcome {
        JournalOutcome::Posted { number, .. } => {
            let number = number.clone();
            view! { <span class="font-mono text-xs tabular-nums">{number}</span> }.into_any()
        }
        // Not a failure, and it must not read as one: the stock moved, and this
        // workspace does not keep books.
        JournalOutcome::NoLedger => {
            view! { <Badge label=l!("moves.journal.no_ledger_short") tone=Tone::Neutral /> }
                .into_any()
        }
        JournalOutcome::NotRequired => {
            view! { <span class="text-xs text-content-muted">{l!("common.none")}</span> }.into_any()
        }
    }
}

fn kind_label(kind: MoveKind) -> String {
    match kind {
        MoveKind::Receipt => l!("moves.kind.receipt"),
        MoveKind::Delivery => l!("moves.kind.delivery"),
        MoveKind::Internal => l!("moves.kind.internal"),
        MoveKind::Adjustment => l!("moves.kind.adjustment"),
        MoveKind::Manufacturing => l!("moves.kind.manufacturing"),
        MoveKind::Neither => l!("common.none"),
    }
}

/// Value in is good news, value out is not, and a rearrangement is neither.
const fn kind_tone(kind: MoveKind) -> Tone {
    match kind {
        MoveKind::Receipt => Tone::Success,
        MoveKind::Delivery => Tone::Brand,
        MoveKind::Adjustment => Tone::Warning,
        MoveKind::Internal | MoveKind::Manufacturing | MoveKind::Neither => Tone::Neutral,
    }
}
