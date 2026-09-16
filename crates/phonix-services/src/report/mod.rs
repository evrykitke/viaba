//! Reporting: the exports a workspace asks for, and what writes them.
//!
//! A module of its own rather than a corner of `workspace`, because ADR 0008
//! puts the format writers here: a writer is a plain function from a
//! definition, its rows and its settings to bytes, called by the request path
//! and by the exporter alike. [`exports`] is the request half of that; the
//! writers arrive beside it.
//!
//! **A PDF is not written here.** It is the report's own page printed by a
//! browser - ADR 0008 §9.1 - which is `phonix_web::server::printing`. What
//! belongs here is a format that is not a page.

pub mod exports;
pub mod writers;
