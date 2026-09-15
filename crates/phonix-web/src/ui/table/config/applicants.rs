//! Who has applied, and for what.
//!
//! The stage column is what the grid is read for. A pipeline is a question
//! about where everybody has got to, and the answer is worthless if the four
//! open stages and the three closed ones look alike.

use app_hr::applicant::{ApplicantSummary, Stage};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_applicants;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// An offer is the one worth noticing, a rejection the one worth not.
const fn stage_tone(stage: Stage) -> Tone {
    match stage {
        Stage::Hired => Tone::Success,
        Stage::Offer => Tone::Brand,
        Stage::Rejected | Stage::Withdrawn => Tone::Neutral,
        Stage::Applied | Stage::Screening | Stage::Interview => Tone::Warning,
    }
}

pub fn applicants_grid() -> GridConfig<ApplicantSummary> {
    GridConfig::new("applicants", Source::in_memory(list_applicants))
        .searching(l!("applicants.search"))
        .exports_as("applicants")
        .sorted_by(Sort::descending("applied_on"))
        .min_width("sm:min-w-[40rem]")
        .empty(
            Icon::UserPlus,
            l!("applicants.empty.title"),
            l!("applicants.empty.detail"),
        )
        .column(
            Column::new("name", l!("applicants.name"), |row: &ApplicantSummary| {
                Cell::text(row.display_name())
            })
            .findable()
            .pinned()
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new(
                "job_title",
                l!("applicants.job"),
                |row: &ApplicantSummary| Cell::text(&row.job_title),
            )
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new("email", l!("field.email"), |row: &ApplicantSummary| {
                Cell::maybe(row.email.clone())
            })
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "applied_on",
                l!("applicants.applied_on"),
                |row: &ApplicantSummary| Cell::text(row.applied_on.to_string()),
            )
            .essential()
            .sortable()
            .class("tabular-nums"),
        )
        .column(
            Column::new("stage", l!("applicants.stage"), |row: &ApplicantSummary| {
                Cell::text(crate::i18n::t(&row.stage.label()))
            })
            .essential()
            .render(|row| {
                view! {
                    <Badge
                        label=crate::i18n::t(&row.stage.label())
                        tone=stage_tone(row.stage)
                    />
                }
                .into_any()
            }),
        )
        .filter(
            Filter::new(
                "stage",
                l!("applicants.stage"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("open", l!("applicants.filter.open")),
                    FilterChoice::new("applied", l!("applicants.stage.applied")),
                    FilterChoice::new("screening", l!("applicants.stage.screening")),
                    FilterChoice::new("interview", l!("applicants.stage.interview")),
                    FilterChoice::new("offer", l!("applicants.stage.offer")),
                    FilterChoice::new("hired", l!("applicants.stage.hired")),
                    FilterChoice::new("rejected", l!("applicants.stage.rejected")),
                    FilterChoice::new("withdrawn", l!("applicants.stage.withdrawn")),
                ],
            )
            // "Open" is the one anybody actually wants: everybody still in
            // play, without naming the four stages that means today.
            .matching(|row: &ApplicantSummary, wanted| match wanted {
                "open" => row.stage.is_open(),
                "all" | "" => true,
                named => Stage::parse(named).is_some_and(|stage| stage == row.stage),
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("applicants.new"), Icon::Plus, "/people/applicants/new")
                .require(permissions::APPLICANTS_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &ApplicantSummary| format!("/people/applicants/{}", row.id),
            )
            .require(permissions::APPLICANTS),
        )
}
