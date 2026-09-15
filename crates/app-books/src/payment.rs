//! A customer paying: what it holds, what it settles, and what is left over.
//!
//! # Three states, and only one of them can be edited
//!
//! The invoice's states, for the invoice's reasons. A draft has no number and
//! counts towards nothing. [`PaymentStatus::Posted`] is the document: numbered,
//! snapshotted and frozen, and the moment the money exists in the ledger.
//! [`PaymentStatus::Voided`] is one withdrawn - a cheque that bounced, a
//! transfer that was reversed - and it keeps its number.
//!
//! # Allocation is a relation, not a running total
//!
//! One cheque settles four invoices; one invoice is settled by three
//! instalments; a payment on account settles nothing yet. A `paid` column on
//! the invoice could express none of those, and would stop reconciling the
//! first time anything was corrected. So [`Allocation`] is a row, and what is
//! left over is the difference rather than a second number to keep in step.
//!
//! # What this deliberately refuses
//!
//! An allocation in a currency the invoice was not raised in. Settling a dollar
//! invoice with a euro cheque realises an exchange difference, which is a
//! posting this ledger does not make yet - and storing the number without the
//! posting would be a loss nobody ever sees. It is a refusal with a sentence
//! rather than a silent conversion.

use chrono::{DateTime, NaiveDate, Utc};
use phonix_core::identity::UserId;
use phonix_core::locale::Currency;
use phonix_core::money::{ExchangeRate, Money, MoneyError};
use phonix_core::{Message, msg};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_REFERENCE_LEN: usize = 120;
pub const MAX_NOTE_LEN: usize = 2000;

/// The most invoices one payment may be spread across.
///
/// Not a technical limit. It is the point past which the screen stops being
/// usable, and a ceiling that is stated is better than a timeout that is not.
pub const MAX_ALLOCATIONS: usize = 200;

/// Which way the money went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// A customer paying this workspace.
    #[default]
    In,
    /// This workspace paying a supplier. The mirror, and not built: nothing
    /// raises one and the service refuses it. It is named so that the day the
    /// purchase ledger wants one it is a value rather than a second table.
    Out,
}

impl Direction {
    pub const ALL: &'static [Self] = &[Self::In, Self::Out];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }
}

/// Where a payment is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    #[default]
    Draft,
    Posted,
    Voided,
}

impl PaymentStatus {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Posted, Self::Voided];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Posted => "posted",
            Self::Voided => "voided",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Whether this payment counts against what the workspace is owed.
    ///
    /// A draft is not money and a voided one has been withdrawn, so only a
    /// posted one does. The same predicate `InvoiceStatus::is_receivable` is,
    /// on the other side of the same subtraction.
    pub const fn settles(self) -> bool {
        matches!(self, Self::Posted)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("payments.status.draft"),
            Self::Posted => msg!("payments.status.posted"),
            Self::Voided => msg!("payments.status.voided"),
        }
    }
}

/// The customer, as the document records them.
///
/// A copy, not a reference - the same rule and the same reason as
/// [`crate::invoice::PartySnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayerSnapshot {
    pub party_id: Uuid,
    pub code: String,
    pub name: String,
}

impl PayerSnapshot {
    /// `ACME01 · Acme Fasteners`. One spelling for two screens.
    pub fn label(&self) -> String {
        format!("{} \u{b7} {}", self.code, self.name)
    }
}

/// One settlement: this much of this payment against that invoice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    pub id: Uuid,
    pub invoice_id: Uuid,
    /// The invoice's number, for the screen. `None` cannot happen for a posted
    /// invoice and is what a draft would be - which nothing may allocate to.
    pub invoice_number: Option<String>,
    pub issued_on: NaiveDate,
    pub due_on: Option<NaiveDate>,
    /// What the invoice came to, gross.
    pub invoiced: Money,
    /// How much of this payment settles it.
    pub amount: Money,
}

