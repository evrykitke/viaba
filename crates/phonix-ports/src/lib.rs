//! How one app reaches another.
//!
//! A port is a trait declared by the app that *needs* a capability, in a crate
//! neither app owns; the app that provides it implements it. Inventory needs to
//! post a journal, so it depends on this crate — not on `app-books`, which is
//! what implements `Ledger`. The two are introduced at the composition root in
//! `phonix-server`, the only place allowed to know both apps exist.
//!
//! A port with no implementation answers rather than failing: stock movements
//! still happen when nobody bought the accounting module, the journal is simply
//! not posted. If "no ledger" were an error, Inventory would have to require
//! Books and the independence would be a dependency graph in disguise.
//!
//! Two ports so far. [`cost_centre::CostCentres`] is implemented over `hr`;
//! [`ledger::Ledger`] is implemented over `books` and declared now because
//! Inventory is a real caller of it. `Stock` is still only named in
//! `docs/adr/0006-apps-ports-and-defaults.md` and waits for the same reason
//! these two did not — a trait extracted for one caller is that caller's
//! service with a `dyn` in front of it.
//!
//! Compiled to wasm, so this crate may not panic.

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

pub mod cost_centre;
pub mod error;
pub mod ledger;

pub use cost_centre::{CostCentre, CostCentres};
pub use error::PortError;
pub use ledger::{
    AccountRole, JournalRequest, Ledger, LedgerAccount, LedgerError, NoLedger, PostedRef, Posting,
    Side,
};
