//! Posting to the general ledger.
//!
//! # This module writes journals and never changes one
//!
//! There is a `post` and there is a `reverse`, and no `save`, `edit` or
//! `delete`. That is not an omission - it is rule 2 of ADR 0006 section 5. A
//! posted journal is evidence, and a document that can be edited after it was
//! filed is evidence of nothing.
//!
//! # What is checked, and where
//!
//! | Rule | Enforced by |
//! | --- | --- |
//! | debits equal credits | `JournalEntry::assemble` - it cannot be built otherwise |
//! | one base currency | the same, and again here against the workspace's own |
//! | the period is open | [`post`], through `period::for_posting` |
//! | the accounts exist | the foreign key, reported as a refusal |
//! | the number is gap-free | allocated inside the write's transaction |
//!
//! The base currency is the accountant's decision, taken from the organization
//! profile. A journal that converts to anything else is refused rather than
//! quietly stored: two base currencies in one ledger is two sets of books.
//!
//! # A foreign currency needs a rate on file
//!
//! A line in anything but the base currency is checked against
//! `core.exchange_rates` for its own date, and the base amount it declares has
//! to be what that rate produces. Not a courtesy check - it is the difference
//! between a conversion that is evidence and one somebody typed. A rate that
//! has not been recorded is a refusal, never a fallback of one: a base amount
//! worked out later from today's rate silently restates history, which is the
//! classic bug this ledger exists to avoid.

use app_books::journal::{
    Dimension, DimensionValue, JournalDraft, JournalDraftLine, JournalEntry, JournalSummary,
    Posted, Source,
};
use chrono::NaiveDate;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::msg;
use phonix_core::permissions;
use phonix_core::query::{Page, PageRequest};
use phonix_db::books::journal as store;
use phonix_db::error::DbError;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};
use phonix_ports::CostCentres;

pub use phonix_db::books::journal::JournalQuery;

/// One page of what a list screen should show.
pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    query: JournalQuery,
    request: PageRequest,
) -> ServiceResult<Page<JournalSummary>> {
    caller.require(permissions::JOURNALS)?;
    Ok(store::page(pool, &query, &request).await?)
}

/// One journal, with its lines.
pub async fn detail(pool: &PgPool, caller: &Caller, id: Uuid) -> ServiceResult<Posted> {
    caller.require(permissions::JOURNALS)?;

    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("journal", msg!("journals.gone")))
}

/// Post a journal.
///
/// The one write in this module. Takes a [`JournalEntry`], which cannot exist
/// unbalanced, so nothing here re-checks the arithmetic - there is no way to
/// call this with a journal that does not balance.
pub async fn post(pool: &PgPool, caller: &Caller, entry: JournalEntry) -> ServiceResult<Posted> {
    caller.require(permissions::JOURNALS_POST)?;
    acting_user(caller)?;

    post_unchecked(pool, caller, entry).await
}

/// Post without asking whether the caller may.
///
/// For the postings a *document* makes - an invoice, one day a goods receipt.
/// The permission that governs those is the document's own: somebody allowed to
/// post an invoice does not separately need the journal permission, and
/// requiring it would mean every salesperson holds the ledger's.
pub(crate) async fn post_unchecked(
    pool: &PgPool,
    caller: &Caller,
    entry: JournalEntry,
) -> ServiceResult<Posted> {
    let ready = ready(pool, entry).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let id = match write(&mut tx, &ready, caller).await {
        Ok(written) => written.id,
        Err(err) => {
            // Rolling back returns the number.
            tx.rollback().await.map_err(DbError::Query)?;
            return Err(err);
        }
    };

    tx.commit().await.map_err(DbError::Query)?;

    let posted = detail_unchecked(pool, id).await?;
    record(pool, caller, &posted).await;

    Ok(posted)
}

/// Put a journal on the audit trail.
///
/// After the commit, and best-effort like every audit write: losing a trail row
/// is bad, refusing a post because the trail is unwritable is worse.
///
/// Called by whoever committed - which is [`post_unchecked`] for a journal
/// somebody typed, and the *document* for a journal a document raised inside
/// its own transaction. Both leave the same row, because "what posted this" is
/// a question about the ledger and not about which code path wrote it.
pub(crate) async fn record(pool: &PgPool, caller: &Caller, posted: &Posted) {
    audit::created(
        pool,
        caller,
        Target::new(kinds::JOURNAL, posted.id)
            .named(&posted.number)
            .fact("period", &posted.period_label)
            .fact("source", &posted.source.doc_type),
        posted,
    )
    .await;
}

