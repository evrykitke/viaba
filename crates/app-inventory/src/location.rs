//! Where stock is - including the places that are not places.
//!
//! # Inventory is double entry, and this is what makes it so
//!
//! Every change to stock is a **move from one location to another**. Nothing
//! is ever created or destroyed; it only changes hands. A receipt is a move
//! from the supplier's location into ours. A delivery is a move from ours into
//! the customer's. A stock count that finds three fewer than the system thought
//! is a move of three into inventory loss.
//!
//! That is why [`LocationKind`] has seven variants and not two. `Vendor`,
//! `Customer`, `InventoryLoss` and `Production` are not warehouses - they are
//! the other side of an entry, exactly as a revenue account is the other side
//! of a receivable. Give them up and every movement needs a sign, a reason code
//! and a rule about which combinations are legal; keep them and the sum of
//! every quantity ever moved is zero, forever, and a discrepancy is arithmetic
//! rather than an opinion.
//!
//! This is the model Odoo uses, and it is the reason its stock reporting
//! reconciles without a nightly job.
//!
//! # The seven kinds
//!
//! ```text
//!   Internal        real shelves. The only kind that is "on hand".
//!   View            a grouping. Holds nothing; its children hold everything.
//!   Vendor          the supplier's side of a receipt.
//!   Customer        the customer's side of a delivery.
//!   InventoryLoss   the other side of a count difference, a scrap, a write-off.
//!   Production      the other side of what a works order consumes and makes.
//!   Transit         gone from one warehouse, not yet arrived at the next.
//! ```
//!
//! [`LocationKind::Transit`] is the one that systems most often leave out, and
//! leaving it out is why they cannot say where a lorry's worth of stock is on a
//! Tuesday afternoon. It is ours, it is on the balance sheet, and it is in
//! neither building.
//!
//! # A small workspace has one warehouse and never thinks about this
//!
//! `config/defaults/inventory.toml` seeds the whole arrangement: one warehouse,
//! its stock location, and the virtual locations every workspace needs whether
//! or not it knows it. Nobody has to design a location tree to receive a box.

use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_LOCATION_CODE_LEN: usize = 40;
pub const MAX_LOCATION_NAME_LEN: usize = 120;

/// How deep the tree may go. Warehouse, stock, zone, aisle, bay, shelf, bin is
/// seven.
pub const MAX_LOCATION_DEPTH: usize = 8;

/// What a location *is*, which decides what a move across it means.
///
/// A closed set, and it has to be: this is the type the valuation rules and the
/// stock report both dispatch on, and a free-text category would let a
/// workspace invent a kind that neither of them knows how to treat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// A shelf, a bin, a room. Real stock, ours, on hand and on the balance
    /// sheet. The only kind a stock report counts.
    Internal,

    /// A grouping - a whole warehouse, a floor. Holds nothing itself: its total
    /// is the sum of what is beneath it, never a number of its own. Posting to
    /// a grouping *and* to its children is how a report counts the same pallet
    /// twice.
    View,

    /// The supplier's side. Stock arrives *from* here, which is what makes a
    /// receipt a move rather than an appearance.
    Vendor,

    /// The customer's side. Stock leaves *to* here.
    Customer,

    /// Where a count difference, a scrap or a write-off goes. Not a loss of
    /// stock from the system - a move of it somewhere that is not the
    /// warehouse, which is how the discrepancy stays visible and countable
    /// instead of being subtracted into nothing.
    InventoryLoss,

    /// What a works order consumes on one side and produces on the other.
    /// Present from the first migration although manufacturing is not built:
    /// adding a location kind later means restating every historic move.
    Production,

    /// Gone from one warehouse, not yet arrived at the next. Ours, on the
    /// balance sheet, and in neither building. The third state a transfer
    /// needs and the one "subtract here, add there" cannot express.
    Transit,
}

