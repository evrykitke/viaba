//! The four statements a workspace is asked for, assembled from the ledger.
//!
//! ```text
//!   trial balance      every account, both columns, and they agree
//!   balance sheet      what is owned, what is owed, at a date
//!   profit and loss    what was earned and spent, between two dates
//!   customer statement what one customer was invoiced, and what is overdue
//! ```
//!
//! # One query behind three of them
//!
//! A report is a [`AccountMovement`] per account: what the balance was before
//! the span opened, and what was debited and credited inside it. The trial
//! balance prints that directly, the profit and loss reads the span, and the
//! balance sheet reads both - the part before the financial year opened is
//! retained earnings and the part inside it is this year's profit. Three
//! statements from one shape is why they cannot disagree with each other.
//!
//! # Signs
//!
//! A movement is held debit-positive, because that is how a ledger is written
//! and the only convention under which everything sums to nothing. Each
//! statement flips what it presents: a sales account with a credit balance of
//! four thousand is `-4000` here and `4000` on the profit and loss, because
//! nobody reads revenue as a negative number. [`AccountMovement::natural`] is
//! that flip, and it is per account type - a contra-revenue account runs the
//! other way from the class it belongs to.
//!
//! # Nothing here reads the clock
//!
//! Every span is given. A report that defaulted to "today" inside this crate
//! would render differently on the server and in the browser either side of
//! midnight, and the screen chooses the dates anyway.

use chrono::NaiveDate;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, MoneyError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::{AccountClass, AccountType, Side};

/// What one account did, either side of a date.
///
/// `opening` is every posting before the span, net and debit-positive.
/// `debits` and `credits` are the span's own, each a positive total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMovement {
    pub account_id: Uuid,
    pub number: String,
    pub name: String,
    pub account_type: AccountType,
    pub opening: Money,
    pub debits: Money,
    pub credits: Money,
}

impl AccountMovement {
    /// The balance at the end of the span, debit-positive.
    pub fn closing(&self) -> Result<Money, MoneyError> {
        self.opening.checked_add(self.movement()?)
    }

    /// What the span alone did, debit-positive.
    pub fn movement(&self) -> Result<Money, MoneyError> {
        self.debits.checked_sub(self.credits)
    }

    /// The closing balance the way this account is read.
    ///
    /// Positive for an account sitting on its normal side. Per type rather
    /// than per class, because accumulated depreciation and sales returns both
    /// run against the class they belong to.
    pub fn natural(&self) -> Result<Money, MoneyError> {
        Ok(flip(self.closing()?, self.account_type))
    }

    /// The span's movement, the way this account is read. What a profit and
    /// loss prints.
    pub fn natural_movement(&self) -> Result<Money, MoneyError> {
        Ok(flip(self.movement()?, self.account_type))
    }

    /// Nothing before, nothing during. Left off every statement: a chart of
    /// two hundred accounts is mostly this, and a page of zeroes hides the
    /// dozen lines somebody came to read.
    pub const fn is_quiet(&self) -> bool {
        self.opening.is_zero() && self.debits.is_zero() && self.credits.is_zero()
    }
}

const fn flip(amount: Money, account_type: AccountType) -> Money {
    match account_type.normal_balance() {
        Side::Debit => amount,
        Side::Credit => amount.negate(),
    }
}

/// One line of a statement: an account and what it comes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportLine {
    pub account_id: Uuid,
    pub number: String,
    pub name: String,
    /// Presented the way the section reads, never debit-positive.
    pub amount: Money,
}

/// A set of lines and what they add up to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportGroup {
    pub lines: Vec<ReportLine>,
    pub total: Money,
}

impl ReportGroup {
    fn assemble(currency: Currency, lines: Vec<ReportLine>) -> Result<Self, MoneyError> {
        let total = Money::total(currency, lines.iter().map(|line| line.amount))?;

        Ok(Self { lines, total })
    }

