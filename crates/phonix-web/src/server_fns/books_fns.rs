//! Sales invoices.
//!
//! # There is no endpoint that prices an invoice
//!
//! Deliberately. The browser prices it locally with `app_books::pricing`, which
//! is the same code the server posts with - that is the whole reason the crate
//! compiles to wasm. A "calculate totals" round trip would be a second
//! implementation of the arithmetic living in the network, and the first thing
//! to disagree with the document.
//!
//! What the browser does need is the resolved tax treatments, which depend on
//! the document's date and on a rate table it cannot see. [`tax_treatments`]
//! hands them over once, and everything after that is local.

use app_books::account::{Account, AccountInput, AccountSummary, RoleMapping};
use app_books::invoice::{Invoice, InvoiceInput, InvoiceSummary, PostOutcome, Settlement};
use app_books::journal::{JournalDraft, JournalSummary, Posted};
use app_books::payment::{Payment, PaymentInput, PaymentSummary, Settleable};
use app_books::period::Period;
use app_books::report::{
    BalanceSheet, CustomerStatement, IncomeStatement, LedgerSummary, TrialBalance,
};
use chrono::NaiveDate;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use phonix_core::form::Submission;
use phonix_core::query::{Page, PageRequest};
use phonix_master::party::PartySummary;
use phonix_ports::ledger::{AccountRole, LedgerAccount};
use phonix_tax::group::TaxTreatment;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The chart of accounts, in number order.
#[server(name = ListAccounts, prefix = "/api", endpoint = "books/accounts")]
pub async fn list_accounts() -> Result<Vec<Account>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// One page of the chart, for the grid.
#[server(name = PageAccounts, prefix = "/api", endpoint = "books/accounts/page", input = Json)]
pub async fn page_accounts(request: PageRequest) -> Result<Page<Account>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account::page(&pool, &caller, request)
        .await
        .map_err(service_error)
}

/// One account.
#[server(name = AccountDetail, prefix = "/api", endpoint = "books/accounts/detail")]
pub async fn account_detail(account_id: Uuid) -> Result<Account, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account::detail(&pool, &caller, account_id)
        .await
        .map_err(service_error)
}

/// The editable part of one, for the form to open on.
#[server(name = AccountEdit, prefix = "/api", endpoint = "books/accounts/edit")]
pub async fn account_edit(account_id: Uuid) -> Result<AccountInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account::edit(&pool, &caller, account_id)
        .await
        .map_err(service_error)
}

/// Add an account or change one. Which, comes from the draft's own `id`.
#[server(name = SaveAccount, prefix = "/api", endpoint = "books/accounts/save")]
pub async fn save_account(draft: AccountInput) -> Result<Submission<AccountInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// What a journal screen is *about*: an account's ledger, one period, one app.
///
/// What the *viewer* asked for - the search, the page, the span, whether to
/// show corrections - travels beside it in a `PageRequest`. The two are
/// separate because one is written by whatever opened the screen and the other
/// changes with every click.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalFilter {
    pub period_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub source_app: Option<String>,
}

/// What has been posted to the ledger.
/// Every role this build knows, and the account each one means here.
#[server(name = AccountRoles, prefix = "/api", endpoint = "books/accounts/roles")]
pub async fn account_roles() -> Result<Vec<RoleMapping>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account_role::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The chart a role may be pointed at, each account carrying the roles it fits.
///
/// The same answer the item screen's picker is built from, from the same place.
/// Two lists of "what may carry this role" would eventually disagree, and the
/// one that disagreed would be the one nobody was looking at.
#[server(name = RoleChart, prefix = "/api", endpoint = "books/accounts/roles/chart")]
pub async fn role_chart() -> Result<Vec<LedgerAccount>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account_role::chart(&pool, &caller)
        .await
        .map_err(service_error)
}

/// Point a role at an account, or `None` to stop it meaning anything.
#[server(name = SetAccountRole, prefix = "/api", endpoint = "books/accounts/roles/set")]
pub async fn set_account_role(
    role: AccountRole,
    account_id: Option<Uuid>,
) -> Result<(), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::account_role::set(&pool, &caller, role, account_id)
        .await
        .map_err(service_error)
}