/// One payment, whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    /// `None` while it is a draft. Taken from the sequence at post.
    pub number: Option<String>,
    pub status: PaymentStatus,
    pub direction: Direction,
    pub party: PayerSnapshot,
    pub received_on: NaiveDate,
    /// Where the money landed: a bank or cash account in this workspace's
    /// chart.
    pub account_id: Uuid,
    pub account_number: String,
    pub account_name: String,
    pub currency: Currency,
    pub amount: Money,
    /// The whole conversion snapshot. `None` when the payment is already in the
    /// base currency, because there is nothing to convert and a rate of one is
    /// not evidence of a quotation.
    pub rate: Option<ExchangeRate>,
    pub base_amount: Option<Money>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub allocations: Vec<Allocation>,
    pub posted_at: Option<DateTime<Utc>>,
    pub posted_by: Option<UserId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Payment {
    /// What this payment has been spread across.
    pub fn allocated(&self) -> Result<Money, MoneyError> {
        Money::total(
            self.currency,
            self.allocations.iter().map(|line| line.amount),
        )
    }

    /// What is sitting on the customer's account: received and not yet set
    /// against anything.
    ///
    /// Worked out rather than stored, which is the whole reason allocation is a
    /// relation - see the module header.
    pub fn on_account(&self) -> Result<Money, MoneyError> {
        self.amount.checked_sub(self.allocated()?)
    }

    pub const fn is_editable(&self) -> bool {
        self.status.is_editable()
    }

    /// What to call this on a screen. The number once it has one, and something
    /// a person can still recognise before that.
    pub fn label(&self) -> String {
        match &self.number {
            Some(number) => number.clone(),
            None => format!("{} \u{b7} {}", self.received_on, self.party.name),
        }
    }
}

/// One payment as a list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSummary {
    pub id: Uuid,
    pub number: Option<String>,
    pub status: PaymentStatus,
    pub party_id: Uuid,
    pub party_name: String,
    pub received_on: NaiveDate,
    pub account_name: String,
    pub currency: Currency,
    pub amount: Money,
    /// What has been set against invoices. The difference from `amount` is what
    /// is on account, which is the figure worth seeing in a list.
    pub allocated: Money,
    pub reference: Option<String>,
}

impl PaymentSummary {
    pub fn on_account(&self) -> Result<Money, MoneyError> {
        self.amount.checked_sub(self.allocated)
    }
}

/// One invoice a payment could be set against, with what is left on it.
///
/// What the allocation half of the screen is built from. The outstanding figure
/// is the invoice's gross less everything already posted against it, worked out
/// by the query rather than stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settleable {
    pub invoice_id: Uuid,
    pub number: String,
    pub issued_on: NaiveDate,
    pub due_on: Option<NaiveDate>,
    pub currency: Currency,
    pub invoiced: Money,
    /// Already settled by other posted payments.
    pub settled: Money,
    /// `invoiced - settled`. Never negative.
    pub outstanding: Money,
}

impl Settleable {
    /// Whether this invoice is past its due date and still owed.
    ///
    /// Takes today explicitly rather than reading the clock, so a row renders
    /// the same on the server and in the browser.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.due_on.is_some_and(|due| due < today) && is_above_zero(self.outstanding)
    }
}

/// A payment being written on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentInput {
    pub id: Option<Uuid>,
    pub party_id: Option<Uuid>,
    pub received_on: NaiveDate,
    pub account_id: Option<Uuid>,
    pub currency: Currency,
    /// As typed. It becomes a [`Money`] at [`check`](PaymentInput::check),
    /// because a box somebody has not finished typing in is not yet a number.
    pub amount: String,
    pub reference: String,
    pub note: String,
    pub allocations: Vec<AllocationInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationInput {
    pub invoice_id: Uuid,
    /// As typed. Empty or zero means this invoice is not being settled by this
    /// payment, which is how a row is cleared without removing it from a list
    /// the screen is drawing from outstanding invoices.
    pub amount: String,
}

