//! Promotions, transfers and exits.
//!
//! The status column is the one that earns its place. A draft has not happened
//! yet and a confirmed one has, and a list that showed them alike would let
//! somebody read a proposal as a fact — which is the whole reason the document
//! exists rather than the assignment row alone.

use app_hr::movement::{MovementKind, MovementStatus, MovementSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_movements;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// A confirmed movement reads as done, a draft as outstanding, a cancelled one
/// as neither.
const fn status_tone(status: MovementStatus) -> Tone {
    match status {
        MovementStatus::Confirmed => Tone::Success,
        MovementStatus::Draft => Tone::Warning,
        MovementStatus::Cancelled => Tone::Neutral,
    }
}

/// An exit is the one a reader scans for, so it is the one that is loud.
const fn kind_tone(kind: MovementKind) -> Tone {
    match kind {
        MovementKind::Exit => Tone::Danger,
        MovementKind::Promotion | MovementKind::Transfer => Tone::Brand,
    }
}

pub fn movements_grid() -> GridConfig<MovementSummary> {
    GridConfig::new("movements", Source::in_memory(list_movements))
        .searching(l!("movements.search"))
        .exports_as("movements")
        .sorted_by(Sort::descending("effective_on"))
        .min_width("sm:min-w-[40rem]")
        .empty(
            Icon::ArrowRight,
            l!("movements.empty.title"),
            l!("movements.empty.detail"),
        )
        .column(
            Column::new(
                "employee_name",
                l!("movements.person"),
                |row: &MovementSummary| Cell::text(&row.employee_name),
            )
            .findable()
            .pinned()
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &MovementSummary| {
                Cell::maybe(row.number.clone())
            })
            .searchable()
            .sortable()
            .class("font-mono tabular-nums text-xs text-content-muted"),
        )
        .column(
            Column::new("kind", l!("movements.kind"), |row: &MovementSummary| {
                Cell::text(crate::i18n::t(&row.kind.label()))
            })
            .essential()
            .render(|row| {
                view! {
                    <Badge
                        label=crate::i18n::t(&row.kind.label())
                        tone=kind_tone(row.kind)
                    />
                }
                .into_any()
            }),
        )
        .column(
            Column::new(
                "effective_on",
                l!("movements.effective_on"),
                |row: &MovementSummary| Cell::text(row.effective_on.to_string()),
            )
            .essential()
            .sortable()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "end_reason",
                l!("movements.end_reason"),
                |row: &MovementSummary| {
                    Cell::maybe(row.end_reason.map(|reason| crate::i18n::t(&reason.label())))
                },
            )
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("status", l!("field.status"), |row: &MovementSummary| {
                Cell::text(crate::i18n::t(&row.status.label()))
            })
            .essential()
            .render(|row| {
                view! {
                    <Badge
                        label=crate::i18n::t(&row.status.label())
                        tone=status_tone(row.status)
                    />
                }
                .into_any()
            }),
        )
        .filter(
            Filter::new(
                "status",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("draft", l!("movements.status.draft")),
                    FilterChoice::new("confirmed", l!("movements.status.confirmed")),
                    FilterChoice::new("cancelled", l!("movements.status.cancelled")),
                ],
            )
            .matching(|row: &MovementSummary, wanted| match wanted {
                "draft" => row.status == MovementStatus::Draft,
                "confirmed" => row.status == MovementStatus::Confirmed,
                "cancelled" => row.status == MovementStatus::Cancelled,
                _ => true,
            }),
        )
        .filter(
            Filter::new(
                "kind",
                l!("movements.kind"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("promotion", l!("movements.kind.promotion")),
                    FilterChoice::new("transfer", l!("movements.kind.transfer")),
                    FilterChoice::new("exit", l!("movements.kind.exit")),
                ],
            )
            .matching(|row: &MovementSummary, wanted| match wanted {
                "promotion" => row.kind == MovementKind::Promotion,
                "transfer" => row.kind == MovementKind::Transfer,
                "exit" => row.kind == MovementKind::Exit,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("movements.new"), Icon::Plus, "/people/movements/new")
                .require(permissions::MOVEMENTS_RAISE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &MovementSummary| format!("/people/movements/{}", row.id),
            )
            .require(permissions::MOVEMENTS),
        )
}
