//! The menu itself.
//!
//! # Adding a screen
//!
//! One node, in one place:
//!
//! ```ignore
//! NavNode::leaf("users", "nav.users", Icon::Users, "/admin/users")
//!     .require(names::USERS)
//!     .keywords(&["people", "accounts", "staff"])
//! ```
//!
//! The sidebar, the command palette and the breadcrumb all pick it up from
//! there. Three rules the tests enforce, so they are worth knowing before the
//! test tells you:
//!
//! * `key` is unique across the whole tree - expansion state is keyed by it.
//! * `href` is absolute, because the sidebar renders the same link from every
//!   route.
//! * `require` names a constant from [`phonix_core::authorization::names`], not
//!   a literal. A permission this build does not define fails the test rather
//!   than quietly hiding a menu nobody can explain the absence of.
//!
//! # Adding a module
//!
//! An inventory module is a group under a new top-level node, and the
//! permissions it names have to exist in `phonix-core` first - the definition
//! tree there is the source of truth, this is a view of it. Declare
//! `Pages.Inventory`, `Pages.Inventory.Items`, `Pages.Inventory.Requisitions`
//! and so on in `authorization::definitions`, then:
//!
//! ```ignore
//! NavNode::group("inventory", "nav.inventory", Icon::Boxes, &[
//!     NavNode::leaf("items", "nav.items", Icon::Package, "/inventory/items")
//!         .require(names::INVENTORY_ITEMS),
//! ])
//! .require(names::INVENTORY)
//! ```
//!
//! Depth is free: a group inside a group inside a group highlights and expands
//! the same way, because [`Trail`](super::Trail) walks the tree rather than
//! knowing how tall it is.

use phonix_core::authorization::names;

use super::NavNode;
use crate::icons::Icon;

/// Where a finished sign-in lands, and the first entry in the menu.
pub const DASHBOARD: &str = "/dashboard";

