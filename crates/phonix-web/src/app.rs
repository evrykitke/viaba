//! Application shell and routing.

use leptos::prelude::*;
use leptos_meta::{HashedStylesheet, Meta, MetaTags, Title, provide_meta_context};
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

use crate::components::layout::Layout;
use crate::components::user_link::{OpenCard, UserCardLayer};
use crate::i18n::{self, Locale};
use crate::pages::account::AccountPage;
use crate::pages::admin::api_key_new::ApiKeyNewPage;
use crate::pages::admin::api_keys::ApiKeysPage;
use crate::pages::admin::apps::AppsPage;
use crate::pages::admin::audit_event::AuditEventPage;
use crate::pages::admin::audit_logs::AuditLogsPage;
use crate::pages::admin::entity_change::EntityChangePage;
use crate::pages::admin::roles::{RoleNewPage, RolePage, RolesPage};
use crate::pages::admin::settings::SettingsPage;
use crate::pages::admin::ui_library::UiLibraryPage;
use crate::pages::admin::user_edit::UserEditPage;
use crate::pages::admin::user_invite::UserInvitePage;
use crate::pages::admin::user_permissions::UserPermissionsPage;
use crate::pages::admin::users::UsersPage;
use crate::pages::auth::{
    AcceptInvitationPage, ChallengePage, ForgotPasswordPage, SignInPage, SignUpPage,
};
use crate::pages::inventory::home::InventoryHomePage;
use crate::pages::inventory::item::ItemPage;
use crate::pages::inventory::item_categories::{
    ItemCategoriesPage, ItemCategoryNewPage, ItemCategoryPage,
};
use crate::pages::inventory::items::{ItemNewPage, ItemsPage};
use crate::pages::inventory::consolidation::{ConsolidationNewPage, ConsolidationPage};
use crate::pages::inventory::procurement::{
    ConsolidationsPage, PurchaseOrdersPage, ReceiptsPage, RequisitionsPage,
};
use crate::pages::inventory::bill::{BillNewPage, BillPage, BillsPage, UnbilledPage};
use crate::pages::inventory::purchase_order::{PurchaseOrderNewPage, PurchaseOrderPage};
use crate::pages::inventory::requisition::{RequisitionNewPage, RequisitionPage};
use crate::pages::inventory::receipt::{ReceiptNewPage, ReceiptPage};
use crate::pages::inventory::stock::{StockMovesPage, StockPage};
use crate::pages::inventory::stock_locations::{
    StockLocationNewPage, StockLocationPage, StockLocationsPage,
};
use crate::pages::inventory::units::{UnitNewPage, UnitPage, UnitsPage};
use crate::pages::inventory::warehouses::{WarehouseNewPage, WarehousePage, WarehousesPage};
use crate::pages::master::home::MasterHomePage;
use crate::pages::master::parties::{PartiesPage, PartyNewPage};
use crate::pages::master::party::PartyPage;
use crate::pages::master::tax::TaxPage;
use crate::pages::master::tax_group::{TaxGroupNewPage, TaxGroupPage};
use crate::pages::master::taxes::{TaxNewPage, TaxesPage};
use crate::pages::people::department::DepartmentPage;
use crate::pages::people::departments::{DepartmentNewPage, DepartmentsPage};
use crate::pages::people::home::PeopleHomePage;
// Aliased: `pages::account` is the viewer's own profile, and both are called
// AccountPage in their own module.
use crate::pages::sales::account::AccountPage as ChartAccountPage;
use crate::pages::sales::accounts::{AccountNewPage, AccountsPage};
use crate::pages::sales::home::SalesHomePage;
use crate::pages::sales::journal::JournalPage;
use crate::pages::sales::journal_new::JournalNewPage;
use crate::pages::sales::journals::JournalsPage;
use crate::pages::sales::periods::PeriodsPage;
use crate::pages::sales::invoice::{InvoiceNewPage, InvoicePage};
use crate::pages::sales::invoices::InvoicesPage;
use crate::pages::{dashboard::DashboardPage, not_found::NotFoundPage};
use crate::profiler::ProfilerBridge;
use crate::theme::{Theme, ThemePreference};
use crate::ui::alert::{AlertLayer, Alerts};