impl LocationKind {
    pub const ALL: &'static [Self] = &[
        Self::Internal,
        Self::View,
        Self::Vendor,
        Self::Customer,
        Self::InventoryLoss,
        Self::Production,
        Self::Transit,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::View => "view",
            Self::Vendor => "vendor",
            Self::Customer => "customer",
            Self::InventoryLoss => "inventory_loss",
            Self::Production => "production",
            Self::Transit => "transit",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == raw)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Internal => msg!("locations.kind.internal"),
            Self::View => msg!("locations.kind.view"),
            Self::Vendor => msg!("locations.kind.vendor"),
            Self::Customer => msg!("locations.kind.customer"),
            Self::InventoryLoss => msg!("locations.kind.inventory_loss"),
            Self::Production => msg!("locations.kind.production"),
            Self::Transit => msg!("locations.kind.transit"),
        }
    }

    /// Whether a move may name this kind at all. Everything but a grouping.
    pub const fn can_hold_stock(self) -> bool {
        !matches!(self, Self::View)
    }

    /// Whether stock here is **on hand**: ours, in a building, countable and
    /// pickable. The one predicate the stock report is built on.
    pub const fn is_on_hand(self) -> bool {
        matches!(self, Self::Internal)
    }

    /// Whether stock here is on the workspace's balance sheet.
    ///
    /// Internal and transit. Not a vendor's shelf and not a customer's, and not
    /// inventory loss or production - those two are where value leaves the
    /// stock account for an expense or a works order.
    pub const fn is_owned(self) -> bool {
        matches!(self, Self::Internal | Self::Transit)
    }

    /// Whether this is somebody else's, which is what makes a move across it a
    /// purchase or a sale rather than a rearrangement.
    pub const fn is_external(self) -> bool {
        matches!(self, Self::Vendor | Self::Customer)
    }

    /// Whether a picking may take stock from here. Transit is owned and not
    /// pickable: it is on a lorry.
    pub const fn is_pickable(self) -> bool {
        matches!(self, Self::Internal)
    }

    /// Whether a workspace may create these by hand.
    ///
    /// The counterpart kinds are seeded once and then left alone: a second
    /// inventory-loss location means two places a count difference could go,
    /// and nothing would say which.
    pub const fn is_user_creatable(self) -> bool {
        matches!(self, Self::Internal | Self::View | Self::Transit)
    }
}

/// What a move between two location kinds *means*, and therefore what it does
/// to the ledger.
///
/// Derived rather than stored. A document says where stock came from and where
/// it went; this is what that turns out to be, and deriving it is what stops a
/// receipt that was posted as an adjustment from being valued as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveKind {
    /// Vendor to ours. Stock and the liability both go up.
    Receipt,
    /// Ours to a customer. Stock goes down and cost of sales goes up.
    Delivery,
    /// Ours to ours, including through transit. No value leaves the business,
    /// so no journal - the exception being a move into transit, which changes
    /// which account holds it.
    Internal,
    /// Ours to inventory loss, or back. A count difference, damage, a scrap.
    Adjustment,
    /// Ours to production, or back. Consumption and output.
    Manufacturing,
    /// Between two places that are both somebody else's. Not ours to record,
    /// and refused rather than stored.
    Neither,
}

impl MoveKind {
    /// What moving from `from` to `to` amounts to.
    pub const fn between(from: LocationKind, to: LocationKind) -> Self {
        match (from.is_owned(), to.is_owned()) {
            (true, true) => Self::Internal,
            (false, false) => Self::Neither,
            (false, true) => match from {
                LocationKind::Vendor => Self::Receipt,
                LocationKind::InventoryLoss => Self::Adjustment,
                LocationKind::Production => Self::Manufacturing,
                // A customer sending stock back is a return, which is a receipt
                // whose counterparty happens to be the person who bought it.
                LocationKind::Customer => Self::Receipt,
                _ => Self::Internal,
            },
            (true, false) => match to {
                LocationKind::Customer => Self::Delivery,
                LocationKind::InventoryLoss => Self::Adjustment,
                LocationKind::Production => Self::Manufacturing,
                // Sending stock back to a supplier: a delivery whose
                // counterparty is the one who sold it.
                LocationKind::Vendor => Self::Delivery,
                _ => Self::Internal,
            },
        }
    }

    /// Whether this move changes the value the workspace holds in stock.
    ///
    /// An internal move does not, even through transit: the same value sits in
    /// a different row. Everything else does, and everything else therefore
    /// posts a journal through the `Ledger` port.
    pub const fn changes_stock_value(self) -> bool {
        !matches!(self, Self::Internal | Self::Neither)
    }
}

