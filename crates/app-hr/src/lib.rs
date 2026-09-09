//! Human resources: who works here, how the workspace is arranged, and what
//! it charges to.
//!
//! [`department`] came first. It is built before the ledger that consumes it
//! because a cost centre is a dimension on a journal line, and a dimension has
//! to be in the ledger's shape from its first migration. See
//! `docs/adr/0006-apps-ports-and-defaults.md` sections 6.4 and 9.
//!
//! Then the people: [`employee`], with [`job_position`] and [`work_location`]
//! beside it.
//!
//! # An employee is not a user
//!
//! Most people who work somewhere never sign in to its accounting system, and
//! some people who sign in do not work there - an outsourced bookkeeper, an
//! auditor. So the link between an employee and a login is optional in *both*
//! directions and unique in both: one login is one person.
//!
//! Creating an employee therefore creates no account, and nothing here ever
//! creates one implicitly. A login is a deliberate, per-person act - see
//! [`employee::Employee::can_be_invited`] - and it goes through the ordinary
//! invitation flow, so the person sets their own password and no administrator
//! ever knows it.
//!
//! # What changes about somebody is dated, and what does not is not
//!
//! [`employee::Employee`] holds a name, a way to reach them and an identifier.
//! Their department, role, manager and place are an
//! [`employee::Assignment`] with dates on it, and their employment is an
//! [`employee::Engagement`]. That is the whole reason this app is five tables:
//! see the head of `migrations/apps/hr/0002_people.sql`.
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
pub mod employee;
pub mod job_position;
pub mod work_location;

/// The app's id, its schema name, and the key its number series are declared
/// under in `config/numbering/hr.toml`.
pub const APP_ID: &str = "hr";

/// The thing this app numbers. A department code is not a document number, but
/// it is the same problem, so it uses the same allocator.
pub const DEPARTMENT: &str = "department";
pub const EMPLOYEE: &str = "employee";
pub const JOB_POSITION: &str = "job_position";
pub const WORK_LOCATION: &str = "work_location";

/// What this app needs before it is useful, checked on its home page.
///
/// Advisory: departments are worth having whether or not any of them is
/// chargeable, and nothing here refuses to save without one. The gap it names
/// is the one Books and Inventory feel — a requisition with nowhere to charge
/// itself to.
pub const SETUP: &[phonix_core::SetupItem] = &[phonix_core::SetupItem::advisory(
    "cost_centres",
    "hr.setup.cost_centres",
    "/people/departments",
    "hr.setup.cost_centres_missing",
)];

pub use department::{
    DeleteOutcome, Department, DepartmentError, DepartmentInput, DepartmentSummary,
    MAX_DEPARTMENT_CODE_LEN, MAX_DEPARTMENT_DEPTH, MAX_DEPARTMENT_NAME_LEN, in_tree_order,
};
pub use employee::{
    Assignment, AssignmentInput, Employee, EmployeeError, EmployeeInput, EmployeeSummary,
    EmploymentType, EndReason, Engagement, LeavingInput,
};
pub use job_position::{JobPosition, JobPositionError, JobPositionInput, JobPositionSummary};
pub use work_location::{
    LocationKind, WorkLocation, WorkLocationError, WorkLocationInput, WorkLocationSummary,
};

/// Whether a code is shaped the way every `*_code_format` constraint in this
/// schema demands.
///
/// Shared rather than written four times: a code this accepts is one the column
/// will take, and four copies of the rule is four chances for one of them to
/// drift and start refusing what the database allows.
pub(crate) fn is_code_shaped(code: &str) -> bool {
    let mut chars = code.chars();

    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A trimmed string, or nothing where it was only spaces.
///
/// The column is nullable and an empty string is not the same as absent: a
/// blank work email means "we do not have one", and storing `''` makes every
/// `IS NULL` in the schema quietly wrong.
pub(crate) fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// The snapshot type the `CostCentres` port passes. Owned by neither app.
pub use phonix_ports::CostCentre;