    pub const fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

// --- Trial balance --------------------------------------------------------

/// One account on the trial balance.
///
/// Both closing columns are held rather than one signed figure, because that
/// is the report: a trial balance is two columns that agree, and deciding
/// which column a balance sits in is the whole of the arithmetic being
/// checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBalanceRow {
    pub account_id: Uuid,
    pub number: String,
    pub name: String,
    pub account_type: AccountType,
    pub opening: Money,
    pub debits: Money,
    pub credits: Money,
    /// The closing balance where it falls on the debit side, else zero.
    pub closing_debit: Money,
    /// The closing balance where it falls on the credit side, else zero.
    pub closing_credit: Money,
}

/// Every account, both columns, and the proof they agree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialBalance {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub currency: Currency,
    pub rows: Vec<TrialBalanceRow>,
    pub debits: Money,
    pub credits: Money,
    pub closing_debits: Money,
    pub closing_credits: Money,
}

impl TrialBalance {
    /// Assemble from one account movement per account, in the order they
    /// should print.
    pub fn assemble(
        from: NaiveDate,
        to: NaiveDate,
        currency: Currency,
        movements: &[AccountMovement],
    ) -> Result<Self, MoneyError> {
        let zero = Money::zero(currency);
        let mut rows = Vec::new();

        for movement in movements.iter().filter(|movement| !movement.is_quiet()) {
            let closing = movement.closing()?;

            rows.push(TrialBalanceRow {
                account_id: movement.account_id,
                number: movement.number.clone(),
                name: movement.name.clone(),
                account_type: movement.account_type,
                opening: movement.opening,
                debits: movement.debits,
                credits: movement.credits,
                closing_debit: if closing.is_negative() { zero } else { closing },
                closing_credit: if closing.is_negative() {
                    closing.negate()
                } else {
                    zero
                },
            });
        }

        Ok(Self {
            from,
            to,
            currency,
            debits: Money::total(currency, rows.iter().map(|row| row.debits))?,
            credits: Money::total(currency, rows.iter().map(|row| row.credits))?,
            closing_debits: Money::total(currency, rows.iter().map(|row| row.closing_debit))?,
            closing_credits: Money::total(currency, rows.iter().map(|row| row.closing_credit))?,
            rows,
        })
    }

    /// Whether the two closing columns agree.
    ///
    /// They cannot fail to, short of a bug: a journal cannot be posted
    /// unbalanced. Which is exactly why it is worth printing - the report
    /// whose only job is to say "still balanced" has to say it out loud.
    pub fn is_balanced(&self) -> bool {
        self.closing_debits == self.closing_credits && self.debits == self.credits
    }
}

// --- Profit and loss ------------------------------------------------------

/// What was earned and what it cost, between two dates.
///
/// Five groups and three subtotals, which is the shape every reader of one of
/// these expects. Depreciation sits in operating expenses rather than on its
/// own line: a workspace that wants it separately gives it its own account,
/// and the report follows the chart rather than overruling it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomeStatement {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub currency: Currency,
    pub revenue: ReportGroup,
    pub cost_of_sales: ReportGroup,
    pub gross_profit: Money,
    pub operating_expenses: ReportGroup,
    pub operating_profit: Money,
    pub other_income: ReportGroup,
    pub other_expenses: ReportGroup,
    pub net_profit: Money,
}