impl PaymentInput {
    /// What a blank form opens on.
    ///
    /// `today` and the workspace's own currency are passed in rather than read
    /// here: this crate has no clock and no database, and a default that
    /// guessed at either would differ between the browser and the server.
    pub fn blank(today: NaiveDate, currency: Currency) -> Self {
        Self {
            id: None,
            party_id: None,
            received_on: today,
            account_id: None,
            currency,
            amount: String::new(),
            reference: String::new(),
            note: String::new(),
            allocations: Vec::new(),
        }
    }

    pub fn from_payment(payment: &Payment) -> Self {
        Self {
            id: Some(payment.id),
            party_id: Some(payment.party.party_id),
            received_on: payment.received_on,
            account_id: Some(payment.account_id),
            currency: payment.currency,
            amount: payment.amount.to_storage_string(),
            reference: payment.reference.clone().unwrap_or_default(),
            note: payment.note.clone().unwrap_or_default(),
            allocations: payment
                .allocations
                .iter()
                .map(|line| AllocationInput {
                    invoice_id: line.invoice_id,
                    amount: line.amount.to_storage_string(),
                })
                .collect(),
        }
    }

    /// Check what was typed, and say what is still wrong.
    ///
    /// What it cannot check is whether the invoices named are this customer's,
    /// still posted, and not already settled - all three need the database, and
    /// all three are the service's.
    pub fn check(&self) -> Result<CheckedPayment, PaymentError> {
        let party_id = self.party_id.ok_or(PaymentError::PartyRequired)?;
        let account_id = self.account_id.ok_or(PaymentError::AccountRequired)?;

        if self.reference.chars().count() > MAX_REFERENCE_LEN {
            return Err(PaymentError::ReferenceTooLong);
        }
        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(PaymentError::NoteTooLong);
        }

        let amount = Money::parse(self.currency, self.amount.trim())?;
        if !is_above_zero(amount) {
            return Err(PaymentError::AmountRequired);
        }

        let mut allocations: Vec<CheckedAllocation> = Vec::new();

        for line in &self.allocations {
            let typed = line.amount.trim();
            if typed.is_empty() {
                continue;
            }

            let amount = Money::parse(self.currency, typed)?;

            // Not being settled by this payment. Dropped rather than refused,
            // which is what lets the screen list every outstanding invoice and
            // have somebody fill in two of them.
            if amount.is_zero() {
                continue;
            }
            if amount.is_negative() {
                return Err(PaymentError::AllocationNegative);
            }

            // Two rows for one invoice would be one settlement counted twice,
            // and the check below that nothing is over-paid would pass them
            // both.
            if allocations
                .iter()
                .any(|held| held.invoice_id == line.invoice_id)
            {
                return Err(PaymentError::AllocatedTwice);
            }

            allocations.push(CheckedAllocation {
                invoice_id: line.invoice_id,
                amount,
            });
        }

        if allocations.len() > MAX_ALLOCATIONS {
            return Err(PaymentError::TooManyAllocations);
        }

        let allocated = Money::total(self.currency, allocations.iter().map(|line| line.amount))?;

        // More set against invoices than was received. The opposite - less - is
        // money on account and is ordinary.
        if allocated.compare(amount)?.is_gt() {
            return Err(PaymentError::OverAllocated);
        }

        Ok(CheckedPayment {
            id: self.id,
            party_id,
            received_on: self.received_on,
            account_id,
            currency: self.currency,
            amount,
            reference: non_empty(&self.reference),
            note: non_empty(&self.note),
            allocations,
        })
    }
}

/// `Money` has `is_zero` and `is_negative` and no third predicate, because two
/// answer every question a ledger asks. This is the third one, once, rather
/// than the pair spelled out at five call sites.
const fn is_above_zero(amount: Money) -> bool {
    !amount.is_zero() && !amount.is_negative()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// A payment that passed [`PaymentInput::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedPayment {
    pub id: Option<Uuid>,
    pub party_id: Uuid,
    pub received_on: NaiveDate,
    pub account_id: Uuid,
    pub currency: Currency,
    pub amount: Money,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub allocations: Vec<CheckedAllocation>,
}

