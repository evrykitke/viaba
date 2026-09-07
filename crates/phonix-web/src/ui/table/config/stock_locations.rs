//! The locations grid: a tree drawn as a table.
//!
//! Rows arrive in tree order with a depth on each; this indents by that depth
//! and otherwise treats them as a flat list. Sorting a column throws the shape
//! away, which is right - somebody sorting by name is asking a flat question.
//!
//! The **kind** column is the point of the screen. It is what decides whether
//! stock here is on hand, on the balance sheet, or somebody else's - and it is
//! the column that makes a receipt from `Vendors` legible as a receipt.

use leptos::prelude::*;
use phonix_core::permissions;

use app_inventory::location::{LocationKind, LocationSummary};

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{delete_stock_location, list_stock_locations};
use crate::ui::table::{Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// Where stock is, including the places that are not places.
pub fn stock_locations_grid() -> GridConfig<LocationSummary> {
    GridConfig::new("stock-locations", Source::in_memory(list_stock_locations))
        .searching(l!("locations.search"))
        .exports_as("locations")
        // No `sorted_by`: a default sort would flatten the tree on arrival.
        .min_width("sm:min-w-[48rem]")
        .empty(
            Icon::Boxes,
            l!("locations.empty.title"),
            l!("locations.empty.detail"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &LocationSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| name_cell(row).into_any()),
        )
        .column(
            Column::new("path", l!("locations.path"), |row: &LocationSummary| {
                Cell::text(&row.code)
            })
            .findable()
            // Already under the name; here to sort and export on its own.
            .hidden(),
        )
        .column(
            Column::new("kind", l!("locations.kind"), |row: &LocationSummary| {
                Cell::text(crate::i18n::t(&row.kind.label()))
            })
            .sortable()
            .essential()
            .render(|row| kind_cell(row).into_any()),
        )
        .column(
            Column::new(
                "warehouse",
                l!("locations.warehouse"),
                |row: &LocationSummary| Cell::maybe(row.warehouse_name.clone()),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("is_active", l!("field.status"), |row: &LocationSummary| {
                Cell::bool(row.is_active)
            })
            .sortable()
            .render(|row| status_cell(row).into_any()),
        )
        .filter(
            Filter::new("kind", l!("locations.kind"), kind_choices()).matching(
                |row: &LocationSummary, wanted| match wanted {
                    // The question most often asked of this screen, and the one
                    // no single kind answers: "where is our stock actually
                    // sitting", as opposed to the counterpart locations.
                    "on_hand" => row.kind.is_on_hand(),
                    other => row.kind.as_str() == other,
                },
            ),
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
            .matching(|row: &LocationSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("locations.new"), Icon::Plus, "/inventory/locations/new")
                .require(permissions::STOCK_LOCATIONS_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(l!("common.edit"), Icon::Pencil, |row: &LocationSummary| {
                format!("/inventory/locations/{}", row.id)
            })
            .require(permissions::STOCK_LOCATIONS_MANAGE),
        )
        .action(
            RowAction::run(
                l!("common.delete"),
                Icon::Trash2,
                |row: LocationSummary, grid| {
                    leptos::task::spawn_local(async move {
                        use app_inventory::location::DeleteOutcome;

                        match delete_stock_location(row.id).await {
                            Ok(DeleteOutcome::Deleted) => {
                                grid.report(l!("locations.deleted", name = row.code));
                                grid.refresh();
                            }
                            Ok(DeleteOutcome::HasChildren { count }) => {
                                grid.warn(l!("locations.delete.has_children", count = count));
                            }
                            Ok(DeleteOutcome::HasMovements) => {
                                grid.warn(l!("locations.delete.has_movements"));
                            }
                            Err(err) => grid.warn(err.to_string()),
                        }
                    });
                },
            )
            // The counterparts are seeded once and left alone: a second
            // inventory-loss location would be two places a count difference
            // could go, with nothing to say which.
            .when(|row: &LocationSummary| row.kind.is_user_creatable())
            .require(permissions::STOCK_LOCATIONS_MANAGE)
            .tone(Tone::Danger)
            .confirm(l!("locations.delete.confirm")),
        )
}

fn kind_choices() -> Vec<FilterChoice> {
    let mut choices = vec![
        FilterChoice::all(l!("common.all")),
        FilterChoice::new("on_hand", l!("locations.only_on_hand")),
    ];

    choices.extend(
        LocationKind::ALL
            .iter()
            .map(|kind| FilterChoice::new(kind.as_str(), crate::i18n::t(&kind.label()))),
    );

    choices
}

/// The name, indented by depth, with the full path beneath it.
fn name_cell(row: &LocationSummary) -> impl IntoView {
    let name = row.name.clone();
    let path = row.code.clone();
    let indent = format!("padding-left:{}rem", f64::from(row.depth.min(6)) * 0.75);
    // A path that is only the name says nothing twice.
    let show_path = path != name;

    view! {
        <div style=indent>
            <span class="font-medium">{name}</span>
            <Show when=move || show_path>
                <div class="font-mono text-[0.65rem] text-content-subtle">{path.clone()}</div>
            </Show>
        </div>
    }
}

/// The kind, coloured by what it means for the balance sheet.
///
/// Internal is ours and countable; transit is ours and on a lorry; everything
/// else is the other side of an entry. Three tones rather than seven, because
/// the distinction that matters is which of those three a row is.
fn kind_cell(row: &LocationSummary) -> impl IntoView {
    let kind = row.kind;
    let label = crate::i18n::t(&kind.label());

    let tone = if kind.is_on_hand() {
        Tone::Success
    } else if kind.is_owned() {
        Tone::Brand
    } else {
        Tone::Neutral
    };

    view! { <Badge tone=tone label=label /> }
}

fn status_cell(row: &LocationSummary) -> impl IntoView {
    let active = row.is_active;

    view! {
        <Show
            when=move || active
            fallback=|| view! { <Badge tone=Tone::Neutral label=l!("common.inactive") /> }
        >
            <Badge tone=Tone::Success label=l!("common.active") />
        </Show>
    }
}