impl IncomeStatement {
    pub fn assemble(
        from: NaiveDate,
        to: NaiveDate,
        currency: Currency,
        movements: &[AccountMovement],
    ) -> Result<Self, MoneyError> {
        let mut revenue = Vec::new();
        let mut cost_of_sales = Vec::new();
        let mut operating = Vec::new();
        let mut other_income = Vec::new();
        let mut other_expenses = Vec::new();

        for movement in movements {
            let amount = movement.natural_movement()?;

            // Zero for the span, whatever it holds. A profit and loss covers a
            // period, and an account that did nothing in it did nothing in it.
            if amount.is_zero() {
                continue;
            }

            let line = ReportLine {
                account_id: movement.account_id,
                number: movement.number.clone(),
                name: movement.name.clone(),
                amount,
            };

            match movement.account_type {
                AccountType::Revenue | AccountType::ContraRevenue => revenue.push(line),
                AccountType::OtherIncome => other_income.push(line),
                AccountType::CostOfSales => cost_of_sales.push(line),
                AccountType::OperatingExpense | AccountType::Depreciation => operating.push(line),
                AccountType::OtherExpense | AccountType::IncomeTax => other_expenses.push(line),
                // A balance sheet account. It has no business here, and an
                // assembler that put it somewhere would be inventing a figure.
                _ => {}
            }
        }

        let revenue = ReportGroup::assemble(currency, revenue)?;
        let cost_of_sales = ReportGroup::assemble(currency, cost_of_sales)?;
        let operating_expenses = ReportGroup::assemble(currency, operating)?;
        let other_income = ReportGroup::assemble(currency, other_income)?;
        let other_expenses = ReportGroup::assemble(currency, other_expenses)?;

        let gross_profit = revenue.total.checked_sub(cost_of_sales.total)?;
        let operating_profit = gross_profit.checked_sub(operating_expenses.total)?;
        let net_profit = operating_profit
            .checked_add(other_income.total)?
            .checked_sub(other_expenses.total)?;

        Ok(Self {
            from,
            to,
            currency,
            revenue,
            cost_of_sales,
            gross_profit,
            operating_expenses,
            operating_profit,
            other_income,
            other_expenses,
            net_profit,
        })
    }
}

// --- Balance sheet --------------------------------------------------------

/// What is owned and what is owed, at one date.
///
/// # Where the profit goes
///
/// Nothing has been closed into retained earnings - there is no year-end
/// routine, and a balance sheet that ignored the profit and loss accounts
/// would be out by exactly the profit. So the two are shown as what they are:
/// what the revenue and expense accounts held when the financial year opened
/// is *brought forward*, and what they have done since is *this year's
/// result*. Both sit in equity, and together they are what makes the two sides
/// agree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceSheet {
    pub as_at: NaiveDate,
    /// The day the financial year opened. Everything before it is brought
    /// forward.
    pub year_opened: NaiveDate,
    pub currency: Currency,
    pub assets: ReportGroup,
    pub liabilities: ReportGroup,
    /// The equity accounts themselves, without the two earnings lines.
    pub equity: ReportGroup,
    pub brought_forward: Money,
    pub result_for_year: Money,
    pub total_assets: Money,
    /// Liabilities, equity and both earnings lines. What the assets are funded
    /// by.
    pub total_funding: Money,
}

impl BalanceSheet {
    /// Assemble from movements queried across the financial year: `opening` is
    /// everything before `year_opened`, and the span runs to `as_at`.
    pub fn assemble(
        as_at: NaiveDate,
        year_opened: NaiveDate,
        currency: Currency,
        movements: &[AccountMovement],
    ) -> Result<Self, MoneyError> {
        let mut assets = Vec::new();
        let mut liabilities = Vec::new();
        let mut equity = Vec::new();
        let mut brought_forward = Money::zero(currency);
        let mut result_for_year = Money::zero(currency);

        for movement in movements.iter().filter(|movement| !movement.is_quiet()) {
            let class = movement.account_type.class();

            if class.closes_at_year_end() {
                // Credit-positive, so a profit adds to what funds the assets.
                brought_forward =
                    brought_forward.checked_sub(movement.opening)?;
                result_for_year = result_for_year.checked_sub(movement.movement()?)?;
                continue;
            }

            let amount = movement.natural()?;
            if amount.is_zero() {
                continue;
            }

            let line = ReportLine {
                account_id: movement.account_id,
                number: movement.number.clone(),
                name: movement.name.clone(),
                amount,
            };

            match class {
                AccountClass::Asset => assets.push(line),
                AccountClass::Liability => liabilities.push(line),
                AccountClass::Equity => equity.push(line),
                AccountClass::Revenue | AccountClass::Expense => {}
            }
        }

        let assets = ReportGroup::assemble(currency, assets)?;
        let liabilities = ReportGroup::assemble(currency, liabilities)?;
        let equity = ReportGroup::assemble(currency, equity)?;

        let total_funding = liabilities
            .total
            .checked_add(equity.total)?
            .checked_add(brought_forward)?
            .checked_add(result_for_year)?;

        Ok(Self {
            as_at,
            year_opened,
            currency,
            total_assets: assets.total,
            assets,
            liabilities,
            equity,
            brought_forward,
            result_for_year,
            total_funding,
        })
    }