#[server(name = ListJournals, prefix = "/api", endpoint = "books/journals", input = Json)]
pub async fn list_journals(
    filter: JournalFilter,
    request: PageRequest,
) -> Result<Page<JournalSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let query = phonix_services::books::journal::JournalQuery {
        period_id: filter.period_id,
        account_id: filter.account_id,
        source_app: filter.source_app,
    };

    phonix_services::books::journal::list(&pool, &caller, query, request)
        .await
        .map_err(service_error)
}

/// What a journal form needs before somebody can type into it.
///
/// One round trip rather than four. All of it is workspace-shaped rather than
/// document-shaped, so it is fetched once when the screen opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalContext {
    /// What this workspace keeps its books in. The accountant set it, and it is
    /// what the currency picker opens on.
    pub base_currency: String,
    /// Only the accounts a person may post to directly: a control account is
    /// owned by a sub-ledger, and posting to one by hand breaks a
    /// reconciliation.
    pub accounts: Vec<Account>,
    /// From the `CostCentres` port. Empty where no app provides one, which is a
    /// journal that simply cannot be charged to anything.
    pub cost_centres: Vec<phonix_ports::CostCentre>,
    /// What the workspace deals in, which is not every currency there is. A
    /// picker offering all of them would offer a hundred with no rate on file.
    pub currencies: Vec<phonix_core::locale::Currency>,
}

/// Everything the journal form opens on.
#[server(name = JournalFormContext, prefix = "/api", endpoint = "books/journals/context")]
pub async fn journal_context() -> Result<JournalContext, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};
    use phonix_ports::CostCentres;

    let (pool, caller) = pool_and_caller().await?;

    let accounts = phonix_services::books::account::list(&pool, &caller)
        .await
        .map_err(service_error)?
        .into_iter()
        .filter(app_books::account::Account::is_postable)
        .collect();

    let base = phonix_services::workspace::profile::base_currency(&pool)
        .await
        .map_err(service_error)?;

    // A port that cannot answer costs the picker, not the screen: a journal
    // with nothing to charge to is still a journal.
    let cost_centres = phonix_services::hr::HrCostCentres::new(pool.clone())
        .list()
        .await
        .unwrap_or_default();

    let mut currencies: Vec<phonix_core::locale::Currency> =
        phonix_services::currency::enabled(&pool)
            .await
            .map_err(service_error)?
            .into_iter()
            .map(|row| row.currency)
            .collect();

    // The workspace's own is always offered, even if somebody switched it off
    // in the currency list: it is what the books are kept in.
    if !currencies.contains(&base) {
        currencies.insert(0, base);
    }

    Ok(JournalContext {
        base_currency: base.code().to_owned(),
        accounts,
        cost_centres,
        currencies,
    })
}

/// Post what somebody typed.
#[server(name = PostJournal, prefix = "/api", endpoint = "books/journals/post", input = Json)]
pub async fn post_journal(draft: JournalDraft) -> Result<Posted, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::journal::post_draft(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// One journal, with its lines.
#[server(name = JournalDetail, prefix = "/api", endpoint = "books/journals/detail")]
pub async fn journal_detail(journal_id: Uuid) -> Result<Posted, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::journal::detail(&pool, &caller, journal_id)
        .await
        .map_err(service_error)
}

/// Reverse one, with a second journal that names it.
#[server(name = ReverseJournal, prefix = "/api", endpoint = "books/journals/reverse")]
pub async fn reverse_journal(
    journal_id: Uuid,
    on: NaiveDate,
    narration: Option<String>,
) -> Result<Posted, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::journal::reverse(&pool, &caller, journal_id, on, narration)
        .await
        .map_err(service_error)
}

/// The accounting calendar.
#[server(name = ListPeriods, prefix = "/api", endpoint = "books/periods")]
pub async fn list_periods() -> Result<Vec<Period>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::period::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// Open a financial year, and say how many periods that created.
#[server(name = OpenFinancialYear, prefix = "/api", endpoint = "books/periods/open")]
pub async fn open_financial_year(year: i32) -> Result<u64, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::period::open_year(&pool, &caller, year)
        .await
        .map_err(service_error)
}