impl CheckedPayment {
    pub fn allocated(&self) -> Result<Money, MoneyError> {
        Money::total(
            self.currency,
            self.allocations.iter().map(|line| line.amount),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedAllocation {
    pub invoice_id: Uuid,
    pub amount: Money,
}

/// How a post turned out.
///
/// Outcomes rather than errors for the two that are expected, on exactly the
/// terms [`crate::invoice::PostOutcome`] is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostOutcome {
    Posted {
        number: String,
    },
    /// It is not a draft any more - somebody else posted it first.
    NotADraft,
    /// There is no active `payment` series in this workspace.
    NoSeries,
}

impl PostOutcome {
    pub fn message(&self) -> Message {
        match self {
            Self::Posted { .. } => msg!("payments.posted"),
            Self::NotADraft => msg!("payments.error.not_editable"),
            Self::NoSeries => msg!("payments.error.no_series"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PaymentError {
    #[error("a payment needs a customer")]
    PartyRequired,
    #[error("that party is not a customer")]
    NotACustomer,
    #[error("a payment needs an account for the money to land in")]
    AccountRequired,
    #[error("that account does not hold money")]
    NotACashAccount,
    #[error("a payment needs an amount above nothing")]
    AmountRequired,
    #[error("an allocation is positive - it is settling an invoice, not raising one")]
    AllocationNegative,
    #[error("one invoice cannot be settled twice by the same payment")]
    AllocatedTwice,
    #[error("more has been set against invoices than was received")]
    OverAllocated,
    #[error("that is more than is left on the invoice")]
    OverSettled,
    #[error("a payment settles at most 200 invoices")]
    TooManyAllocations,
    #[error("that invoice belongs to a different customer")]
    WrongCustomer,
    #[error("only a posted invoice can be settled")]
    InvoiceNotPosted,
    #[error("a payment can only settle an invoice raised in the same currency")]
    CurrencyMismatch,
    #[error("a reference is at most 120 characters")]
    ReferenceTooLong,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("that amount is not a number")]
    Money(#[from] MoneyError),
    #[error("a posted payment cannot be changed")]
    NotEditable,
    #[error("only a posted payment can be withdrawn")]
    NotVoidable,
    #[error("paying a supplier is not built yet")]
    DirectionNotBuilt,
}

impl PaymentError {
    pub fn field(self) -> &'static str {
        match self {
            Self::PartyRequired | Self::NotACustomer | Self::WrongCustomer => "party_id",
            Self::AccountRequired | Self::NotACashAccount => "account_id",
            Self::AmountRequired | Self::Money(_) => "amount",
            Self::AllocationNegative
            | Self::AllocatedTwice
            | Self::OverAllocated
            | Self::OverSettled
            | Self::TooManyAllocations
            | Self::InvoiceNotPosted
            | Self::CurrencyMismatch => "allocations",
            Self::ReferenceTooLong => "reference",
            Self::NoteTooLong => "note",
            Self::NotEditable | Self::NotVoidable | Self::DirectionNotBuilt => "status",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::PartyRequired => msg!("payments.error.party_required"),
            Self::NotACustomer => msg!("payments.error.not_a_customer"),
            Self::AccountRequired => msg!("payments.error.account_required"),
            Self::NotACashAccount => msg!("payments.error.not_a_cash_account"),
            Self::AmountRequired => msg!("payments.error.amount_required"),
            Self::AllocationNegative => msg!("payments.error.allocation_negative"),
            Self::AllocatedTwice => msg!("payments.error.allocated_twice"),
            Self::OverAllocated => msg!("payments.error.over_allocated"),
            Self::OverSettled => msg!("payments.error.over_settled"),
            Self::TooManyAllocations => msg!("payments.error.too_many_allocations"),
            Self::WrongCustomer => msg!("payments.error.wrong_customer"),
            Self::InvoiceNotPosted => msg!("payments.error.invoice_not_posted"),
            Self::CurrencyMismatch => msg!("payments.error.currency_mismatch"),
            Self::ReferenceTooLong => msg!("payments.error.reference_too_long"),
            Self::NoteTooLong => msg!("payments.error.note_too_long"),
            Self::Money(err) => err.message(),
            Self::NotEditable => msg!("payments.error.not_editable"),
            Self::NotVoidable => msg!("payments.error.not_voidable"),
            Self::DirectionNotBuilt => msg!("payments.error.direction_not_built"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENCY: Currency = Currency::USD;

    fn money(amount: &str) -> Money {
        Money::parse(CURRENCY, amount).unwrap()
    }

    fn input(amount: &str, allocations: &[(u128, &str)]) -> PaymentInput {
        PaymentInput {
            id: None,
            party_id: Some(Uuid::from_u128(7)),
            received_on: NaiveDate::from_ymd_opt(2026, 3, 14).unwrap(),
            account_id: Some(Uuid::from_u128(3)),
            currency: CURRENCY,
            amount: amount.to_owned(),
            reference: String::new(),
            note: String::new(),
            allocations: allocations
                .iter()
                .map(|(invoice, amount)| AllocationInput {
                    invoice_id: Uuid::from_u128(*invoice),
                    amount: (*amount).to_owned(),
                })
                .collect(),
        }
    }

    /// One cheque across two invoices, which is the ordinary case and the whole
    /// reason allocation is a relation.
    #[test]
    fn one_payment_settles_several_invoices() {
        let checked = input("1200.00", &[(1, "1000.00"), (2, "200.00")])
            .check()
            .unwrap();

        assert_eq!(checked.allocations.len(), 2);
        assert_eq!(checked.allocated().unwrap(), money("1200.00"));
    }

    /// Received and not yet set against anything. Ordinary, and the difference
    /// rather than a stored number.
    #[test]
    fn what_is_left_over_sits_on_the_account() {
        let checked = input("1200.00", &[(1, "500.00")]).check().unwrap();

        let on_account = checked
            .amount
            .checked_sub(checked.allocated().unwrap())
            .unwrap();

        assert_eq!(on_account, money("700.00"));
    }

    /// The one direction that is not ordinary: settling more than was received.
    #[test]
    fn more_allocated_than_received_is_refused() {
        assert_eq!(
            input("100.00", &[(1, "60.00"), (2, "60.00")]).check(),
            Err(PaymentError::OverAllocated)
        );
    }

    /// A screen that lists every outstanding invoice needs empty rows to be
    /// nothing rather than an error.
    #[test]
    fn a_row_left_blank_or_zero_settles_nothing() {
        let checked = input("100.00", &[(1, ""), (2, "0"), (3, "100.00")])
            .check()
            .unwrap();

        assert_eq!(checked.allocations.len(), 1);
        assert_eq!(checked.allocations[0].invoice_id, Uuid::from_u128(3));
    }

    /// One settlement counted twice would pass the over-allocation check.
    #[test]
    fn the_same_invoice_cannot_appear_twice() {
        assert_eq!(
            input("100.00", &[(1, "40.00"), (1, "60.00")]).check(),
            Err(PaymentError::AllocatedTwice)
        );
    }

    /// A payment of nothing is not a payment.
    #[test]
    fn a_payment_needs_an_amount() {
        assert_eq!(input("0", &[]).check(), Err(PaymentError::AmountRequired));
    }

    /// A draft is not money and a voided payment has been withdrawn.
    #[test]
    fn only_a_posted_payment_settles_anything() {
        assert!(PaymentStatus::Posted.settles());
        assert!(!PaymentStatus::Draft.settles());
        assert!(!PaymentStatus::Voided.settles());
    }
}
