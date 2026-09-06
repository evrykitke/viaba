//! The general ledger: what a journal is, and why an unbalanced one does not
//! exist.
//!
//! # Double entry is a property of the type
//!
//! [`JournalEntry::assemble`] is the only way to make one, and it refuses
//! anything that does not balance. There is no setter and no public field, so
//! no code path can reach the repository with a journal whose debits and
//! credits differ - not a forgotten validation in a service, not a caller from
//! another app. See `docs/adr/0006-apps-ports-and-defaults.md` section 5 rule 1.
//!
//! # It balances in the base currency
//!
//! Not in the transaction currency. A journal that records a realised exchange
//! difference has lines in two currencies and is perfectly correct; what must
//! always be true is that the workspace's own currency nets to nothing. Every
//! line therefore carries the six-column snapshot and the balance is checked on
//! `base_amount`.
//!
//! # Posted is the only state
//!
//! There is no draft journal. A journal exists because it was posted, and a
//! mistake is corrected by [`JournalEntry::reversal_of`] rather than by an
//! edit.

use chrono::NaiveDate;
use phonix_core::Currency;
use phonix_core::i18n::Message;
use phonix_core::money::Money;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::Side;

/// The longest narration a journal may carry, matching the migration.
pub const MAX_NARRATION_LEN: usize = 500;

/// The longest memo one line may carry.
pub const MAX_MEMO_LEN: usize = 500;

/// The most lines one journal may hold.
///
/// A journal with more than this is a batch that should have been several, and
/// the limit exists so one runaway posting cannot fill a page nobody can read.
pub const MAX_LINES: usize = 500;

/// What a journal line may be tagged with, beside its account.
///
/// Orthogonal to the account, which is the whole point: an account says *what*,
/// a dimension says *whose*. See ADR 0006 section 6.4 - two named columns is
/// the ceiling the mid-tier packages hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    /// Supplied by whatever implements `phonix_ports::CostCentres`.
    CostCentre,
}

impl Dimension {
    pub const ALL: &'static [Self] = &[Self::CostCentre];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CostCentre => "cost_centre",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    pub fn label(self) -> Message {
        match self {
            Self::CostCentre => msg!("journals.dimension.cost_centre"),
        }
    }
}

/// One dimension value on one line, as a snapshot.
///
/// Id, code and name, copied at the moment of posting. A reference would let a
/// department renamed next year rewrite last year's report, and the id alone
/// would leave a report unable to name anything without reaching into another
/// app's schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionValue {
    pub dimension: Dimension,
    pub id: Uuid,
    pub code: String,
    pub name: String,
}

impl DimensionValue {
    /// `DEPT-004 · Finance`.
    pub fn label(&self) -> String {
        format!("{} · {}", self.code, self.name)
    }
}

/// Where a journal came from.
///
/// Every journal names one, which is what turns reconciling a sub-ledger to the
/// general ledger into a `GROUP BY` rather than an investigation. ADR 0006
/// section 5 rule 3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The app id that raised it - `books`, one day `inventory`.
    pub app: String,
    /// What kind of document: `sales_invoice`, `manual`, `reversal`.
    pub doc_type: String,
    /// The document, where there is one. A manual journal has no document
    /// behind it and is its own evidence.
    pub doc_id: Option<Uuid>,
}

impl Source {
    pub fn new(app: impl Into<String>, doc_type: impl Into<String>, doc_id: Option<Uuid>) -> Self {
        Self {
            app: app.into(),
            doc_type: doc_type.into(),
            doc_id,
        }
    }

    /// Somebody typed it in. There is no document, and the journal is the
    /// record.
    pub fn manual() -> Self {
        Self::new(crate::APP_ID, doc_types::MANUAL, None)
    }

    /// A correction to a journal, naming the one it corrects.
    pub fn reversal(journal_id: Uuid) -> Self {
        Self::new(crate::APP_ID, doc_types::REVERSAL, Some(journal_id))
    }
}

/// The document types this app posts under.
pub mod doc_types {
    /// Typed in by a person.
    pub const MANUAL: &str = "manual";
    /// A correction that reverses another journal.
    pub const REVERSAL: &str = "reversal";
    /// Raised by posting a sales invoice.
    pub const SALES_INVOICE: &str = "sales_invoice";
}

