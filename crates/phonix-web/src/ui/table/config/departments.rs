//! The departments grid: a tree drawn as a table.
//!
//! Rows arrive in tree order with a depth on each; this indents by that depth
//! and otherwise treats them as a flat list, so sorting, searching and
//! exporting need not know there is a hierarchy. Sorting a column does throw
//! the shape away, which is right — somebody sorting by name is asking a flat
//! question.
//!
//! The cost-centre column is the point of the screen: it is what decides what a
//! requisition or a journal line may name.

use leptos::prelude::*;
use phonix_core::permissions;

use app_hr::department::DepartmentSummary;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{delete_department, list_departments};
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// How the workspace is arranged.
pub fn departments_grid() -> GridConfig<DepartmentSummary> {
    GridConfig::new("departments", Source::in_memory(list_departments))
        .searching(l!("departments.search"))
        .exports_as("departments")
        // No `sorted_by`: a default sort would flatten the tree on arrival.
        .min_width("sm:min-w-[44rem]")
        .empty(
            Icon::Building2,
            l!("departments.empty.title"),
            l!("departments.empty.detail"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &DepartmentSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| name_cell(row).into_any()),
        )
        .column(
            Column::new("code", l!("field.code"), |row: &DepartmentSummary| {
                Cell::text(&row.code)
            })
            .findable()
            // Already under the name; here to sort and export on its own.
            .hidden(),
        )
        .column(
            Column::new(
                "is_cost_centre",
                l!("departments.cost_centre"),
                |row: &DepartmentSummary| Cell::bool(row.is_cost_centre),
            )
            .sortable()
            .essential()
            .render(|row| chargeable_cell(row).into_any()),
        )
        .column(
            Column::new(
                "manager",
                l!("departments.manager"),
                |row: &DepartmentSummary| Cell::maybe(row.manager_name.clone()),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "is_active",
                l!("field.status"),
                |row: &DepartmentSummary| Cell::bool(row.is_active),
            )
            .sortable()
            .render(|row| status_cell(row).into_any()),
        )
        .filter(
            Filter::new(
                "chargeable",
                l!("departments.cost_centre"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("yes", l!("departments.only_cost_centres")),
                    FilterChoice::new("no", l!("departments.only_groupings")),
                ],
            )
            .matching(|row: &DepartmentSummary, wanted| match wanted {
                "yes" => row.is_cost_centre,
                "no" => !row.is_cost_centre,
                _ => true,
            }),
        )
        .filter(
            Filter::new(
                "status",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("active", l!("common.active")),
                    FilterChoice::new("inactive", l!("common.inactive")),
                ],
            )
            .matching(|row: &DepartmentSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("departments.new"), Icon::Plus, "/people/departments/new")
                .require(permissions::DEPARTMENTS_CREATE)
                .primary(),
        )
        .action(
            RowAction::link(
                l!("common.edit"),
                Icon::Pencil,
                |row: &DepartmentSummary| format!("/people/departments/{}", row.id),
            )
            .require(permissions::DEPARTMENTS_EDIT),
        )
        .action(
            RowAction::run(
                l!("common.delete"),
                Icon::Trash2,
                |row: DepartmentSummary, grid| {
                    leptos::task::spawn_local(async move {
                        use app_hr::department::DeleteOutcome;

                        // Both refusals are reported, not thrown: neither is a
                        // fault, and both are reached from a stale list.
                        match delete_department(row.id).await {
                            Ok(DeleteOutcome::Deleted) => {
                                grid.report(l!("departments.deleted", name = row.name));
                                grid.refresh();
                            }
                            Ok(DeleteOutcome::HasChildren { count }) => {
                                grid.warn(l!("departments.delete.has_children", count = count));
                            }
                            Ok(DeleteOutcome::MayBeInUse) => {
                                grid.warn(l!("departments.delete.in_use"));
                            }
                            Err(err) => grid.warn(err.to_string()),
                        }
                    });
                },
            )
            // Offered only where it would do something: the service refuses
            // a cost centre and anything with children.
            .when(|row: &DepartmentSummary| !row.is_cost_centre && row.child_count == 0)
            .require(permissions::DEPARTMENTS_DELETE)
            .tone(Tone::Danger)
            .confirm(l!("departments.delete.confirm")),
        )
}

/// The name, indented to its depth, with the code under it. Capped at four
/// levels — past that the indent hides the names rather than showing the tree.
fn name_cell(row: &DepartmentSummary) -> impl IntoView {
    let name = row.name.clone();
    let code = row.code.clone();
    let indent = format!("padding-left:{}rem", (row.depth.min(4) as f32) * 0.875);

    view! {
        <div class="min-w-0" style=indent>
            <div class="flex flex-wrap items-center gap-1.5">
                <span class="truncate-fade font-medium text-content">{name}</span>
            </div>
            <code class="text-2xs text-content-subtle">{code}</code>
        </div>
    }
}

/// Whether anything may be charged here. A grouping gets its own badge rather
/// than a blank, which would read as unfinished.
fn chargeable_cell(row: &DepartmentSummary) -> impl IntoView {
    if row.is_cost_centre {
        view! { <Badge label=l!("departments.cost_centre.yes") tone=Tone::Success /> }.into_any()
    } else {
        view! { <Badge label=l!("departments.cost_centre.no") /> }.into_any()
    }
}

fn status_cell(row: &DepartmentSummary) -> impl IntoView {
    if row.is_active {
        view! { <Badge label=l!("common.active") tone=Tone::Success /> }.into_any()
    } else {
        view! { <Badge label=l!("common.inactive") /> }.into_any()
    }
}
