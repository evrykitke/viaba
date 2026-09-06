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

use app_books::journal::{JournalEntry, JournalSummary, Posted};
use chrono::NaiveDate;
use phonix_core::locale::Currency;
use phonix_core::money::Rounding;
use phonix_core::msg;
use phonix_core::permissions;
use phonix_db::books::journal as store;
use phonix_db::error::DbError;
use phonix_db::numbering::SequenceKey;
use phonix_db::sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, Target, kinds};
use crate::caller::{Caller, acting_user};
use crate::error::{ServiceError, ServiceResult};

pub use phonix_db::books::journal::JournalQuery;

/// Journals a list screen should show.
pub async fn list(
    pool: &PgPool,
    caller: &Caller,
    query: JournalQuery,
) -> ServiceResult<Vec<JournalSummary>> {
    caller.require(permissions::JOURNALS)?;
    Ok(store::list(pool, &query).await?)
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

    // Outside the transaction for the same reason. Every journal queues through
    // the sequence's one row, so anything that can happen before the lock does.
    let generator = crate::numbering::NumberGenerator::open(pool).await?;
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let key = SequenceKey::new(app_books::APP_ID, app_books::JOURNAL);
    let allocated = match generator.next(&mut tx, key, entry.entry_date()).await {
        Ok(allocated) => allocated,
        // The series is missing or switched off. Rolled back rather than left
        // half-open; the fix is a settings screen, not a retry.
        Err(ServiceError::Db(DbError::UnusableSequence { .. })) => {
            tx.rollback().await.map_err(DbError::Query)?;
            return Err(ServiceError::rejected(
                "number",
                msg!("journals.error.no_series"),
            ));
        }
        Err(err) => return Err(err),
    };

    let id = match store::insert(&mut tx, &entry, &allocated.number, period.id, caller.user_id())
        .await
    {
        Ok(id) => id,
        Err(err) => {
            // Rolling back returns the number.
            tx.rollback().await.map_err(DbError::Query)?;

            // A line naming an account that is not there. Reported on the
            // lines rather than as a fault: it is reachable from a stale
            // picker, which is somebody else having tidied the chart.
            if names_a_missing_row(&err) {
                return Err(ServiceError::rejected(
                    "lines",
                    msg!("journals.error.unknown_account"),
                ));
            }

            return Err(err.into());
        }
    };

    tx.commit().await.map_err(DbError::Query)?;

    let posted = detail_unchecked(pool, id).await?;

    audit::created(
        pool,
        caller,
        Target::new(kinds::JOURNAL, id)
            .named(&posted.number)
            .fact("period", &posted.period_label)
            .fact("source", &posted.source.doc_type),
        &posted,
    )
    .await;

    Ok(posted)
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

    let original = detail(pool, caller, id).await?;

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

    let entry = app_books::journal::JournalEntry::reversal_of(&original, on, narration)
        .map_err(|err| ServiceError::rejected(err.field(), err.message()))?;

    post_unchecked(pool, caller, entry).await
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
    Ok(crate::workspace::profile::current(pool).await?.currency)
}
