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
//! The screen opens on current staff all the same, because leavers accumulate
//! for ever and would eventually be most of it. That is one click from
//! everybody, and the filter says which it is showing.
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
use crate::server_fns::hr_fns::page_employees;
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn employees_grid() -> GridConfig<EmployeeSummary> {
    // The whole staff list, leavers included. Searching, sorting and both
    // filters are answered where the rows are; the manager picker's list is
    // `employed` and a different question.
    GridConfig::new("employees", Source::paged(page_employees))
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
                    Cell::text(row.started_on.map(|on| on.to_string()).unwrap_or_default())
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
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("employed", l!("employees.state.employed")),
                    FilterChoice::new("left", l!("employees.state.left")),
                ],
            )
            // Leavers never stop accumulating, so the unnarrowed list is the
            // one question this screen is least often opened to ask.
            .opening_on("employed"),
        )
        .filter(Filter::new(
            "login",
            l!("employees.filter.login"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("has", l!("employees.filter.login.has")),
                FilterChoice::new("none", l!("employees.filter.login.none")),
            ],
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> GridConfig<EmployeeSummary> {
        Owner::new().with(employees_grid)
    }

    /// Literals rather than imports: `phonix-web` does not depend on
    /// `phonix-db`, and the point is that the two were written to agree. The
    /// source is `phonix_db::hr::employee::SORTABLE`.
    const SERVER_SORTS: &[&str] = &["name", "code", "job_title", "department", "started_on"];

    /// The columns the `WHERE` looks inside. Same reasoning. It also reads the
    /// given and preferred names, which are not columns - a search finding more
    /// than the grid shows rather than less.
    const SERVER_SEARCHES: &[&str] = &[
        "name",
        "code",
        "job_title",
        "department",
        "manager",
        "work_location",
        "work_email",
    ];

    #[test]
    fn every_sortable_column_is_one_the_server_can_order_by() {
        for column in grid().columns.iter().filter(|column| column.sortable) {
            assert!(
                SERVER_SORTS.contains(&column.field()),
                "{} offers a sort the reader will ignore",
                column.field(),
            );
        }
    }

    #[test]
    fn every_searchable_column_is_one_the_server_looks_inside() {
        for column in grid().columns.iter().filter(|column| column.searchable) {
            assert!(
                SERVER_SEARCHES.contains(&column.field()),
                "{} is offered to the search box and never searched",
                column.field(),
            );
        }
    }

    #[test]
    fn it_opens_on_current_staff_by_name() {
        // The one grid that opens narrowed. `employed` is a state the store
        // answers, so this reaches the reader rather than being ignored.
        let grid = grid();
        let request = grid.initial_request();
        let sort = request.sort.clone().expect("an opening order");

        assert_eq!(sort, Sort::ascending("name"));
        assert!(SERVER_SORTS.contains(&sort.field.as_str()));
        assert_eq!(request.filter("state"), Some("employed"));

        // Everything else opens on everything, and no filter is answered in
        // the browser now that the source is paged.
        assert_eq!(request.filter("login"), None);
        for filter in &grid.filters {
            assert!(!filter.is_local(), "{}", filter.key());
        }
    }

    #[test]
    fn every_state_the_filter_offers_is_one_the_reader_answers() {
        // `phonix_db::hr::employee::page` matches these two and treats anything
        // else as no filter at all, which would silently show every row.
        let grid = grid();
        let state = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        let offered: Vec<&str> = state
            .choices
            .iter()
            .map(|choice| choice.value)
            .filter(|value| !value.is_empty())
            .collect();

        assert_eq!(offered, vec!["employed", "left"]);
    }
}
