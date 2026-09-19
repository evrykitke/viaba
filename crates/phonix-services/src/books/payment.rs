//! Recording a customer paying, and posting it.
//!
//! # Posting is where the money exists
//!
//! A draft is somebody keying a bank statement. Posting takes a number, freezes
//! the customer onto the record, converts once at the rate for the day the
//! money arrived, and writes the journal - all in one transaction, for the
//! reason [`crate::books::invoice::post`] does: a payment that posted while its
//! entry was refused is money nobody recorded, and the refusal has to be the
//! payment's refusal.
//!
//! ```text
//!   DR  the bank or cash account named on it
//!       CR  accounts receivable
//! ```
//!
//! The debit names [`AccountRole::Cash`] *and* the account chosen on the
//! document. The role is what a reader and a cash-flow report understand; the
//! override is the workspace saying which of its four bank accounts this one
//! landed in. That is exactly what `Posting::account_id` is for.
//!
//! # What the check in `app-books` cannot do
//!
//! [`PaymentInput::check`] knows the arithmetic: nothing negative, nothing
//! twice, nothing more allocated than received. It cannot know whether the
//! invoices named are this customer's, still posted, in the same currency, or
//! already settled by somebody else's cheque - all four need the database, and
//! all four are [`validate`]'s.
//!
//! # Withdrawing is not deleting
//!
//! A bounced cheque keeps its number, keeps its allocations, and reverses its
//! journal. What changes is that nothing counts it any more: every statement
//! that adds allocations up filters on `status = 'posted'`, so the invoices it
//! settled are owed again the moment it is withdrawn.

use app_books::payment::{
    CheckedPayment, PayerSnapshot, Payment, PaymentError, PaymentInput, PaymentStatus,
    PaymentSummary, PostOutcome, Settleable,
};
use chrono::NaiveDate;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{ExchangeRate, Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_core::query::{Page, PageRequest};
use phonix_db::books::payment as store;
use phonix_db::error::DbError;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::{AccountRole, JournalRequest, Posting, Side};
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    request: PageRequest,
) -> ServiceResult<Page<PaymentSummary>> {
    caller.require(permissions::PAYMENTS)?;
    Ok(store::page(pool, &request).await?)
}

