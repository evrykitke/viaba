//! What has been quoted and agreed, and how much of it has gone out.
//!
//! # Three states, side by side
//!
//! An order's own state - draft, quoted, confirmed - is what this workspace
//! did to it. The other two are what happened afterwards, worked out from the
//! lines rather than stored: how much has shipped, and how much has been
//! billed. Showing all three is the point of the screen. A confirmed order with
//! nothing shipped is a despatch waiting to happen; one fully shipped and
//! nothing billed is an invoice waiting to be raised, and it is the second that
//! quietly costs money.
//!
//! # Paged
//!
//! Nothing deletes from this list - a quotation nobody accepted is still what
//! was quoted - so it grows for as long as the workspace trades. It is a
//! [`Source::paged`], and what follows is what [`audit`](super::audit) sets
//! out: only columns the reader can order by are sortable, only columns it
//! searches are searchable, and the three filters and the span carry a key
//! rather than a closure. The lists live in
//! `phonix_db::inventory::sales_order` and are checked against this file below.

use app_inventory::sales_order::{Progress, SaleState, SaleSummary};
use leptos::prelude::*;
use phonix_core::permissions;
use phonix_core::query::Sort;

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_sales_orders;
use crate::ui::table::{
    Align, Cell, Column, DateFilter, Filter, FilterChoice, RowAction, Source, ToolbarAction,
};