/// A line as a caller offers it, before the journal has been assembled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLineInput {
    pub account_id: Uuid,
    pub side: Side,
    /// As transacted. Must be positive - a side plus a magnitude, never a
    /// signed amount.
    pub amount: Money,
    /// The same amount in the workspace's own currency, on `rate_date`.
    pub base_amount: Money,
    /// Quote per unit of base, as applied. One when the line is already in the
    /// base currency, and stored anyway so a reader never has to infer it.
    pub exchange_rate: String,
    pub rate_date: NaiveDate,
    pub memo: Option<String>,
    pub dimensions: Vec<DimensionValue>,
}

impl JournalLineInput {
    /// A line in the workspace's own currency, where there is nothing to
    /// convert.
    pub fn in_base(account_id: Uuid, side: Side, amount: Money, on: NaiveDate) -> Self {
        Self {
            account_id,
            side,
            amount,
            base_amount: amount,
            exchange_rate: "1".to_owned(),
            rate_date: on,
            memo: None,
            dimensions: Vec::new(),
        }
    }

    #[must_use]
    pub fn memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }

    #[must_use]
    pub fn charged_to(mut self, value: DimensionValue) -> Self {
        self.dimensions.retain(|it| it.dimension != value.dimension);
        self.dimensions.push(value);
        self
    }
}

/// A balanced journal. There is no other kind.
///
/// The fields are private and there is no way to change one after assembly:
/// anything that could edit a line could unbalance it, and an unbalanced
/// journal must not be representable rather than merely rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    entry_date: NaiveDate,
    narration: String,
    source: Source,
    lines: Vec<JournalLineInput>,
    reverses_id: Option<Uuid>,
}

impl JournalEntry {
    /// Build one, or say why it is not a journal.
    ///
    /// Everything checked here is a property of the entry itself. What it
    /// cannot check - that the accounts exist, that the period is open - needs
    /// a database and belongs in the service that posts it.
    pub fn assemble(
        entry_date: NaiveDate,
        narration: impl Into<String>,
        source: Source,
        lines: Vec<JournalLineInput>,
    ) -> Result<Self, JournalError> {
        let narration = narration.into().trim().to_owned();

        if narration.is_empty() {
            return Err(JournalError::NarrationRequired);
        }
        if narration.chars().count() > MAX_NARRATION_LEN {
            return Err(JournalError::NarrationTooLong);
        }

        // Two, not one. A single-line journal cannot balance unless it is zero,
        // and a zero journal records nothing.
        if lines.len() < 2 {
            return Err(JournalError::TooFewLines);
        }
        if lines.len() > MAX_LINES {
            return Err(JournalError::TooManyLines);
        }

        let mut base_currency = None;

        for line in &lines {
            if line.amount.is_zero() || line.amount.is_negative() {
                return Err(JournalError::AmountNotPositive);
            }
            if line.base_amount.is_zero() || line.base_amount.is_negative() {
                return Err(JournalError::AmountNotPositive);
            }
            if line
                .memo
                .as_ref()
                .is_some_and(|memo| memo.chars().count() > MAX_MEMO_LEN)
            {
                return Err(JournalError::MemoTooLong);
            }

            // One base currency across the journal. Two would mean two
            // balances, and "does this balance" would have two answers.
            match base_currency {
                None => base_currency = Some(line.base_amount.currency()),
                Some(currency) if currency == line.base_amount.currency() => {}
                Some(_) => return Err(JournalError::MixedBaseCurrency),
            }
        }

        let Some(base_currency) = base_currency else {
            return Err(JournalError::TooFewLines);
        };

        let mut debits = Money::zero(base_currency);
        let mut credits = Money::zero(base_currency);

        for line in &lines {
            let running = match line.side {
                Side::Debit => &mut debits,
                Side::Credit => &mut credits,
            };

            *running = running
                .checked_add(line.base_amount)
                .map_err(|_| JournalError::TooLarge)?;
        }

        if debits != credits {
            return Err(JournalError::Unbalanced);
        }

        // A journal of two zero lines would balance and move nothing. The
        // per-line check above already refuses it; this is the statement of
        // what the pair of checks is for.
        if debits.is_zero() {
            return Err(JournalError::AmountNotPositive);
        }

        Ok(Self {
            entry_date,
            narration,
            source,
            lines,
            reverses_id: None,
        })
    }

