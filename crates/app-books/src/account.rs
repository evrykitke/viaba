//! The chart of accounts: what a workspace may post to.
//!
//! An account is *typed*, and the type is what the software reasons about. The
//! number is a convention the workspace may change — an accountant who has used
//! 1200 for the bank since 1994 will renumber the chart, and nothing may break
//! when they do. So [`AccountType`] decides the normal balance, whether the
//! account closes at year end, and whether a sub-ledger owns it; the number
//! decides only the order things appear in.
//!
//! That is why the ranges in ADR 0006 section 4 are documentation for the
//! default chart rather than a rule enforced here. A workspace numbering its
//! revenue in the 7000s is unusual, not wrong.
//!
//! # Contra accounts are why the balance is per type and not per class
//!
//! Accumulated depreciation is an asset that carries a credit balance, and
//! sales returns are revenue that carries a debit one. A `normal_balance()`
//! derived from the class would be wrong for both, and wrong in the direction
//! that makes a balance sheet look right while the depreciation is backwards.

use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longest account number the column holds.
pub const MAX_ACCOUNT_NUMBER_LEN: usize = 20;

/// Longest account name the column holds.
pub const MAX_ACCOUNT_NAME_LEN: usize = 120;

/// Which side of the entry an amount sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Debit,
    Credit,
}

impl Side {
    pub const ALL: &'static [Self] = &[Self::Debit, Self::Credit];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }

    /// Read a stored value back. `None` rather than a default: guessing a side
    /// puts the amount on the wrong one.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|side| side.as_str() == raw)
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Debit => Self::Credit,
            Self::Credit => Self::Debit,
        }
    }

    pub fn label(self) -> Message {
        match self {
            Self::Debit => msg!("books.side.debit"),
            Self::Credit => msg!("books.side.credit"),
        }
    }
}

/// The five fundamentals. What a statement an account appears on, and whether
/// it survives the year end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountClass {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

impl AccountClass {
    pub const ALL: &'static [Self] = &[
        Self::Asset,
        Self::Liability,
        Self::Equity,
        Self::Revenue,
        Self::Expense,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::Equity => "equity",
            Self::Revenue => "revenue",
            Self::Expense => "expense",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Asset => msg!("books.class.asset"),
            Self::Liability => msg!("books.class.liability"),
            Self::Equity => msg!("books.class.equity"),
            Self::Revenue => msg!("books.class.revenue"),
            Self::Expense => msg!("books.class.expense"),
        }
    }

    /// Whether the balance is cleared into retained earnings at year end.
    ///
    /// Revenue and expense do; the balance sheet carries forward. This is the
    /// whole of what "profit and loss account" means to the software.
    pub const fn closes_at_year_end(self) -> bool {
        matches!(self, Self::Revenue | Self::Expense)
    }

    /// Which statement it appears on. `true` for the balance sheet.
    pub const fn is_balance_sheet(self) -> bool {
        !self.closes_at_year_end()
    }
}

/// What an account is for.
///
/// Granular enough to answer three questions a class cannot: which side the
/// balance normally sits on (contra accounts differ from their class), whether
/// a sub-ledger owns the account, and which account a posting routine should
/// reach for when it needs "the receivables one".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    // -- assets --
    Cash,
    Bank,
    AccountsReceivable,
    Inventory,
    PrepaidExpense,
    OtherCurrentAsset,
    FixedAsset,
    /// Contra-asset: an asset account carrying a credit balance.
    AccumulatedDepreciation,
    /// Any other contra-asset — an allowance for doubtful debts, an allowance
    /// for obsolete stock. Credit balance against an asset class.
    ContraAsset,
    IntangibleAsset,
    OtherAsset,

    // -- liabilities --
    AccountsPayable,
    /// Goods received and not yet invoiced. The account a starter chart leaves
    /// out and a three-way match cannot work without — see ADR 0006 §6.5.
    GoodsReceivedNotInvoiced,
    TaxPayable,
    AccruedLiability,
    OtherCurrentLiability,
    LongTermLiability,

    // -- equity --
    Equity,
    RetainedEarnings,
    /// Contra-equity: drawings, treasury shares, dividends declared. Debit
    /// balance against an equity class.
    ContraEquity,

    // -- revenue --
    Revenue,
    /// Contra-revenue: returns, discounts allowed. Debit balance.
    ContraRevenue,
    OtherIncome,

    // -- expense --
    CostOfSales,
    OperatingExpense,
    Depreciation,
    OtherExpense,
    IncomeTax,
}