/// One journal, read without asking whether the caller may read journals.
///
/// For the document that just raised it: somebody allowed to post an invoice
/// does not separately need the ledger's permission to be told what the invoice
/// posted.
pub(crate) async fn detail_of(pool: &PgPool, id: Uuid) -> ServiceResult<Posted> {
    detail_unchecked(pool, id).await
}

/// A journal with everything settled that can be settled before a transaction
/// opens: the base currency checked, the rates checked, the period resolved and
/// the allocator open.
///
/// Split out from [`post_unchecked`] so a *document* can write its own journal
/// inside its own transaction - see [`crate::books::invoice::post`]. Every
/// journal queues through one sequence row, so the less that happens while that
/// lock is held, the better; this type is the line between the two.
pub(crate) struct Ready {
    entry: JournalEntry,
    period_id: Uuid,
    generator: crate::numbering::NumberGenerator,
}

/// What a written journal is, to a caller that has not committed yet.
pub(crate) struct Written {
    pub id: Uuid,
    pub number: String,
}

/// Check a journal against the workspace and find it a period.
///
/// Reads only. Nothing here writes, so a caller may do this before deciding
/// whether to open a transaction at all.
pub(crate) async fn ready(pool: &PgPool, entry: JournalEntry) -> ServiceResult<Ready> {
    let base = base_currency(pool).await?;

    // The accountant set this. A journal converting to anything else is two
    // sets of books, so it is refused rather than stored.
    for line in entry.lines() {
        if line.base_amount.currency() != base {
            return Err(ServiceError::rejected(
                "lines",
                msg!(
                    "journals.error.wrong_base_currency",
                    expected = base.code(),
                    found = line.base_amount.currency().code()
                ),
            ));
        }
    }

    check_rates(pool, &entry, base).await?;

    // Rule 4. Resolved before the transaction opens: it reads one row and does
    // not need the sequence's lock held while it does.
    let period = crate::books::period::for_posting(pool, entry.entry_date()).await?;
    let generator = crate::numbering::NumberGenerator::open(pool).await?;

    Ok(Ready {
        entry,
        period_id: period.id,
        generator,
    })
}

