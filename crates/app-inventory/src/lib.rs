//! Inventory: what the workspace stocks, where it is, and how it got there.
//!
//! # The one idea everything else follows from
//!
//! **Stock is never created or destroyed; it only moves.** Every change is a
//! move from one location to another, and the locations include the ones that
//! are not places - the supplier, the customer, inventory loss, production.
//! A receipt is a move from a vendor location. A count difference is a move to
//! inventory loss. See [`location`] for why, and for the seven kinds.
//!
//! That makes inventory double entry in the same sense the ledger is: the sum
//! of every quantity ever moved is zero, and a stock report reconciles by
//! arithmetic rather than by a nightly job. It is the model Odoo uses, and it
//! is the reason its stock figures tie out.
//!
//! # This crate depends on no other app
//!
//! Not on `app-books`, which it posts to. Not on `app-hr`, whose departments it
//! charges to. Both are reached through `phonix-ports`, so a build without
//! either still receives goods - the movement happens and records that no
//! journal was posted. ADR 0006 sections 2 and 7.
//!
//! # Where the pieces live
//!
//! ```text
//!   quantity     an exact decimal count. Six places, never a float.
//!   unit         units of measure, and what may convert to what.
//!   location     the seven kinds of place, and what a move between two means.
//!   warehouse    a building, and the tree of locations it is made of.
//!   category     costing method, valuation and removal strategy.
//!   item         the thing itself: code generated, barcode typed.
//!   variant      the red medium shirt. What stock is actually held against.
//!   image        pictures, which a point-of-sale screen is made of.
//!   accounts     which account a posting lands on: item, then category, then
//!                the role Books resolves.
//!   lot          which particular units these are, and when they expire.
//!   movement     the stock ledger. One row per move, and nothing is edited.
//!   quant        how much is in one place. A cache the moves can prove.
//!   valuation    what it cost, and what leaves the stock account.
//!   purchase     the commitment: what was ordered, and what is still owed.
//!   receipt      goods arriving, which is where value enters the business.
//! ```
//!
//! Compiled to wasm, so this crate may not panic.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

pub mod accounts;
pub mod category;
pub mod defaults;
pub mod image;
pub mod item;
pub mod location;
pub mod lot;
pub mod movement;
pub mod purchase;
pub mod quant;
pub mod quantity;
pub mod receipt;
pub mod unit;
pub mod valuation;
pub mod variant;
pub mod warehouse;

/// The app's id, its schema name, and the key its number series are declared
/// under in `config/numbering/inventory.toml`.
pub const APP_ID: &str = "inventory";

/// The things this app numbers.
///
/// An item code is not a document number and uses the same allocator, for the
/// reason a department code does: it is the same problem, and a second
/// allocator would solve it slightly differently. ADR 0006 section 3.
pub const ITEM: &str = "item";
pub const PURCHASE_ORDER: &str = "purchase_order";
pub const RECEIPT: &str = "receipt";
pub const DELIVERY: &str = "delivery";
pub const INTERNAL_TRANSFER: &str = "internal_transfer";
pub const ADJUSTMENT: &str = "adjustment";

/// What this app needs before it is useful, checked on its home page.
///
/// All three are seeded by `config/defaults/inventory.toml`, so a workspace
/// that never opens a setup screen has them already. They are listed anyway,
/// because a workspace that deleted the default warehouse should be told what
/// it is missing rather than shown an empty receipt form.
///
/// The ledger mapping is **advisory**: stock movements are a warehouse fact and
/// happen whether or not anybody bought the accounting module. What it warns
/// about is the case where somebody did buy it and no account is mapped, which
/// is a silently unposted journal rather than a refusal.
pub const SETUP: &[phonix_core::SetupItem] = &[
    phonix_core::SetupItem::blocking(
        "warehouse",
        "inventory.setup.warehouse",
        "/inventory/warehouses",
        "inventory.setup.warehouse_missing",
    ),
    phonix_core::SetupItem::blocking(
        "units",
        "inventory.setup.units",
        "/inventory/units",
        "inventory.setup.units_missing",
    ),
    phonix_core::SetupItem::advisory(
        "valuation_accounts",
        "inventory.setup.valuation_accounts",
        "/inventory/settings",
        "inventory.setup.valuation_accounts_missing",
    ),
];

pub use accounts::{AccountOverrides, AccountRef};
pub use category::{Category, CategoryInput, CategorySummary, CostingMethod, RemovalStrategy, Valuation};
pub use image::{Gallery, Image};
pub use item::{Item, ItemInput, ItemKind, ItemSummary, Tracking};
pub use location::{Location, LocationInput, LocationKind, LocationSummary, MoveKind};
pub use lot::{Lot, LotInput, LotSummary};
pub use movement::{JournalOutcome, MoveRequest, MoveState, MoveSummary, StockMove};
pub use purchase::{OrderInput, OrderLine, OrderState, OrderSummary, PurchaseOrder};
pub use quant::{OnHandRow, Quant};
pub use quantity::Quantity;
pub use receipt::{Backorder, Receipt, ReceiptInput, ReceiptLine, ReceiptSummary};
pub use unit::{Conversion, Unit, UnitClass, UnitInput};
pub use valuation::{Consumed, Issue, Layer};
pub use variant::{Attribute, AttributeValue, Selection, Variant, VariantChoice, VariantSummary};
pub use warehouse::{DeliverySteps, ReceiptSteps, Warehouse, WarehouseInput, WarehouseSummary};