impl AccountType {
    pub const ALL: &'static [Self] = &[
        Self::Cash,
        Self::Bank,
        Self::AccountsReceivable,
        Self::Inventory,
        Self::PrepaidExpense,
        Self::OtherCurrentAsset,
        Self::FixedAsset,
        Self::AccumulatedDepreciation,
        Self::ContraAsset,
        Self::IntangibleAsset,
        Self::OtherAsset,
        Self::AccountsPayable,
        Self::GoodsReceivedNotInvoiced,
        Self::TaxPayable,
        Self::AccruedLiability,
        Self::OtherCurrentLiability,
        Self::LongTermLiability,
        Self::Equity,
        Self::RetainedEarnings,
        Self::ContraEquity,
        Self::Revenue,
        Self::ContraRevenue,
        Self::OtherIncome,
        Self::CostOfSales,
        Self::OperatingExpense,
        Self::Depreciation,
        Self::OtherExpense,
        Self::IncomeTax,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::Bank => "bank",
            Self::AccountsReceivable => "accounts_receivable",
            Self::Inventory => "inventory",
            Self::PrepaidExpense => "prepaid_expense",
            Self::OtherCurrentAsset => "other_current_asset",
            Self::FixedAsset => "fixed_asset",
            Self::AccumulatedDepreciation => "accumulated_depreciation",
            Self::ContraAsset => "contra_asset",
            Self::IntangibleAsset => "intangible_asset",
            Self::OtherAsset => "other_asset",
            Self::AccountsPayable => "accounts_payable",
            Self::GoodsReceivedNotInvoiced => "goods_received_not_invoiced",
            Self::TaxPayable => "tax_payable",
            Self::AccruedLiability => "accrued_liability",
            Self::OtherCurrentLiability => "other_current_liability",
            Self::LongTermLiability => "long_term_liability",
            Self::Equity => "equity",
            Self::RetainedEarnings => "retained_earnings",
            Self::ContraEquity => "contra_equity",
            Self::Revenue => "revenue",
            Self::ContraRevenue => "contra_revenue",
            Self::OtherIncome => "other_income",
            Self::CostOfSales => "cost_of_sales",
            Self::OperatingExpense => "operating_expense",
            Self::Depreciation => "depreciation",
            Self::OtherExpense => "other_expense",
            Self::IncomeTax => "income_tax",
        }
    }

    /// Read a stored value back.
    ///
    /// `None` rather than a default: the type decides the normal balance, and a
    /// guess would report a credit balance as a debit one.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    pub const fn class(self) -> AccountClass {
        match self {
            Self::Cash
            | Self::Bank
            | Self::AccountsReceivable
            | Self::Inventory
            | Self::PrepaidExpense
            | Self::OtherCurrentAsset
            | Self::FixedAsset
            | Self::AccumulatedDepreciation
            | Self::ContraAsset
            | Self::IntangibleAsset
            | Self::OtherAsset => AccountClass::Asset,

            Self::AccountsPayable
            | Self::GoodsReceivedNotInvoiced
            | Self::TaxPayable
            | Self::AccruedLiability
            | Self::OtherCurrentLiability
            | Self::LongTermLiability => AccountClass::Liability,

            Self::Equity | Self::RetainedEarnings | Self::ContraEquity => AccountClass::Equity,

            Self::Revenue | Self::ContraRevenue | Self::OtherIncome => AccountClass::Revenue,

            Self::CostOfSales
            | Self::OperatingExpense
            | Self::Depreciation
            | Self::OtherExpense
            | Self::IncomeTax => AccountClass::Expense,
        }
    }

    /// Which side the balance normally sits on.
    ///
    /// Per type rather than per class, because the two contra types invert it.
    pub const fn normal_balance(self) -> Side {
        match self {
            // The contra types: opposite their class, which is the whole
            // reason this is per type.
            Self::AccumulatedDepreciation | Self::ContraAsset => Side::Credit,
            Self::ContraRevenue | Self::ContraEquity => Side::Debit,
            _ => match self.class() {
                AccountClass::Asset | AccountClass::Expense => Side::Debit,
                AccountClass::Liability | AccountClass::Equity | AccountClass::Revenue => {
                    Side::Credit
                }
            },
        }
    }

    pub const fn closes_at_year_end(self) -> bool {
        self.class().closes_at_year_end()
    }

    /// Whether a sub-ledger owns this account's balance.
    ///
    /// A control account is the general ledger's side of a sub-ledger — the
    /// receivables total is the sales ledger, the inventory total is the stock
    /// ledger. Posting a journal to one by hand breaks the equality the two are
    /// reconciled on, and the break is silent until somebody runs the
    /// reconciliation. So the ledger refuses it: the sub-ledger posts here, a
    /// person does not.
    pub const fn is_control(self) -> bool {
        matches!(
            self,
            Self::AccountsReceivable
                | Self::AccountsPayable
                | Self::Inventory
                | Self::GoodsReceivedNotInvoiced
        )
    }

    /// Whether a person may name this account on a journal they type.
    pub const fn allows_manual_posting(self) -> bool {
        !self.is_control()
    }

    pub fn label(self) -> Message {
        match self {
            Self::Cash => msg!("books.account_type.cash"),
            Self::Bank => msg!("books.account_type.bank"),
            Self::AccountsReceivable => msg!("books.account_type.accounts_receivable"),
            Self::Inventory => msg!("books.account_type.inventory"),
            Self::PrepaidExpense => msg!("books.account_type.prepaid_expense"),
            Self::OtherCurrentAsset => msg!("books.account_type.other_current_asset"),
            Self::FixedAsset => msg!("books.account_type.fixed_asset"),
            Self::AccumulatedDepreciation => msg!("books.account_type.accumulated_depreciation"),
            Self::ContraAsset => msg!("books.account_type.contra_asset"),
            Self::IntangibleAsset => msg!("books.account_type.intangible_asset"),
            Self::OtherAsset => msg!("books.account_type.other_asset"),
            Self::AccountsPayable => msg!("books.account_type.accounts_payable"),
            Self::GoodsReceivedNotInvoiced => {
                msg!("books.account_type.goods_received_not_invoiced")
            }
            Self::TaxPayable => msg!("books.account_type.tax_payable"),
            Self::AccruedLiability => msg!("books.account_type.accrued_liability"),
            Self::OtherCurrentLiability => msg!("books.account_type.other_current_liability"),
            Self::LongTermLiability => msg!("books.account_type.long_term_liability"),
            Self::Equity => msg!("books.account_type.equity"),
            Self::RetainedEarnings => msg!("books.account_type.retained_earnings"),
            Self::ContraEquity => msg!("books.account_type.contra_equity"),
            Self::Revenue => msg!("books.account_type.revenue"),
            Self::ContraRevenue => msg!("books.account_type.contra_revenue"),
            Self::OtherIncome => msg!("books.account_type.other_income"),
            Self::CostOfSales => msg!("books.account_type.cost_of_sales"),
            Self::OperatingExpense => msg!("books.account_type.operating_expense"),
            Self::Depreciation => msg!("books.account_type.depreciation"),
            Self::OtherExpense => msg!("books.account_type.other_expense"),
            Self::IncomeTax => msg!("books.account_type.income_tax"),
        }
    }
}

