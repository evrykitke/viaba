//! The `inventory` app's tables: what the workspace stocks, and where.
//!
//! # Every statement here is qualified
//!
//! `inventory.items`, never `items`. A request runs on `core,public` - the app
//! schemas are deliberately absent from the search path - so an unqualified
//! reference is a loud error rather than a quiet wrong answer.
//!
//! # This schema points at `core`, and at nothing else
//!
//! `core.users` and `core.file_uploads` are proper foreign keys. A supplier is a
//! `master.parties` id **without one**, and an account is a `books.accounts` id
//! without one either - which is what makes an app uninstallable, and why the
//! columns beside those ids are a snapshot rather than a join.
//!
//! # Stock hangs off a variant, not an item
//!
//! Every table that records a quantity references `item_variants (id)`. An item
//! that varies by nothing has exactly one variant, so there is no second code
//! path for "an item without variants" - see
//! `migrations/apps/inventory/0002_variants_and_images.sql`.

pub mod account_mapping;
pub mod category;
pub mod defaults;
pub mod image;
pub mod bill;
pub mod item;
pub mod location;
pub mod lot;
pub mod movement;
pub mod purchase;
pub mod quant;
pub mod receipt;
pub mod unit;
pub mod valuation;
pub mod variant;
pub mod warehouse;