/// The workspace menu, top to bottom.
///
/// Order here is order on screen. Nothing sorts it: the sequence is a design
/// decision - what people reach for most, first - and an alphabetical sidebar
/// would bury the dashboard under "Audit logs".
pub static MENU: &[NavNode] = &[
    NavNode::leaf(
        "dashboard",
        "nav.dashboard",
        Icon::LayoutDashboard,
        DASHBOARD,
    )
    .require(names::DASHBOARD)
    .keywords(&["home", "overview", "start"]),
    // Sales before master data: raising an invoice is the daily work, and
    // keeping the customer list tidy is what somebody does on the way to it.
    NavNode::group(
        "sales",
        "nav.sales",
        Icon::ShoppingCart,
        &[
            // First in each app's group: the group heading opens and closes,
            // it does not navigate, so without this an app's own front page
            // is reachable from the launcher and from nowhere in the menu.
            NavNode::leaf("sales-overview", "nav.overview", Icon::LayoutGrid, "/sales")
                .require(names::SALES)
                .keywords(&["books", "home", "start"]),
            NavNode::leaf(
                "invoices",
                "nav.invoices",
                Icon::FileText,
                "/sales/invoices",
            )
            .require(names::INVOICES)
            .keywords(&["bill", "billing", "receivable", "sales", "customer"]),
            NavNode::leaf(
                "accounts",
                "nav.accounts",
                Icon::ListTree,
                "/sales/accounts",
            )
            .require(names::ACCOUNTS)
            .keywords(&["chart", "ledger", "gl", "nominal", "coa"]),
            NavNode::leaf(
                "journals",
                "nav.journals",
                Icon::ScrollText,
                "/sales/journals",
            )
            .require(names::JOURNALS)
            .keywords(&["ledger", "gl", "posting", "entry", "double entry"]),
            NavNode::leaf("periods", "nav.periods", Icon::Calendar, "/sales/periods")
                .require(names::PERIODS)
                .keywords(&["close", "month end", "year end", "calendar", "lock"]),
        ],
    )
    .require(names::SALES),
    NavNode::group(
        "master",
        "nav.master",
        Icon::Boxes,
        &[
            NavNode::leaf(
                "master-overview",
                "nav.overview",
                Icon::LayoutGrid,
                "/master",
            )
            .require(names::MASTER)
            .keywords(&["home", "start"]),
            NavNode::leaf("parties", "nav.parties", Icon::Users, "/master/parties")
                .require(names::PARTIES)
                .keywords(&["customers", "suppliers", "clients", "vendors", "contacts"]),
            NavNode::leaf("taxes", "nav.taxes", Icon::Receipt, "/master/taxes")
                .require(names::TAXES)
                .keywords(&["vat", "gst", "sales tax", "rates", "groups"]),
        ],
    )
    .require(names::MASTER),
    // Inventory, between master data and people: an item list is read by more
    // of a workspace than a department list is, and by fewer people than the
    // customer list.
    NavNode::group(
        "inventory",
        "nav.inventory",
        Icon::Warehouse,
        &[
            NavNode::leaf(
                "inventory-overview",
                "nav.overview",
                Icon::LayoutGrid,
                "/inventory",
            )
            .require(names::INVENTORY)
            .keywords(&["stock", "home", "start"]),
            NavNode::leaf("items", "nav.items", Icon::Package, "/inventory/items")
                .require(names::ITEMS)
                // "Product" and "SKU" are what half the world calls these, and
                // "barcode" is what somebody holding a scanner searches for.
                .keywords(&[
                    "products", "sku", "goods", "stock", "barcode", "upc", "variants",
                ]),
            NavNode::leaf(
                "item-categories",
                "nav.item_categories",
                Icon::ListTree,
                "/inventory/categories",
            )
            .require(names::ITEM_CATEGORIES)
            // The costing method lives on the category, and somebody looking
            // for "FIFO" is looking for this screen without knowing its name.
            .keywords(&[
                "costing", "valuation", "fifo", "average", "standard cost", "removal",
            ]),
            NavNode::leaf(
                "warehouses",
                "nav.warehouses",
                Icon::Warehouse,
                "/inventory/warehouses",
            )
            .require(names::WAREHOUSES)
            .keywords(&["depot", "site", "building", "receiving", "shipping", "steps"]),
            // Buying sits above stock: an order is raised before the goods
            // it brings can be counted, and a buyer opens these two far more
            // often than the location tree they arranged once in March.
            NavNode::leaf(
                "purchase-orders",
                "nav.purchase_orders",
                Icon::ScrollText,
                "/inventory/orders",
            )
            .require(names::PURCHASE_ORDERS)
            .keywords(&["po", "buying", "procurement", "supplier", "vendor", "order"]),
            NavNode::leaf("receipts", "nav.receipts", Icon::Package, "/inventory/receipts")
                .require(names::RECEIPTS)
                .keywords(&[
                    "goods in", "grn", "delivery note", "receiving", "incoming", "backorder",
                ]),
            // Stock sits above the setup screens: what is on the shelf is
            // what somebody opens this app to find out, and the location tree
            // is what they arranged once in March.
            NavNode::leaf("stock", "nav.stock", Icon::Boxes, "/inventory/stock")
                .require(names::STOCK)
                .keywords(&[
                    "on hand", "quantity", "availability", "quants", "count", "lots",
                ]),
            NavNode::leaf("stock-moves", "nav.stock_moves", Icon::ArrowRight, "/inventory/moves")
                .require(names::STOCK)
                .keywords(&[
                    "movements", "history", "receipts", "deliveries", "adjustments", "stock card",
                ]),
            NavNode::leaf(
                "stock-locations",
                "nav.stock_locations",
                Icon::Boxes,
                "/inventory/locations",
            )
            .require(names::STOCK_LOCATIONS)
            .keywords(&["bin", "shelf", "zone", "aisle", "transit", "inventory loss"]),
            NavNode::leaf("units", "nav.units", Icon::Ruler, "/inventory/units")
                .require(names::UNITS)
                .keywords(&["uom", "measure", "kilogram", "litre", "each", "conversion"]),
        ],
    )
    .require(names::INVENTORY),
    // People, after master data and before administration: arranging the
    // company is closer to keeping the customer list tidy than it is to
    // managing user accounts, and somebody looking for departments looks in
    // neither of the other two.
    NavNode::group(
        "people",
        "nav.hr",
        Icon::Building2,
        &[
            NavNode::leaf(
                "people-overview",
                "nav.overview",
                Icon::LayoutGrid,
                "/people",
            )
            .require(names::PEOPLE)
            .keywords(&["hr", "home", "start"]),
            NavNode::leaf(
                "departments",
                "nav.departments",
                Icon::Building2,
                "/people/departments",
            )
            .require(names::DEPARTMENTS)
            // "Cost centre" is here rather than as a screen of its own: a cost
            // centre is a department with a flag, and somebody searching the
            // palette for one should land on the grid that has the filter.
            .keywords(&[
                "cost centre",
                "cost center",
                "division",
                "team",
                "org",
                "structure",
            ]),
        ],
    )
    .require(names::PEOPLE),
    NavNode::group(
        "administration",
        "nav.administration",
        Icon::SlidersHorizontal,
        &[
            NavNode::leaf("users", "nav.users", Icon::Users, "/admin/users")
                .require(names::USERS)
                .keywords(&["people", "accounts", "staff", "members", "invite"]),
            NavNode::leaf("roles", "nav.roles", Icon::ShieldCheck, "/admin/roles")
                .require(names::ROLES)
                .keywords(&["permissions", "access", "groups"]),
            NavNode::leaf(
                "settings",
                "nav.settings",
                Icon::Settings,
                "/admin/settings",
            )
            .require(names::SETTINGS)
            .keywords(&["workspace", "configuration", "preferences", "tenant"]),
            NavNode::leaf("apps", "nav.apps", Icon::Blocks, "/admin/apps")
                .require(names::APPS)
                .keywords(&["install", "modules", "subscription", "books", "store"]),
            NavNode::leaf(
                "api-keys",
                "nav.api_keys",
                Icon::KeySquare,
                "/admin/api-keys",
            )
            .require(names::API_KEYS)
            .keywords(&["api", "integration", "token", "credentials", "developer"]),
            NavNode::leaf(
                "audit-logs",
                "nav.audit_logs",
                Icon::ScrollText,
                "/admin/audit-logs",
            )
            .require(names::AUDIT_LOGS)
            .keywords(&["history", "activity", "trail", "who did what"]),
            // Last, and gated on a permission nobody is given by default: it
            // is a developer reference that happens to live in the shipped
            // binary, not something a workspace bought.
            NavNode::leaf("ui-library", "nav.ui_library", Icon::Palette, "/admin/ui")
                .require(names::UI_LIBRARY)
                .keywords(&["components", "kit", "design", "showcase", "widgets"]),
        ],
    )
    .require(names::ADMINISTRATION),
];