/// The HTML document the server streams.
///
/// Leptos renders the whole document (not just `<body>`), so `<head>` content
/// set by `leptos_meta` anywhere in the tree lands in the real `<head>` during
/// SSR rather than being patched in after hydration.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    // Read here, on the very first element, rather than in a component further
    // down: the theme has to be on `<html>` in the bytes the server sends, or
    // the page paints light and flips to dark once the bundle boots. That flash
    // is the whole reason the preference lives in a cookie.
    //
    // `data-theme` is deliberately absent for "follow the system" - see
    // `ThemeMode::attribute`.
    let appearance = ThemePreference::from_request();

    // Resolved here, on `<html>`, for the same reason and at the same moment as
    // the theme: `lang` is what a screen reader picks a voice from and what
    // `:lang()` rules key off, and `dir` decides which way the whole page runs.
    // Both have to be in the bytes the server sends.
    //
    // It is also the language the browser half reads back during hydration, so
    // this attribute is not decoration - it is the handover.
    let catalog = i18n::current_catalog();
    let language = catalog.language();
    let overlay = i18n::overlay_json(&catalog);

    // Stamped rather than recomputed in the browser, for the same reason the
    // language is. Coverage is a fraction of `BUILTIN`, and `BUILTIN` lives in
    // two separately compiled binaries - the server's and the bundle's. They
    // are the same in a release and routinely differ for a few seconds under
    // `cargo leptos watch`, which is enough for the two halves to disagree
    // about whether the "partly translated" note exists. A node that is there
    // on one side and not the other is the fatal kind of hydration mismatch.
    let coverage = catalog.coverage();

    view! {
        <!DOCTYPE html>
        <html
            lang=language.code()
            dir=language.direction().attribute()
            class="h-full"
            data-theme=appearance.mode.attribute()
            data-accent=appearance.accent.key()
            data-i18n-coverage=coverage.to_string()
        >
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                // Injects the hashed CSS filename so a deploy cannot serve a
                // stale stylesheet from cache.
                <HashedStylesheet options=options.clone() id="leptos" />
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() />
                <MetaTags />

                // The words this document was rendered with, for the bundle to
                // hydrate against. `{}` when the page is in English, because
                // the English catalog is compiled into the bundle already.
                //
                // Before `HydrationScripts` would be tidier; it is here instead
                // because it must not delay them, and a parser reaching this
                // has already started fetching the wasm.
                <script type="application/json" id=i18n::CATALOG_ELEMENT_ID inner_html=overlay />
            </head>
            <body class="h-full bg-surface text-content antialiased">
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn app() -> impl IntoView {
    provide_meta_context();

    // Seeded from the same cookie the document was rendered with, so the
    // appearance menu opens showing what is actually on screen.
    Theme::provide(ThemePreference::from_request());

    // Above everything, because everything says something. On the server this
    // resolves from the request; in the browser it reads back what the server
    // decided, so the two halves cannot render different words.
    Locale::provide(i18n::current_catalog());

    // Above the router, so an alert raised by a save that then navigates is not
    // unmounted by its own success. See `ui::alert::host`.
    Alerts::provide();
    // Beside the alerts, and mounted at the root for the same reason: the card
    // is opened from inside a scrolling table and must not be drawn there.
    OpenCard::provide();

    view! {
        <Title text="Phonix" />
        <Meta name="description" content="Phonix" />
        <Meta name="color-scheme" content="light dark" />

        <AlertLayer />
        <UserCardLayer />

        <Router>
            // Renders nothing. Inside the router because that is where the
            // location is readable, and the development profiler's toolbar has
            // no other way to know the route once hydration has taken over.
            <ProfilerBridge />

            <Routes fallback=NotFoundPage>
                // Every screen hangs off one parent route rather than being
                // wrapped in `<Layout>`: see `components::layout` for what that
                // wrapping costs. The path is empty, so it adds no segment.
                <ParentRoute path=path!("") view=Layout>
                    // Signed out, "/" is the sign-in screen. Once a session
                    // exists the dashboard is at "/dashboard" and the handoff
                    // endpoint sends the browser straight there.
                    <Route path=path!("/") view=SignInPage />
                    <Route path=path!("/signup") view=SignUpPage />
                    // Public, like the two above: an invitation is followed by
                    // somebody who has no session yet.
                    <Route path=path!("/invitations/accept") view=AcceptInvitationPage />
                    // Public for the plainest reason of the four: whoever is
                    // here has forgotten the password a session is made from.
                    // `landing` has to agree - see PASSWORD_RESET_PATH.
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />

                    // Half-authenticated: the password was accepted and the
                    // second factor has not been. `LoginResult::next_path`
                    // sends the browser here, so this route has to exist
                    // before anybody is allowed to enrol a factor.
                    <Route path=path!("/auth/challenge") view=ChallengePage />

                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/account") view=AccountPage />

                    // Sales. The first app, and the first routes that are a
                    // product rather than infrastructure.
                    // The app's own front page, which is where the launcher
                    // and the store send anybody who picks Books.
                    <Route path=path!("/sales") view=SalesHomePage />
                    <Route path=path!("/sales/accounts") view=AccountsPage />
                    // Before `:id`, so "new" is a screen rather than an
                    // account id that fails to parse.
                    <Route path=path!("/sales/accounts/new") view=AccountNewPage />
                    <Route path=path!("/sales/accounts/:id") view=ChartAccountPage />
                    <Route path=path!("/sales/journals") view=JournalsPage />
                    // Before `:id`, so "new" is a screen rather than a journal
                    // id that fails to parse.
                    <Route path=path!("/sales/journals/new") view=JournalNewPage />
                    <Route path=path!("/sales/journals/:id") view=JournalPage />
                    <Route path=path!("/sales/periods") view=PeriodsPage />
                    <Route path=path!("/sales/invoices") view=InvoicesPage />
                    // Before the parameter, so "new" is a screen rather than an
                    // invoice id that fails to parse.
                    <Route path=path!("/sales/invoices/new") view=InvoiceNewPage />
                    // One address for both the editor and the document: posting
                    // does not move an invoice, it changes what may be done to
                    // it, and a link somebody sent last week should still open
                    // the thing they meant.
                    <Route path=path!("/sales/invoices/:id") view=InvoicePage />

                    // Master data. Not under /admin, for the reason the
                    // permission tree is not: keeping a customer list up to
                    // date is ordinary commercial work.
                    <Route path=path!("/master") view=MasterHomePage />
                    <Route path=path!("/master/parties") view=PartiesPage />
                    // Before the parameter, so "new" is a screen rather than a
                    // party id that fails to parse.
                    <Route path=path!("/master/parties/new") view=PartyNewPage />
                    <Route path=path!("/master/parties/:id") view=PartyPage />
                    // People. Also not under /admin, and for a sharper version
                    // of the same reason: a requisition names a cost centre
                    // before it names anything else, so the department list is
                    // something most of a workspace reads.
                    <Route path=path!("/people") view=PeopleHomePage />
                    <Route path=path!("/people/departments") view=DepartmentsPage />
                    // Before the parameter, so "new" is a screen rather than a
                    // department id that fails to parse.
                    <Route path=path!("/people/departments/new") view=DepartmentNewPage />
                    <Route path=path!("/people/departments/:id") view=DepartmentPage />

                    // Inventory. Every "new" sits before its parameter, so the
                    // word is a screen rather than an id that fails to parse.
                    <Route path=path!("/inventory") view=InventoryHomePage />
                    <Route path=path!("/inventory/items") view=ItemsPage />
                    <Route path=path!("/inventory/items/new") view=ItemNewPage />
                    <Route path=path!("/inventory/items/:id") view=ItemPage />
                    <Route path=path!("/inventory/categories") view=ItemCategoriesPage />
                    <Route path=path!("/inventory/categories/new") view=ItemCategoryNewPage />
                    <Route path=path!("/inventory/categories/:id") view=ItemCategoryPage />
                    <Route path=path!("/inventory/warehouses") view=WarehousesPage />
                    <Route path=path!("/inventory/warehouses/new") view=WarehouseNewPage />
                    <Route path=path!("/inventory/warehouses/:id") view=WarehousePage />
                    <Route path=path!("/inventory/locations") view=StockLocationsPage />
                    <Route path=path!("/inventory/locations/new") view=StockLocationNewPage />
                    <Route path=path!("/inventory/locations/:id") view=StockLocationPage />
                    // Before the orders, because that is the order of the
                    // chain: asked for, ordered, received, billed.
                    <Route path=path!("/inventory/requisitions") view=RequisitionsPage />
                    <Route path=path!("/inventory/requisitions/new") view=RequisitionNewPage />
                    <Route path=path!("/inventory/requisitions/:id") view=RequisitionPage />
                    <Route path=path!("/inventory/consolidations") view=ConsolidationsPage />
                    <Route
                        path=path!("/inventory/consolidations/new")
                        view=ConsolidationNewPage
                    />
                    <Route
                        path=path!("/inventory/consolidations/:id")
                        view=ConsolidationPage
                    />
                    <Route path=path!("/inventory/orders") view=PurchaseOrdersPage />
                    <Route path=path!("/inventory/orders/new") view=PurchaseOrderNewPage />
                    <Route path=path!("/inventory/orders/:id") view=PurchaseOrderPage />
                    <Route path=path!("/inventory/bills") view=BillsPage />
                    <Route path=path!("/inventory/bills/new") view=BillNewPage />
                    <Route path=path!("/inventory/bills/:id") view=BillPage />
                    <Route path=path!("/inventory/unbilled") view=UnbilledPage />
                    <Route path=path!("/inventory/receipts") view=ReceiptsPage />
                    <Route path=path!("/inventory/receipts/new") view=ReceiptNewPage />
                    <Route path=path!("/inventory/receipts/:id") view=ReceiptPage />
                    <Route path=path!("/inventory/stock") view=StockPage />
                    <Route path=path!("/inventory/moves") view=StockMovesPage />
                    <Route path=path!("/inventory/units") view=UnitsPage />
                    <Route path=path!("/inventory/units/new") view=UnitNewPage />
                    <Route path=path!("/inventory/units/:id") view=UnitPage />

                    <Route path=path!("/master/taxes") view=TaxesPage />
                    <Route path=path!("/master/taxes/new") view=TaxNewPage />
                    <Route path=path!("/master/taxes/:id") view=TaxPage />
                    <Route path=path!("/master/tax-groups/new") view=TaxGroupNewPage />
                    <Route path=path!("/master/tax-groups/:id") view=TaxGroupPage />

                    // Administration. Each of these is named by a node in
                    // `navigation::tree` and gated on the matching permission
                    // there; the screens themselves state their own.
                    <Route path=path!("/admin/users") view=UsersPage />
                    <Route path=path!("/admin/users/invite") view=UserInvitePage />
                    <Route path=path!("/admin/users/:id/edit") view=UserEditPage />
                    <Route path=path!("/admin/users/:id/permissions") view=UserPermissionsPage />
                    <Route path=path!("/admin/roles") view=RolesPage />
                    // Before the parameter, so "new" is a screen rather than a
                    // role id that fails to parse.
                    <Route path=path!("/admin/roles/new") view=RoleNewPage />
                    <Route path=path!("/admin/roles/:id") view=RolePage />
                    <Route path=path!("/admin/settings") view=SettingsPage />
                    <Route path=path!("/admin/audit-logs") view=AuditLogsPage />
                    <Route path=path!("/admin/audit-logs/:id") view=AuditEventPage />
                    <Route path=path!("/admin/changes/:id") view=EntityChangePage />
                    <Route path=path!("/admin/api-keys") view=ApiKeysPage />
                    // Before the list would ever grow a parameter, and for the
                    // same reason `roles/new` sits above `roles/:id`.
                    <Route path=path!("/admin/api-keys/new") view=ApiKeyNewPage />
                    <Route path=path!("/admin/apps") view=AppsPage />
                    <Route path=path!("/admin/ui") view=UiLibraryPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}