/// Number a prepared journal and store it, in a transaction the caller owns.
///
/// The caller rolls back on an error, which is what returns the number: the
/// `UPDATE` that allocates one holds a row lock until the transaction ends, so
/// a post that fails cannot burn a number and the series stays gap-free.
pub(crate) async fn write(
    tx: &mut phonix_db::sqlx::PgConnection,
    ready: &Ready,
    caller: &Caller,
) -> ServiceResult<Written> {
    let key = SequenceKey::new(app_books::APP_ID, app_books::JOURNAL);

    let allocated = match ready
        .generator
        .next(&mut *tx, key, ready.entry.entry_date())
        .await
    {
        Ok(allocated) => allocated,
        // The series is missing or switched off. The fix is a settings screen,
        // not a retry, so it is a refusal naming the field rather than a fault.
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            return Err(ServiceError::rejected(
                "number",
                msg!("journals.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    let id = store::insert(
        &mut *tx,
        &ready.entry,
        &allocated.number,
        ready.period_id,
        caller.user_id(),
    )
    .await
    .map_err(|err| {
        // A line naming an account that is not there. Reported on the lines
        // rather than as a fault: it is reachable from a stale picker, which is
        // somebody else having tidied the chart.
        if names_a_missing_row(&err) {
            ServiceError::rejected("lines", msg!("journals.error.unknown_account"))
        } else {
            err.into()
        }
    })?;

    Ok(Written {
        id,
        number: allocated.number,
    })
}

/// Turn what somebody typed into a journal, and post it.
///
/// The only path from a screen to the ledger. Everything the form could not
/// know is resolved here: what the workspace's own currency is, what the rate
/// was on the day, and what each cost centre is called - and only then is the
/// entry assembled, which is where the balance is enforced.
pub async fn post_draft(
    pool: &PgPool,
    caller: &Caller,
    draft: JournalDraft,
) -> ServiceResult<Posted> {
    caller.require(permissions::JOURNALS_POST)?;
    acting_user(caller)?;

    let base = base_currency(pool).await?;

    let Some(entry_date) = draft.entry_date else {
        return Err(ServiceError::rejected(
            "entry_date",
            msg!("journals.error.date_required"),
        ));
    };

    // Preselected on the form, so this is the base currency unless somebody
    // deliberately chose otherwise - and choosing otherwise is what needs a
    // rate on file.
    let currency = match draft.currency.as_deref() {
        None => base,
        Some(code) => Currency::parse(code)
            .map_err(|_| ServiceError::rejected("currency", msg!("journals.error.bad_currency")))?,
    };

    let rate = if currency == base {
        None
    } else {
        Some(
            crate::currency::rate_on(pool, currency, base, entry_date, None)
                .await?
                .ok_or_else(|| {
                    ServiceError::rejected(
                        "currency",
                        msg!(
                            "journals.error.no_rate",
                            pair = format!("{}/{}", currency.code(), base.code()),
                            date = entry_date
                        ),
                    )
                })?,
        )
    };

    let centres = crate::hr::HrCostCentres::new(pool.clone());
    let mut lines = Vec::new();

    for line in draft.filled_lines() {
        lines.push(resolve_line(line, currency, rate.as_ref(), entry_date, &centres).await?);
    }

    // `assemble` is what refuses an unbalanced journal, too few lines, or a
    // narration nobody wrote. Nothing above re-checks any of that.
    let entry = JournalEntry::assemble(entry_date, draft.narration, Source::manual(), lines)
        .map_err(|err| ServiceError::rejected(err.field(), err.message()))?;

    post_unchecked(pool, caller, entry).await
}

/// One typed row, with its amount parsed, its conversion applied and its cost
/// centre resolved through the port.
async fn resolve_line(
    line: &JournalDraftLine,
    currency: Currency,
    rate: Option<&phonix_core::money::ExchangeRate>,
    entry_date: NaiveDate,
    centres: &crate::hr::HrCostCentres,
) -> ServiceResult<app_books::journal::JournalLineInput> {
    let Some(account_id) = line.account_id else {
        return Err(ServiceError::rejected(
            "lines",
            msg!("journals.error.account_required"),
        ));
    };

    let Some(side) = line.side else {
        return Err(ServiceError::rejected(
            "lines",
            msg!("journals.error.side_required"),
        ));
    };

    let amount = Money::parse(currency, line.amount.trim())
        .map_err(|err| ServiceError::rejected("lines", err.message()))?;

    // Converted here and stored, never recomputed later from a newer rate -
    // which is the bug that silently restates a filed period.
    let (base_amount, exchange_rate) = match rate {
        None => (amount, "1".to_owned()),
        Some(rate) => {
            let converted = amount
                .convert(rate, Rounding::HalfUp)
                .map_err(|err| ServiceError::rejected("lines", err.message()))?;

            (converted.base_amount, rate.rate.to_storage_string())
        }
    };

    let mut input = app_books::journal::JournalLineInput {
        account_id,
        side,
        amount,
        base_amount,
        exchange_rate,
        rate_date: entry_date,
        memo: (!line.memo.trim().is_empty()).then(|| line.memo.trim().to_owned()),
        dimensions: Vec::new(),
    };

    if let Some(cost_centre_id) = line.cost_centre_id {
        // Through the port, so Books learns the code and name without knowing
        // that `hr` has tables. What comes back is stored as a snapshot.
        let centre = centres
            .resolve(cost_centre_id)
            .await
            .map_err(from_port)?
            .ok_or_else(|| {
                ServiceError::rejected("lines", msg!("journals.error.unknown_cost_centre"))
            })?;

        input = input.charged_to(DimensionValue {
            dimension: Dimension::CostCentre,
            id: centre.id,
            code: centre.code,
            name: centre.name,
        });
    }

    Ok(input)
}

/// Reverse a journal: every line, on the other side, as a second journal that
/// names the first.
///
/// Dated separately, because a mistake found in April is April's event even
/// when the mistake was March's - and March may well be closed by now. That is
/// the whole reason a reversal is a new journal rather than an undo.
pub async fn reverse(
    pool: &PgPool,
    caller: &Caller,
    id: Uuid,
    on: NaiveDate,
    narration: Option<String>,
) -> ServiceResult<Posted> {
    caller.require(permissions::JOURNALS_REVERSE)?;
    acting_user(caller)?;

    let entry = reversal_entry(pool, id, on, narration).await?;

    post_unchecked(pool, caller, entry).await
}

/// The reversing entry for a journal, checked and assembled but not posted.
///
/// Shared by [`reverse`], which posts it on its own, and by a document being
/// withdrawn, which writes it inside the same transaction that withdraws the
/// document - see [`crate::books::invoice::void`]. One place that decides what
/// a reversal contains and what refuses one.
pub(crate) async fn reversal_entry(
    pool: &PgPool,
    id: Uuid,
    on: NaiveDate,
    narration: Option<String>,
) -> ServiceResult<JournalEntry> {
    let original = detail_unchecked(pool, id).await?;

    // Asked before writing, so the refusal names the correction that already
    // exists rather than surfacing as a unique-index violation.
    if let Some(existing) = store::reversal_of(pool, id).await? {
        return Err(ServiceError::rejected(
            "journal",
            msg!("journals.error.already_reversed", number = existing),
        ));
    }

    let narration = narration.unwrap_or_else(|| {
        // A default that says what it is. Somebody reading the ledger in a
        // year should not have to open two documents to know why this exists.
        format!("Reverses {}", original.number)
    });

    app_books::journal::JournalEntry::reversal_of(&original, on, narration)
        .map_err(|err| ServiceError::rejected(err.field(), err.message()))
}

/// Every foreign-currency line converts at a rate the workspace has recorded.
///
/// Checked here rather than trusted from the caller because the caller may be
/// another app across a port. What is stored has to be reproducible from
/// `core.exchange_rates` a year later, or the conversion is not evidence.
async fn check_rates(pool: &PgPool, entry: &JournalEntry, base: Currency) -> ServiceResult<()> {
    for line in entry.lines() {
        let currency = line.amount.currency();

        if currency == base {
            continue;
        }

        let Some(rate) =
            crate::currency::rate_on(pool, currency, base, line.rate_date, None).await?
        else {
            return Err(ServiceError::rejected(
                "lines",
                msg!(
                    "journals.error.no_rate",
                    pair = format!("{}/{}", currency.code(), base.code()),
                    date = line.rate_date
                ),
            ));
        };

        let converted = line
            .amount
            .convert(&rate, Rounding::HalfUp)
            .map_err(|err| ServiceError::rejected("lines", err.message()))?;

        if converted.base_amount != line.base_amount {
            return Err(ServiceError::rejected(
                "lines",
                msg!(
                    "journals.error.rate_mismatch",
                    pair = format!("{}/{}", currency.code(), base.code()),
                    date = line.rate_date
                ),
            ));
        }
    }

    Ok(())
}

/// What a port failure means to the service that asked.
///
/// The two vocabularies are deliberately different - `PortError` lives below
/// every app and knows nothing about callers or permissions - so the mapping is
/// written out rather than derived. `Refused` is the provider answering no,
/// which belongs beside the control that caused it; `Unavailable` is the
/// provider failing to answer at all, which must fail this posting rather than
/// be read as "there are no cost centres".
fn from_port(err: phonix_ports::PortError) -> ServiceError {
    match err {
        phonix_ports::PortError::Refused(message) => ServiceError::rejected("lines", message),
        unavailable @ phonix_ports::PortError::Unavailable { .. } => {
            tracing::error!(error = %unavailable, "a port failed during a posting");
            ServiceError::rejected("lines", msg!("journals.error.port_unavailable"))
        }
    }
}

fn names_a_missing_row(err: &DbError) -> bool {
    matches!(
        err,
        DbError::Query(phonix_db::sqlx::Error::Database(db)) if db.is_foreign_key_violation()
    )
}

/// A journal by id, with no permission check. Used straight after writing one,
/// where the caller has already been allowed to write it.
async fn detail_unchecked(pool: &PgPool, id: Uuid) -> ServiceResult<Posted> {
    store::find(pool, id)
        .await?
        .ok_or_else(|| ServiceError::rejected("journal", msg!("journals.gone")))
}

/// What this workspace's books are kept in. The accountant's decision, on the
/// organization profile.
async fn base_currency(pool: &PgPool) -> ServiceResult<Currency> {
    crate::workspace::profile::base_currency(pool).await
}
