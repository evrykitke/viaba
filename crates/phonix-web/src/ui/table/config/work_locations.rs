//! Where people work.
//!
//! The kind column is the one that earns its place: it says whether the
//! workspace controls the premises, which decides who is on the fire register
//! and who has to be asked about their own desk rather than told.

use app_hr::work_location::{LocationKind, WorkLocationSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_work_locations;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn work_locations_grid() -> GridConfig<WorkLocationSummary> {
    GridConfig::new("work_locations", Source::in_memory(list_work_locations))
        .searching(l!("work_locations.search"))
        .exports_as("places")
        .sorted_by(Sort::ascending("name"))
        .min_width("sm:min-w-[36rem]")
        .empty(
            Icon::Warehouse,
            l!("work_locations.empty.title"),
            l!("work_locations.empty.detail"),
        )
        .column(
            Column::new(
                "name",
                l!("work_locations.name"),
                |row: &WorkLocationSummary| Cell::text(&row.name),
            )
            .findable()
            .pinned()
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new(
                "code",
                l!("work_locations.code"),
                |row: &WorkLocationSummary| Cell::text(&row.code),
            )
            .searchable()
            .sortable()
            .class("font-mono tabular-nums text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "kind",
                l!("work_locations.kind"),
                |row: &WorkLocationSummary| Cell::text(kind_label(row.kind)),
            )
            .essential()
            .render(|row| {
                view! { <Badge label=kind_label(row.kind) tone=kind_tone(row.kind) /> }.into_any()
            }),
        )
        .column(
            Column::new(
                "address",
                l!("work_locations.address"),
                |row: &WorkLocationSummary| Cell::text(row.address.clone().unwrap_or_default()),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "headcount",
                l!("work_locations.headcount"),
                |row: &WorkLocationSummary| Cell::number(row.headcount as f64),
            )
            .sortable()
            .essential()
            .align(Align::End)
            .class("tabular-nums"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &WorkLocationSummary| {
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
                "kind",
                l!("work_locations.kind"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("office", l!("work_locations.kind.office")),
                    FilterChoice::new("home", l!("work_locations.kind.home")),
                    FilterChoice::new("other", l!("work_locations.kind.elsewhere")),
                ],
            )
            .matching(|row: &WorkLocationSummary, wanted| match wanted {
                "office" => matches!(row.kind, LocationKind::Office),
                "home" => matches!(row.kind, LocationKind::Home),
                "other" => matches!(row.kind, LocationKind::Other),
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("work_locations.new"), Icon::Plus, "/people/places/new")
                .require(permissions::WORK_LOCATIONS_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &WorkLocationSummary| format!("/people/places/{}", row.id),
            )
            .require(permissions::WORK_LOCATIONS),
        )
}

fn kind_label(kind: LocationKind) -> String {
    match kind {
        LocationKind::Office => l!("work_locations.kind.office"),
        LocationKind::Home => l!("work_locations.kind.home"),
        LocationKind::Other => l!("work_locations.kind.elsewhere"),
    }
}

/// Only premises the workspace controls read as its own.
const fn kind_tone(kind: LocationKind) -> Tone {
    match kind {
        LocationKind::Office => Tone::Brand,
        LocationKind::Home | LocationKind::Other => Tone::Neutral,
    }
}