/// Close a period, or open it again.
#[server(name = SetPeriodClosed, prefix = "/api", endpoint = "books/periods/close")]
pub async fn set_period_closed(period_id: Uuid, closed: bool) -> Result<Period, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let result = if closed {
        phonix_services::books::period::close(&pool, &caller, period_id).await
    } else {
        phonix_services::books::period::reopen(&pool, &caller, period_id).await
    };

    result.map_err(service_error)
}

/// The year the "open a financial year" button should offer.
///
/// Not the current year plus one. A workspace whose calendar has never been
/// opened is offered the year it is *in* - offering it next year is the wrong
/// answer to the only question it has, which is why nothing can be posted
/// today. Once this year is open the question changes to the far end of the
/// calendar, and so does the answer.
#[server(name = NextYearToOpen, prefix = "/api", endpoint = "books/periods/next-year")]
pub async fn next_year_to_open() -> Result<i32, ServerFnError> {
    use crate::state::{service_error, tenant_pool};

    let pool = tenant_pool().await?;

    phonix_services::books::period::next_year_to_open(&pool)
        .await
        .map_err(service_error)
}

// --- Reports -------------------------------------------------------------
//
// Every one of these takes its dates from the screen. A report is a document
// about a span somebody chose, and a default worked out on the server would be
// the server's own idea of today.

/// The span every report opens on: the financial year, so far.
///
/// A screen asks for this before it asks for a report. The alternative - each
/// page working out "this year" for itself - would be worked out twice, once
/// on the server and once at hydration, and the two disagree at midnight.
#[server(name = ReportSpan, prefix = "/api", endpoint = "books/reports/span")]
pub async fn report_span() -> Result<(NaiveDate, NaiveDate), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::default_span(&pool, &caller)
        .await
        .map_err(service_error)
}

/// Every account, both columns, for a span.
#[server(name = TrialBalanceReport, prefix = "/api", endpoint = "books/reports/trial-balance")]
pub async fn trial_balance(from: NaiveDate, to: NaiveDate) -> Result<TrialBalance, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::trial_balance(&pool, &caller, from, to)
        .await
        .map_err(service_error)
}

/// What is owned and what is owed, at one date.
#[server(name = BalanceSheetReport, prefix = "/api", endpoint = "books/reports/balance-sheet")]
pub async fn balance_sheet(as_at: NaiveDate) -> Result<BalanceSheet, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::balance_sheet(&pool, &caller, as_at)
        .await
        .map_err(service_error)
}

/// What was earned and what it cost, between two dates.
#[server(name = ProfitAndLossReport, prefix = "/api", endpoint = "books/reports/profit-and-loss")]
pub async fn profit_and_loss(
    from: NaiveDate,
    to: NaiveDate,
) -> Result<IncomeStatement, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::profit_and_loss(&pool, &caller, from, to)
        .await
        .map_err(service_error)
}

/// What one customer has been invoiced, and how long ago.
#[server(name = CustomerStatementReport, prefix = "/api", endpoint = "books/reports/statement")]
pub async fn customer_statement(
    party_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<CustomerStatement, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::customer_statement(&pool, &caller, party_id, from, to)
        .await
        .map_err(service_error)
}

/// The customers a statement may be run for.
///
/// Gated on reports rather than on the customer file: somebody in credit
/// control reads statements and need not hold master data.
#[server(name = StatementCustomers, prefix = "/api", endpoint = "books/reports/customers")]
pub async fn statement_customers() -> Result<Vec<PartySummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::statement_customers(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The figures the app's front page carries.
#[server(name = LedgerSummaryFigures, prefix = "/api", endpoint = "books/reports/summary")]
pub async fn ledger_summary() -> Result<LedgerSummary, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::report::summary(&pool, &caller)
        .await
        .map_err(service_error)
}

/// What an invoice screen is *about*: one customer's ledger, or all of them.
///
/// What the *viewer* asked for - the state, the span, the search, the page -
/// travels beside it in a `PageRequest`. See `phonix_db::books::invoice`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceQuery {
    pub party_id: Option<Uuid>,
}

