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
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn requisitions_grid() -> GridConfig<RequisitionSummary> {
    GridConfig::new("requisitions", Source::paged(list_requisitions))
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
            ),
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
            ),
        )
        .date_filter(DateFilter::new("raised", l!("requisitions.raised_on")))
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

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<RequisitionSummary> {
        Owner::new().with(requisitions_grid)
    }

    /// Mirrors the database sort fields without adding a crate dependency.
    const SERVER_SORTS: &[&str] = &["cost_centre", "raised_on", "needed_by", "estimate", "line_count"];

    /// Mirrors the database search fields.
    const SERVER_SEARCHES: &[&str] = &["number", "cost_centre", "raised_by", "warehouse"];

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
    fn it_opens_on_everything_newest_first() {
        let grid = grid();
        let sort = grid.initial_request().sort.expect("an opening order");

        assert_eq!(sort, Sort::descending("raised_on"));
        assert!(SERVER_SORTS.contains(&sort.field.as_str()));

        // The empty choice keeps the initial list unfiltered.
        for filter in &grid.filters {
            assert_eq!(filter.default_value(), "", "{}", filter.key());
            assert!(
                !filter.is_local(),
                "{} is answered in the wrong place",
                filter.key()
            );
        }

        let range = grid.date_filters.first().expect("the grid offers a span");

        // Mirrors the database date-range key.
        assert_eq!(range.key(), "raised");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_is_offered_under_some_group() {
        // Every state must have a filter group.
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for state in RequisitionState::ALL {
            assert!(
                states.choices.iter().any(|c| c.value == state.group()),
                "{} is in no group the grid offers",
                state.as_str(),
            );
        }

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                !RequisitionState::in_group(choice.value).is_empty(),
                "{} is offered and covers nothing",
                choice.value,
            );
        }
    }

    #[test]
    fn the_ordered_filter_offers_the_two_words_the_reader_answers() {
        let grid = grid();
        let filter = grid.filters.iter().find(|f| f.key() == "ordered").unwrap();

        let offered: Vec<&str> = filter
            .choices
            .iter()
            .map(|choice| choice.value)
            .filter(|value| !value.is_empty())
            .collect();

        assert_eq!(offered, ["outstanding", "complete"]);
    }
}