    /// Whether the two sides agree. They do, unless something is wrong that a
    /// person needs to be told about.
    pub fn is_balanced(&self) -> bool {
        self.total_assets == self.total_funding
    }
}

// --- Customer statement ---------------------------------------------------

/// One document on a customer's statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementLine {
    pub invoice_id: Uuid,
    pub number: String,
    pub issued_on: NaiveDate,
    pub due_on: Option<NaiveDate>,
    /// What the document says, in the currency it was raised in.
    pub invoiced: Money,
    /// What that was worth in the workspace's own currency on the day. Equal
    /// to `invoiced` where the two currencies are the same.
    pub amount: Money,
    /// The balance after this line.
    pub running: Money,
}

impl StatementLine {
    /// The date the clock runs from: the day it falls due, or the day it was
    /// issued where nobody set terms.
    pub const fn owed_from(&self) -> NaiveDate {
        match self.due_on {
            Some(due) => due,
            None => self.issued_on,
        }
    }
}

/// How overdue the balance is, at the statement's own date.
///
/// Five buckets, by how long past due each document is. The usual ladder, and
/// the reason anybody prints a statement rather than a balance.
///
/// Every outstanding document is on it, including the ones that fall before
/// the span and make up the opening balance - so the five add up to the
/// closing balance. An ageing of only the current month would put nothing in
/// the far column, which is the one column anybody reads it for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ageing {
    pub not_yet_due: Money,
    pub to_30: Money,
    pub to_60: Money,
    pub to_90: Money,
    pub over_90: Money,
}

impl Ageing {
    fn assemble(
        currency: Currency,
        as_at: NaiveDate,
        lines: &[StatementLine],
    ) -> Result<Self, MoneyError> {
        let zero = Money::zero(currency);
        let mut buckets = [zero; 5];

        for line in lines {
            let days = (as_at - line.owed_from()).num_days();
            let index = match days {
                ..=0 => 0,
                1..=30 => 1,
                31..=60 => 2,
                61..=90 => 3,
                _ => 4,
            };

            if let Some(bucket) = buckets.get_mut(index) {
                *bucket = bucket.checked_add(line.amount)?;
            }
        }

        let at = |index: usize| buckets.get(index).copied().unwrap_or(zero);

        Ok(Self {
            not_yet_due: at(0),
            to_30: at(1),
            to_60: at(2),
            to_90: at(3),
            over_90: at(4),
        })
    }
}

/// What one customer was invoiced, and what of it is still outstanding.
///
/// # This is an invoiced statement, not a settled one
///
/// Nothing in this workspace records a customer paying: there is no cash book
/// and no allocation, so every posted invoice counts as outstanding until one
/// exists. The balance here is therefore what has been *billed* in the span,
/// and the ageing is how long ago it was billed. Said plainly on the screen as
/// well, because a statement that quietly meant something other than "this is
/// what you owe" would be worse than no statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerStatement {
    pub party_id: Uuid,
    pub party_code: String,
    pub party_name: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub currency: Currency,
    /// Billed before the span opened.
    pub opening: Money,
    pub lines: Vec<StatementLine>,
    pub billed: Money,
    pub closing: Money,
    pub ageing: Ageing,
}

