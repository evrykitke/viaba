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

use app_books::account::{Account, AccountInput};
use app_books::journal::{JournalDraft, JournalSummary, Posted};
use app_books::period::Period;
use app_books::invoice::{Invoice, InvoiceInput, InvoiceStatus, InvoiceSummary, PostOutcome};
use chrono::NaiveDate;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use phonix_core::form::Submission;
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

/// Which journals a screen is asking for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalFilter {
    pub period_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub source_app: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

/// What has been posted to the ledger.
#[server(name = ListJournals, prefix = "/api", endpoint = "books/journals", input = Json)]
pub async fn list_journals(filter: JournalFilter) -> Result<Vec<JournalSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    let query = phonix_services::books::journal::JournalQuery {
        period_id: filter.period_id,
        account_id: filter.account_id,
        source_app: filter.source_app,
        from: filter.from,
        to: filter.to,
    };

    phonix_services::books::journal::list(&pool, &caller, query)
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

    let profile = phonix_services::workspace::profile::current(&pool)
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
    if !currencies.contains(&profile.currency) {
        currencies.insert(0, profile.currency);
    }

    Ok(JournalContext {
        base_currency: profile.currency.code().to_owned(),
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

/// The financial year this workspace is currently in - what the calendar screen
/// offers to open when it runs out.
#[server(name = CurrentFinancialYear, prefix = "/api", endpoint = "books/periods/current-year")]
pub async fn current_financial_year() -> Result<i32, ServerFnError> {
    use crate::state::{service_error, tenant_pool};

    let pool = tenant_pool().await?;

    phonix_services::books::period::current_year(&pool)
        .await
        .map_err(service_error)
}

/// Which invoices a screen is asking for.
///
/// Crosses the wire as one value rather than four arguments, for the reason the
/// repository takes one: a status passed where a party id goes would compile
/// and would list the wrong documents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceQuery {
    pub party_id: Option<Uuid>,
    pub status: Option<InvoiceStatus>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
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
pub async fn list_invoices(query: InvoiceQuery) -> Result<Vec<InvoiceSummary>, ServerFnError> {
    use phonix_db::books::invoice::InvoiceFilter;

    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::books::invoice::list(
        &pool,
        &caller,
        InvoiceFilter {
            party_id: query.party_id,
            status: query.status,
            from: query.from,
            to: query.to,
            search: None,
        },
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

    phonix_services::books::invoice::post(&pool, &caller, invoice_id)
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