/// One account in the chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub number: String,
    pub name: String,
    pub account_type: AccountType,
    /// What it is for, where the name is not enough. From the default chart for
    /// a seeded account, and the reason somebody can tell 5100 from 5200.
    pub description: Option<String>,
    pub is_active: bool,
    /// True for a row the app seeded. The workspace may edit or delete it — a
    /// default is what you start with, not what you are held to — but a screen
    /// can say where it came from.
    pub is_default: bool,
}

impl Account {
    pub const fn class(&self) -> AccountClass {
        self.account_type.class()
    }

    pub const fn normal_balance(&self) -> Side {
        self.account_type.normal_balance()
    }

    /// Whether a person may post to it directly: an active, non-control row.
    pub const fn is_postable(&self) -> bool {
        self.is_active && self.account_type.allows_manual_posting()
    }

    /// `1200 · Bank current account`. Here so two screens do not spell one row
    /// two ways.
    pub fn label(&self) -> String {
        format!("{} · {}", self.number, self.name)
    }
}

/// One row of the chart, as a grid reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSummary {
    pub id: Uuid,
    pub number: String,
    pub name: String,
    pub account_type: AccountType,
    pub is_active: bool,
    pub is_default: bool,
    /// Whether anything has ever been posted to it. Carried on the row so the
    /// delete button does not need a query each.
    pub has_entries: bool,
}