/// One node of the location tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub id: Uuid,
    /// The full path, as somebody reads it: `WH/Stock/Zone A/Shelf 1`. Built
    /// from the tree and stored, because it is what every picker sees and
    /// rebuilding it per row in a grid is a query per row.
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub kind: LocationKind,
    /// The warehouse this belongs to, for an internal or view location.
    /// `None` for the virtual counterparts, which belong to no building.
    pub warehouse_id: Option<Uuid>,
    /// A replenishment destination: reordering rules may target it.
    pub is_replenished: bool,
    /// How often somebody should count what is here, in days. `None` for a
    /// location on no cycle. See `docs/adr/0006` section 7 on cycle counting.
    pub count_frequency_days: Option<i32>,
    pub is_active: bool,
}

impl Location {
    /// Whether a move may be posted here right now.
    pub const fn accepts_stock(&self) -> bool {
        self.is_active && self.kind.can_hold_stock()
    }

    /// Whether this location's quantities appear in the on-hand report.
    pub const fn is_on_hand(&self) -> bool {
        self.is_active && self.kind.is_on_hand()
    }
}

/// One row of the tree, with what it takes to draw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub kind: LocationKind,
    pub warehouse_id: Option<Uuid>,
    pub warehouse_name: Option<String>,
    pub is_replenished: bool,
    pub is_active: bool,
    /// 0 for a root. Drives the indentation in the grid.
    pub depth: u16,
    pub child_count: i64,
}

/// The editable part of a location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationInput {
    pub id: Option<Uuid>,
    /// The last segment only - `Shelf 1`, not `WH/Stock/Shelf 1`. The path is
    /// derived from the parent, so renaming a warehouse renames everything
    /// beneath it rather than leaving a tree of stale prefixes.
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub kind: LocationKind,
    pub warehouse_id: Option<Uuid>,
    pub is_replenished: bool,
    pub count_frequency_days: Option<i32>,
    pub is_active: bool,
}

impl LocationInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            name: String::new(),
            parent_id: None,
            kind: LocationKind::Internal,
            warehouse_id: None,
            is_replenished: false,
            count_frequency_days: None,
            is_active: true,
        }
    }

    /// A blank form for something inside another location - which is what "add
    /// a location here" almost always means.
    pub fn under(parent: &Location) -> Self {
        Self {
            parent_id: Some(parent.id),
            warehouse_id: parent.warehouse_id,
            ..Self::blank()
        }
    }

    pub fn from_location(location: &Location) -> Self {
        Self {
            id: Some(location.id),
            name: location.name.clone(),
            parent_id: location.parent_id,
            kind: location.kind,
            warehouse_id: location.warehouse_id,
            is_replenished: location.is_replenished,
            count_frequency_days: location.count_frequency_days,
            is_active: location.is_active,
        }
    }

    /// Trim, and say what is still wrong.
    pub fn check(&self) -> Result<Self, LocationError> {
        let name = self.name.trim();

        if name.is_empty() {
            return Err(LocationError::NameRequired);
        }
        if name.chars().count() > MAX_LOCATION_NAME_LEN {
            return Err(LocationError::NameTooLong);
        }
        // `/` separates the segments of the derived path, so a segment
        // containing one would produce a path nothing could parse back.
        if name.contains('/') {
            return Err(LocationError::NameHasSeparator);
        }

        if let (Some(id), Some(parent_id)) = (self.id, self.parent_id)
            && id == parent_id
        {
            return Err(LocationError::OwnParent);
        }

        if !self.kind.is_user_creatable() {
            return Err(LocationError::KindNotCreatable);
        }

        // A counterpart location belongs to no building. Hanging one inside a
        // warehouse would put in-transit stock in that warehouse's total.
        if self.kind == LocationKind::Transit && self.parent_id.is_some() {
            return Err(LocationError::TransitHasParent);
        }

        if self.count_frequency_days.is_some_and(|days| days < 1) {
            return Err(LocationError::CountFrequencyNotPositive);
        }

        Ok(Self {
            id: self.id,
            name: name.to_owned(),
            parent_id: self.parent_id,
            kind: self.kind,
            warehouse_id: self.warehouse_id,
            // Only somewhere stock actually sits can be a replenishment target.
            is_replenished: self.is_replenished && self.kind.is_on_hand(),
            count_frequency_days: self.count_frequency_days,
            is_active: self.is_active,
        })
    }
}

/// Build the path a location is known by: its parent's, then its own name.
pub fn path_under(parent: Option<&str>, name: &str) -> String {
    match parent {
        Some(parent) if !parent.is_empty() => format!("{parent}/{name}"),
        _ => name.to_owned(),
    }
}

