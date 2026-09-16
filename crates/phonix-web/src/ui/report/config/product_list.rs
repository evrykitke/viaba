//! Every item this workspace sells or stocks, as a list to read on paper.
//!
//! The first report of the *list* kind: many rows under a heading that repeats
//! rather than one record dressed as a document. It is not a document type, so
//! no workspace setting reaches it - the paper a product list is printed on is
//! not something a tenant has an opinion about.
//!
//! # It draws a page of the read, not the read
//!
//! The data is a [`Page`] from the item list that already exists, so the
//! report holds a hundred rows and knows how many there are rather than
//! holding all of them. Reading a list whole to print it is the hazard this
//! backlog has four commits about.

use app_inventory::item::ItemSummary;
use phonix_core::permissions;
use phonix_core::query::Page;
use phonix_core::report::{BandKind, ExportFormat, ReportKind, ReportTheme};

use crate::l;
use crate::ui::report::{Band, Field, ReportDefinition};
use crate::ui::table::Cell;

/// How many items one run of the report draws.
///
/// Bounded on purpose, and generous enough that most workspaces are one run.
/// The viewer's page navigation is what will move between them - see the
/// paginator.
pub const ROWS_PER_RUN: u32 = 100;

/// The product list.
pub fn product_list() -> ReportDefinition<Page<ItemSummary>> {
    ReportDefinition::new(
        "product-list",
        permissions::ITEMS,
        l!("items.title"),
        ReportKind::List,
    )
    // The dense look: a list is read for how many rows reach a page.
    .theme(ReportTheme::Compact)
    .exports(ExportFormat::Csv)
    .band(Band::new(BandKind::ReportHeader))
    // Repeated at the top of every page, so a torn-off sheet still says
    // what it is.
    .band(Band::new(BandKind::PageHeader).field(Field::text("name", l!("items.title"))))
    .band(Band::lines(
        |page: &Page<ItemSummary>| page.rows.clone(),
        vec![
            Field::new("code", l!("field.code"), |item: &ItemSummary| {
                Cell::text(&item.code)
            }),
            Field::new("name", l!("field.name"), |item: &ItemSummary| {
                Cell::text(&item.name)
            }),
            Field::new("category", l!("items.category"), |item: &ItemSummary| {
                Cell::text(&item.category_name)
            }),
            Field::new("unit", l!("items.unit"), |item: &ItemSummary| {
                Cell::text(&item.stock_unit_code)
            }),
            Field::figure("cost", l!("items.cost"), |item: &ItemSummary| {
                Cell::text(item.cost.to_display_string())
            }),
            // An item that is not tracked has no on-hand figure, which is
            // a different thing from having none of it.
            Field::figure("on_hand", l!("items.on_hand"), |item: &ItemSummary| {
                item.on_hand
                    .map_or(Cell::Empty, |held| Cell::text(held.to_string()))
            }),
        ],
    ))
    .band(Band::new(BandKind::ReportFooter).field(Field::bare(
        "count",
        |page: &Page<ItemSummary>| {
            Cell::text(l!(
                "reports.showing",
                shown = page.rows.len(),
                total = page.total
            ))
        },
    )))
}