impl AccountSummary {
    pub const fn class(&self) -> AccountClass {
        self.account_type.class()
    }
}

/// The editable part of an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInput {
    /// Absent means create.
    pub id: Option<Uuid>,
    pub number: String,
    pub name: String,
    pub account_type: AccountType,
    pub description: Option<String>,
    pub is_active: bool,
}

impl AccountInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            number: String::new(),
            name: String::new(),
            // Nothing is a safe default here, so the least harmful one: an
            // operating expense is the commonest account a workspace adds and
            // the one whose misfiling is cheapest to correct.
            account_type: AccountType::OperatingExpense,
            description: None,
            is_active: true,
        }
    }

    pub fn from_account(account: &Account) -> Self {
        Self {
            id: Some(account.id),
            number: account.number.clone(),
            name: account.name.clone(),
            account_type: account.account_type,
            description: account.description.clone(),
            is_active: account.is_active,
        }
    }

    /// Trim, and say what is still wrong.
    ///
    /// Unlike a department, an account number is never generated: the default
    /// chart supplies one and an accountant adding an account has a number in
    /// mind before they have a name. So blank is refused on create too.
    pub fn check(&self) -> Result<Self, AccountError> {
        let number = self.number.trim();
        let name = self.name.trim();

        if number.is_empty() {
            return Err(AccountError::NumberRequired);
        }
        if number.chars().count() > MAX_ACCOUNT_NUMBER_LEN {
            return Err(AccountError::NumberTooLong);
        }
        // Matches `accounts_number_format`. Dots because sub-accounts are
        // conventionally written 1200.01.
        if !number.starts_with(|c: char| c.is_ascii_alphanumeric())
            || !number
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        {
            return Err(AccountError::NumberShape);
        }

        if name.is_empty() {
            return Err(AccountError::NameRequired);
        }
        if name.chars().count() > MAX_ACCOUNT_NAME_LEN {
            return Err(AccountError::NameTooLong);
        }

        let description = self
            .description
            .as_deref()
            .map(str::trim)
            .filter(|it| !it.is_empty())
            .map(str::to_owned);

        Ok(Self {
            id: self.id,
            number: number.to_owned(),
            name: name.to_owned(),
            account_type: self.account_type,
            description,
            is_active: self.is_active,
        })
    }
}

/// What can be wrong with an account somebody typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AccountError {
    #[error("an account needs a number")]
    NumberRequired,
    #[error("an account number is at most 20 characters")]
    NumberTooLong,
    #[error("an account number may contain only letters, digits, dots, hyphens and underscores")]
    NumberShape,
    #[error("an account needs a name")]
    NameRequired,
    #[error("an account name is at most 120 characters")]
    NameTooLong,
}

