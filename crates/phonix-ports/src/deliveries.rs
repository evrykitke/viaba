//! Telling Inventory that a delivery has been invoiced.
//!
//! The one port that runs the other way. `Ledger` and `CostCentres` are things
//! Inventory needs from elsewhere; this is what Books needs from Inventory, and
//! it exists for the reason those do: `books.invoice_lines` carries a delivery
//! line's id as a bare `UUID` with no foreign key behind it, because ADR 0006
//! section 8 forbids one between two apps. Something has to resolve that id,
//! and it may not be a join.
//!
//! # The check and the write are one call
//!
//! A caller cannot ask "how much of this line is left to invoice" and then
//! invoice it: between the two answers somebody else's invoice may have taken
//! it. So [`Deliveries::invoice`] is given every line at once, refuses the whole
//! set if any line would be over-invoiced, and applies the rest in one
//! transaction. Books never sees a partial answer, which is what makes
//! "invoiced twice" a refusal rather than a reconciliation.
//!
//! # A missing Inventory is an answer
//!
//! A workspace may keep books and hold no stock. [`NoDeliveries`] answers
//! [`DeliveriesError::NoDeliveries`], and an invoice line naming a delivery in
//! that workspace is a line naming something that cannot exist - which is a
//! refusal rather than a silent success.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::PortError;

/// The name this port is known by in a log line or an error.
pub const PORT: &str = "deliveries";

/// One delivery line, and how much of it an invoice is charging for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoicedLine {
    /// The row in Inventory this invoice line bills. A bare id: see the module
    /// docs for why it is not a foreign key.
    pub delivery_line_id: Uuid,
    /// Decimal digits, in the item's **stock** unit - the unit the delivery
    /// line is already in, so nothing here has to convert. Text for the reason
    /// every amount crossing a boundary in this codebase is text.
    pub quantity: String,
}

/// Why nothing was recorded.
///
/// Each variant is a different thing for Books to say to somebody, which is why
/// this is not one opaque error. All of them refuse the whole call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeliveriesError {
    /// Nobody implements this port here: the workspace keeps books and holds no
    /// stock. An invoice line naming a delivery cannot be right.
    #[error("no deliveries are available in this workspace")]
    NoDeliveries,

    /// The id names no delivery line.
    #[error("that delivery line does not exist")]
    UnknownLine(Uuid),

    /// The delivery has not been despatched, so there is nothing to charge for
    /// yet. A draft is a plan, not goods gone.
    #[error("that delivery has not been despatched")]
    NotDespatched(Uuid),

    /// More was asked for than was delivered and not already invoiced. The
    /// refusal that makes billing twice impossible rather than detectable.
    #[error("only {left} of that delivery line is left to invoice, not {asked}")]
    MoreThanDelivered {
        delivery_line_id: Uuid,
        /// What remains, as decimal digits in the stock unit.
        left: String,
        asked: String,
    },

    /// A quantity that is not a decimal, or is out of range. A bug in the
    /// caller.
    #[error("'{0}' is not a quantity")]
    NotAQuantity(String),

    /// Inventory is there and could not answer.
    #[error("inventory failed: {0}")]
    Unavailable(String),
}

impl From<PortError> for DeliveriesError {
    fn from(err: PortError) -> Self {
        match err {
            PortError::Refused(message) => Self::Unavailable(message.to_string()),
            PortError::Unavailable { detail, .. } => Self::Unavailable(detail),
        }
    }
}

/// Somewhere that knows what has been delivered.
#[async_trait::async_trait]
pub trait Deliveries: Send + Sync {
    /// Record that these quantities have been invoiced, or refuse all of them.
    ///
    /// Idempotent only in the sense that it is exact: calling it twice with the
    /// same lines invoices them twice, and the second call is refused if that
    /// would exceed what was delivered. The caller posts an invoice once.
    async fn invoice(&self, lines: &[InvoicedLine]) -> Result<(), DeliveriesError>;
}

/// A port with nobody behind it: Inventory is not compiled in, or the workspace
/// holds no stock.
pub struct NoDeliveries;

#[async_trait::async_trait]
impl Deliveries for NoDeliveries {
    async fn invoice(&self, lines: &[InvoicedLine]) -> Result<(), DeliveriesError> {
        // An invoice with no delivery lines has nothing to tell anybody, and a
        // workspace with no Inventory may still raise one.
        if lines.is_empty() {
            return Ok(());
        }

        Err(DeliveriesError::NoDeliveries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_absent_inventory_still_lets_an_ordinary_invoice_through() {
        // The distinction this rests on: a workspace that holds no stock raises
        // invoices for services all day, and none of those lines name a
        // delivery. Only a line that does is refused.
        let port = NoDeliveries;

        assert_eq!(port.invoice(&[]).await, Ok(()));

        let line = InvoicedLine {
            delivery_line_id: Uuid::nil(),
            quantity: "1".to_owned(),
        };

        assert_eq!(
            port.invoice(&[line]).await,
            Err(DeliveriesError::NoDeliveries)
        );
    }
}
