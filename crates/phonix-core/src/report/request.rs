//! An export somebody asked for, and how far it has got.
//!
//! # Why an export is a row and not an answer
//!
//! A statement over a year of a busy ledger is minutes of rendering. A server
//! function returning those bytes holds a connection open for all of it and
//! loses the work the moment somebody closes the tab. A row survives both, and
//! the file it ends at is an ordinary stored file afterwards.
//!
//! Only an unbounded export becomes one. A receipt is one payment and its
//! allocations; it renders in the request and never reaches this table, so
//! nothing here is on the path of a document somebody is waiting for. See
//! `docs/adr/0008-reporting.md` §9.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

use super::ExportFormat;
use crate::files::FileId;
use crate::identity::UserId;

/// How far an export has got.
///
/// Four states because four can be told apart from outside: it is waiting, a
/// worker has it, there is a file, or there is a reason there is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    #[default]
    Requested,
    Running,
    Ready,
    Failed,
}

impl ExportState {
    pub const ALL: &'static [Self] = &[Self::Requested, Self::Running, Self::Ready, Self::Failed];

    /// The stored value, matching the column's CHECK constraint.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Running => "running",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    /// The state a stored value names, or nothing.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    /// Whether anything more will happen to this request.
    ///
    /// What a screen asking after one stops asking on.
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Ready | Self::Failed)
    }
}

/// What somebody asked for, before there is a row for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewExport {
    /// The definition's own id - `customer-statement`, `product-list`.
    pub report_id: String,
    /// What the report was run with: the customer, the span. Opaque here and
    /// handed back to the report when the worker renders it.
    pub parameters: Json,
    pub format: ExportFormat,
}

/// One asked-for export, as it stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportRequest {
    pub id: Uuid,
    pub report_id: String,
    pub parameters: Json,
    pub format: ExportFormat,
    pub state: ExportState,
    /// Who asked. `None` once that account is gone, which is not the same as
    /// nobody having asked - see [`may_render`](Self::may_render).
    pub requested_by: Option<UserId>,
    pub requested_at: DateTime<Utc>,
    /// The bytes, once there are any.
    pub file_id: Option<FileId>,
    /// Why there are not, in the worker's words, so a screen can say more
    /// than "it did not work".
    pub failure: Option<String>,
}

impl ExportRequest {
    /// Whether a worker still has somebody to render this as.
    ///
    /// A worker has no caller of its own, so it renders as whoever asked and
    /// re-checks their permission first. An account deleted between the
    /// request and the run leaves nobody to check, and rendering anyway would
    /// be a way to read a report as nobody at all.
    pub const fn may_render(&self) -> bool {
        self.requested_by.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_round_trips_through_its_stored_form() {
        for state in ExportState::ALL {
            assert_eq!(ExportState::parse(state.as_str()), Some(*state));
        }
    }

    #[test]
    fn only_a_finished_request_is_finished() {
        assert!(!ExportState::Requested.is_finished());
        assert!(!ExportState::Running.is_finished());
        assert!(ExportState::Ready.is_finished());
        assert!(ExportState::Failed.is_finished());
    }
}