impl AccountError {
    /// Which control to attach the message to.
    pub fn field(self) -> &'static str {
        match self {
            Self::NumberRequired | Self::NumberTooLong | Self::NumberShape => "number",
            Self::NameRequired | Self::NameTooLong => "name",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NumberRequired => msg!("account.error.number_required"),
            Self::NumberTooLong => msg!("account.error.number_too_long"),
            Self::NumberShape => msg!("account.error.number_shape"),
            Self::NameRequired => msg!("account.error.name_required"),
            Self::NameTooLong => msg!("account.error.name_too_long"),
        }
    }
}

/// What a delete answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    /// Gone. Also the answer when it was already gone.
    Deleted,
    /// Something has been posted to it. An account with history is never
    /// removed — the history would stop naming anything. Deactivation instead.
    HasEntries,
}

/// Longest description the column holds.
pub const MAX_ACCOUNT_DESCRIPTION_LEN: usize = 500;

/// The chart a workspace starts with, as `config/defaults/books.toml` declares
/// it.
///
/// The shape lives here rather than in `phonix-config` because the rules about
/// what makes a chart valid are Books' rules — see
/// [`phonix_config::defaults`](../../phonix_config/defaults/index.html), which
/// knows only how to find the file and parse it into whatever shape the app
/// asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultChart {
    /// `[[account]]` in the file.
    #[serde(default)]
    pub account: Vec<DefaultAccount>,
}

/// One account the file declares.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultAccount {
    pub number: String,
    pub name: String,
    /// `type` in the file, which is a keyword in Rust.
    ///
    /// Typed rather than a `String`, so an account type the build does not know
    /// is refused while somebody is watching rather than at the moment a
    /// workspace is provisioned.
    #[serde(rename = "type")]
    pub account_type: AccountType,
    #[serde(default)]
    pub description: Option<String>,
}

impl DefaultChart {
    /// Everything that has to be true before the chart is worth installing.
    ///
    /// Checked at load, for the reason `numbering` checks a mask at load: a
    /// chart installed from a broken definition is unpicked by hand afterwards,
    /// in a live workspace.
    pub fn check(&self) -> Result<(), DefaultChartError> {
        let mut seen: Vec<String> = Vec::new();

        for entry in &self.account {
            let draft = AccountInput {
                id: None,
                number: entry.number.clone(),
                name: entry.name.clone(),
                account_type: entry.account_type,
                description: entry.description.clone(),
                is_active: true,
            };

            // The same rules a person typing an account is held to. Two sets
            // would eventually disagree, and the file would be the one that was
            // wrong in production.
            draft.check().map_err(|source| DefaultChartError::Account {
                number: entry.number.clone(),
                source,
            })?;

            if let Some(description) = &entry.description
                && description.trim().chars().count() > MAX_ACCOUNT_DESCRIPTION_LEN
            {
                return Err(DefaultChartError::DescriptionTooLong {
                    number: entry.number.clone(),
                });
            }

            // Case-insensitive, matching `accounts_number_key`. Without this
            // the duplicate is silently dropped by ON CONFLICT DO NOTHING and
            // the workspace is missing an account nobody can account for.
            let key = entry.number.trim().to_lowercase();
            if seen.contains(&key) {
                return Err(DefaultChartError::Duplicate {
                    number: entry.number.clone(),
                });
            }
            seen.push(key);
        }

        Ok(())
    }
}

