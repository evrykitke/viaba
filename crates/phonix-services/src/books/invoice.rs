//! Raising, pricing, posting and voiding a sales invoice.
//!
//! # Four permissions, because they are four different powers
//!
//! Reading a customer's invoices is not raising one, raising one is not
//! *posting* it - which takes a number nobody can hand back - and posting is
//! not voiding a document that has already been sent.
//!
//! # Posting is the only irreversible act
//!
//! [`post`] takes a number from `core.number_sequences` in the same transaction
//! as the write, so a failure returns the number rather than burning it. What
//! it cannot undo is the *decision*: after it, the document is frozen and a
//! mistake is corrected by voiding it and raising another. That is not a
//! limitation, it is what makes an invoice evidence.
//!
//! # Posting an invoice is an accounting event, in one transaction
//!
//! The number, the freeze and the journal all commit together or none of them
//! do. That is a deliberate difference from the way a supplier bill posts -
//! see `inventory::bill`, which commits the document and *then* asks the
//! ledger, because a warehouse must not stop for the accounting module and
//! `NoLedger` is a real answer over there.
//!
//! Here there is no such answer. Books is the ledger. An invoice that posted
//! while its journal was refused - a closed period, an unmapped role - would be
//! revenue nobody recorded, found weeks later by a reconciliation. So the
//! refusal is the invoice's refusal: nothing is written, the number is returned
//! to the series, and the screen says which date or which setting is in the
//! way.
//!
//! [`void`] is the mirror. Withdrawing the document reverses its journal in the
//! same transaction, dated the day the withdrawal happens rather than the day
//! the invoice was issued - a mistake found in April is April's event, and
//! March may well be closed by now.
//!
//! # The snapshot happens here
//!
//! The party's name and address are copied onto the draft every time it is
//! saved, and the tax rates are resolved against the document's own date. By
//! the time an invoice is posted, nothing on it needs looking up again.

use app_books::invoice::{
    CheckedInvoice, Invoice, InvoiceInput, InvoiceStatus, InvoiceSummary, PartySnapshot,
    PostOutcome,
};
use app_books::pricing::{PricedInvoice, PricedLine};
use chrono::NaiveDate;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::permissions;
use phonix_core::query::{Page, PageRequest};
use phonix_core::{Message, msg};
use phonix_db::books::invoice as store;
use phonix_db::books::invoice::{DraftWrite, InvoiceFilter};
use phonix_db::error::DbError;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use phonix_master::address::AddressPurpose;
use phonix_tax::compute::DocumentTax;
use phonix_tax::group::TaxTreatment;
use uuid::Uuid;

use crate::audit::{self, Target};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

/// One page of what a list screen should show.
pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    filter: InvoiceFilter,
    request: PageRequest,
) -> ServiceResult<Page<InvoiceSummary>> {
    caller.require(permissions::INVOICES)?;
    Ok(store::page(pool, filter, &request).await?)
}

/// One invoice, whole.
///
/// Gated on `Invoices` rather than `Invoices.Edit`, matching every other detail
/// screen here: reading a document is part of being able to see the list, and
/// somebody who cannot edit gets a read-only screen rather than a refusal.
pub async fn find(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Invoice> {
    caller.require(permissions::INVOICES)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("invoice", msg!("books.error.gone")))
}

