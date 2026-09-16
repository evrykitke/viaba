//! Reporting: the exports a workspace asks for, and what writes them.
//!
//! A module of its own rather than a corner of `workspace`, because ADR 0008
//! puts the format writers here: a writer is a plain function from a
//! definition, its rows and its settings to bytes, called by the request path
//! and by the exporter alike. [`exports`] is the request half of that; the
//! writers arrive beside it.

pub mod exports;
pub mod pdf;
pub mod writers;
