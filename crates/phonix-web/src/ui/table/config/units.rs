//! The units grid.
//!
//! Grouped by class in the sort order, base unit first, because "one what?" is
//! the question the factor column answers and it only makes sense next to the
//! unit it is measured against.

use leptos::prelude::*;
use phonix_core::permissions;

use app_inventory::unit::{Unit, UnitClass, factor_to_string};

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{delete_unit, list_units};
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// What stock is counted in.
pub fn units_grid() -> GridConfig<Unit> {
    GridConfig::new("units", Source::in_memory(list_units))
        .searching(l!("units.search"))
        .exports_as("units")
        .min_width("sm:min-w-[40rem]")
        .empty(Icon::Ruler, l!("units.empty.title"), l!("units.empty.detail"))
        .column(
            Column::new("code", l!("field.code"), |row: &Unit| Cell::text(&row.code))
                .findable()
                .pinned()
                .essential()
                .class("font-mono text-xs"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &Unit| Cell::text(&row.name))
                .findable()
                .essential(),
        )
        .column(
            Column::new("class", l!("units.class"), |row: &Unit| {
                Cell::text(crate::i18n::t(&row.class.label()))
            })
            .sortable()
            .essential(),
        )
        .column(
            Column::new("factor", l!("units.factor"), |row: &Unit| {
                Cell::text(factor_to_string(row.factor_scaled))
            })
            .align(Align::End)
            .class("font-mono text-xs")
            .render(|row| factor_cell(row).into_any()),
        )
        .column(
            Column::new("is_active", l!("field.status"), |row: &Unit| {
                Cell::bool(row.is_active)
            })
            .sortable()
            .render(|row| status_cell(row).into_any()),
        )
        .filter(
            Filter::new("class", l!("units.class"), class_choices()).matching(
                |row: &Unit, wanted| row.class.as_str() == wanted,
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
            .matching(|row: &Unit, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(l!("units.new"), Icon::Plus, "/inventory/units/new")
                .require(permissions::UNITS_MANAGE)
                .primary(),
        )
        .action(
            RowAction::link(l!("common.edit"), Icon::Pencil, |row: &Unit| {
                format!("/inventory/units/{}", row.id)
            })
            .require(permissions::UNITS_MANAGE),
        )
        .action(
            RowAction::run(
                l!("common.delete"),
                Icon::Trash2,
                |row: Unit, grid| {
                    leptos::task::spawn_local(async move {
                        use app_inventory::unit::DeleteOutcome;

                        // Both refusals are reported, not thrown: neither is a
                        // fault, and both are reached from a list that was
                        // right when it was drawn.
                        match delete_unit(row.id).await {
                            Ok(DeleteOutcome::Deleted) => {
                                grid.report(l!("units.deleted", name = row.code));
                                grid.refresh();
                            }
                            Ok(DeleteOutcome::InUse { count }) => {
                                grid.warn(l!("units.delete.in_use", count = count));
                            }
                            Ok(DeleteOutcome::IsTheReference) => {
                                grid.warn(l!("units.delete.is_reference"));
                            }
                            Err(err) => grid.warn(err.to_string()),
                        }
                    });
                },
            )
            // Offered only where it could do something. The reference unit of a
            // class is what every other unit in it is measured against, and the
            // service refuses it.
            .when(|row: &Unit| !row.is_base)
            .require(permissions::UNITS_MANAGE)
            .tone(Tone::Danger)
            .confirm(l!("units.delete.confirm")),
        )
}

fn class_choices() -> Vec<FilterChoice> {
    let mut choices = vec![FilterChoice::all(l!("common.all"))];

    choices.extend(
        UnitClass::ALL
            .iter()
            .map(|class| FilterChoice::new(class.as_str(), crate::i18n::t(&class.label()))),
    );

    choices
}

/// The factor, and a mark on the one its class is measured against.
///
/// "1" on a row that is not the base reads as an error; "1" beside the word
/// *reference* reads as the definition it is.
fn factor_cell(row: &Unit) -> impl IntoView {
    let factor = factor_to_string(row.factor_scaled);
    let is_base = row.is_base;

    view! {
        <div class="flex items-center justify-end gap-2">
            <span>{factor}</span>
            <Show when=move || is_base>
                <Badge tone=Tone::Brand label=l!("units.reference") />
            </Show>
        </div>
    }
}

fn status_cell(row: &Unit) -> impl IntoView {
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