impl CustomerStatement {
    /// Assemble from every posted invoice up to `to`, in date order. The ones
    /// before `from` make the opening balance and do not print.
    pub fn assemble(
        party_id: Uuid,
        party_code: String,
        party_name: String,
        from: NaiveDate,
        to: NaiveDate,
        currency: Currency,
        invoices: Vec<StatementLine>,
    ) -> Result<Self, MoneyError> {
        let mut opening = Money::zero(currency);
        let mut lines = Vec::new();

        // Over everything outstanding, which is everything: what fell before
        // the span is in the opening balance and is still owed.
        let ageing = Ageing::assemble(currency, to, &invoices)?;

        for mut line in invoices {
            if line.issued_on < from {
                opening = opening.checked_add(line.amount)?;
                continue;
            }

            let previous = lines
                .last()
                .map_or(opening, |last: &StatementLine| last.running);

            line.running = previous.checked_add(line.amount)?;
            lines.push(line);
        }

        let billed = Money::total(currency, lines.iter().map(|line| line.amount))?;
        let closing = opening.checked_add(billed)?;

        Ok(Self {
            party_id,
            party_code,
            party_name,
            from,
            to,
            currency,
            opening,
            billed,
            closing,
            ageing,
            lines,
        })
    }
}

// --- The front page -------------------------------------------------------

