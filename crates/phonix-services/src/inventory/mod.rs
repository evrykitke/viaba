//! Inventory: what the workspace stocks, where it is, and how it got there.
//!
//! # Where the rules live
//!
//! In `app-inventory`, which compiles to wasm so the browser checks the same
//! things this does. What is here is everything a rule cannot know on its own:
//! whether a code is taken, whether a parent is really above a child, whether
//! stock exists, and who is allowed to ask.
//!
//! # The two refusals that matter
//!
//! An item's **stock unit** and its **tracking mode** are frozen the moment any
//! stock exists. Both are rules no type can enforce, because both need to know
//! whether anything has been counted:
//!
//! * Changing the stock unit restates every quantity ever recorded - two
//!   hundred kilograms silently becomes two hundred grams.
//! * Turning tracking on leaves the existing two hundred belonging to no lot,
//!   and every FEFO pick and every recall afterwards skips them.
//!
//! Neither is discovered during a recall. They are refused here.

pub mod bill;
pub mod category;
pub mod item;
pub mod location;
pub mod purchase;
pub mod receipt;
pub mod requisition;
pub mod stock;
pub mod unit;
pub mod warehouse;