pub async fn find(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Payment> {
    caller.require(permissions::PAYMENTS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("payment", msg!("payments.gone")))
}

pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<PaymentInput> {
    Ok(PaymentInput::from_payment(&find(pool, caller, id).await?))
}

/// A blank payment: today, the workspace's own currency, and the account the
/// `cash` role names.
///
/// The account is a *default*, not a decision. Somebody banking a cheque into
/// the deposit account changes it, and the journal follows the document rather
/// than the role.
pub async fn blank(pool: &PgPool, caller: &Caller) -> ServiceResult<PaymentInput> {
    caller.require(permissions::PAYMENTS_CREATE)?;

    let currency = base_currency(pool).await?;
    let mut blank = PaymentInput::blank(today(), currency);

    blank.account_id =
        phonix_db::books::account_role::account_for(pool, AccountRole::Cash.as_str()).await?;

    Ok(blank)
}

/// The invoices this customer still owes, for the allocation half of the
/// screen.
///
/// `editing` is the payment being reopened, so its own allocations count as
/// available rather than as already gone.
pub async fn settleable(
    pool: &PgPool,
    caller: &Caller,
    party_id: Uuid,
    currency: Currency,
    editing: Option<Uuid>,
) -> ServiceResult<Vec<Settleable>> {
    caller.require(permissions::PAYMENTS)?;
    Ok(store::settleable(pool, party_id, currency, editing).await?)
}

/// Every account money may land in: the bank and cash accounts of the chart.
pub async fn cash_accounts(
    pool: &PgPool,
    caller: &Caller,
) -> ServiceResult<Vec<app_books::account::AccountSummary>> {
    caller.require(permissions::PAYMENTS)?;

    Ok(phonix_db::books::account::list(pool)
        .await?
        .into_iter()
        .filter(|account| {
            account.is_active
                && matches!(
                    account.account_type,
                    app_books::account::AccountType::Bank | app_books::account::AccountType::Cash
                )
        })
        .map(|account| app_books::account::AccountSummary {
            id: account.id,
            number: account.number,
            name: account.name,
            account_type: account.account_type,
            is_active: account.is_active,
            is_default: account.is_default,
            has_entries: false,
        })
        .collect())
}

/// Write a payment, or change one. `id` absent means create.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: PaymentInput,
) -> ServiceResult<Submission<PaymentInput>> {
    caller.require(match draft.id {
        None => permissions::PAYMENTS_CREATE,
        Some(_) => permissions::PAYMENTS_EDIT,
    })?;
    acting_user(caller)?;

    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let party = match payer_snapshot(pool, checked.party_id).await? {
        Ok(party) => party,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    if let Err(err) = validate(pool, &checked).await? {
        return Ok(Submission::rejected(err.field(), err.message()));
    }

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match checked.id {
        None => store::insert(&mut tx, &checked, &party, caller.user_id()).await?,
        Some(id) => {
            if !store::update(&mut tx, id, &checked, &party, caller.user_id()).await? {
                tx.rollback().await.map_err(DbError::Query)?;
                return Ok(Submission::rejected(
                    "status",
                    PaymentError::NotEditable.message(),
                ));
            }
            id
        }
    };

    store::save_allocations(&mut tx, id, &checked.allocations).await?;
    tx.commit().await.map_err(DbError::Query)?;

    let stored = PaymentInput {
        id: Some(id),
        ..draft
    };

    let target = Target::new(kinds::PAYMENT, id)
        .named(&party.name)
        .fact("amount", checked.amount.to_display_string());

    match checked.id {
        None => audit::created(pool, caller, target, &stored).await,
        Some(_) => audit::updated(pool, caller, target, &stored, &stored).await,
    }

    Ok(Submission::Saved(stored))
}

/// Number the payment, freeze it, and post what it does to the ledger.
///
/// One transaction, for the reason the invoice's post is one: a payment that
/// posted while its journal was refused is money nobody recorded. See the
/// module header.
pub async fn post(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<PostOutcome> {
    caller.require(permissions::PAYMENTS_POST)?;
    acting_user(caller)?;

    crate::workspace::setup::require_ready(pool, app_books::APP_ID).await?;

    let payment = find(pool, caller, id).await?;
    if payment.status != PaymentStatus::Draft {
        return Ok(PostOutcome::NotADraft);
    }

    // Re-checked at post and not only at save: a cheque keyed on Monday and
    // posted on Friday may be settling an invoice somebody else has since been
    // paid for.
    let checked = CheckedPayment {
        id: Some(payment.id),
        party_id: payment.party.party_id,
        received_on: payment.received_on,
        account_id: payment.account_id,
        currency: payment.currency,
        amount: payment.amount,
        reference: payment.reference.clone(),
        note: payment.note.clone(),
        allocations: payment
            .allocations
            .iter()
            .map(|line| app_books::payment::CheckedAllocation {
                invoice_id: line.invoice_id,
                amount: line.amount,
            })
            .collect(),
    };

    if let Err(err) = validate(pool, &checked).await? {
        return Err(ServiceError::rejected(err.field(), err.message()));
    }

    // The conversion, worked out before the transaction opens: it reads two
    // tables and does not need the sequence's row lock held while it does.
    let conversion = conversion_for(pool, &payment).await?;
    let base_amount = conversion
        .as_ref()
        .map_or(payment.amount, |(_, base)| *base);

    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    // The check above is the message; this is the enforcement. Two payments
    // against one invoice at the same moment both read the same "outstanding"
    // and neither write conflicts with the other, so the invoice ends up
    // settled twice unless the second waits for the first. See
    // `store::lock_invoices`.
    let invoice_ids: Vec<Uuid> = checked
        .allocations
        .iter()
        .map(|line| line.invoice_id)
        .collect();

    store::lock_invoices(&mut tx, &invoice_ids).await?;

    if let Err(err) = over_settled(&mut tx, &checked).await? {
        tx.rollback().await.map_err(DbError::Query)?;
        return Err(ServiceError::rejected(err.field(), err.message()));
    }

    let key = SequenceKey::new(app_books::APP_ID, app_books::PAYMENT);
    let allocated = match generator.next(&mut tx, key, payment.received_on).await {
        Ok(allocated) => allocated,
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Ok(PostOutcome::NoSeries);
        }
        Err(err) => return Err(err),
    };

    let stored = store::post(
        &mut tx,
        id,
        &allocated.number,
        conversion.as_ref().map(|(rate, _)| rate),
        conversion.as_ref().map(|(_, base)| *base),
        caller.user_id(),
    )
    .await?;

    if !stored {
        tx.rollback().await.map_err(DbError::Query)?;
        return Ok(PostOutcome::NotADraft);
    }

    let journal = match post_journal(pool, &mut tx, caller, &payment, &allocated.number).await {
        Ok(journal) => journal,
        Err(err) => {
            // A closed period, a missing rate, a role with no account behind
            // it. The number goes back and it stays a draft.
            tx.rollback().await.map_err(DbError::Query)?;
            return Err(err);
        }
    };

    tx.commit().await.map_err(DbError::Query)?;

    let after = find(pool, caller, id).await?;
    let mut target = Target::new(kinds::PAYMENT, id)
        .named(&payment.party.name)
        .fact("number", &allocated.number)
        .fact("amount", base_amount.to_display_string());

    if let Some(journal_id) = journal {
        let posted = super::journal::detail_of(pool, journal_id).await?;
        target = target.fact("journal", &posted.number);
        super::journal::record(pool, caller, &posted).await;
    }

    audit::updated(pool, caller, target, &PaymentStatus::Draft, &after.status).await;

    Ok(PostOutcome::Posted {
        number: allocated.number,
    })
}

/// Withdraw a posted payment, and reverse what it posted.
///
/// A bounced cheque. It keeps its number and its allocations; what it loses is
/// its effect - see the module header.
pub async fn void(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<()> {
    caller.require(permissions::PAYMENTS_VOID)?;
    acting_user(caller)?;

    let payment = find(pool, caller, id).await?;
    if payment.status != PaymentStatus::Posted {
        return Err(ServiceError::rejected(
            "status",
            PaymentError::NotVoidable.message(),
        ));
    }

    // Dated today rather than the day the money arrived: a cheque that bounces
    // in April is April's event, and the period the payment was posted into may
    // well be closed. The same rule voiding an invoice follows.
    let today = today();

    let reversal = match phonix_db::books::journal::of_document(pool, id).await? {
        Some((journal_id, number)) => {
            let entry = super::journal::reversal_entry(
                pool,
                journal_id,
                today,
                Some(format!("Reverses {number}")),
            )
            .await?;

            Some(super::journal::ready(pool, entry).await?)
        }
        None => None,
    };

    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    if !store::void(&mut *tx, id, caller.user_id()).await? {
        tx.rollback().await.map_err(DbError::Query)?;
        return Err(ServiceError::rejected(
            "status",
            PaymentError::NotVoidable.message(),
        ));
    }

    let written = match reversal {
        Some(ready) => match super::journal::write(&mut tx, &ready, caller).await {
            Ok(written) => Some(written),
            Err(err) => {
                tx.rollback().await.map_err(DbError::Query)?;
                return Err(err);
            }
        },
        None => None,
    };

    tx.commit().await.map_err(DbError::Query)?;

    let mut target = Target::new(kinds::PAYMENT, id)
        .named(&payment.party.name)
        .fact("number", payment.number.clone().unwrap_or_default())
        .fact("amount", payment.amount.to_display_string());

    if let Some(written) = written {
        target = target.fact("reversal", &written.number);

        let posted = super::journal::detail_of(pool, written.id).await?;
        super::journal::record(pool, caller, &posted).await;
    }

    audit::updated(
        pool,
        caller,
        target,
        &PaymentStatus::Posted,
        &PaymentStatus::Voided,
    )
    .await;

    Ok(())
}

/// Remove a draft.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<()> {
    caller.require(permissions::PAYMENTS_EDIT)?;
    acting_user(caller)?;

    let payment = find(pool, caller, id).await?;

    if !store::delete_draft(pool, id).await? {
        return Err(ServiceError::rejected(
            "status",
            PaymentError::NotEditable.message(),
        ));
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::PAYMENT, id)
            .named(&payment.party.name)
            .fact("amount", payment.amount.to_display_string()),
        &PaymentInput::from_payment(&payment),
    )
    .await;

    Ok(())
}