    /// The journal that undoes this one: every line, on the other side.
    ///
    /// The only correction there is. A posted journal is never edited and never
    /// deleted, so putting a mistake right means saying so in a second journal
    /// that names the first - which is what an auditor reads, and what a
    /// package that lets you open a three-year-old document and change the
    /// amount cannot show them.
    ///
    /// Dated separately, because a correction found in April is April's event
    /// even when the mistake was March's - and March may well be closed.
    pub fn reversal_of(
        original: &Posted,
        on: NaiveDate,
        narration: impl Into<String>,
    ) -> Result<Self, JournalError> {
        let lines = original
            .lines
            .iter()
            .map(|line| JournalLineInput {
                side: line.side.opposite(),
                ..line.clone_input()
            })
            .collect();

        let mut reversal = Self::assemble(on, narration, Source::reversal(original.id), lines)?;

        reversal.reverses_id = Some(original.id);

        Ok(reversal)
    }

    pub const fn entry_date(&self) -> NaiveDate {
        self.entry_date
    }

    pub fn narration(&self) -> &str {
        &self.narration
    }

    pub const fn source(&self) -> &Source {
        &self.source
    }

    pub fn lines(&self) -> &[JournalLineInput] {
        &self.lines
    }

    pub const fn reverses_id(&self) -> Option<Uuid> {
        self.reverses_id
    }

    /// What the journal moves, one side of it. Both sides are equal by
    /// construction, so either answers "how big is this".
    pub fn total(&self) -> Money {
        let currency = self
            .lines
            .first()
            .map_or_else(Currency::default, |line| line.base_amount.currency());

        self.lines
            .iter()
            .filter(|line| line.side == Side::Debit)
            .try_fold(Money::zero(currency), |total, line| {
                total.checked_add(line.base_amount)
            })
            .unwrap_or_else(|_| Money::zero(currency))
    }
}

/// A journal as it was stored: what [`JournalEntry`] became, plus what the
/// ledger assigned it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posted {
    pub id: Uuid,
    pub number: String,
    pub entry_date: NaiveDate,
    pub period_id: Uuid,
    pub period_label: String,
    pub narration: String,
    pub source: Source,
    pub reverses_id: Option<Uuid>,
    pub posted_at: chrono::DateTime<chrono::Utc>,
    pub lines: Vec<PostedLine>,
}

/// One stored line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostedLine {
    pub id: Uuid,
    pub position: i32,
    pub account_id: Uuid,
    /// Snapshot, so a journal reads without a join and a renumbered account
    /// does not rewrite what was filed.
    pub account_number: String,
    pub account_name: String,
    pub side: Side,
    pub amount: Money,
    pub base_amount: Money,
    pub exchange_rate: String,
    pub rate_date: NaiveDate,
    pub memo: Option<String>,
    pub dimensions: Vec<DimensionValue>,
}

impl PostedLine {
    /// Back into an input, for building a reversal.
    pub fn clone_input(&self) -> JournalLineInput {
        JournalLineInput {
            account_id: self.account_id,
            side: self.side,
            amount: self.amount,
            base_amount: self.base_amount,
            exchange_rate: self.exchange_rate.clone(),
            rate_date: self.rate_date,
            memo: self.memo.clone(),
            dimensions: self.dimensions.clone(),
        }
    }
}

/// A journal as a list screen reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalSummary {
    pub id: Uuid,
    pub number: String,
    pub entry_date: NaiveDate,
    pub period_label: String,
    pub narration: String,
    pub source_app: String,
    pub source_doc_type: String,
    pub source_doc_id: Option<Uuid>,
    pub is_reversal: bool,
    /// One side of it, which is both.
    pub total: Money,
    pub line_count: i64,
}

