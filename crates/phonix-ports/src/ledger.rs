//! Posting a journal without knowing who keeps the books.
//!
//! Declared by the apps that *need* a ledger - Inventory first, then sales and
//! payroll - and implemented by whoever provides one. `app-inventory` depends
//! on this crate and on nothing of `app-books` at all, which is the whole point
//! of ADR 0006 section 2: a build without Books still compiles, and a workspace
//! that never bought the accounting module still receives goods.
//!
//! # The caller names a role, not an account id
//!
//! A goods receipt knows it is increasing stock and increasing goods-received-
//! not-invoiced. It does not know, and must not know, which account numbers
//! those are - the chart belongs to Books and a workspace may renumber it
//! tomorrow. So a posting names an [`AccountRole`] and the ledger decides.
//!
//! This is what an accounting system calls *account determination*, and getting
//! it backwards - having each sub-ledger store account ids - is how a chart
//! becomes unrenumberable.
//!
//! # A missing ledger is an answer
//!
//! There is no implementation when Books is not compiled in or not switched on.
//! [`NoLedger`] answers [`LedgerError::NoLedger`], and the caller has to have
//! decided in advance what that means. Inventory's answer is in its own docs:
//! the stock movement still happens and records that no ledger was available.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::PortError;

/// The name this port is known by in a log line or an error.
pub const PORT: &str = "ledger";

/// What an account is *for*, as a caller across an app boundary knows it.
///
/// A closed set, because each one is a promise the ledger has to be able to
/// keep. Adding a role is a change to both sides and should be: a sub-ledger
/// that could ask for an arbitrary account would be reaching into the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountRole {
    /// Stock on hand, at cost. The control account a stock ledger reconciles
    /// to.
    Inventory,
    /// Goods received and not yet invoiced. The liability that exists from the
    /// moment goods arrive rather than from the moment the paperwork does -
    /// ADR 0006 section 6.5.
    GoodsReceivedNotInvoiced,
    /// What is owed to suppliers.
    AccountsPayable,
    /// The difference between the price ordered and the price billed, posted
    /// rather than absorbed silently into the stock value.
    PurchasePriceVariance,
    /// Freight, duty and handling capitalised into the value of goods.
    LandedCost,
    /// Where a stock adjustment's other side goes when its type names no
    /// account of its own.
    InventoryAdjustment,
    /// Stock that has left one location and not yet arrived at another. On the
    /// balance sheet and in neither place - the third state a transfer needs.
    InventoryInTransit,
    /// What stock cost when it was sold.
    CostOfSales,
}

impl AccountRole {
    pub const ALL: &'static [Self] = &[
        Self::Inventory,
        Self::GoodsReceivedNotInvoiced,
        Self::AccountsPayable,
        Self::PurchasePriceVariance,
        Self::LandedCost,
        Self::InventoryAdjustment,
        Self::InventoryInTransit,
        Self::CostOfSales,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inventory => "inventory",
            Self::GoodsReceivedNotInvoiced => "goods_received_not_invoiced",
            Self::AccountsPayable => "accounts_payable",
            Self::PurchasePriceVariance => "purchase_price_variance",
            Self::LandedCost => "landed_cost",
            Self::InventoryAdjustment => "inventory_adjustment",
            Self::InventoryInTransit => "inventory_in_transit",
            Self::CostOfSales => "cost_of_sales",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|role| role.as_str() == raw)
    }
}

/// Which side of the entry an amount sits on.
///
/// Declared here rather than borrowed from `app-books`, because a port may not
/// name a type belonging to either app. It is the same two words, and that is
/// the price of the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Debit,
    Credit,
}

/// One line a caller wants posted.
///
/// The amount is a decimal as text for the reason every amount in this codebase
/// crosses a boundary as text: the ledger's own `Money` is exact, and a float
/// in between would not be. Always positive - the side says which way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posting {
    pub role: AccountRole,
    pub side: Side,
    /// Decimal digits, in the currency named on the entry.
    pub amount: String,
    /// What this line is for, where the entry's narration is not enough.
    pub memo: Option<String>,
    /// The cost centre this line is charged to, resolved by the caller through
    /// [`CostCentres`](crate::CostCentres) before it gets here.
    pub cost_centre_id: Option<Uuid>,
}

