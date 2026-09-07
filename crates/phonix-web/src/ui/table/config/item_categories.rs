//! The item categories grid: a tree, drawn as a table.
//!
//! The costing column is the point of the screen. It is the one setting in the
//! whole inventory app that decides what the workspace says its stock is
//! *worth*, and seeing it per row is how an accountant notices that raw
//! materials and finished goods are being costed two different ways.

use leptos::prelude::*;
use phonix_core::permissions;

use app_inventory::category::{CategorySummary, CostingMethod, Valuation};

use super::GridConfig;
use crate::components::page::{Badge, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::list_item_categories;
use crate::ui::table::{Align, Cell, Column, Filter, FilterChoice, RowAction, Source, ToolbarAction};

/// How each kind of stock is costed, valued and picked.
pub fn item_categories_grid() -> GridConfig<CategorySummary> {
    GridConfig::new("item-categories", Source::in_memory(list_item_categories))
        .searching(l!("categories.search"))
        .exports_as("item-categories")
        // No `sorted_by`: a default sort would flatten the tree on arrival.
        .min_width("sm:min-w-[48rem]")
        .empty(
            Icon::ListTree,
            l!("categories.empty.title"),
            l!("categories.empty.detail"),
        )
        .column(
            Column::new("name", l!("field.name"), |row: &CategorySummary| {
                Cell::text(&row.name)
            })
            .findable()
            .pinned()
            .essential()
            .render(|row| name_cell(row).into_any()),
        )
        .column(
            Column::new("path", l!("categories.path"), |row: &CategorySummary| {
                Cell::text(&row.code)
            })
            .findable()
            .hidden(),
        )
        .column(
            Column::new(
                "costing_method",
                l!("categories.costing"),
                |row: &CategorySummary| Cell::text(crate::i18n::t(&row.costing_method.label())),
            )
            .sortable()
            .essential()
            .render(|row| costing_cell(row).into_any()),
        )
        .column(
            Column::new(
                "valuation",
                l!("categories.valuation"),
                |row: &CategorySummary| Cell::text(crate::i18n::t(&row.valuation.label())),
            )
            .sortable()
            .render(|row| valuation_cell(row).into_any()),
        )
        .column(
            Column::new(
                "removal_strategy",
                l!("categories.removal"),
                |row: &CategorySummary| Cell::text(crate::i18n::t(&row.removal_strategy.label())),
            )
            .sortable()
            .class("text-xs text-content-muted"),
        )
        .column(
            Column::new(
                "item_count",
                l!("categories.items"),
                |row: &CategorySummary| Cell::number(row.item_count as f64),
            )
            .sortable()
            .align(Align::End)
            .class("font-mono text-xs"),
        )
        .filter(
            Filter::new("costing_method", l!("categories.costing"), costing_choices()).matching(
                |row: &CategorySummary, wanted| row.costing_method.as_str() == wanted,
            ),
        )
        .filter(
            Filter::new(
                "valuation",
                l!("categories.valuation"),
                vec![
                    FilterChoice::all(l!("common.all")),
                    FilterChoice::new("automated", l!("categories.valuation.automated")),
                    FilterChoice::new("manual", l!("categories.valuation.manual")),
                ],
            )
            .matching(|row: &CategorySummary, wanted| row.valuation.as_str() == wanted),
        )
        .toolbar(
            ToolbarAction::link(
                l!("categories.new"),
                Icon::Plus,
                "/inventory/categories/new",
            )
            .require(permissions::ITEM_CATEGORIES_MANAGE)
            .primary(),
        )
        .action(
            RowAction::link(l!("common.edit"), Icon::Pencil, |row: &CategorySummary| {
                format!("/inventory/categories/{}", row.id)
            })
            .require(permissions::ITEM_CATEGORIES_MANAGE),
        )
}

fn costing_choices() -> Vec<FilterChoice> {
    let mut choices = vec![FilterChoice::all(l!("common.all"))];

    choices.extend(
        CostingMethod::ALL
            .iter()
            .map(|method| FilterChoice::new(method.as_str(), crate::i18n::t(&method.label()))),
    );

    choices
}

fn name_cell(row: &CategorySummary) -> impl IntoView {
    let name = row.name.clone();
    let indent = format!("padding-left:{}rem", f64::from(row.depth.min(6)) * 0.75);

    view! { <div style=indent class="font-medium">{name}</div> }
}

/// The costing method, with the one that needs a person keeping it up to date
/// marked out.
///
/// A standard cost nobody reviews drifts from reality and the variance account
/// quietly absorbs the difference. The other two need no maintenance.
fn costing_cell(row: &CategorySummary) -> impl IntoView {
    let method = row.costing_method;
    let label = crate::i18n::t(&method.label());

    let tone = match method {
        CostingMethod::Standard => Tone::Warning,
        CostingMethod::Average | CostingMethod::Fifo => Tone::Brand,
    };

    view! { <Badge tone=tone label=label /> }
}

/// Automated or manual.
///
/// Manual is marked, not because it is wrong, but because it means the stock
/// account is right for one afternoon a month and the person reading this grid
/// should know which categories those are.
fn valuation_cell(row: &CategorySummary) -> impl IntoView {
    let valuation = row.valuation;
    let label = crate::i18n::t(&valuation.label());

    let tone = match valuation {
        Valuation::Automated => Tone::Success,
        Valuation::Manual => Tone::Neutral,
    };

    view! { <Badge tone=tone label=label /> }
}
