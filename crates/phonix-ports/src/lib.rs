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
//! [`cost_centre::CostCentres`] is the only port so far. `Ledger` and `Stock`
//! are named in `docs/adr/0006-apps-ports-and-defaults.md` and wait until they
//! have a real implementation — a trait extracted for one caller is that
//! caller's service with a `dyn` in front of it.
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

pub use cost_centre::{CostCentre, CostCentres};
pub use error::PortError;
