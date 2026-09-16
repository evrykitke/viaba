//! Reports: one definition per report, drawn by one component.
//!
//! A peer of [`table`](super::table), arranged the same way - a module
//! contributes a *value* and the kit renders it. What a band is, what page it
//! is printed on and what the look measures out to all come from
//! [`phonix_core::report`], so the PDF writer measures the same report the
//! screen does.
//!
//! # A definition holds no query
//!
//! Rows are handed to the renderer by the screen, from a typed server function
//! that already exists. A definition that composed a query of its own would
//! escape the permission, the tenancy and the paging of the read it should
//! have used. See `docs/adr/0008-reporting.md` §3.
//!
//! # One file per report
//!
//! Definitions live under `config/`, named for the report, the way
//! [`table::config`](super::table::config) holds one file per entity:
//!
//! ```text
//! ui/report/config/customer_statement.rs
//!     -> pub fn customer_statement() -> ReportDefinition<CustomerStatement>
//! ```

mod definition;
mod render;

pub use definition::{Band, Extent, Field, ReportDefinition};
pub use render::Report;
