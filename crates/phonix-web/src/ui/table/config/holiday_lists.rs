//! The calendars that say which days are not worked.
//!
//! The span is the column that earns its place. A calendar answers nothing
//! outside the dates it covers, so "United Kingdom" with no year beside it is a
//! row nobody can act on — the question is always whether it still reaches the
//! year somebody is asking about.

use app_hr::holiday::HolidayListSummary;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_holiday_lists;
use crate::ui::table::{
    Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn holiday_lists_grid() -> GridConfig<HolidayListSummary> {
    GridConfig::new("holiday_lists", Source::in_memory(list_holiday_lists))
        .searching(l!("holidays.search"))
        .exports_as("holiday-calendars")
        .sorted_by(Sort::descending("valid_from"))
        .min_width("sm:min-w-[40rem]")
        .empty(
            Icon::Calendar,
            l!("holidays.empty.title"),
            l!("holidays.empty.detail"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &HolidayListSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .pinned()
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new("code", l!("field.code"), |row: &HolidayListSummary| {
                Cell::text(&row.code)
            })
            .searchable()
            .sortable()
            .class("font-mono tabular-nums text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "valid_from",
                l!("holidays.valid_from"),
                |row: &HolidayListSummary| Cell::text(row.valid_from.to_string()),
            )
            .essential()
            .sortable(),
        )
        .column(
            Column::new(
                "valid_to",
                l!("holidays.valid_to"),
                |row: &HolidayListSummary| Cell::text(row.valid_to.to_string()),
            )
            .essential()
            .sortable(),
        )
        .column(
            Column::new(
                "holiday_count",
                l!("holidays.days"),
                |row: &HolidayListSummary| Cell::number(row.holiday_count as f64),
            )
            .sortable()
            .essential()
            .align(Align::End)
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "headcount",
                l!("holidays.headcount"),
                |row: &HolidayListSummary| Cell::number(row.headcount as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &HolidayListSummary| {
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
            .matching(|row: &HolidayListSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("holidays.new"), Icon::Plus, "/people/holidays/new")
                .require(permissions::HOLIDAY_LISTS_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &HolidayListSummary| format!("/people/holidays/{}", row.id),
            )
            .require(permissions::HOLIDAY_LISTS),
        )
}
