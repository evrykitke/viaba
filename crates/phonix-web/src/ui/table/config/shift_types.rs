//! The shifts people are expected to work.
//!
//! The grace column earns its place beside the hours: two workspaces with the
//! same nine-to-five differ entirely in whether five past counts, and that
//! number is the whole of the difference.

use app_hr::shift::ShiftTypeSummary;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_shift_types;
use crate::ui::table::{
    Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn shift_types_grid() -> GridConfig<ShiftTypeSummary> {
    GridConfig::new("shift_types", Source::in_memory(list_shift_types))
        .searching(l!("shifts.search"))
        .exports_as("shifts")
        .sorted_by(Sort::ascending("starts_at"))
        .min_width("sm:min-w-[36rem]")
        .empty(
            Icon::Clock,
            l!("shifts.empty.title"),
            l!("shifts.empty.detail"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &ShiftTypeSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .pinned()
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new("code", l!("field.code"), |row: &ShiftTypeSummary| {
                Cell::text(&row.code)
            })
            .searchable()
            .sortable()
            .class("font-mono tabular-nums text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "starts_at",
                l!("shifts.starts_at"),
                |row: &ShiftTypeSummary| Cell::text(row.starts_at.format("%H:%M").to_string()),
            )
            .essential()
            .sortable()
            .class("tabular-nums"),
        )
        .column(
            Column::new("ends_at", l!("shifts.ends_at"), |row: &ShiftTypeSummary| {
                Cell::text(row.ends_at.format("%H:%M").to_string())
            })
            .essential()
            .sortable()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "late_grace_minutes",
                l!("shifts.late_grace"),
                |row: &ShiftTypeSummary| Cell::number(row.late_grace_minutes as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "headcount",
                l!("shifts.headcount"),
                |row: &ShiftTypeSummary| Cell::number(row.headcount as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &ShiftTypeSummary| {
                Cell::text(if row.is_active {
                    l!("common.active")
                } else {
                    l!("common.inactive")
                })
            })
            .render(|row| {
                let (label, tone) = if row.is_active {
                    (l!("common.active"), Tone::Success)
                } else {
                    (l!("common.inactive"), Tone::Neutral)
                };

                view! { <Badge label=label tone=tone /> }.into_any()
            }),
        )
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("active", l!("common.active")),
                    FilterChoice::new("inactive", l!("common.inactive")),
                ],
            )
            .matching(|row: &ShiftTypeSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("shifts.new"), Icon::Plus, "/people/shifts/new")
                .require(permissions::SHIFT_TYPES_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &ShiftTypeSummary| format!("/people/shifts/{}", row.id),
            )
            .require(permissions::SHIFT_TYPES),
        )
}