pub fn sales_orders_grid() -> GridConfig<SaleSummary> {
    GridConfig::new("sales-orders", Source::paged(list_sales_orders))
        .searching(l!("sales_orders.search"))
        .exports_as("sales-orders")
        .sorted_by(Sort::descending("order_date"))
        .min_width("sm:min-w-[60rem]")
        .empty(
            Icon::ScrollText,
            l!("sales_orders.empty.title"),
            l!("sales_orders.empty.detail"),
        )
        .column(
            Column::new("number", l!("field.number"), |row: &SaleSummary| {
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
                "customer",
                l!("sales_orders.customer"),
                |row: &SaleSummary| Cell::text(&row.customer_name),
            )
            .searchable()
            .sortable()
            .essential(),
        )
        .column(
            Column::new(
                "order_date",
                l!("sales_orders.ordered"),
                |row: &SaleSummary| Cell::text(row.order_date.to_string()),
            )
            .sortable()
            .essential()
            .class("tabular-nums"),
        )
        .column(
            Column::new(
                "promised_on",
                l!("sales_orders.promised"),
                |row: &SaleSummary| {
                    Cell::text(
                        row.promised_on
                            .map(|date| date.to_string())
                            .unwrap_or_default(),
                    )
                },
            )
            .sortable()
            .class("tabular-nums text-content-muted"),
        )
        .column(
            Column::new("state", l!("field.status"), |row: &SaleSummary| {
                Cell::text(state_label(row.state))
            })
            .essential()
            .render(|row| {
                view! { <Badge label=state_label(row.state) tone=state_tone(row.state) /> }
                    .into_any()
            }),
        )
        .column(
            Column::new(
                "delivered",
                l!("sales_orders.delivered"),
                |row: &SaleSummary| Cell::text(delivered_label(row.delivery_state)),
            )
            .essential()
            .render(|row| {
                view! {
                    <Badge
                        label=delivered_label(row.delivery_state)
                        tone=progress_tone(row.delivery_state)
                    />
                }
                .into_any()
            }),
        )
        .column(
            Column::new(
                "invoiced",
                l!("sales_orders.invoiced"),
                |row: &SaleSummary| Cell::text(invoiced_label(row.invoice_state)),
            )
            .render(|row| {
                view! {
                    <Badge
                        label=invoiced_label(row.invoice_state)
                        tone=progress_tone(row.invoice_state)
                    />
                }
                .into_any()
            }),
        )
        .column(
            Column::new("net", l!("sales_orders.net"), |row: &SaleSummary| {
                Cell::number(row.net.scaled() as f64)
            })
            .sortable()
            .essential()
            .align(Align::End)
            .render(|row| {
                let text = format!("{} {}", row.net.to_display_string(), row.currency);
                view! { <span class="tabular-nums">{text}</span> }.into_any()
            }),
        )
        .column(
            Column::new("warehouse", l!("nav.warehouses"), |row: &SaleSummary| {
                Cell::text(&row.warehouse_name)
            })
            .searchable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new("line_count", l!("sales_orders.lines"), |row: &SaleSummary| {
                Cell::number(row.line_count as f64)
            })
            .sortable()
            .align(Align::End)
            .class("tabular-nums text-content-muted"),
        )
        // The values are groups rather than states, and the grouping is
        // `SaleState::group` - so the reader turns each back into the states it
        // covers rather than the list being written out twice. A quotation is
        // its own group here and a purchase order's is not, because "what have
        // we quoted that nobody has said yes to" is a list somebody chases.
        .filter(Filter::new(
            "state",
            l!("field.status"),
            vec![
                FilterChoice::all(l!("common.all")),
                FilterChoice::new("open", l!("sales_orders.state.confirmed")),
                FilterChoice::new("quoted", l!("sales_orders.state.sent")),
                FilterChoice::new("draft", l!("sales_orders.state.draft")),
                FilterChoice::new("done", l!("sales_orders.state.done")),
                FilterChoice::new("cancelled", l!("sales_orders.state.cancelled")),
            ],
        ))
        .filter(
            // The despatch list, as a filter. "Confirmed and not fully shipped"
            // is the question a warehouse asks every morning, and it is now one
            // the database answers rather than one the browser answers over
            // whatever it happened to have fetched.
            Filter::new(
                "delivered",
                l!("sales_orders.delivered"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("outstanding", l!("sales_orders.delivered.partly")),
                    FilterChoice::new("complete", l!("sales_orders.delivered.everything")),
                ],
            ),
        )
        .filter(
            // And the one the sales ledger asks: what has gone out and has not
            // been billed for.
            Filter::new(
                "invoiced",
                l!("sales_orders.invoiced"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("outstanding", l!("sales_orders.invoiced.partly")),
                    FilterChoice::new("complete", l!("sales_orders.invoiced.everything")),
                ],
            ),
        )
        .date_filter(DateFilter::new("ordered", l!("sales_orders.ordered")))
        .toolbar(
            ToolbarAction::link(
                l!("sales_orders.new"),
                Icon::Plus,
                "/inventory/sales-orders/new",
            )
            .require(permissions::SALES_ORDERS_CREATE)
            .primary(),
        )
        .action(
            RowAction::link(l!("common.open"), Icon::ArrowRight, |row: &SaleSummary| {
                format!("/inventory/sales-orders/{}", row.id)
            })
            .require(permissions::SALES_ORDERS),
        )
}

/// The number, or what a draft is recognisable by before it has one.
fn number_cell(row: &SaleSummary) -> impl IntoView {
    if row.number.is_empty() {
        return view! {
            <span class="text-xs italic text-content-muted">
                {l!("sales_orders.state.draft")}
            </span>
        }
        .into_any();
    }

    let number = row.number.clone();
    view! { <span class="font-mono tabular-nums">{number}</span> }.into_any()
}

fn state_label(state: SaleState) -> String {
    match state {
        SaleState::Draft => l!("sales_orders.state.draft"),
        SaleState::Sent => l!("sales_orders.state.sent"),
        SaleState::Confirmed => l!("sales_orders.state.confirmed"),
        SaleState::Done => l!("sales_orders.state.done"),
        SaleState::Cancelled => l!("sales_orders.state.cancelled"),
    }
}

const fn state_tone(state: SaleState) -> Tone {
    match state {
        SaleState::Confirmed => Tone::Brand,
        SaleState::Done => Tone::Success,
        SaleState::Cancelled => Tone::Danger,
        SaleState::Draft | SaleState::Sent => Tone::Neutral,
    }
}

fn delivered_label(progress: Progress) -> String {
    match progress {
        Progress::Nothing => l!("sales_orders.delivered.nothing"),
        Progress::Partly => l!("sales_orders.delivered.partly"),
        Progress::Everything => l!("sales_orders.delivered.everything"),
        Progress::Over => l!("sales_orders.delivered.over"),
    }
}

fn invoiced_label(progress: Progress) -> String {
    match progress {
        Progress::Nothing => l!("sales_orders.invoiced.nothing"),
        Progress::Partly => l!("sales_orders.invoiced.partly"),
        Progress::Everything => l!("sales_orders.invoiced.everything"),
        Progress::Over => l!("sales_orders.invoiced.over"),
    }
}

/// Over is a warning rather than a success: it is not wrong, and somebody
/// should look at it.
const fn progress_tone(progress: Progress) -> Tone {
    match progress {
        Progress::Nothing => Tone::Neutral,
        Progress::Partly => Tone::Brand,
        Progress::Everything => Tone::Success,
        Progress::Over => Tone::Warning,
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::Owner;

    use super::*;

    fn grid() -> GridConfig<SaleSummary> {
        Owner::new().with(sales_orders_grid)
    }

    /// Written as literals rather than imported: `phonix-web` does not depend
    /// on `phonix-db`, and the point of the test is that the two lists were
    /// written to agree. The source is
    /// `phonix_db::inventory::sales_order::SORTABLE`.
    const SERVER_SORTS: &[&str] = &[
        "number",
        "customer",
        "order_date",
        "promised_on",
        "net",
        "line_count",
    ];

    /// The columns the `WHERE` actually looks inside. Same reasoning.
    const SERVER_SEARCHES: &[&str] = &["number", "customer", "warehouse"];

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
    fn it_opens_newest_first_by_a_column_the_server_can_order_by() {
        let sort = grid().initial_request().sort.expect("an opening order");

        assert_eq!(sort, Sort::descending("order_date"));
        assert!(SERVER_SORTS.contains(&sort.field.as_str()));
    }

    #[test]
    fn the_filters_and_the_span_leave_the_answering_to_the_server() {
        let grid = grid();

        for filter in &grid.filters {
            assert!(
                !filter.is_local(),
                "{} is answered in the wrong place",
                filter.key()
            );
            assert_eq!(filter.default_value(), "");
        }

        let range = grid.date_filters.first().expect("the grid offers a span");

        // `phonix_db::inventory::sales_order::ORDERED`.
        assert_eq!(range.key(), "ordered");
        assert!(!range.is_local());
    }

    #[test]
    fn every_state_group_offered_covers_at_least_one_state() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for choice in states.choices.iter().filter(|c| !c.value.is_empty()) {
            assert!(
                !SaleState::in_group(choice.value).is_empty(),
                "{} is offered and covers nothing",
                choice.value,
            );
        }
    }

    #[test]
    fn every_state_is_offered_under_some_group() {
        let grid = grid();
        let states = grid.filters.iter().find(|f| f.key() == "state").unwrap();

        for state in SaleState::ALL {
            assert!(
                states.choices.iter().any(|c| c.value == state.group()),
                "{} is in no group the grid offers",
                state.as_str(),
            );
        }
    }

    #[test]
    fn both_progress_filters_offer_the_two_words_the_reader_answers() {
        let grid = grid();

        for key in ["delivered", "invoiced"] {
            let filter = grid.filters.iter().find(|f| f.key() == key).unwrap();

            let offered: Vec<&str> = filter
                .choices
                .iter()
                .map(|choice| choice.value)
                .filter(|value| !value.is_empty())
                .collect();

            // `progress_names` in the reader matches these two and treats
            // anything else as unfiltered.
            assert_eq!(offered, ["outstanding", "complete"], "{key}");
        }
    }
}