/// The editable part of one invoice, for the form to open on.
pub async fn edit(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<InvoiceInput> {
    Ok(InvoiceInput::from_invoice(&find(pool, caller, id).await?))
}

/// Create a draft, or rewrite one.
///
/// Which of the two it is comes from the draft: `id` absent means create. That
/// is the form's own answer rather than a second parameter, so a screen cannot
/// open the create form and submit it against an existing invoice.
pub async fn save(
    pool: &PgPool,
    caller: &Caller,
    draft: InvoiceInput,
) -> ServiceResult<Submission<InvoiceInput>> {
    caller.require(if draft.id.is_none() {
        permissions::INVOICES_CREATE
    } else {
        permissions::INVOICES_EDIT
    })?;
    // A document must be attributable: `Caller::System` has no account behind
    // it, and an invoice nobody raised is one nobody can be asked about.
    acting_user(caller)?;

    // The same rules the browser applied, applied again. The browser's check is
    // a courtesy; this one is the control.
    let checked = match draft.check() {
        Ok(checked) => checked,
        Err(err) => return Ok(Submission::rejected(err.field(), err.message())),
    };

    let party = match snapshot_party(pool, checked.party_id).await? {
        Some(party) => party,
        None => {
            return Ok(Submission::rejected(
                "party_id",
                msg!("books.error.party_gone"),
            ));
        }
    };

    let priced = match price(pool, &checked).await? {
        Ok(priced) => priced,
        Err(rejection) => return Ok(rejection.into_submission()),
    };

    let existed = checked.id;
    let id = match store::save_draft(
        pool,
        DraftWrite {
            checked: &checked,
            party: &party,
            priced: &priced,
            actor: caller.user_id(),
        },
    )
    .await
    {
        Ok(id) => id,
        // The `WHERE status = 'draft'` matched nothing. An expected path: two
        // tabs, and the other one posted it.
        Err(DbError::InvoiceNotEditable) => {
            return Ok(Submission::rejected(
                "status",
                msg!("books.error.not_editable"),
            ));
        }
        Err(err) => return Err(err.into()),
    };

    // Books claims the party as a customer.
    //
    // Through the repository rather than through
    // `master::party::claim_role`, which requires `Parties.Edit` - and being
    // allowed to raise an invoice against somebody *is* the authority to mark
    // them a customer. Requiring the master-data permission as well would mean
    // nobody in sales could invoice anyone.
    //
    // It is what stops the party being deleted out from under a document
    // later: `master` has no foreign key into `books` - on purpose - so a role
    // is the only way it can know.
    if let Ok(role) = phonix_master::party::PartyRole::parse(app_books::CUSTOMER_ROLE) {
        phonix_db::master::party::claim_role(pool, checked.party_id, &role).await?;
    }

    // Re-read rather than the draft echoed back, for the reason every save in
    // this application re-reads: the totals were computed here and the line
    // numbers were assigned by the store, and a form showing the draft would
    // show neither.
    let after = edit(pool, caller, id).await?;

    match existed {
        Some(_) => {
            audit::updated(
                pool,
                caller,
                Target::new(kinds::SALES_INVOICE, id)
                    .named(&party.name)
                    .fact("total", priced.gross.to_display_string()),
                &draft,
                &after,
            )
            .await;
        }
        None => {
            audit::created(
                pool,
                caller,
                Target::new(kinds::SALES_INVOICE, id)
                    .named(&party.name)
                    .fact("total", priced.gross.to_display_string())
                    // A draft carries no number, and saying so stops the trail
                    // reading as though a document had been issued.
                    .fact("number", "none until it is posted"),
                &after,
            )
            .await;
        }
    }

    Ok(Submission::Saved(after))
}

/// Number the invoice, freeze it, and post what it does to the ledger.
///
/// # Why the whole thing is one transaction
///
/// The `UPDATE` that allocates a number takes a row lock Postgres holds until
/// the transaction ends. Allocating and storing in the same transaction is what
/// makes a failed post *return* the number: a retry cannot burn one, and the
/// sequence stays gap-free. Allocating first and storing afterwards would leave
/// a number handed out to a document that was never written.
///
/// The journal is written inside it too, and for a second reason: an invoice
/// whose entry was refused is revenue that was never recorded. See the module
/// header on why that is the opposite of what a goods receipt does.
///
/// # Two sequences, always in this order
///
/// The invoice's number is taken before the journal's. Both are one row apiece
/// and both are held to the commit, so two posts running at once queue through
/// them in the same order and cannot deadlock against each other. Anything that
/// posts a journal beside an invoice should take them in this order too.
pub async fn post(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<PostOutcome> {
    caller.require(permissions::INVOICES_POST)?;
    acting_user(caller)?;

    // What the app cannot work without, checked before a number is spent on a
    // document that has nowhere to land. Named in a sentence rather than left
    // to surface as a violation from further down - ADR 0006 section 4.
    crate::workspace::setup::require_ready(pool, app_books::APP_ID).await?;

    let invoice = find(pool, caller, id).await?;
    if invoice.status != InvoiceStatus::Draft {
        return Ok(PostOutcome::NotADraft);
    }

    // The conversion snapshot, worked out before the transaction opens: it
    // reads two tables and does not need the sequence's row lock held while it
    // does. Every document of one type queues through that one row, so anything
    // that can happen outside it should.
    let conversion = conversion_for(pool, &invoice).await?;

    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(phonix_db::DbError::Query)?;

    let key = SequenceKey::new(app_books::APP_ID, app_books::SALES_INVOICE);
    let allocated = match generator.next(&mut tx, key, invoice.issued_on).await {
        Ok(allocated) => allocated,
        // The series is missing or switched off. Rolled back rather than left
        // half-open, and reported as an outcome because the fix is a settings
        // screen rather than a retry.
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(phonix_db::DbError::Query)?;
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
        // Somebody else posted it between the read and the write. Rolling back
        // returns the number, which is the whole reason these are one
        // transaction.
        tx.rollback().await.map_err(phonix_db::DbError::Query)?;
        return Ok(PostOutcome::NotADraft);
    }

    let journal = match post_journal(pool, &mut tx, caller, &invoice, &allocated.number).await {
        Ok(journal) => journal,
        Err(err) => {
            // A closed period, a missing rate, a role with no account behind
            // it. Both numbers go back and the document stays a draft, which
            // is what makes the refusal actionable rather than a mess to
            // unpick.
            tx.rollback().await.map_err(phonix_db::DbError::Query)?;
            return Err(err);
        }
    };

    tx.commit().await.map_err(phonix_db::DbError::Query)?;

    // Recorded after the commit, and best-effort like every audit write: losing
    // a trail row is bad, refusing a post because the trail is unwritable is
    // worse.
    let after = find(pool, caller, id).await?;
    let mut target = Target::new(kinds::SALES_INVOICE, id)
        .named(&invoice.party.name)
        .fact("number", &allocated.number)
        .fact("total", invoice.totals.gross.to_display_string());

    if let Some(journal_id) = journal {
        let posted = super::journal::detail_of(pool, journal_id).await?;
        target = target.fact("journal", &posted.number);
        super::journal::record(pool, caller, &posted).await;
    }

    audit::updated(pool, caller, target, &InvoiceStatus::Draft, &after.status).await;

    Ok(PostOutcome::Posted {
        number: allocated.number,
    })
}

/// What the invoice does to the ledger, written into the transaction that is
/// posting it.
///
/// `None` where it does nothing: a document at no charge is a legitimate
/// invoice and moves no money, and a journal of three zeroes is not a better
/// record of that than no journal at all - see [`app_books::posting`].
///
/// The lookups it makes - the workspace's currency, the rate for the day, what
/// each role means here - read the pool rather than the transaction. None of
/// them touch anything the transaction has written, and doing them on the
/// connection that holds two sequence locks would hold those locks for the
/// duration of every one.
async fn post_journal(
    pool: &PgPool,
    tx: &mut phonix_db::sqlx::PgConnection,
    caller: &Caller,
    invoice: &Invoice,
    number: &str,
) -> ServiceResult<Option<Uuid>> {
    // The document as it will be the moment this commits: numbered, and
    // posted. Assembled here rather than re-read, because the row it would be
    // read from is inside a transaction nobody else can see yet.
    let mut document = invoice.clone();
    document.status = InvoiceStatus::Posted;
    document.number = Some(number.to_owned());

    let Some(request) = app_books::posting::sales_invoice(&document) else {
        return Ok(None);
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

/// Withdraw a posted invoice, and reverse what it posted.
///
/// It keeps its number: a number that disappears is a gap, and a gap is what an
/// auditor asks about. What it loses is its claim on anybody - and the ledger
/// has to lose it too, or the receivable stays on the balance sheet under a
/// document that has been withdrawn.
///
/// # The reversal is dated today, not the day of the invoice
///
/// A mistake found in April is April's event even when the mistake was March's,
/// and March may well be closed by now. Back-dating the correction into the
/// period being corrected would mean a withdrawal could only ever happen in a
/// month still open - which is to say, not when it is actually noticed.
///
/// `today` is the *server's* day. The screen never sends one: a date read in
/// the browser is the browser's timezone and its clock, and neither belongs in
/// a ledger.
pub async fn void(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<()> {
    caller.require(permissions::INVOICES_VOID)?;
    acting_user(caller)?;

    let invoice = find(pool, caller, id).await?;
    if invoice.status != InvoiceStatus::Posted {
        return Err(ServiceError::rejected(
            "status",
            msg!("books.error.not_voidable"),
        ));
    }

    let today = chrono::Utc::now().date_naive();

    // The reversal is prepared before the transaction opens, for the reason
    // everything else is: it reads several tables and does not need the
    // sequence's lock held while it does.
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
        // Nothing was posted, so there is nothing to take back. An invoice at
        // no charge, or one raised before this app posted anything at all.
        None => None,
    };

    let mut tx = pool.begin().await.map_err(phonix_db::DbError::Query)?;

    if !store::void(&mut *tx, id, caller.user_id()).await? {
        tx.rollback().await.map_err(phonix_db::DbError::Query)?;
        return Err(ServiceError::rejected(
            "status",
            msg!("books.error.not_voidable"),
        ));
    }

    let written = match reversal {
        Some(ready) => match super::journal::write(&mut tx, &ready, caller).await {
            Ok(written) => Some(written),
            Err(err) => {
                tx.rollback().await.map_err(phonix_db::DbError::Query)?;
                return Err(err);
            }
        },
        None => None,
    };

    tx.commit().await.map_err(phonix_db::DbError::Query)?;

    let mut target = Target::new(kinds::SALES_INVOICE, id)
        .named(&invoice.party.name)
        // Recorded because the document keeps it, and "which number was
        // withdrawn" is the question a gap in the sequence provokes.
        .fact("number", invoice.number.clone().unwrap_or_default())
        .fact("total", invoice.totals.gross.to_display_string());

    if let Some(written) = written {
        target = target.fact("reversal", &written.number);

        let posted = super::journal::detail_of(pool, written.id).await?;
        super::journal::record(pool, caller, &posted).await;
    }

    audit::updated(
        pool,
        caller,
        target,
        &InvoiceStatus::Posted,
        &InvoiceStatus::Voided,
    )
    .await;

    Ok(())
}

/// Remove a draft.
///
/// Only a draft, and the statement says so too. A posted invoice has a number,
/// and a numbered document that vanishes is the gap the sequence design exists
/// to prevent - voiding is what withdraws one.
pub async fn delete(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<()> {
    caller.require(permissions::INVOICES_EDIT)?;
    acting_user(caller)?;

    let invoice = find(pool, caller, id).await?;
    if !invoice.is_editable() {
        return Err(ServiceError::rejected(
            "status",
            msg!("books.error.not_deletable"),
        ));
    }

    if !store::delete_draft(pool, id).await? {
        return Err(ServiceError::rejected(
            "status",
            msg!("books.error.not_deletable"),
        ));
    }

    audit::deleted(
        pool,
        caller,
        Target::new(kinds::SALES_INVOICE, id)
            .named(&invoice.party.name)
            .fact("total", invoice.totals.gross.to_display_string()),
        &InvoiceInput::from_invoice(&invoice),
    )
    .await;

    Ok(())
}

/// The journal this invoice raised, if it raised one.
///
/// Gated on reading invoices rather than on reading journals. Somebody allowed
/// to look at an invoice is allowed to be told what it did to the books; making
/// this the ledger's permission would hide the consequence from the person
/// responsible for the document.
///
/// `None` for a draft, for an invoice at no charge, and for one posted before
/// this app posted anything at all.
pub async fn journal_of(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
) -> ServiceResult<Option<(Uuid, String)>> {
    caller.require(permissions::INVOICES)?;

    Ok(phonix_db::books::journal::of_document(pool, id).await?)
}

/// Every active tax treatment, resolved for a date.
///
/// What a document screen fetches once so the browser can price locally. Gated
/// on `Invoices` rather than on `Master.Taxes`: this is somebody raising an
/// invoice, not somebody editing the tax tables.
pub async fn treatments(
    pool: &PgPool,
    caller: &Caller,
    on: NaiveDate,
) -> ServiceResult<Vec<TaxTreatment>> {
    caller.require(permissions::INVOICES)?;
    crate::master::tax::treatments_on(pool, on).await
}

// --- the work ------------------------------------------------------------

/// Copy the customer onto the document.
///
/// The *registered* name where there is one, because an invoice is a legal
/// instrument and names the entity rather than its trading style. The billing
/// address, falling back the way [`Party::address_for`] falls back.
async fn snapshot_party(pool: &PgPool, party_id: Uuid) -> ServiceResult<Option<PartySnapshot>> {
    let Some(party) = phonix_db::master::party::find(pool, party_id).await? else {
        return Ok(None);
    };

    Ok(Some(PartySnapshot {
        party_id: party.id,
        code: party.code.clone(),
        name: party.document_name().to_owned(),
        tax_id: party.tax_id.clone(),
        address: party.postal_address(AddressPurpose::Billing),
    }))
}

/// Why an invoice could not be priced, in a shape a form can render.
enum Rejection {
    Message(&'static str, Message),
}

impl Rejection {
    fn into_submission(self) -> Submission<InvoiceInput> {
        match self {
            Self::Message(field, message) => Submission::rejected(field, message),
        }
    }
}

/// Resolve every line's tax treatment and add the document up.
///
/// The treatments are resolved against the **document's own date**, not today:
/// a backdated invoice is charged at the rate that was in force when it was
/// issued, which is the whole reason rates are effective-dated.
async fn price(
    pool: &PgPool,
    checked: &CheckedInvoice,
) -> ServiceResult<Result<DocumentTax, Rejection>> {
    let mut lines = Vec::with_capacity(checked.lines.len());

    for line in &checked.lines {
        let treatment = match line.tax_group_id {
            // A line outside the scope of tax. Not the same as a zero-rated
            // one, which is a group whose rate is zero.
            None => TaxTreatment::none(),
            Some(group_id) => {
                match crate::master::tax::treatment_on(pool, group_id, checked.issued_on).await {
                    Ok(treatment) => treatment,
                    // The group is gone, switched off, or one of its taxes has
                    // no rate on that date. All three are things to fix on the
                    // line, and the tax service already said which.
                    Err(ServiceError::Rejected(errors)) => {
                        let message = errors.first().map_or_else(
                            || msg!("books.error.cannot_price"),
                            |e| e.message.clone(),
                        );
                        return Ok(Err(Rejection::Message("lines", message)));
                    }
                    Err(err) => return Err(err),
                }
            }
        };

        lines.push(PricedLine {
            quantity: line.quantity,
            unit_price: line.unit_price,
            treatment,
        });
    }

    let priced = PricedInvoice {
        currency: checked.currency,
        pricing: checked.pricing,
        rounding_level: checked.rounding_level,
        rounding: checked.rounding,
        lines,
    };

    match priced.compute() {
        Ok(totals) => Ok(Ok(totals)),
        Err(err) => Ok(Err(Rejection::Message("lines", err.message()))),
    }
}

/// The conversion snapshot, when the invoice is not in the workspace's own
/// currency.
///
/// `None` when it is: there is nothing to convert, and a rate of one is not
/// evidence of a quotation somebody published.
///
/// A missing rate is refused rather than assumed. An invoice whose base amount
/// was invented is one that reconciles against nothing, and the fix - record
/// the rate - is a screen away.
async fn conversion_for(
    pool: &PgPool,
    invoice: &Invoice,
) -> ServiceResult<Option<(phonix_core::money::ExchangeRate, Money)>> {
    let base: Currency = phonix_db::organization::load(pool).await?.profile.currency;
    if base == invoice.currency {
        return Ok(None);
    }

    let Some(rate) =
        crate::currency::rate_on(pool, invoice.currency, base, invoice.issued_on, None).await?
    else {
        return Err(ServiceError::rejected(
            "currency",
            msg!(
                "books.error.no_rate",
                pair = format!("{}/{}", invoice.currency.code(), base.code())
            ),
        ));
    };

    // Converted once, from the document's own total, and stored beside the rate
    // it was converted at. Recomputing this later from today's rate is the
    // classic bug and it silently rewrites history.
    let converted = invoice
        .totals
        .gross
        .convert(&rate, Rounding::HalfUp)
        .map_err(|err| ServiceError::rejected("currency", err.message()))?;

    Ok(Some((rate, converted.base_amount)))
}
