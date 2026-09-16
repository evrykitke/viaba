//! What each report is, one file per report.
//!
//! The same arrangement as [`table::config`](crate::ui::table::config): a
//! module contributes a function returning a definition, and the kit draws it.
//! A definition names no query - its data is handed to the renderer by the
//! screen, from a server function that already exists.

pub mod balance_sheet;
pub mod customer_statement;
pub mod product_list;
pub mod profit_and_loss;
pub mod receipt;
pub mod sample;
pub mod trial_balance;
