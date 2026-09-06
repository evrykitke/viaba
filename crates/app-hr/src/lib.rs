//! Human resources: how the workspace is arranged, and what it charges to.
//!
//! One table so far — [`department`]. It is built before the ledger that will
//! consume it because a cost centre is a dimension on a journal line, and a
//! dimension has to be in the ledger's shape from its first migration. See
//! `docs/adr/0006-apps-ports-and-defaults.md` sections 6.4 and 9.
//!
//! Named `hr` rather than `departments` because an `app_id` is a schema name in
//! every tenant database and can never be renamed.
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

pub mod department;

/// The app's id, its schema name, and the key its number series are declared
/// under in `config/numbering/hr.toml`.
pub const APP_ID: &str = "hr";

/// The thing this app numbers. A department code is not a document number, but
/// it is the same problem, so it uses the same allocator.
pub const DEPARTMENT: &str = "department";

pub use department::{
    DeleteOutcome, Department, DepartmentError, DepartmentInput, DepartmentSummary,
    MAX_DEPARTMENT_CODE_LEN, MAX_DEPARTMENT_DEPTH, MAX_DEPARTMENT_NAME_LEN, in_tree_order,
};

/// The snapshot type the `CostCentres` port passes. Owned by neither app.
pub use phonix_ports::CostCentre;
