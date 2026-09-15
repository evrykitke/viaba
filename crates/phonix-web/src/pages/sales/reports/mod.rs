//! The four statements.
//!
//! ```text
//! /accounting/reports/trial-balance       every account, both columns
//! /accounting/reports/balance-sheet       what is owned and owed, at a date
//! /accounting/reports/profit-and-loss     what was earned and spent, between two
//! /accounting/reports/statement           one customer's account
//! ```
//!
//! # Nothing here reads the clock
//!
//! A screen that worked out "this year" would work it out twice - once while
//! the server renders and once while the browser hydrates - and either side of
//! midnight those are different answers. A hydration mismatch takes the whole
//! application down with it, so the span a report opens on comes from the
//! server, which also happens to be the only place that knows when the
//! financial year began. See [`shared::opening_span`].
//!
//! # Every figure is in the workspace's own currency
//!
//! Reports read the `base_amount` recorded on each journal line at post. A
//! statement that added a euro invoice to a sterling one would be adding two
//! different things, and nothing here re-converts at today's rate: a filed
//! period stays as it was filed.

pub mod balance_sheet;
pub mod customer_statement;
pub mod profit_and_loss;
pub mod shared;
pub mod trial_balance;