/// Every invoice a list screen should show.
///
/// # Why this one is JSON and its neighbours are not
///
/// The default encoding is url-encoded, where a struct is spelt out field by
/// field - `query[status]=draft`. Every field of [`InvoiceQuery`] is optional,
/// so the default value spells out to *nothing at all*, and an empty body does
/// not say "here is a query with nothing in it", it says the argument is
/// missing: `missing field `query``. The list screen asks unfiltered, so that
/// was every first load.
///
/// JSON has a way to write an empty object. Any argument that is a struct of
/// nothing but options wants it.
#[server(name = ListInvoices, prefix = "/api", endpoint = "books/invoices", input = Json)]
pub async fn list_invoices(
    query: InvoiceQuery,
    request: PageRequest,
) -> Result<Page<InvoiceSummary>, ServerFnError> {
    use phonix_db::books::invoice::InvoiceFilter;

    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::list(
        &pool,
        &caller,
        InvoiceFilter {
            party_id: query.party_id,
        },
        request,
    )
    .await
    .map_err(service_error)
}

/// One invoice, whole: its lines and the tax each carried.
#[server(name = InvoiceDetail, prefix = "/api", endpoint = "books/invoices/detail")]
pub async fn invoice_detail(invoice_id: Uuid) -> Result<Invoice, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::find(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

/// What has been credited and paid against one invoice.
///
/// Its own call rather than part of [`invoice_detail`]: the document is what
/// was raised and does not change, and this is what has happened to it since.
/// A panel that reloads after a credit note is posted should not refetch the
/// lines to say so.
#[server(name = InvoiceSettlement, prefix = "/api", endpoint = "books/invoices/settlement")]
pub async fn invoice_settlement(invoice_id: Uuid) -> Result<Settlement, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::settlement(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

/// The editable part of one invoice, for the form to open on.
#[server(name = InvoiceEdit, prefix = "/api", endpoint = "books/invoices/edit")]
pub async fn invoice_edit(invoice_id: Uuid) -> Result<InvoiceInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::edit(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

/// Create a draft, or rewrite one.
#[server(name = SaveInvoice, prefix = "/api", endpoint = "books/invoices/save")]
pub async fn save_invoice(draft: InvoiceInput) -> Result<Submission<InvoiceInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Number a draft and issue it.
///
/// Comes back as a [`PostOutcome`] rather than a `Result`, because two of its
/// three answers are things a screen renders beside the button: somebody else
/// posted it, and this workspace has no number series set up.
#[server(name = PostInvoice, prefix = "/api", endpoint = "books/invoices/post")]
pub async fn post_invoice(invoice_id: Uuid) -> Result<PostOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    // Inventory's side of the `Deliveries` port, so a line billing a delivery
    // can be checked against what actually went out.
    let deliveries = phonix_services::inventory::delivery::InventoryDeliveries::new(pool.clone());

    phonix_services::books::invoice::post(&pool, &caller, &deliveries, invoice_id)
        .await
        .map_err(service_error)
}

/// A credit note prefilled from the invoice it credits.
#[server(name = CreditAgainstInvoice, prefix = "/api", endpoint = "books/invoices/credit")]
pub async fn credit_against_invoice(
    invoice_id: Uuid,
) -> Result<Submission<InvoiceInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::credit_against(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

/// An invoice prefilled with what a despatch has not been charged for.
#[server(name = InvoiceAgainstDelivery, prefix = "/api", endpoint = "books/invoices/against-delivery")]
pub async fn invoice_against_delivery(
    delivery_id: Uuid,
) -> Result<Submission<InvoiceInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let deliveries = phonix_services::inventory::delivery::InventoryDeliveries::new(pool.clone());

    phonix_services::books::invoice::against_delivery(&pool, &caller, &deliveries, delivery_id)
        .await
        .map_err(service_error)
}

/// The journal a posted invoice raised: its id, and its number.
#[server(name = InvoiceJournal, prefix = "/api", endpoint = "books/invoices/journal")]
pub async fn invoice_journal(invoice_id: Uuid) -> Result<Option<(Uuid, String)>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::journal_of(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

/// Withdraw a posted invoice. It keeps its number.
#[server(name = VoidInvoice, prefix = "/api", endpoint = "books/invoices/void")]
pub async fn void_invoice(invoice_id: Uuid) -> Result<(), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::void(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

/// Remove a draft.
#[server(name = DeleteInvoice, prefix = "/api", endpoint = "books/invoices/delete")]
pub async fn delete_invoice(invoice_id: Uuid) -> Result<(), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::delete(&pool, &caller, invoice_id)
        .await
        .map_err(service_error)
}

// --- Payments -------------------------------------------------------------
//
// The other side of the invoice. Posting one is an accounting event on the
// same terms - one transaction for the number, the freeze and the journal -
// and it is what makes "what are we owed" a balance rather than the sum of
// every invoice ever raised.

/// `Json` for the reason `list_invoices` is: a [`PageRequest`] carries a map
/// of filters, and form encoding has no way to write an empty one.
#[server(name = ListPayments, prefix = "/api", endpoint = "books/payments", input = Json)]
pub async fn list_payments(request: PageRequest) -> Result<Page<PaymentSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::list(&pool, &caller, request)
        .await
        .map_err(service_error)
}

#[server(name = PaymentDetail, prefix = "/api", endpoint = "books/payments/detail")]
pub async fn payment_detail(payment_id: Uuid) -> Result<Payment, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::find(&pool, &caller, payment_id)
        .await
        .map_err(service_error)
}

#[server(name = PaymentEdit, prefix = "/api", endpoint = "books/payments/edit")]
pub async fn payment_edit(payment_id: Uuid) -> Result<PaymentInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::edit(&pool, &caller, payment_id)
        .await
        .map_err(service_error)
}

