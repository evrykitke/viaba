//! The `hr` app: who works here, how the workspace is arranged, and what it
//! charges to.
//!
//! [`department`], [`employee`], [`job_position`] and [`work_location`] are
//! what the HR screens call, and everything in them is gated. [`cost_centre`]
//! is what *another app* calls while posting, and takes no caller at all — see
//! the note there.
//!
//! # Creating an employee creates no login
//!
//! [`employee::create_login`] is the only path from a person to an account, it
//! has its own permission, and it needs `Users.Create` on top — so an HR grant
//! by itself can never let anybody into the system. Most people who work
//! somewhere never sign in, and the schema is built for that rather than around
//! it.

pub mod cost_centre;
pub mod department;
pub mod employee;
pub mod job_position;
pub mod work_location;

pub use cost_centre::HrCostCentres;
pub use department::DeleteOutcome;
