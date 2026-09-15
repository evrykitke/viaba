//! Sales: what this workspace invoices.
//!
//! ```text
//! /sales                   the app's home    counts and the ways in
//! /sales/accounts          the chart          a grid and a tree
//! /sales/accounts/new      add one            a form
//! /sales/accounts/:id      one account        Details | History
//! /sales/accounts/roles    account determination  a role per row
//! /sales/journals          the ledger         a grid
//! /sales/journals/new      post one           two money columns
//! /sales/journals/:id      one journal        the document, read-only
//! /sales/periods           the calendar       open a year, close a month
//! /sales/reports/...       the four statements
//! /sales/invoices          the list          a grid
//! /sales/invoices/new      raise one         the editor
//! /sales/invoices/:id      one invoice       the editor, or the document
//! /sales/payments          money in          a grid
//! /sales/payments/new      record one        the editor
//! /sales/payments/:id      one payment       the editor, or the document
//! ```
//!
//! # One route, two screens
//!
//! `/sales/invoices/:id` is the editor while the invoice is a draft and the
//! *document* once it has been posted. One address rather than two, because
//! posting does not move an invoice - it changes what may be done to it, and a
//! link somebody sent last week should still open the thing they meant.

pub mod account;
pub mod account_roles;
pub mod accounts;
pub mod chart_tree;
pub mod home;
pub mod invoice;
pub mod invoices;
pub mod journal;
pub mod journal_new;
pub mod journals;
pub mod payment;
pub mod periods;
pub mod reports;