/// A blank payment: today, the workspace's currency, and the account the `cash`
/// role names. The account is a default, not a decision.
#[server(name = BlankPayment, prefix = "/api", endpoint = "books/payments/blank")]
pub async fn blank_payment() -> Result<PaymentInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::blank(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The invoices one customer still owes, for the allocation half of the screen.
#[server(name = SettleableInvoices, prefix = "/api", endpoint = "books/payments/settleable")]
pub async fn settleable_invoices(
    party_id: Uuid,
    currency: String,
    editing: Option<Uuid>,
) -> Result<Vec<Settleable>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let currency = phonix_core::locale::Currency::parse(&currency)
        .map_err(|err| ServerFnError::new(err.to_string()))?;

    phonix_services::books::payment::settleable(&pool, &caller, party_id, currency, editing)
        .await
        .map_err(service_error)
}

/// Every account money may land in: the bank and cash accounts of the chart.
#[server(name = CashAccounts, prefix = "/api", endpoint = "books/payments/accounts")]
pub async fn cash_accounts() -> Result<Vec<AccountSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::cash_accounts(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = SavePayment, prefix = "/api", endpoint = "books/payments/save", input = Json)]
pub async fn save_payment(draft: PaymentInput) -> Result<Submission<PaymentInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Number a draft and post it. The money reaches the ledger here.
#[server(name = PostPayment, prefix = "/api", endpoint = "books/payments/post")]
pub async fn post_payment(
    payment_id: Uuid,
) -> Result<app_books::payment::PostOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::post(&pool, &caller, payment_id)
        .await
        .map_err(service_error)
}

/// Withdraw a posted payment - a cheque that bounced. It keeps its number.
#[server(name = VoidPayment, prefix = "/api", endpoint = "books/payments/void")]
pub async fn void_payment(payment_id: Uuid) -> Result<(), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::void(&pool, &caller, payment_id)
        .await
        .map_err(service_error)
}

#[server(name = DeletePayment, prefix = "/api", endpoint = "books/payments/delete")]
pub async fn delete_payment(payment_id: Uuid) -> Result<(), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::delete(&pool, &caller, payment_id)
        .await
        .map_err(service_error)
}

/// The journal a posted payment raised: its id, and its number.
#[server(name = PaymentJournal, prefix = "/api", endpoint = "books/payments/journal")]
pub async fn payment_journal(payment_id: Uuid) -> Result<Option<(Uuid, String)>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::payment::journal_of(&pool, &caller, payment_id)
        .await
        .map_err(service_error)
}

/// Every active tax treatment, resolved for a document date.
///
/// Fetched once when the editor opens or the date changes, and then the browser
/// prices every line locally. That is what makes the totals appear as somebody
/// types rather than a third of a second after they stop.
#[server(name = TaxTreatments, prefix = "/api", endpoint = "books/treatments")]
pub async fn tax_treatments(on: NaiveDate) -> Result<Vec<TaxTreatment>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::treatments(&pool, &caller, on)
        .await
        .map_err(service_error)
}
