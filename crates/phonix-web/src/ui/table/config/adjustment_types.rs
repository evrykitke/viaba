//! The adjustment types grid.
//!
//! The account column is what the screen is for. A workspace that never opens
//! this list posts every discrepancy to one place and cannot tell theft from a
//! warehouse that cannot count - so the column says "the default" out loud
//! rather than leaving a blank cell that reads as nothing having happened.

use leptos::prelude::*;
use phonix_core::permissions;

use app_inventory::adjustment::{AdjustmentTypeSummary, Direction};

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{delete_adjustment_type, list_adjustment_types};
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// Why a stock figure was corrected by hand.
pub fn adjustment_types_grid() -> GridConfig<AdjustmentTypeSummary> {
    GridConfig::new("adjustment_types", Source::in_memory(list_adjustment_types))
        .searching(l!("adjustment_types.search"))
        .exports_as("adjustment-types")
        .min_width("sm:min-w-[52rem]")
        .empty(
            Icon::SlidersHorizontal,
            l!("adjustment_types.empty.title"),
            l!("adjustment_types.empty.detail"),
        )
        .column(
            Column::new("code", l!("field.code"), |row: &AdjustmentTypeSummary| {
                Cell::text(&row.code)
            })
            .findable()
            .pinned()
            .essential()
            .class("font-mono text-xs"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &AdjustmentTypeSummary| {
                Cell::text(&row.name)
            })
            .findable()
            .essential(),
        )
        .column(
            Column::new(
                "direction",
                l!("adjustment_types.direction"),
                |row: &AdjustmentTypeSummary| Cell::text(crate::i18n::t(&row.direction.label())),
            )
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "account",
                l!("adjustment_types.account"),
                |row: &AdjustmentTypeSummary| match &row.account_number {
                    Some(number) => Cell::text(format!(
                        "{number} · {}",
                        row.account_name.clone().unwrap_or_default()
                    )),
                    None => Cell::text(l!("adjustment_types.account.default")),
                },
            )
            .findable()
            .render(|row| account_cell(row).into_any()),
        )
        .column(
            Column::new(
                "needs_approval",
                l!("adjustment_types.approval"),
                |row: &AdjustmentTypeSummary| Cell::bool(row.needs_approval),
            )
            .sortable()
            .render(|row| approval_cell(row).into_any()),
        )
        .column(
            Column::new(
                "move_count",
                l!("adjustment_types.booked"),
                |row: &AdjustmentTypeSummary| Cell::number(row.move_count as f64),
            )
            .align(Align::End)
            .sortable(),
        )
        .column(
            Column::new(
                "is_active",
                l!("field.status"),
                |row: &AdjustmentTypeSummary| Cell::bool(row.is_active),
            )
            .sortable()
            .render(|row| status_cell(row).into_any()),
        )
        .filter(
            Filter::new(
                "direction",
                l!("adjustment_types.direction"),
                direction_choices(),
            )
            .matching(|row: &AdjustmentTypeSummary, wanted| row.direction.as_str() == wanted),
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
            .matching(|row: &AdjustmentTypeSummary, wanted| match wanted {
                "active" => row.is_active,
                "inactive" => !row.is_active,
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(
                l!("adjustment_types.new"),
                Icon::Plus,
                "/inventory/adjustment-types/new",
            )
            .require(permissions::ADJUSTMENT_TYPES_MANAGE)
            .primary(),
        )
        .action(
            RowAction::link(
                l!("common.edit"),
                Icon::Pencil,
                |row: &AdjustmentTypeSummary| {
                    format!("/inventory/adjustment-types/{}", row.id)
                },
            )
            .require(permissions::ADJUSTMENT_TYPES_MANAGE),
        )
        .action(
            RowAction::run(
                l!("common.delete"),
                Icon::Trash2,
                |row: AdjustmentTypeSummary, grid| {
                    leptos::task::spawn_local(async move {
                        use phonix_core::form::Submission;

                        match delete_adjustment_type(row.id).await {
                            Ok(Submission::Saved(())) => {
                                grid.report(l!("adjustment_types.deleted", name = row.code));
                                grid.refresh();
                            }
                            Ok(Submission::Rejected(errors)) => {
                                if let Some(error) = errors.first() {
                                    grid.warn(crate::i18n::t(&error.message));
                                }
                            }
                            Err(err) => grid.warn(err.to_string()),
                        }
                    });
                },
            )
            // Offered only where it could do something. A seeded reason is not
            // the workspace's to throw away, and one that movements point at
            // would take the answer to "what did that cost us" with it - both
            // are refused by the service, and neither is worth a button.
            .when(|row: &AdjustmentTypeSummary| !row.is_system && !row.is_in_use())
            .require(permissions::ADJUSTMENT_TYPES_MANAGE)
            .tone(Tone::Danger)
            .confirm(l!("adjustment_types.delete.confirm")),
        )
}

fn direction_choices() -> Vec<FilterChoice> {
    let mut choices = vec![FilterChoice::all(l!("common.all"))];

    choices.extend(
        Direction::ALL
            .iter()
            .map(|one| FilterChoice::new(one.as_str(), crate::i18n::t(&one.label()))),
    );

    choices
}

/// The account, or the fact that nobody has named one.
///
/// "The default" rather than an empty cell: a blank reads as a row somebody has
/// not finished, and this one is finished and posting somewhere.
fn account_cell(row: &AdjustmentTypeSummary) -> impl IntoView {
    let named = row
        .account_number
        .as_ref()
        .map(|number| format!("{number} · {}", row.account_name.clone().unwrap_or_default()));

    view! {
        {match named {
            Some(named) => view! { <span class="text-sm">{named}</span> }.into_any(),
            None => view! {
                <span class="text-xs text-content-subtle">
                    {l!("adjustment_types.account.default")}
                </span>
            }
                .into_any(),
        }}
    }
}

fn approval_cell(row: &AdjustmentTypeSummary) -> impl IntoView {
    let needed = row.needs_approval;

    view! {
        <Show when=move || needed fallback=|| ()>
            <Badge tone=Tone::Warning label=l!("adjustment_types.approval.needed") />
        </Show>
    }
}

fn status_cell(row: &AdjustmentTypeSummary) -> impl IntoView {
    let active = row.is_active;
    let seeded = row.is_system;

    view! {
        <div class="flex flex-wrap items-center gap-1.5">
            <Show
                when=move || active
                fallback=|| view! { <Badge tone=Tone::Neutral label=l!("common.inactive") /> }
            >
                <Badge tone=Tone::Success label=l!("common.active") />
            </Show>
            <Show when=move || seeded fallback=|| ()>
                <Badge label=l!("adjustment_types.seeded") />
            </Show>
        </div>
    }
}
