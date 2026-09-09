//! Who works here, what they do, and since when.
//!
//! # Leavers are on this list
//!
//! A staff list that dropped somebody the day they left is one nobody can look
//! a former colleague up in — and "who was in this department last year" is a
//! question asked far more often than the current headcount. The state column
//! tells them apart, and it is derived from whether there is an open engagement
//! rather than stored.
//!
//! # The login column is a fact, not a status
//!
//! Most people who work somewhere never sign in. A blank here is the ordinary
//! case and is drawn quietly, because a warning icon against two thirds of a
//! warehouse would be noise rather than information.

use app_hr::employee::EmployeeSummary;
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::list_employees;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn employees_grid() -> GridConfig<EmployeeSummary> {
    GridConfig::new("employees", Source::in_memory(list_employees))
        .searching(l!("employees.search"))
        .exports_as("employees")
        .sorted_by(Sort::ascending("name"))
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::Users,
            l!("employees.empty.title"),
            l!("employees.empty.detail"),
        )
        .column(
            Column::new("name", l!("employees.name"), |row: &EmployeeSummary| {
                Cell::text(row.display_name())
            })
            .findable()
            .pinned()
            .essential()
            .searchable()
            .sortable(),
        )
        .column(
            Column::new("code", l!("employees.code"), |row: &EmployeeSummary| {
                Cell::text(&row.code)
            })
            .searchable()
            .sortable()
            .class("font-mono tabular-nums text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "job_title",
                l!("employees.job_position"),
                |row: &EmployeeSummary| Cell::text(row.job_title.clone().unwrap_or_default()),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "department",
                l!("employees.department"),
                |row: &EmployeeSummary| Cell::text(row.department_name.clone().unwrap_or_default()),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "manager",
                l!("employees.manager"),
                |row: &EmployeeSummary| Cell::text(row.manager_name.clone().unwrap_or_default()),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "work_location",
                l!("employees.work_location"),
                |row: &EmployeeSummary| {
                    Cell::text(row.work_location_name.clone().unwrap_or_default())
                },
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "started_on",
                l!("employees.started_on"),
                |row: &EmployeeSummary| {
                    Cell::text(
                        row.started_on
                            .map(|on| on.to_string())
                            .unwrap_or_default(),
                    )
                },
            )
            .sortable()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &EmployeeSummary| {
                Cell::text(state_label(row))
            })
            .essential()
            .render(|row| {
                let tone = if row.is_employed() {
                    Tone::Success
                } else {
                    Tone::Neutral
                };

                view! { <Badge label=state_label(row) tone=tone /> }.into_any()
            }),
        )
        .column(
            Column::new(
                "work_email",
                l!("employees.work_email"),
                |row: &EmployeeSummary| Cell::text(row.work_email.clone().unwrap_or_default()),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            // A fact rather than a status - see the module docs.
            Column::new("login", l!("employees.login"), |row: &EmployeeSummary| {
                Cell::text(if row.has_login {
                    l!("employees.login.has_one")
                } else {
                    String::new()
                })
            })
            .render(|row| {
                if !row.has_login {
                    return view! { <span class="text-content-subtle">"—"</span> }.into_any();
                }

                view! { <Icon icon=Icon::KeyRound size=crate::icons::IconSize::Xs /> }.into_any()
            }),
        )
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    // Current staff first: it is what the screen is opened for,
                    // and leavers accumulate for ever.
                    FilterChoice::new("employed", l!("employees.state.employed")),
                    FilterChoice::new("left", l!("employees.state.left")),
                    FilterChoice::all(l!("common.all")),
                ],
            )
            .matching(|row: &EmployeeSummary, wanted| match wanted {
                "employed" => row.is_employed(),
                "left" => !row.is_employed(),
                _ => true,
            }),
        )
        .filter(
            Filter::new(
                "login",
                l!("employees.filter.login"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("has", l!("employees.filter.login.has")),
                    FilterChoice::new("none", l!("employees.filter.login.none")),
                ],
            )
            .matching(|row: &EmployeeSummary, wanted| match wanted {
                "has" => row.has_login,
                "none" => !row.has_login,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("employees.new"), Icon::Plus, "/people/employees/new")
                .require(permissions::EMPLOYEES_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &EmployeeSummary| format!("/people/employees/{}", row.id),
            )
            .require(permissions::EMPLOYEES),
        )
}

fn state_label(row: &EmployeeSummary) -> String {
    if row.is_employed() {
        l!("employees.state.employed")
    } else {
        l!("employees.state.left")
    }
}