/// Why a default chart was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefaultChartError {
    #[error("account {number} is not valid: {source}")]
    Account {
        number: String,
        source: AccountError,
    },
    #[error("account {number} has a description longer than 500 characters")]
    DescriptionTooLong { number: String },
    #[error("account {number} is declared twice")]
    Duplicate { number: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AccountInput {
        AccountInput {
            number: "1200".to_owned(),
            name: "Bank current account".to_owned(),
            account_type: AccountType::Bank,
            ..AccountInput::blank()
        }
    }

    #[test]
    fn every_type_round_trips_through_its_stored_form() {
        for account_type in AccountType::ALL {
            assert_eq!(
                AccountType::parse(account_type.as_str()),
                Some(*account_type)
            );
        }
        for class in AccountClass::ALL {
            assert_eq!(AccountClass::parse(class.as_str()), Some(*class));
        }
        for side in Side::ALL {
            assert_eq!(Side::parse(side.as_str()), Some(*side));
        }
    }

    #[test]
    fn stored_forms_are_distinct() {
        // A duplicate would silently merge two types on the way back out.
        let mut seen: Vec<&str> = AccountType::ALL.iter().map(|it| it.as_str()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn serde_spells_a_type_exactly_as_the_column_stores_it() {
        // `rename_all = "snake_case"` and `as_str` are two spellings of one
        // thing. If they ever drift, the default chart stops loading and a
        // stored row stops deserialising - so pin them together.
        use serde::de::IntoDeserializer;

        for account_type in AccountType::ALL {
            let raw: serde::de::value::StrDeserializer<serde::de::value::Error> =
                account_type.as_str().into_deserializer();
            assert_eq!(
                AccountType::deserialize(raw).expect("the stored form deserialises"),
                *account_type,
                "{account_type:?}"
            );
        }

        for side in Side::ALL {
            let raw: serde::de::value::StrDeserializer<serde::de::value::Error> =
                side.as_str().into_deserializer();
            assert_eq!(Side::deserialize(raw).expect("valid"), *side);
        }

        for class in AccountClass::ALL {
            let raw: serde::de::value::StrDeserializer<serde::de::value::Error> =
                class.as_str().into_deserializer();
            assert_eq!(AccountClass::deserialize(raw).expect("valid"), *class);
        }
    }

    fn default_account(number: &str) -> DefaultAccount {
        DefaultAccount {
            number: number.to_owned(),
            name: "Bank current account".to_owned(),
            account_type: AccountType::Bank,
            description: None,
        }
    }

    #[test]
    fn a_default_chart_is_held_to_the_rules_a_typed_account_is() {
        let chart = DefaultChart {
            account: vec![DefaultAccount {
                name: "  ".to_owned(),
                ..default_account("1030")
            }],
        };

        assert_eq!(
            chart.check(),
            Err(DefaultChartError::Account {
                number: "1030".to_owned(),
                source: AccountError::NameRequired,
            })
        );
    }

    #[test]
    fn a_number_declared_twice_is_refused_rather_than_silently_dropped() {
        // ON CONFLICT DO NOTHING would swallow the second one, and the
        // workspace would be missing an account with nothing to say why.
        let chart = DefaultChart {
            account: vec![default_account("1030"), default_account("1030")],
        };

        assert_eq!(
            chart.check(),
            Err(DefaultChartError::Duplicate {
                number: "1030".to_owned()
            })
        );
    }

    #[test]
    fn duplicate_numbers_are_caught_regardless_of_case() {
        // `accounts_number_key` is on lower(number).
        let chart = DefaultChart {
            account: vec![default_account("1030a"), default_account("1030A")],
        };

        assert!(matches!(
            chart.check(),
            Err(DefaultChartError::Duplicate { .. })
        ));
    }

    #[test]
    fn an_empty_chart_is_valid() {
        assert_eq!(DefaultChart::default().check(), Ok(()));
    }

    #[test]
    fn an_unknown_stored_type_is_refused_rather_than_guessed() {
        assert_eq!(AccountType::parse("goodwill_amortisation"), None);
        assert_eq!(AccountType::parse(""), None);
    }

    #[test]
    fn assets_and_expenses_are_debit_balances_and_the_rest_are_credit() {
        assert_eq!(AccountType::Bank.normal_balance(), Side::Debit);
        assert_eq!(AccountType::CostOfSales.normal_balance(), Side::Debit);
        assert_eq!(AccountType::AccountsPayable.normal_balance(), Side::Credit);
        assert_eq!(AccountType::Equity.normal_balance(), Side::Credit);
        assert_eq!(AccountType::Revenue.normal_balance(), Side::Credit);
    }

    #[test]
    fn the_contra_types_invert_their_class() {
        // The whole reason the balance is per type rather than per class.
        let depreciation = AccountType::AccumulatedDepreciation;
        assert_eq!(depreciation.class(), AccountClass::Asset);
        assert_eq!(depreciation.normal_balance(), Side::Credit);

        let allowance = AccountType::ContraAsset;
        assert_eq!(allowance.class(), AccountClass::Asset);
        assert_eq!(allowance.normal_balance(), Side::Credit);

        let returns = AccountType::ContraRevenue;
        assert_eq!(returns.class(), AccountClass::Revenue);
        assert_eq!(returns.normal_balance(), Side::Debit);

        let drawings = AccountType::ContraEquity;
        assert_eq!(drawings.class(), AccountClass::Equity);
        assert_eq!(drawings.normal_balance(), Side::Debit);
    }

    #[test]
    fn only_revenue_and_expense_close_at_year_end() {
        for account_type in AccountType::ALL {
            let closes = matches!(
                account_type.class(),
                AccountClass::Revenue | AccountClass::Expense
            );
            assert_eq!(
                account_type.closes_at_year_end(),
                closes,
                "{account_type:?}"
            );
        }
    }

    #[test]
    fn a_sub_ledgers_control_account_is_not_postable_by_hand() {
        // Posting to one by hand breaks the equality the sub-ledger is
        // reconciled on, silently.
        assert!(!AccountType::AccountsReceivable.allows_manual_posting());
        assert!(!AccountType::Inventory.allows_manual_posting());
        assert!(!AccountType::GoodsReceivedNotInvoiced.allows_manual_posting());

        // Tax is a liability the ledger posts to directly, not a sub-ledger.
        assert!(AccountType::TaxPayable.allows_manual_posting());
        assert!(AccountType::OperatingExpense.allows_manual_posting());
    }

    #[test]
    fn an_account_needs_a_number_even_on_create() {
        // Unlike a department code, which is allocated when left blank.
        let blank = AccountInput {
            number: "  ".to_owned(),
            ..input()
        };
        assert_eq!(blank.check(), Err(AccountError::NumberRequired));
    }

    #[test]
    fn a_sub_account_number_is_allowed_and_trimmed() {
        let sub = AccountInput {
            number: "  1200.01 ".to_owned(),
            ..input()
        };
        assert_eq!(sub.check().expect("valid").number, "1200.01");

        let spaced = AccountInput {
            number: "12 00".to_owned(),
            ..input()
        };
        assert_eq!(spaced.check(), Err(AccountError::NumberShape));
    }

    #[test]
    fn an_account_needs_a_name() {
        let blank = AccountInput {
            name: "   ".to_owned(),
            ..input()
        };
        assert_eq!(blank.check(), Err(AccountError::NameRequired));
    }

    #[test]
    fn a_blank_description_is_stored_as_absent() {
        // So "no description" is one value rather than two.
        let empty = AccountInput {
            description: Some("   ".to_owned()),
            ..input()
        };
        assert_eq!(empty.check().expect("valid").description, None);
    }

    #[test]
    fn a_control_account_is_never_postable_however_active_it_is() {
        let receivables = Account {
            id: Uuid::nil(),
            number: "1100".to_owned(),
            name: "Trade receivables".to_owned(),
            account_type: AccountType::AccountsReceivable,
            description: None,
            is_active: true,
            is_default: true,
        };

        assert!(!receivables.is_postable());
        assert_eq!(receivables.label(), "1100 · Trade receivables");
    }
}