/// A journal a sub-ledger wants written.
///
/// Deliberately not balanced-by-construction: the ledger checks that, because
/// the ledger is what balance means. A caller that hands over an unbalanced set
/// gets [`LedgerError::Unbalanced`] and has a bug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRequest {
    pub entry_date: NaiveDate,
    pub narration: String,
    /// The app asking - `inventory`. Stored on the journal so reconciling a
    /// sub-ledger to the general ledger is a `GROUP BY` rather than an
    /// investigation.
    pub source_app: String,
    /// The kind of document behind it - `goods_receipt`, `stock_adjustment`.
    pub source_doc_type: String,
    pub source_doc_id: Uuid,
    /// ISO 4217. The ledger converts to its own base currency and refuses if it
    /// has no rate on file for the date.
    pub currency: String,
    pub postings: Vec<Posting>,
}

/// Where the ledger filed it.
///
/// Returned rather than `()` so the caller can store it beside its own
/// document: that id is how the sub-ledger is reconciled to the general ledger
/// afterwards, and it is the only reason this is not fire-and-forget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostedRef {
    pub journal_id: Uuid,
    pub number: String,
}

/// Why a journal was not posted.
///
/// Every variant is something the caller has to decide about, which is why this
/// is not one opaque error. `NoLedger` in particular is not a failure - it is
/// the workspace not having bought the accounting module.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    /// Nobody implements this port here. Not a fault: the caller carries on
    /// without a journal, and records that it did.
    #[error("no ledger is available in this workspace")]
    NoLedger,

    /// The debits and the credits do not agree. A bug in the caller.
    #[error("the postings do not balance")]
    Unbalanced,

    /// No account is mapped to a role this posting names, so the ledger cannot
    /// tell where the amount goes.
    #[error("no account is set up for '{0}'")]
    UnmappedRole(&'static str),

    /// The period the entry falls in is closed, or the calendar does not reach
    /// it. The caller's document may still be right; the date is not.
    #[error("{0}")]
    PeriodClosed(String),

    /// Everything else the ledger refused, in its own words.
    #[error("{0}")]
    Refused(phonix_core::Message),

    /// The ledger is there and could not answer.
    #[error("the ledger failed: {0}")]
    Unavailable(String),
}

impl From<PortError> for LedgerError {
    fn from(err: PortError) -> Self {
        match err {
            PortError::Refused(message) => Self::Refused(message),
            PortError::Unavailable { detail, .. } => Self::Unavailable(detail),
        }
    }
}

/// Somewhere to post a journal.
#[async_trait::async_trait]
pub trait Ledger: Send + Sync {
    /// Post a balanced journal, or refuse the whole of it.
    async fn post(&self, entry: JournalRequest) -> Result<PostedRef, LedgerError>;

    /// Whether a role has an account behind it, for a screen that wants to warn
    /// before somebody does the work rather than after.
    ///
    /// Advisory only. [`Self::post`] checks again, because between the two calls
    /// somebody may have retired the account.
    async fn is_mapped(&self, role: AccountRole) -> Result<bool, LedgerError>;
}

/// A port with nobody behind it: Books is not compiled in, or the workspace has
/// not switched it on.
///
/// Answers rather than failing. What a caller does with the answer is the
/// caller's decision, and it has to have made one: receiving goods is a
/// warehouse fact, and a warehouse does not stop because nobody bought the
/// accounting module.
pub struct NoLedger;

#[async_trait::async_trait]
impl Ledger for NoLedger {
    async fn post(&self, _entry: JournalRequest) -> Result<PostedRef, LedgerError> {
        Err(LedgerError::NoLedger)
    }

    async fn is_mapped(&self, _role: AccountRole) -> Result<bool, LedgerError> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_round_trips_and_is_unique() {
        for (index, role) in AccountRole::ALL.iter().enumerate() {
            assert_eq!(AccountRole::parse(role.as_str()), Some(*role));
            assert!(!AccountRole::ALL[..index].contains(role), "{role:?}");
        }

        assert_eq!(AccountRole::parse("not_a_role"), None);
    }

    #[tokio::test]
    async fn an_absent_ledger_says_so_rather_than_failing_silently() {
        // The distinction the whole design rests on: "there is no ledger" is an
        // answer a caller can act on, and it is not the same as a posting that
        // went wrong.
        let port = NoLedger;

        let request = JournalRequest {
            entry_date: NaiveDate::from_ymd_opt(2026, 3, 14).unwrap(),
            narration: "Goods received".to_owned(),
            source_app: "inventory".to_owned(),
            source_doc_type: "goods_receipt".to_owned(),
            source_doc_id: Uuid::nil(),
            currency: "GBP".to_owned(),
            postings: Vec::new(),
        };

        assert_eq!(port.post(request).await, Err(LedgerError::NoLedger));
        assert_eq!(port.is_mapped(AccountRole::Inventory).await, Ok(false));
    }
}
