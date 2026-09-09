//! The roles the organization is made of, and which of them nobody is doing.
//!
//! `filled` is counted over the open assignments rather than stored, so a
//! vacancy appears the moment somebody leaves rather than the moment somebody
//! remembers to mark it. That is the whole reason a role is a row at all: a job
//! title typed onto a person has no row for the job nobody is doing.

use app_hr::job_position::JobPositionSummary;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_job_positions;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn job_positions_grid() -> GridConfig<JobPositionSummary> {
    GridConfig::new("job_positions", Source::in_memory(list_job_positions))
        .searching(l!("job_positions.search"))
        .exports_as("roles")
        .sorted_by(Sort::ascending("title"))
        .min_width("sm:min-w-[40rem]")
        .empty(
            Icon::ListChecks,
            l!("job_positions.empty.title"),
            l!("job_positions.empty.detail"),
        )
        .column(
            Column::new(
                "title",
                l!("job_positions.job_title"),
                |row: &JobPositionSummary| Cell::text(&row.title),
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
                l!("job_positions.code"),
                |row: &JobPositionSummary| Cell::text(&row.code),
            )
            .searchable()
            .sortable()
            .class("font-mono tabular-nums text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "department",
                l!("job_positions.department"),
                |row: &JobPositionSummary| {
                    Cell::text(row.department_name.clone().unwrap_or_default())
                },
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "filled",
                l!("job_positions.filled"),
                |row: &JobPositionSummary| Cell::number(row.filled as f64),
            )
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                if row.is_vacant() {
                    return view! {
                        <Badge label=l!("job_positions.vacant") tone=Tone::Warning />
                    }
                    .into_any();
                }

                let filled = row.filled.to_string();
                view! { <span class="tabular-nums">{filled}</span> }.into_any()
            }),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &JobPositionSummary| {
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
                "held",
                l!("job_positions.filled"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    // The one somebody opens this screen to find.
                    FilterChoice::new("vacant", l!("job_positions.vacant")),
                ],
            )
            .matching(|row: &JobPositionSummary, wanted| match wanted {
                "vacant" => row.is_vacant(),
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("job_positions.new"), Icon::Plus, "/people/roles/new")
                .require(permissions::JOB_POSITIONS_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &JobPositionSummary| format!("/people/roles/{}", row.id),
            )
            .require(permissions::JOB_POSITIONS),
        )
}