/// The handful of figures an app's home page carries.
///
/// Assembled on the server and sent whole, which is what keeps it off the
/// clock: a page that worked out "this year" in the browser as well as on the
/// server would disagree with itself either side of midnight, and a hydration
/// mismatch takes the whole application down. See `components::app_home`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerSummary {
    pub currency: Currency,
    pub as_at: NaiveDate,
    pub year_opened: NaiveDate,
    /// What has been earned since the financial year opened.
    pub revenue: Money,
    /// What is left of it after everything it cost.
    pub result: Money,
    pub total_assets: Money,
    /// Posted invoices, the lot of them: nothing here records a customer
    /// paying, so none of them has been settled. See [`CustomerStatement`].
    pub owed_by_customers: Money,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENCY: Currency = Currency::USD;

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    fn money(amount: &str) -> Money {
        Money::parse(CURRENCY, amount).unwrap()
    }

    fn movement(
        number: &str,
        account_type: AccountType,
        opening: &str,
        debits: &str,
        credits: &str,
    ) -> AccountMovement {
        AccountMovement {
            account_id: Uuid::from_u128(u128::from(number.len() as u32)),
            number: number.to_owned(),
            name: format!("Account {number}"),
            account_type,
            opening: money(opening),
            debits: money(debits),
            credits: money(credits),
        }
    }

    /// The ledger in miniature: stock bought for 600, half of it sold for 500.
    fn ledger() -> Vec<AccountMovement> {
        vec![
            movement("1200", AccountType::Inventory, "0", "600", "300"),
            movement("1100", AccountType::AccountsReceivable, "0", "500", "0"),
            movement("2000", AccountType::AccountsPayable, "0", "0", "600"),
            movement("4000", AccountType::Revenue, "0", "0", "500"),
            movement("5000", AccountType::CostOfSales, "0", "300", "0"),
            movement("6000", AccountType::OperatingExpense, "0", "0", "0"),
        ]
    }

    #[test]
    fn a_trial_balance_agrees_with_itself() {
        let report = TrialBalance::assemble(day(1, 1), day(1, 31), CURRENCY, &ledger()).unwrap();

        // The quiet account is not on it.
        assert_eq!(report.rows.len(), 5);
        assert!(report.is_balanced());
        // Stock 300, receivables 500, cost of sales 300 on one side; payables
        // 600 and sales 500 on the other.
        assert_eq!(report.closing_debits, money("1100"));
        assert_eq!(report.closing_credits, money("1100"));
    }

    #[test]
    fn revenue_reads_as_a_positive_number() {
        // The flip that matters: 4000 holds a credit balance, and nobody reads
        // sales as minus four thousand.
        let report = IncomeStatement::assemble(day(1, 1), day(1, 31), CURRENCY, &ledger()).unwrap();

        assert_eq!(report.revenue.total, money("500"));
        assert_eq!(report.cost_of_sales.total, money("300"));
        assert_eq!(report.gross_profit, money("200"));
        assert_eq!(report.net_profit, money("200"));
        // An account that did nothing in the span is not a line on it.
        assert!(report.operating_expenses.is_empty());
    }

    #[test]
    fn the_balance_sheet_balances_because_the_profit_is_on_it() {
        let report = BalanceSheet::assemble(day(1, 31), day(1, 1), CURRENCY, &ledger()).unwrap();

        assert_eq!(report.total_assets, money("800"));
        assert_eq!(report.liabilities.total, money("600"));
        assert_eq!(report.result_for_year, money("200"));
        assert_eq!(report.brought_forward, Money::zero(CURRENCY));
        assert!(report.is_balanced());
    }

    #[test]
    fn last_years_profit_is_brought_forward_and_this_years_is_not() {
        // Nothing closes into retained earnings, so a sale from last year is
        // still sitting in the revenue account. It belongs above the line that
        // says "this year", not in it.
        let movements = vec![
            movement("1100", AccountType::AccountsReceivable, "900", "500", "0"),
            movement("4000", AccountType::Revenue, "-900", "0", "500"),
        ];

        let report = BalanceSheet::assemble(day(1, 31), day(1, 1), CURRENCY, &movements).unwrap();

        assert_eq!(report.brought_forward, money("900"));
        assert_eq!(report.result_for_year, money("500"));
        assert!(report.is_balanced());
    }

    fn invoice(number: &str, issued: NaiveDate, due: NaiveDate, amount: &str) -> StatementLine {
        StatementLine {
            invoice_id: Uuid::from_u128(u128::from(number.len() as u32)),
            number: number.to_owned(),
            issued_on: issued,
            due_on: Some(due),
            invoiced: money(amount),
            amount: money(amount),
            running: Money::zero(CURRENCY),
        }
    }

    #[test]
    fn what_was_billed_before_the_span_is_the_opening_balance() {
        let statement = CustomerStatement::assemble(
            Uuid::nil(),
            "C-1".to_owned(),
            "A customer".to_owned(),
            day(3, 1),
            day(3, 31),
            CURRENCY,
            vec![
                invoice("INV-1", day(1, 10), day(2, 9), "100"),
                invoice("INV-2", day(3, 5), day(4, 4), "250"),
                invoice("INV-3", day(3, 20), day(4, 19), "50"),
            ],
        )
        .unwrap();

        assert_eq!(statement.opening, money("100"));
        assert_eq!(statement.lines.len(), 2);
        assert_eq!(statement.billed, money("300"));
        assert_eq!(statement.closing, money("400"));

        // The running balance opens where the opening balance left off.
        assert_eq!(statement.lines.first().map(|line| line.running), Some(money("350")));
        assert_eq!(statement.lines.last().map(|line| line.running), Some(money("400")));

        // Neither of the two in the span is due yet on the 31st of March. The
        // one behind the opening balance fell due on the 9th of February, and
        // is in the third bucket rather than left off.
        assert_eq!(statement.ageing.not_yet_due, money("300"));
        assert_eq!(statement.ageing.to_60, money("100"));
    }

    #[test]
    fn ageing_counts_from_the_day_it_fell_due() {
        let statement = CustomerStatement::assemble(
            Uuid::nil(),
            "C-1".to_owned(),
            "A customer".to_owned(),
            day(1, 1),
            day(4, 30),
            CURRENCY,
            vec![
                // Due 20 days ago, and due 110 days ago.
                invoice("INV-1", day(3, 11), day(4, 10), "100"),
                invoice("INV-2", day(1, 1), day(1, 10), "200"),
            ],
        )
        .unwrap();

        assert_eq!(statement.ageing.to_30, money("100"));
        assert_eq!(statement.ageing.over_90, money("200"));
    }
}