/// What a delete answered. Two of the three are things a screen renders beside
/// the button rather than faults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    Deleted,
    HasChildren { count: i64 },
    /// Stock has moved across this location. Deleting it would orphan a move,
    /// and the moves are the audit trail.
    HasMovements,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LocationError {
    #[error("a location needs a name")]
    NameRequired,
    #[error("a location name is at most 120 characters")]
    NameTooLong,
    #[error("a location name may not contain a slash")]
    NameHasSeparator,
    #[error("a location cannot be its own parent")]
    OwnParent,
    #[error("that would put a location underneath itself")]
    Cycle,
    #[error("locations are at most eight levels deep")]
    TooDeep,
    #[error("a transit location does not sit inside a warehouse")]
    TransitHasParent,
    #[error("that kind of location is created by the system, not by hand")]
    KindNotCreatable,
    #[error("a counting frequency is a number of days, at least one")]
    CountFrequencyNotPositive,
    #[error("this location cannot hold stock")]
    NotStockable,
    #[error("both ends of that move belong to somebody else")]
    NeitherEndIsOurs,
}

impl LocationError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NameRequired | Self::NameTooLong | Self::NameHasSeparator => "name",
            Self::OwnParent | Self::Cycle | Self::TooDeep | Self::TransitHasParent => "parent_id",
            Self::KindNotCreatable | Self::NotStockable | Self::NeitherEndIsOurs => "kind",
            Self::CountFrequencyNotPositive => "count_frequency_days",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NameRequired => msg!("locations.error.name_required"),
            Self::NameTooLong => msg!("locations.error.name_too_long"),
            Self::NameHasSeparator => msg!("locations.error.name_has_separator"),
            Self::OwnParent => msg!("locations.error.own_parent"),
            Self::Cycle => msg!("locations.error.cycle"),
            Self::TooDeep => msg!("locations.error.too_deep"),
            Self::TransitHasParent => msg!("locations.error.transit_has_parent"),
            Self::KindNotCreatable => msg!("locations.error.kind_not_creatable"),
            Self::CountFrequencyNotPositive => msg!("locations.error.count_frequency"),
            Self::NotStockable => msg!("locations.error.not_stockable"),
            Self::NeitherEndIsOurs => msg!("locations.error.neither_end_is_ours"),
        }
    }
}

