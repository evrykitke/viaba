//! Books' screens: the ledger, and the two sell-side documents it owns.
//!
//! The module keeps the crate's name; the addresses no longer do.
//!
//! ```text
//! /sales                      the app's home         counts and the ways in
//! /accounting/accounts        the chart              a grid and a tree
//! /accounting/accounts/new    add one                a form
//! /accounting/accounts/:id    one account            Details | History
//! /accounting/accounts/roles  account determination  a role per row
//! /accounting/journals        the ledger             a grid
//! /accounting/journals/new    post one               two money columns
//! /accounting/journals/:id    one journal            the document, read-only
//! /accounting/periods         the calendar           open a year, close a month
//! /accounting/reports/...     the four statements
//! /selling/invoices           the list               a grid
//! /selling/invoices/new       raise one              the editor
//! /selling/invoices/:id       one invoice            the editor, or the document
//! /selling/payments           money in               a grid
//! /selling/payments/new       record one             the editor
//! /selling/payments/:id       one payment            the editor, or the document
//! ```
//!
//! # One route, two screens
//!
//! `/selling/invoices/:id` is the editor while the invoice is a draft and the
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
