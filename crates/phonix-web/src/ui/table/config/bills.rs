//! What suppliers charged, and how well it matched.
//!
//! The supplier's own invoice number is a findable column, because that is what
//! somebody has in front of them when the supplier rings.

use app_inventory::bill::{BillState, BillSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_bills;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn bills_grid() -> GridConfig<BillSummary> {
    GridConfig::new("bills", Source::paged(list_bills))
        .searching(l!("bills.search"))
        .exports_as("supplier-bills")
        .sorted_by(Sort::descending("bill_date"))
        .min_width("sm:min-w-[54rem]")
        .empty(
            Icon::Receipt,
            l!("bills.empty.title"),
            l!("bills.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &BillSummary| {
                Cell::text(&row.number)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| number_cell(row).into_any()),
        )
        .column(
            Column::new("supplier", l!("purchase_orders.supplier"), |row: &BillSummary| {
                Cell::text(&row.supplier_name)
            })
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new("reference", l!("bills.reference"), |row: &BillSummary| {
                Cell::text(&row.supplier_reference)
            })
            .searchable()
            .findable()
            .essential()
            .class("font-mono text-xs text-content-muted"),
        )
        .column(
            Column::new("bill_date", l!("bills.dated"), |row: &BillSummary| {
                Cell::text(row.bill_date.to_string())
            })
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new("due_on", l!("bills.due"), |row: &BillSummary| {
                Cell::text(row.due_on.map(|due| due.to_string()).unwrap_or_default())
            })
            .sortable()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("order", l!("receipts.order"), |row: &BillSummary| {
                Cell::text(row.order_number.clone().unwrap_or_default())
            })
            .findable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &BillSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new("net", l!("bills.net"), |row: &BillSummary| {
                Cell::number(row.net.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = row.net.to_display_string();
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new("variance", l!("bills.variance"), |row: &BillSummary| {
                Cell::number(row.variance.scaled() as f64)
            })
            .sortable()
            .align(Align::End)
            .render(|row| variance_cell(row).into_any()),
        )
        .column(
            Column::new("lines", l!("bills.lines"), |row: &BillSummary| {
                Cell::number(row.line_count as f64)
            })
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        .filter(
            Filter::new(
                "state",
                l!("field.status"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("draft", l!("bills.state.draft")),
                    FilterChoice::new("posted", l!("bills.state.posted")),
                    FilterChoice::new("cancelled", l!("bills.state.cancelled")),
                ],
            ),
        )
        .date_filter(DateFilter::new("billed", l!("bills.dated")))
        .toolbar(
            ToolbarAction::link(l!("common.add"), Icon::Plus, "/inventory/bills/new")
                .require(permissions::BILLS_CREATE)
                .primary(),
        )
        .toolbar(
            ToolbarAction::link(
                l!("bills.unbilled.title"),
                Icon::Clock,
                "/inventory/unbilled",
            )
            .require(permissions::BILLS),
        )
        .action(
            RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &BillSummary| {
                format!("/inventory/bills/{}", row.id)
            })
            .require(permissions::BILLS),
        )
}

fn number_cell(row: &BillSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">{l!("bills.state.draft")}</span>
        }
        .into_any();
    }

    let number = row.number.clone();
    let overridden = row.was_overridden;

    view! {
        <span class="font-mono tabular-nums">
            {number}
            // A posted bill whose match was forced says so in the list, not
            // only on the document.
            {overridden
                .then(|| {
                    view! {
                        <span class="ml-1 text-2xs not-italic text-warning" title=l!("bills.match_note")>
                            "!"
                        </span>
                    }
                })}
        </span>
    }
    .into_any()
}

/// Zero shows as nothing rather than as `0.00`: a column of zeroes with three
/// real numbers in it hides the three.
fn variance_cell(row: &BillSummary) -> impl IntoView {
    if row.variance.is_zero() {
        return view! { <span class="text-content-subtle">"—"</span> }.into_any();
    }

    let text = row.variance.to_display_string();
    let tone = if row.variance.is_negative() {
        "tabular-nums text-success"
    } else {
        "tabular-nums text-warning"
    };

    view! { <span class=tone>{text}</span> }.into_any()
}

fn state_label(state: BillState) -> String {
    crate::i18n::t(&state.label())
}

fn state_tone(state: BillState) -> Tone {
    match state {
        BillState::Draft => Tone::Neutral,
        BillState::Posted => Tone::Success,
        BillState::Cancelled => Tone::Warning,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<BillSummary> {
        Owner::new().with(bills_grid)
    }

    /// Mirrors the database sort fields without adding a crate dependency.
    const SERVER_SORTS: &[&str] = &["number", "supplier", "reference", "bill_date", "due_on", "order", "net", "variance"];

    /// Mirrors the database search fields.
    const SERVER_SEARCHES: &[&str] = &["number", "supplier", "reference", "order"];

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

        assert_eq!(sort, Sort::descending("bill_date"));
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
        assert_eq!(range.key(), "billed");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_offered_is_one_the_reader_parses_back() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                BillState::parse(choice.value).is_some(),
                "{} is offered and cannot be read back",
                choice.value,
            );
        }
    }
}