/// Order a flat list into the tree and say how deep each row is. Same shape and
/// same reasoning as `app_hr::department::in_tree_order`, including that a row
/// whose parent was filtered out is shown as a root rather than dropped.
pub fn in_tree_order(rows: Vec<LocationSummary>) -> Vec<LocationSummary> {
    let present: std::collections::HashSet<Uuid> = rows.iter().map(|row| row.id).collect();

    let mut children: std::collections::HashMap<Option<Uuid>, Vec<LocationSummary>> =
        std::collections::HashMap::new();
    for row in rows {
        let parent = row.parent_id.filter(|id| present.contains(id));
        children.entry(parent).or_default().push(row);
    }

    let mut ordered = Vec::new();
    let mut stack: Vec<(LocationSummary, u16)> = Vec::new();

    if let Some(mut roots) = children.remove(&None) {
        roots.reverse();
        stack.extend(roots.into_iter().map(|row| (row, 0)));
    }

    while let Some((mut row, depth)) = stack.pop() {
        row.depth = depth;
        let id = row.id;
        ordered.push(row);

        if let Some(mut batch) = children.remove(&Some(id)) {
            batch.reverse();
            let depth = depth.saturating_add(1);
            stack.extend(batch.into_iter().map(|row| (row, depth)));
        }
    }

    for (_, batch) in children {
        ordered.extend(batch);
    }

    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    use LocationKind as K;

    fn input() -> LocationInput {
        LocationInput {
            name: "Shelf 1".to_owned(),
            ..LocationInput::blank()
        }
    }

    #[test]
    fn a_receipt_and_a_delivery_are_told_apart_by_their_two_ends() {
        assert_eq!(MoveKind::between(K::Vendor, K::Internal), MoveKind::Receipt);
        assert_eq!(
            MoveKind::between(K::Internal, K::Customer),
            MoveKind::Delivery
        );
        assert_eq!(
            MoveKind::between(K::Internal, K::InventoryLoss),
            MoveKind::Adjustment
        );
        assert_eq!(
            MoveKind::between(K::Internal, K::Production),
            MoveKind::Manufacturing
        );
    }

    #[test]
    fn a_return_is_the_same_move_run_backwards() {
        // A customer return arrives like any other receipt, and a supplier
        // return leaves like any other delivery. One rule, not four.
        assert_eq!(
            MoveKind::between(K::Customer, K::Internal),
            MoveKind::Receipt
        );
        assert_eq!(MoveKind::between(K::Internal, K::Vendor), MoveKind::Delivery);
    }

    #[test]
    fn moving_stock_through_transit_changes_no_value() {
        // The whole reason transit is owned. Value does not leave the business
        // when a lorry does, so there is no journal - only a different row.
        assert_eq!(
            MoveKind::between(K::Internal, K::Transit),
            MoveKind::Internal
        );
        assert_eq!(
            MoveKind::between(K::Transit, K::Internal),
            MoveKind::Internal
        );
        assert!(!MoveKind::between(K::Internal, K::Transit).changes_stock_value());
        assert!(MoveKind::between(K::Vendor, K::Internal).changes_stock_value());
    }

    #[test]
    fn a_move_between_two_outsiders_is_not_ours_to_record() {
        assert_eq!(MoveKind::between(K::Vendor, K::Customer), MoveKind::Neither);
        assert!(!MoveKind::between(K::Vendor, K::Customer).changes_stock_value());
    }

    #[test]
    fn only_internal_stock_is_on_hand_and_only_internal_stock_is_pickable() {
        assert!(K::Internal.is_on_hand());
        // Ours, on the balance sheet, and on a lorry.
        assert!(K::Transit.is_owned());
        assert!(!K::Transit.is_on_hand());
        assert!(!K::Transit.is_pickable());
        assert!(!K::Vendor.is_owned());
        assert!(!K::View.can_hold_stock());
    }

    #[test]
    fn the_counterpart_kinds_are_seeded_rather_than_typed() {
        // Two inventory-loss locations would be two places a count difference
        // could go, with nothing to say which.
        for kind in [K::Vendor, K::Customer, K::InventoryLoss, K::Production] {
            let typed = LocationInput { kind, ..input() };
            assert_eq!(typed.check(), Err(LocationError::KindNotCreatable), "{kind:?}");
        }
    }

    #[test]
    fn a_transit_location_stands_outside_the_warehouse_tree() {
        let transit = LocationInput {
            kind: K::Transit,
            parent_id: Some(Uuid::from_u128(1)),
            ..input()
        };

        assert_eq!(transit.check(), Err(LocationError::TransitHasParent));
    }

    #[test]
    fn a_grouping_is_never_a_replenishment_target() {
        let view = LocationInput {
            kind: K::View,
            is_replenished: true,
            ..input()
        };

        assert!(!view.check().unwrap().is_replenished);
    }

    #[test]
    fn a_name_may_not_contain_the_path_separator() {
        let slashed = LocationInput {
            name: "Zone A/Shelf 1".to_owned(),
            ..input()
        };

        assert_eq!(slashed.check(), Err(LocationError::NameHasSeparator));
    }

    #[test]
    fn a_path_reads_from_the_root_down() {
        assert_eq!(path_under(None, "WH"), "WH");
        assert_eq!(path_under(Some("WH"), "Stock"), "WH/Stock");
        assert_eq!(path_under(Some("WH/Stock"), "Shelf 1"), "WH/Stock/Shelf 1");
    }

    #[test]
    fn the_tree_comes_out_depth_first() {
        let row = |id: u128, parent: Option<u128>, name: &str| LocationSummary {
            id: Uuid::from_u128(id),
            code: name.to_owned(),
            name: name.to_owned(),
            parent_id: parent.map(Uuid::from_u128),
            kind: K::Internal,
            warehouse_id: None,
            warehouse_name: None,
            is_replenished: false,
            is_active: true,
            depth: 0,
            child_count: 0,
        };

        let ordered = in_tree_order(vec![
            row(1, None, "WH"),
            row(2, Some(1), "Stock"),
            row(3, Some(2), "Shelf A"),
            row(4, None, "WH2"),
        ]);

        let seen: Vec<(&str, u16)> = ordered
            .iter()
            .map(|row| (row.name.as_str(), row.depth))
            .collect();

        assert_eq!(
            seen,
            vec![("WH", 0), ("Stock", 1), ("Shelf A", 2), ("WH2", 0)]
        );
    }
}
