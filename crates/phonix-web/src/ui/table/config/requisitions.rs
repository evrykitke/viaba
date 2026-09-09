//! What departments have asked for, and how much of it has been bought.
//!
//! # Two states again, and the second one is not the same as an order's
//!
//! A requisition's own state is what happened to the *request* - somebody asked,
//! somebody answered. Its order progress is what buying did about it, worked out
//! from the lines rather than stored. An approved requisition that is
//! not-ordered is a queue somebody has to work through; an approved one that is
//! fully-ordered is finished without anybody having to close it.
//!
//! # The estimate is the column an approver reads
//!
//! And it is blank whenever a single line was left unpriced, which is
//! deliberate: a total covering three lines out of eight is a number that
//! invites the wrong decision. See `Requisition::estimate`.

use app_inventory::requisition::{OrderProgress, RequisitionState, RequisitionSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_requisitions;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

pub fn requisitions_grid() -> GridConfig<RequisitionSummary> {
    GridConfig::new("requisitions", Source::in_memory(list_requisitions))
        .searching(l!("requisitions.search"))
        .exports_as("requisitions")
        .sorted_by(Sort::descending("raised_on"))
        .min_width("sm:min-w-[54rem]")
        .empty(
            Icon::ClipboardList,
            l!("requisitions.empty.title"),
            l!("requisitions.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &RequisitionSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .class("font-mono tabular-nums")
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new(
                "cost_centre",
                l!("requisitions.cost_centre"),
                |row: &RequisitionSummary| Cell::text(&row.cost_centre_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "raised_by",
                l!("requisitions.raised_by"),
                |row: &RequisitionSummary| {
                    Cell::text(row.raised_by_name.clone().unwrap_or_default())
                },
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "raised_on",
                l!("requisitions.raised_on"),
                |row: &RequisitionSummary| Cell::text(row.raised_on.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "needed_by",
                l!("requisitions.needed_by"),
                |row: &RequisitionSummary| {
                    Cell::text(
                        row.needed_by
                            .map(|date| date.to_string())
                            .unwrap_or_default(),
                    )
                },
            )
            .sortable()
            .essential()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &RequisitionSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            // What buying did, as opposed to what the approver did.
            Column::new(
                "ordered",
                l!("requisitions.ordered"),
                |row: &RequisitionSummary| Cell::text(progress_label(row.order_progress)),
            )
            .render(|row| {
                view! {
                    <Badge
                        label=progress_label(row.order_progress)
                        tone=progress_tone(row.order_progress)
                    />
                }
                .into_any()
            }),
        )
        .column(
            Column::new(
                "estimate",
                l!("requisitions.estimate.total"),
                |row: &RequisitionSummary| match row.estimate {
                    Some(amount) => Cell::number(amount.scaled() as f64),
                    // Not zero. A requisition nobody priced has no total, and
                    // sorting it as if it cost nothing would put it at the
                    // cheap end of a list an approver is triaging by cost.
                    None => Cell::text(String::new()),
                },
            )
            .sortable()
            .align(Align::End)
            .render(|row| estimate_cell(row).into_any()),
        )
        .column(
            Column::new(
                "warehouse",
                l!("requisitions.warehouse"),
                |row: &RequisitionSummary| Cell::text(&row.warehouse_name),
            )
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "line_count",
                l!("requisitions.lines_count"),
                |row: &RequisitionSummary| Cell::number(row.line_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    // First, because it is the one somebody opens this screen
                    // to work through.
                    FilterChoice::new("submitted", l!("requisitions.awaiting")),
                    FilterChoice::new("approved", l!("requisitions.state.approved")),
                    FilterChoice::new("draft", l!("requisitions.state.draft")),
                    FilterChoice::new("closed", l!("requisitions.state.rejected")),
                ],
            )
            .matching(|row: &RequisitionSummary, wanted| match wanted {
                "submitted" => matches!(row.state, RequisitionState::Submitted),
                "approved" => matches!(row.state, RequisitionState::Approved),
                "draft" => matches!(row.state, RequisitionState::Draft),
                "closed" => matches!(
                    row.state,
                    RequisitionState::Rejected | RequisitionState::Cancelled
                ),
                _ => true,
            }),
        )
        .filter(
            Filter::new(
                "ordered",
                l!("requisitions.ordered"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("outstanding", l!("requisitions.ordered.nothing")),
                    FilterChoice::new("complete", l!("requisitions.ordered.everything")),
                ],
            )
            .matching(|row: &RequisitionSummary, wanted| match wanted {
                "outstanding" => !row.order_progress.is_complete(),
                "complete" => row.order_progress.is_complete(),
                _ => true,
            }),
        )
        .toolbar(
            ToolbarAction::link(
                l!("requisitions.new"),
                Icon::Plus,
                "/inventory/requisitions/new",
            )
            .require(permissions::REQUISITIONS_CREATE)
            .primary(),
        )
        .action(
            RowAction::link(
                l!("common.open"),
                Icon::ArrowRight,
                |row: &RequisitionSummary| format!("/inventory/requisitions/{}", row.id),
            )
            .require(permissions::REQUISITIONS),
        )
}

/// The number, or what a draft is recognisable by before it has one.
fn number_cell(row: &RequisitionSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">
                {l!("requisitions.state.draft")}
            </span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

/// The total, where there is an honest one.
fn estimate_cell(row: &RequisitionSummary) -> impl IntoView {
    match row.estimate {
        Some(amount) => {
            let text = amount.to_display_string();
            view! { <span class="tabular-nums">{text}</span> }.into_any()
        }
        None => view! {
            <span class="text-xs text-content-subtle" title=l!("requisitions.estimate.partial")>
                "—"
            </span>
        }
        .into_any(),
    }
}

fn state_label(state: RequisitionState) -> String {
    match state {
        RequisitionState::Draft => l!("requisitions.state.draft"),
        RequisitionState::Submitted => l!("requisitions.state.submitted"),
        RequisitionState::Approved => l!("requisitions.state.approved"),
        RequisitionState::Rejected => l!("requisitions.state.rejected"),
        RequisitionState::Cancelled => l!("requisitions.state.cancelled"),
    }
}

/// Waiting is a warning, not a neutral: it is the state that needs somebody to
/// do something, and it is the whole reason this screen is opened.
const fn state_tone(state: RequisitionState) -> Tone {
    match state {
        RequisitionState::Submitted => Tone::Warning,
        RequisitionState::Approved => Tone::Success,
        RequisitionState::Rejected => Tone::Danger,
        RequisitionState::Draft | RequisitionState::Cancelled => Tone::Neutral,
    }
}

fn progress_label(progress: OrderProgress) -> String {
    match progress {
        OrderProgress::Nothing => l!("requisitions.ordered.nothing"),
        OrderProgress::Partly => l!("requisitions.ordered.partly"),
        OrderProgress::Everything => l!("requisitions.ordered.everything"),
    }
}

const fn progress_tone(progress: OrderProgress) -> Tone {
    match progress {
        OrderProgress::Nothing => Tone::Neutral,
        OrderProgress::Partly => Tone::Brand,
        OrderProgress::Everything => Tone::Success,
    }
}
