//! Books' side of the `Ledger` port.
//!
//! What another app calls when its document has an accounting consequence.
//! Implemented here rather than in `app-books`, which compiles to wasm and has
//! no database - the same shape as `hr::cost_centre`: rules in the app crate,
//! statements in `phonix-db`, the seam in a service.
//!
//! # It takes no permission check
//!
//! Ungated, deliberately. The calling app has already asked whether its own
//! caller may receive goods or write off stock; making them separately hold the
//! journal permission would mean every storekeeper holds the ledger's. The
//! [`Caller`] is carried for `posted_by` and the audit trail, not as an
//! authorisation.
//!
//! # Roles, not account ids
//!
//! A posting names [`AccountRole::Inventory`]; this looks up what that means in
//! this workspace. An unmapped role is refused by name, because the fix is a
//! setting somebody can change rather than anything the caller did wrong.

use app_books::journal::{DimensionValue, Dimension, JournalEntry, JournalLineInput, Source};
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_db::books::account_role as roles;
use phonix_db::sqlx::PgPool;
use phonix_ports::ledger::{AccountRole, JournalRequest, Ledger, LedgerError, PostedRef, Side};

use crate::caller::Caller;
use crate::error::ServiceError;

/// The `Ledger` port, over this workspace's books.
///
/// Owns its pool and its caller so it can be handed over as a `dyn Ledger` with
/// no lifetime to thread - the same reason `HrCostCentres` owns its pool.
#[derive(Clone)]
pub struct BooksLedger {
    pool: PgPool,
    caller: Caller,
}

impl BooksLedger {
    pub const fn new(pool: PgPool, caller: Caller) -> Self {
        Self { pool, caller }
    }
}

#[async_trait::async_trait]
impl Ledger for BooksLedger {
    async fn post(&self, request: JournalRequest) -> Result<PostedRef, LedgerError> {
        let base = crate::workspace::profile::current(&self.pool)
            .await
            .map_err(unavailable)?
            .currency;

        let currency = Currency::parse(&request.currency)
            .map_err(|err| LedgerError::Unavailable(err.to_string()))?;

        // The rate for the document's own date, never today's. A base amount
        // recomputed later from a newer rate silently restates a filed period.
        let rate = if currency == base {
            None
        } else {
            Some(
                crate::currency::rate_on(&self.pool, currency, base, request.entry_date, None)
                    .await
                    .map_err(unavailable)?
                    .ok_or_else(|| {
                        LedgerError::Refused(phonix_core::msg!(
                            "journals.error.no_rate",
                            pair = format!("{}/{}", currency.code(), base.code()),
                            date = request.entry_date
                        ))
                    })?,
            )
        };

        let centres = crate::hr::HrCostCentres::new(self.pool.clone());
        let mut lines = Vec::with_capacity(request.postings.len());

        for posting in &request.postings {
            let account_id = self.account_for(posting.role).await?;

            let amount = Money::parse(currency, posting.amount.trim())
                .map_err(|err| LedgerError::Refused(err.message()))?;

            let (base_amount, exchange_rate) = match rate.as_ref() {
                None => (amount, "1".to_owned()),
                Some(rate) => {
                    let converted = amount
                        .convert(rate, Rounding::HalfUp)
                        .map_err(|err| LedgerError::Refused(err.message()))?;

                    (converted.base_amount, rate.rate.to_storage_string())
                }
            };

            let mut line = JournalLineInput {
                account_id,
                side: match posting.side {
                    Side::Debit => app_books::account::Side::Debit,
                    Side::Credit => app_books::account::Side::Credit,
                },
                amount,
                base_amount,
                exchange_rate,
                rate_date: request.entry_date,
                memo: posting.memo.clone(),
                dimensions: Vec::new(),
            };

            if let Some(cost_centre_id) = posting.cost_centre_id {
                use phonix_ports::CostCentres;

                // Resolved rather than trusted: the caller passed an id, and
                // what a journal stores is the code and name as they are now.
                if let Some(centre) = centres.resolve(cost_centre_id).await? {
                    line = line.charged_to(DimensionValue {
                        dimension: Dimension::CostCentre,
                        id: centre.id,
                        code: centre.code,
                        name: centre.name,
                    });
                }
            }

            lines.push(line);
        }

        // Where balance is enforced. A caller that hands over an unbalanced set
        // has a bug, and this is where it finds out.
        let entry = JournalEntry::assemble(
            request.entry_date,
            request.narration,
            Source::new(
                request.source_app,
                request.source_doc_type,
                Some(request.source_doc_id),
            ),
            lines,
        )
        .map_err(|err| match err {
            app_books::journal::JournalError::Unbalanced => LedgerError::Unbalanced,
            other => LedgerError::Refused(other.message()),
        })?;

        let posted = super::journal::post_unchecked(&self.pool, &self.caller, entry)
            .await
            .map_err(as_ledger_error)?;

        Ok(PostedRef {
            journal_id: posted.id,
            number: posted.number,
        })
    }

    async fn is_mapped(&self, role: AccountRole) -> Result<bool, LedgerError> {
        Ok(roles::account_for(&self.pool, role.as_str())
            .await
            .map_err(|err| LedgerError::Unavailable(err.to_string()))?
            .is_some())
    }
}

impl BooksLedger {
    /// What a role means here, or a refusal naming the role that has no home.
    async fn account_for(&self, role: AccountRole) -> Result<uuid::Uuid, LedgerError> {
        roles::account_for(&self.pool, role.as_str())
            .await
            .map_err(|err| LedgerError::Unavailable(err.to_string()))?
            .ok_or(LedgerError::UnmappedRole(role.as_str()))
    }
}

/// A closed period is the caller's date being wrong rather than the ledger
/// failing, so it keeps its own variant. Everything else the ledger refused
/// arrives with the ledger's own words, which are better than a house phrase.
fn as_ledger_error(err: ServiceError) -> LedgerError {
    match &err {
        ServiceError::Rejected(fields) => match fields.first() {
            Some(field) if field.field == "entry_date" => {
                LedgerError::PeriodClosed(err.to_string())
            }
            Some(field) => LedgerError::Refused(field.message.clone()),
            None => LedgerError::Unavailable(err.to_string()),
        },
        _ => LedgerError::Unavailable(err.to_string()),
    }
}

fn unavailable(err: ServiceError) -> LedgerError {
    LedgerError::Unavailable(err.to_string())
}