/// Why something is not a journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    #[error("a journal needs a narration")]
    NarrationRequired,
    #[error("a narration is at most 500 characters")]
    NarrationTooLong,
    #[error("a journal needs at least two lines")]
    TooFewLines,
    #[error("a journal holds at most 500 lines")]
    TooManyLines,
    #[error("a line moves a positive amount, on one side or the other")]
    AmountNotPositive,
    #[error("a memo is at most 500 characters")]
    MemoTooLong,
    #[error("every line converts to the same base currency")]
    MixedBaseCurrency,
    #[error("the debits and the credits do not agree")]
    Unbalanced,
    #[error("that journal is too large to total")]
    TooLarge,
}

impl JournalError {
    /// Which control to attach the message to.
    pub fn field(self) -> &'static str {
        match self {
            Self::NarrationRequired | Self::NarrationTooLong => "narration",
            _ => "lines",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NarrationRequired => msg!("journals.error.narration_required"),
            Self::NarrationTooLong => msg!("journals.error.narration_too_long"),
            Self::TooFewLines => msg!("journals.error.too_few_lines"),
            Self::TooManyLines => msg!("journals.error.too_many_lines"),
            Self::AmountNotPositive => msg!("journals.error.amount_not_positive"),
            Self::MemoTooLong => msg!("journals.error.memo_too_long"),
            Self::MixedBaseCurrency => msg!("journals.error.mixed_base_currency"),
            Self::Unbalanced => msg!("journals.error.unbalanced"),
            Self::TooLarge => msg!("journals.error.too_large"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Distinct, deterministic ids. `uuid` is built without `v4` here - this
    /// crate compiles to wasm and a random generator is not something a domain
    /// type needs.
    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn gbp() -> Currency {
        Currency::parse("GBP").unwrap()
    }

    fn eur() -> Currency {
        Currency::parse("EUR").unwrap()
    }

    fn on() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 14).unwrap()
    }

    fn money(currency: Currency, units: i64) -> Money {
        Money::from_units(currency, units).unwrap()
    }

    fn line(side: Side, units: i64) -> JournalLineInput {
        JournalLineInput::in_base(id(1), side, money(gbp(), units), on())
    }

    fn assembled(lines: Vec<JournalLineInput>) -> Result<JournalEntry, JournalError> {
        JournalEntry::assemble(on(), "Rent for March", Source::manual(), lines)
    }

    #[test]
    fn a_balanced_journal_assembles() {
        let entry = assembled(vec![line(Side::Debit, 100), line(Side::Credit, 100)]).unwrap();

        assert_eq!(entry.lines().len(), 2);
        assert_eq!(entry.narration(), "Rent for March");
        assert_eq!(entry.entry_date(), on());
        assert_eq!(entry.source().doc_type, doc_types::MANUAL);
    }

    #[test]
    fn an_unbalanced_journal_cannot_be_made() {
        // The rule the whole type exists for. Nothing downstream re-checks it,
        // because nothing downstream can be handed one.
        assert_eq!(
            assembled(vec![line(Side::Debit, 100), line(Side::Credit, 99)]),
            Err(JournalError::Unbalanced)
        );
    }

    #[test]
    fn many_lines_balance_across_both_sides() {
        let entry = assembled(vec![
            line(Side::Debit, 60),
            line(Side::Debit, 40),
            line(Side::Credit, 30),
            line(Side::Credit, 70),
        ])
        .unwrap();

        assert_eq!(entry.lines().len(), 4);
    }

    #[test]
    fn a_journal_needs_two_lines_and_a_narration() {
        assert_eq!(
            assembled(vec![line(Side::Debit, 100)]),
            Err(JournalError::TooFewLines)
        );
        assert_eq!(
            JournalEntry::assemble(
                on(),
                "   ",
                Source::manual(),
                vec![line(Side::Debit, 100), line(Side::Credit, 100)]
            ),
            Err(JournalError::NarrationRequired)
        );
    }

    #[test]
    fn a_line_carries_a_side_and_a_magnitude_never_a_sign() {
        let negative = JournalLineInput::in_base(
            id(2),
            Side::Debit,
            money(gbp(), -100),
            on(),
        );

        assert_eq!(
            assembled(vec![negative, line(Side::Credit, 100)]),
            Err(JournalError::AmountNotPositive)
        );
    }

    #[test]
    fn a_journal_of_nothing_is_not_a_journal() {
        let zero = JournalLineInput::in_base(id(3), Side::Debit, money(gbp(), 0), on());

        assert_eq!(
            assembled(vec![zero.clone(), zero]),
            Err(JournalError::AmountNotPositive)
        );
    }

