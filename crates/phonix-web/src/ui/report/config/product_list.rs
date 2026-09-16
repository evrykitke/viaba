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
use phonix_core::money::Money;
use phonix_core::permissions;
use phonix_core::query::Page;
use phonix_core::report::{BandKind, ChartKind, ExportFormat, Point, ReportKind, ReportTheme};

use crate::l;
use crate::ui::report::{Band, Field, Grouping, ReportDefinition};
use crate::ui::table::Cell;

/// How many items one run of the report draws.
///
/// Bounded on purpose, and generous enough that most workspaces are one run.
/// The viewer's page navigation is what will move between them - see the
/// paginator.
pub const ROWS_PER_RUN: u32 = 100;

/// What each category comes to, as the chart's points.
///
/// The same rows the table below it draws and the same grouping, so the
/// picture cannot say something the figures do not. Money becomes a number
/// here and nowhere else: a chart is a drawing, and a bar two pixels taller
/// than another is not an amount anybody is going to add up.
fn by_category(rows: &[ItemSummary]) -> Vec<Point> {
    let mut totals: Vec<(String, f64)> = Vec::new();

    for item in rows {
        let cost = item.cost.to_display_string().parse::<f64>().unwrap_or(0.0);

        match totals
            .iter_mut()
            .find(|(category, _)| *category == item.category_name)
        {
            Some((_, running)) => *running += cost,
            None => totals.push((item.category_name.clone(), cost)),
        }
    }

    totals
        .into_iter()
        .map(|(category, total)| Point::new(category, vec![total]))
        .collect()
}

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
    .exports(ExportFormat::Pdf)
    .exports(ExportFormat::Csv)
    .band(Band::new(BandKind::ReportHeader))
    // Repeated at the top of every page, so a torn-off sheet still says
    // what it is.
    .band(Band::new(BandKind::PageHeader).field(Field::text("name", l!("items.title"))))
    .band(Band::grouped(
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
        // By category, which is the one column a reader of this list already
        // groups by in their head.
        Grouping::by(|item: &ItemSummary| item.category_name.clone())
            .totalling("cost", |item: &ItemSummary| item.cost),
    ))
    // Under the rows rather than over them: the chart is what the list adds
    // up to, and a reader meets it after the thing it summarises.
    .band(Band::chart(
        BandKind::GroupFooter,
        ChartKind::Column,
        vec![l!("items.cost")],
        |page: &Page<ItemSummary>| by_category(&page.rows),
    ))
    .band(
        Band::new(BandKind::ReportFooter)
            .field(Field::bare("count", |page: &Page<ItemSummary>| {
                Cell::text(l!(
                    "reports.showing",
                    shown = page.rows.len(),
                    total = page.total
                ))
            }))
            // What the subtotals come to, over the rows this run drew rather
            // than over the ones it did not.
            .field(Field::figure(
                "cost",
                l!("reports.total"),
                |page: &Page<ItemSummary>| {
                    let currency = page.rows.first().map(|item| item.cost.currency());

                    currency
                        .and_then(|currency| {
                            Money::total(currency, page.rows.iter().map(|item| item.cost)).ok()
                        })
                        .map_or(Cell::Empty, |total| Cell::text(total.to_display_string()))
                },
            )),
    )
}