/// The journal this payment raised, if it raised one.
pub async fn journal_of(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Option<(Uuid, String)>> {
    caller.require(permissions::PAYMENTS)?;

    Ok(phonix_db::books::journal::of_document(pool, id).await?)
}

// --- the work ------------------------------------------------------------

/// What the check in `app-books` could not know.
///
/// Four questions, all of which need the database: is this invoice this
/// customer's, is it posted, is it in the payment's currency, and is there
/// still that much left on it.
async fn validate(
    pool: &PgPool,
    checked: &CheckedPayment,
) -> ServiceResult<Result<(), PaymentError>> {
    if !matches!(
        phonix_db::books::account::find(pool, checked.account_id).await?,
        Some(ref account)
            if account.is_active
                && matches!(
                    account.account_type,
                    app_books::account::AccountType::Bank
                        | app_books::account::AccountType::Cash
                )
    ) {
        return Ok(Err(PaymentError::NotACashAccount));
    }

    if checked.allocations.is_empty() {
        return Ok(Ok(()));
    }

    let invoice_ids: Vec<Uuid> = checked
        .allocations
        .iter()
        .map(|line| line.invoice_id)
        .collect();

    // Somebody else's invoice. Checked before anything else about the lines,
    // because it is the one that is a mistake rather than a race.
    for (invoice_id, party_id) in store::party_of(pool, &invoice_ids).await? {
        let _ = invoice_id;
        if party_id != checked.party_id {
            return Ok(Err(PaymentError::WrongCustomer));
        }
    }

    // Posted, in the right currency, and with enough left on it. `settled_on`
    // answers all three at once: a draft or voided invoice is simply not in the
    // result.
    let outstanding = store::settled_on(pool, &invoice_ids).await?;

    for line in &checked.allocations {
        let Some((_, code, left)) = outstanding.iter().find(|(id, _, _)| *id == line.invoice_id)
        else {
            return Ok(Err(PaymentError::InvoiceNotPosted));
        };

        if code.as_str() != checked.currency.code() {
            return Ok(Err(PaymentError::CurrencyMismatch));
        }

        let left = match Money::parse(checked.currency, left) {
            Ok(left) => left,
            Err(err) => return Ok(Err(PaymentError::Money(err))),
        };

        match line.amount.compare(left) {
            Ok(ordering) if ordering.is_gt() => return Ok(Err(PaymentError::OverSettled)),
            Ok(_) => {}
            Err(err) => return Ok(Err(PaymentError::Money(err))),
        }
    }

    Ok(Ok(()))
}

/// Whether anything is being settled beyond what is left on it, read inside the
/// transaction and after the lock.
///
/// The same arithmetic [`validate`] does, against a snapshot taken after
/// whatever else was posting has committed. It answers a refusal rather than a
/// message somebody can act on, because by this point nobody is typing: the
/// screen that could have fixed it is two clicks behind.
async fn over_settled(
    tx: &mut phonix_db::sqlx::PgConnection,
    checked: &CheckedPayment,
) -> ServiceResult<Result<(), PaymentError>> {
    if checked.allocations.is_empty() {
        return Ok(Ok(()));
    }

    let invoice_ids: Vec<Uuid> = checked
        .allocations
        .iter()
        .map(|line| line.invoice_id)
        .collect();

    let outstanding = store::settled_on(&mut *tx, &invoice_ids).await?;

    for line in &checked.allocations {
        let Some((_, _, left)) = outstanding.iter().find(|(id, _, _)| *id == line.invoice_id)
        else {
            return Ok(Err(PaymentError::InvoiceNotPosted));
        };

        let left = match Money::parse(checked.currency, left) {
            Ok(left) => left,
            Err(err) => return Ok(Err(PaymentError::Money(err))),
        };

        match line.amount.compare(left) {
            Ok(ordering) if ordering.is_gt() => return Ok(Err(PaymentError::OverSettled)),
            Ok(_) => {}
            Err(err) => return Ok(Err(PaymentError::Money(err))),
        }
    }

    Ok(Ok(()))
}

/// DR the account the money landed in, CR receivables. Inside the transaction
/// that is posting the payment.
async fn post_journal(
    pool: &PgPool,
    tx: &mut phonix_db::sqlx::PgConnection,
    caller: &Caller,
    payment: &Payment,
    number: &str,
) -> ServiceResult<Option<Uuid>> {
    if payment.amount.is_zero() {
        return Ok(None);
    }

    let memo = || Some(payment.party.name.clone());
    let amount = payment.amount.to_storage_string();

    let request = JournalRequest {
        entry_date: payment.received_on,
        narration: format!("{number} \u{b7} {}", payment.party.name),
        source_app: app_books::APP_ID.to_owned(),
        source_doc_type: app_books::journal::doc_types::PAYMENT.to_owned(),
        source_doc_id: payment.id,
        currency: payment.currency.code().to_owned(),
        postings: vec![
            Posting {
                role: AccountRole::Cash,
                // The account named on the document, not the one the role
                // points at: the role is the default a new payment opened on,
                // and this is where the money actually went.
                account_id: Some(payment.account_id),
                side: Side::Debit,
                amount: amount.clone(),
                memo: memo(),
                cost_centre_id: None,
            },
            Posting {
                role: AccountRole::AccountsReceivable,
                account_id: None,
                side: Side::Credit,
                amount,
                memo: memo(),
                cost_centre_id: None,
            },
        ],
    };

    let ledger = super::ledger::BooksLedger::new(pool.clone(), caller.clone());
    let entry = ledger
        .assemble(request)
        .await
        .map_err(super::ledger::refused)?;

    let ready = super::journal::ready(pool, entry).await?;
    let written = super::journal::write(tx, &ready, caller).await?;

    Ok(Some(written.id))
}

/// The customer's code and name as they stand now, refusing a party that is not
/// one.
async fn payer_snapshot(
    pool: &PgPool,
    party_id: Uuid,
) -> ServiceResult<Result<PayerSnapshot, PaymentError>> {
    let Some(party) = phonix_db::master::party::find(pool, party_id).await? else {
        return Ok(Err(PaymentError::PartyRequired));
    };

    if !party.has_role(app_books::CUSTOMER_ROLE) {
        return Ok(Err(PaymentError::NotACustomer));
    }

    // The registered name where there is one: a receipt is evidence and names
    // the entity, exactly as the invoice it settles does.
    let name = party.document_name().to_owned();

    Ok(Ok(PayerSnapshot {
        party_id: party.id,
        code: party.code,
        name,
    }))
}

/// What the payment is worth in the workspace's own currency.
///
/// `None` where it is already in it. The rate is the one for the day the money
/// arrived, and a missing rate is a refusal rather than a guess - the ledger's
/// rule, applied before the ledger has to apply it.
async fn conversion_for(
    pool: &PgPool,
    payment: &Payment,
) -> ServiceResult<Option<(ExchangeRate, Money)>> {
    let base = base_currency(pool).await?;

    if payment.currency == base {
        return Ok(None);
    }

    let rate = crate::currency::rate_on(pool, payment.currency, base, payment.received_on, None)
        .await?
        .ok_or_else(|| {
            ServiceError::rejected(
                "received_on",
                msg!(
                    "journals.error.no_rate",
                    pair = format!("{}/{}", payment.currency.code(), base.code()),
                    date = payment.received_on
                ),
            )
        })?;

    let converted = payment
        .amount
        .convert(&rate, Rounding::HalfUp)
        .map_err(|err| ServiceError::rejected("amount", err.message()))?;

    Ok(Some((rate, converted.base_amount)))
}

async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    crate::workspace::profile::base_currency(pool).await
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