    #[test]
    fn it_balances_in_the_base_currency_not_the_transacted_one() {
        // A euro payment against a sterling payable: the transaction currencies
        // differ and the journal is perfectly correct.
        let mut euro_line = line(Side::Debit, 0);
        euro_line.amount = money(eur(), 120);
        euro_line.base_amount = money(gbp(), 100);
        euro_line.exchange_rate = "1.2".to_owned();

        let entry = assembled(vec![euro_line, line(Side::Credit, 100)]).unwrap();

        assert_eq!(entry.lines().len(), 2);
    }

    #[test]
    fn two_base_currencies_in_one_journal_are_refused() {
        // Otherwise "does this balance" has two answers.
        let mut other = line(Side::Credit, 0);
        other.amount = money(eur(), 100);
        other.base_amount = money(eur(), 100);

        assert_eq!(
            assembled(vec![line(Side::Debit, 100), other]),
            Err(JournalError::MixedBaseCurrency)
        );
    }

    #[test]
    fn a_reversal_turns_every_line_over_and_names_what_it_reverses() {
        let original = Posted {
            id: id(4),
            number: "JNL-2026-00001".to_owned(),
            entry_date: on(),
            period_id: id(5),
            period_label: "2026-03".to_owned(),
            narration: "Rent for March".to_owned(),
            source: Source::manual(),
            reverses_id: None,
            posted_at: chrono::Utc::now(),
            lines: vec![
                PostedLine {
                    id: id(6),
                    position: 0,
                    account_id: id(7),
                    account_number: "6200".to_owned(),
                    account_name: "Rent".to_owned(),
                    side: Side::Debit,
                    amount: money(gbp(), 100),
                    base_amount: money(gbp(), 100),
                    exchange_rate: "1".to_owned(),
                    rate_date: on(),
                    memo: None,
                    dimensions: Vec::new(),
                },
                PostedLine {
                    id: id(8),
                    position: 1,
                    account_id: id(9),
                    account_number: "1200".to_owned(),
                    account_name: "Bank".to_owned(),
                    side: Side::Credit,
                    amount: money(gbp(), 100),
                    base_amount: money(gbp(), 100),
                    exchange_rate: "1".to_owned(),
                    rate_date: on(),
                    memo: None,
                    dimensions: Vec::new(),
                },
            ],
        };

        let later = NaiveDate::from_ymd_opt(2026, 4, 2).unwrap();
        let reversal = JournalEntry::reversal_of(&original, later, "Reverses JNL-2026-00001")
            .unwrap();

        assert_eq!(reversal.reverses_id(), Some(original.id));
        assert_eq!(reversal.source().doc_type, doc_types::REVERSAL);
        assert_eq!(reversal.source().doc_id, Some(original.id));
        // Dated when the mistake was found, not when it was made: March may
        // well be closed by now.
        assert_eq!(reversal.entry_date(), later);

        assert_eq!(reversal.lines()[0].side, Side::Credit);
        assert_eq!(reversal.lines()[1].side, Side::Debit);
        assert_eq!(reversal.lines()[0].base_amount, money(gbp(), 100));
    }

    #[test]
    fn a_dimension_is_set_once_per_kind() {
        let value = |name: &str| DimensionValue {
            dimension: Dimension::CostCentre,
            id: id(10),
            code: "DEPT-004".to_owned(),
            name: name.to_owned(),
        };

        let line = line(Side::Debit, 100)
            .charged_to(value("Finance"))
            .charged_to(value("Operations"));

        assert_eq!(line.dimensions.len(), 1);
        assert_eq!(line.dimensions[0].name, "Operations");
    }

    #[test]
    fn a_dimension_label_names_the_code_first() {
        let value = DimensionValue {
            dimension: Dimension::CostCentre,
            id: Uuid::nil(),
            code: "DEPT-004".to_owned(),
            name: "Finance".to_owned(),
        };

        assert_eq!(value.label(), "DEPT-004 · Finance");
        assert_eq!(Dimension::parse("cost_centre"), Some(Dimension::CostCentre));
        assert_eq!(Dimension::parse("project"), None);
    }
}
