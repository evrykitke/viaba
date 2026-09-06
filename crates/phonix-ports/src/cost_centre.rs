//! What a posting may be charged to.
//!
//! `app-hr` implements this over `hr.departments`; Books and Inventory call it
//! and depend on `app-hr` nowhere.
//!
//! A ledger does not keep its own cost-centre list because it would then keep
//! half of one — a cost centre *is* a department, and two lists that have to
//! agree eventually will not.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::PortError;

/// The name this port is known by in a log line or an error.
pub const PORT: &str = "cost_centre";

/// What a document keeps when it charges something to a cost centre.
///
/// A snapshot rather than a reference: a journal line stores all three fields,
/// so a department renamed next year does not rewrite last year's report. Three
/// fields and no more — a manager or an active flag would be state Books ends
/// up disagreeing with HR about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostCentre {
    pub id: Uuid,
    pub code: String,
    pub name: String,
}

impl CostCentre {
    /// `DEPT-004 · Finance`. Here so two apps do not spell one row two ways.
    pub fn label(&self) -> String {
        format!("{} · {}", self.code, self.name)
    }
}

/// The cost centres this workspace has, for whoever is posting.
#[async_trait::async_trait]
pub trait CostCentres: Send + Sync {
    /// Everything that may be charged to *now*, for a picker. Retired rows and
    /// groupings are left out — offering a parent beside its children is how a
    /// report double-counts.
    async fn list(&self) -> Result<Vec<CostCentre>, PortError>;

    /// One by id, for the moment a posting names it.
    ///
    /// Separate from [`Self::list`] because it must answer for retired rows: a
    /// document raised two years ago names one. `Ok(None)` is a real answer —
    /// unknown, or a department that is not chargeable.
    async fn resolve(&self, id: Uuid) -> Result<Option<CostCentre>, PortError>;
}

/// A port with nobody behind it: the provider is not compiled in, or the
/// workspace has not switched it on.
///
/// Answers rather than failing, so a workspace without HR can still post
/// journals — they just cannot be charged to anything.
pub struct NoCostCentres;

#[async_trait::async_trait]
impl CostCentres for NoCostCentres {
    async fn list(&self) -> Result<Vec<CostCentre>, PortError> {
        Ok(Vec::new())
    }

    async fn resolve(&self, _id: Uuid) -> Result<Option<CostCentre>, PortError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_names_the_code_first() {
        let centre = CostCentre {
            id: Uuid::nil(),
            code: "DEPT-004".to_owned(),
            name: "Finance".to_owned(),
        };

        assert_eq!(centre.label(), "DEPT-004 · Finance");
    }

    #[tokio::test]
    async fn an_absent_provider_answers_empty_rather_than_failing() {
        let port = NoCostCentres;

        assert_eq!(port.list().await, Ok(Vec::new()));
        assert_eq!(port.resolve(Uuid::nil()).await, Ok(None));
    }
}
