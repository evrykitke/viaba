//! Books' front page: how the workspace stands, and the way in to it.
//!
//! # The counts come from the list endpoint
//!
//! Not from a purpose-built `invoice_counts` server function. The invoices grid
//! already loads every row into the browser - see
//! [`crate::ui::table::config::invoices`] - so a workspace at a scale where
//! that is fine is one where this is fine too, and a second endpoint would be a
//! second thing to keep in step with the first.
//!
//! When the grid needs a server-side source, so will this, and they should get
//! one together.
//!
//! # The money figures are worked out on the server, whole
//!
//! [`crate::components::app_home`] leaves money off a front page for two
//! reasons, and the ledger answers both. A figure that reads the clock differs
//! between the server's render and the browser's, so `ledger_summary` takes
//! the date on the server and sends the answer rather than the ingredients.
//! And a total across several currencies is either wrong or needs a rate for a
//! date - so these are not totals across currencies: every journal line
//! recorded what it was worth in the workspace's own currency at the moment it
//! was posted, and that is the column being added.

use leptos::prelude::*;
use phonix_core::apps;
use phonix_core::i18n::Message;
use phonix_core::permissions;

use crate::components::app_home::{AppHome, Shortcut, Stat};
use crate::i18n::t;
use crate::icons::Icon;
use crate::server_fns::books_fns::{InvoiceQuery, ledger_summary, list_invoices};

#[component]
pub fn sales_home_page() -> impl IntoView {
    let invoices = Resource::new(
        || (),
        |()| async move { list_invoices(InvoiceQuery::default()).await.ok() },
    );

    // Absent rather than zero where the reader may not read reports, or where
    // Books has nothing in it yet. A front page that says "0.00" to somebody
    // who is simply not allowed to see the figure is telling them something
    // untrue.
    let summary = Resource::new(|| (), |()| async move { ledger_summary().await.ok() });

    let stats = Signal::derive(move || {
        let mut stats = Vec::new();

        if let Some(Some(rows)) = invoices.get() {
            let count = |wanted: &str| {
                rows.iter()
                    .filter(|row| row.status.as_str() == wanted)
                    .count()
            };

            // Counts of *states*, never of periods. A figure that reads the
            // clock can differ between the server's render and the browser's,
            // and near midnight that is a hydration mismatch.
            stats.push(Stat::new(
                t(&Message::new("books.status.draft")),
                count("draft"),
            ));
            stats.push(Stat::new(
                t(&Message::new("books.status.posted")),
                count("posted"),
            ));
        }

        if let Some(Some(summary)) = summary.get() {
            let money = |amount: phonix_core::money::Money| {
                format!("{} {}", amount.to_display_string(), summary.currency.code())
            };

            stats.push(Stat::new(
                t(&Message::new("books.home.owed")),
                money(summary.owed_by_customers),
            ));
            stats.push(Stat::new(
                t(&Message::new("books.home.revenue")),
                money(summary.revenue),
            ));
            stats.push(Stat::new(
                t(&Message::new("books.home.result")),
                money(summary.result),
            ));
            stats.push(Stat::new(
                t(&Message::new("books.home.assets")),
                money(summary.total_assets),
            ));
        }

        stats
    });

    #[allow(
        clippy::expect_used,
        reason = "the catalog is a compiled constant and a test asserts Books is in it"
    )]
    let app = apps::find(apps::BOOKS).expect("books is in the catalog");

    let shortcuts = vec![
        Shortcut::new(
            t(&Message::new("invoices.new")),
            t(&Message::new("books.home.new_detail")),
            "/sales/invoices/new",
            Icon::Plus,
        )
        .require(permissions::INVOICES_CREATE)
        .primary(),
        Shortcut::new(
            t(&Message::new("nav.invoices")),
            t(&Message::new("books.home.list_detail")),
            "/sales/invoices",
            Icon::FileText,
        )
        .require(permissions::INVOICES),
        // The four statements, as one way in. Each has its own screen and its
        // own menu entry; what belongs here is the door, and the trial balance
        // is the one somebody checking the books opens first.
        Shortcut::new(
            t(&Message::new("nav.reports")),
            t(&Message::new("books.home.reports_detail")),
            "/sales/reports/trial-balance",
            Icon::ChartColumn,
        )
        .require(permissions::REPORTS),
        // Books' own screens are only half of what somebody here needs: an
        // invoice cannot be raised without a customer, and the customer lives
        // in master data. Linking across the boundary is a *link*, which is
        // what the boundary permits - Books still holds no code of master's.
        Shortcut::new(
            t(&Message::new("nav.parties")),
            t(&Message::new("books.home.customers_detail")),
            "/master/parties",
            Icon::Users,
        )
        .require(permissions::PARTIES),
        // And the other half: what was agreed before the invoice was raised.
        // The order lives in Inventory, because it is about items and a
        // warehouse - see the selling section of `navigation::tree` - and this
        // is the door to it from the side that bills for it.
        Shortcut::new(
            t(&Message::new("nav.sales_orders")),
            t(&Message::new("books.home.orders_detail")),
            "/inventory/sales-orders",
            Icon::ScrollText,
        )
        .require(permissions::SALES_ORDERS),
    ];

    view! {
        <Suspense fallback=|| view! { <div class="h-48" /> }>
            <AppHome app=app stats=stats shortcuts=shortcuts.clone() />
        </Suspense>
    }
}
