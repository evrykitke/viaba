//! The `hr` app: departments, and the ones that are cost centres.
//!
//! [`department`] is what the HR screens call, and everything in it is gated.
//! [`cost_centre`] is what *another app* calls while posting, and takes no
//! caller at all — see the note there.

pub mod cost_centre;
pub mod department;

pub use cost_centre::HrCostCentres;
pub use department::DeleteOutcome;
