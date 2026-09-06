//! Books: what a workspace sells, and what it is owed for it.
//!
//! The first real app, and the one that proves the foundation was worth
//! building. An invoice needs every piece of it at once: a party from `master`,
//! a tax group from `master`, a number series from `core`, a currency from
//! `core`, and `Money` to add it up. If any of those had been shaped wrongly,
//! this is where it would show.
//!
//! | Module | Question it answers |
//! | ------ | ------------------- |
//! | [`account`] | What may be posted to, and which way its balance runs? |
//! | [`quantity`] | How many, at four decimal places? |
//! | [`invoice`] | What is on the document, and what state is it in? |
//! | [`pricing`] | What does it come to? |
//!
//! # An invoice is a draft until it is posted
//!
//! A draft can be edited and deleted and carries no number. Posting is the act
//! that makes it a document: it takes a number from `core.number_sequences` in
//! the same transaction as the write, resolves the tax group against the
//! document's own date, and freezes the party's name and address onto the
//! record. After that nothing about it changes except its status - a mistake is
//! corrected by voiding it and raising another, because an invoice that can be
//! edited after it has been sent is not evidence of anything.
//!
//! # Everything a reprint needs is on the document
//!
//! The party's name and address, the tax code, name and rate of every line, the
//! exchange rate and the date it was published. None of it is re-resolved at
//! print time. A customer who moves, a tax that changes and a currency that
//! drifts must not rewrite an invoice that was already sent - the same rule
//! `entity_events` follows, and the reason [`invoice::PartySnapshot`] exists.
//!
//! # This crate may not panic
//!
//! Same rule as `phonix-core`, and for the same reason: it is compiled into the
//! WebAssembly bundle, where one panic freezes every handler on the page.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

pub mod account;
pub mod invoice;
pub mod journal;
pub mod period;
pub mod pricing;
pub mod quantity;

/// The app's id, which is also the name of the schema it owns and the key its
/// number series are declared under in `config/numbering/books.toml`.
pub const APP_ID: &str = "books";

/// The document type this app numbers. One, so far.
pub const SALES_INVOICE: &str = "sales_invoice";

/// The other. A journal takes a number at the moment it is posted, from the
/// same allocator and in the same transaction as the write.
pub const JOURNAL: &str = "journal";

/// What Books claims about a party.
///
/// It marks one `customer` and never looks at any other claim: a party that
/// Procurement also calls a supplier is the same row, and that is the point.
pub const CUSTOMER_ROLE: &str = phonix_master::party::roles::CUSTOMER;

/// What this app needs before it is useful, checked on its home page.
///
/// The chart blocks rather than advises: an account is the other side of every
/// posting, and a workspace whose chart somebody emptied should be told that in
/// a sentence rather than by a foreign-key violation from four layers down. It
/// is seeded from `config/defaults/books.toml`, so it is green on the first
/// morning without anybody typing anything.
pub const SETUP: &[phonix_core::SetupItem] = &[
    phonix_core::SetupItem::blocking(
        "chart_of_accounts",
        "books.setup.chart",
        "/sales/accounts",
        "books.setup.chart_missing",
    ),
    // Master's screen, reached by a link. Books holds no code of master's, and
    // an invoice with no tax code to name is Books' problem to report.
    phonix_core::SetupItem::advisory(
        "tax_codes",
        "books.setup.taxes",
        "/master/taxes",
        "books.setup.taxes_missing",
    ),
];

pub use account::{
    Account, AccountClass, AccountError, AccountInput, AccountSummary, AccountType, DefaultAccount,
    DefaultChart, DefaultChartError, MAX_ACCOUNT_DESCRIPTION_LEN, MAX_ACCOUNT_NAME_LEN,
    MAX_ACCOUNT_NUMBER_LEN, Side,
};
pub use period::{NewPeriod, Period, PeriodError};
pub use journal::{
    Dimension, DimensionValue, JournalEntry, JournalError, JournalLineInput, JournalSummary,
    MAX_LINES, MAX_MEMO_LEN, MAX_NARRATION_LEN, Posted, PostedLine, Source,
};
pub use invoice::{
    Invoice, InvoiceError, InvoiceInput, InvoiceLine, InvoiceLineInput, InvoiceStatus,
    InvoiceSummary, InvoiceTotals, PartySnapshot, PostOutcome,
};
pub use pricing::{PricedInvoice, PricedLine, PricingError};
pub use quantity::{MAX_QUANTITY_SCALED, QUANTITY_SCALE, Quantity, QuantityError};
