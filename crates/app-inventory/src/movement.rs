//! The stock ledger: one row per movement, and nothing is ever edited.
//!
//! # A move is the only way a quantity changes
//!
//! There is no "set the stock to 40". Every change is a move from one location
//! to another, both ends named, and the seven kinds in
//! [`LocationKind`](crate::location::LocationKind) include the ones that are
//! not places. That is what makes the sum of every quantity ever moved zero,
//! and it is why [`crate::quant`] can be rebuilt from this table rather than
//! believed.
//!
//! # Append-only, for the reason a journal is
//!
//! A move in [`MoveState::Done`] is never updated and never deleted. A mistake
//! is corrected by a move the other way, which is the same rule Books applies
//! to a posted journal and the same reason: a record that can be edited after
//! the fact is not evidence of anything. Cancelling is only open to a draft.
//!
//! # The quantity is always positive
//!
//! The direction is the two ends, never a sign. A negative quantity would make
//! every report ask "is this a receipt of minus three or a return of three",
//! and the two mean different things to a supplier.
//!
//! # What happens when there is no ledger
//!
//! [`JournalOutcome`] has three states and no failure. A move whose kind
//! changes what the workspace holds asks the `Ledger` port to post; if nobody
//! implements it the move still happens and says so, because receiving goods is
//! a warehouse fact. Any *other* answer from the ledger - a closed period, an
//! unmapped role - rolls the whole movement back, so that a workspace which has
//! a ledger never has a stock figure its stock account disagrees with.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::Money;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::location::{LocationKind, MoveKind};
use crate::quantity::Quantity;

pub const MAX_REFERENCE_LEN: usize = 120;

/// Where a move is in its short life.
///
/// Three, and the third is only reachable from the first. A draft is a plan -
/// what a picking list holds before somebody walks the aisle; `Done` is what
/// has actually happened and what a quant is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveState {
    /// Planned. Counts against nothing except a reservation.
    Draft,
    /// It happened. Quants are changed, and this row never will be again.
    Done,
    /// It did not happen and never will. Only a draft may get here.
    Cancelled,
}

impl MoveState {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Done, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == raw)
    }

    /// Whether this row is part of what is on hand.
    pub const fn counts(self) -> bool {
        matches!(self, Self::Done)
    }

    pub const fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("moves.state.draft"),
            Self::Done => msg!("moves.state.done"),
            Self::Cancelled => msg!("moves.state.cancelled"),
        }
    }
}

/// What the ledger did about this move.
///
/// Three states, none of them a failure - see the module header for why a
/// refused posting leaves no move behind at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum JournalOutcome {
    /// Nothing to post: the value did not leave the business, or the item's
    /// category is on manual valuation.
    NotRequired,
    /// Nobody implements the `Ledger` port here. The stock moved anyway.
    NoLedger,
    Posted { journal_id: Uuid, number: String },
}

impl JournalOutcome {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::NoLedger => "no_ledger",
            Self::Posted { .. } => "posted",
        }
    }

    pub fn parse(raw: &str, journal_id: Option<Uuid>, number: Option<String>) -> Option<Self> {
        match raw {
            "not_required" => Some(Self::NotRequired),
            "no_ledger" => Some(Self::NoLedger),
            "posted" => Some(Self::Posted {
                journal_id: journal_id?,
                number: number?,
            }),
            _ => None,
        }
    }

    pub const fn journal_id(&self) -> Option<Uuid> {
        match self {
            Self::Posted { journal_id, .. } => Some(*journal_id),
            _ => None,
        }
    }

    pub fn number(&self) -> Option<&str> {
        match self {
            Self::Posted { number, .. } => Some(number),
            _ => None,
        }
    }

    pub fn label(&self) -> Message {
        match self {
            Self::NotRequired => msg!("moves.journal.not_required"),
            Self::NoLedger => msg!("moves.journal.no_ledger"),
            Self::Posted { number, .. } => msg!("moves.journal.posted", number = number.clone()),
        }
    }
}

/// The document a move came from.
///
/// The same three fields a journal's source carries, and for the same reason:
/// "which receipt is this shelf's worth of stock" has to have an answer that
/// does not involve a human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveSource {
    /// `goods_receipt`, `stock_adjustment`, `stock_transfer`.
    pub doc_type: String,
    pub doc_id: Uuid,
}

impl MoveSource {
    pub fn new(doc_type: impl Into<String>, doc_id: Uuid) -> Self {
        Self {
            doc_type: doc_type.into(),
            doc_id,
        }
    }
}

/// One movement, as it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMove {
    pub id: Uuid,
    pub variant_id: Uuid,
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub lot_id: Option<Uuid>,
    /// Always positive, in the item's stock unit.
    pub quantity: Quantity,
    /// The stock unit as it was when this happened. A snapshot, because the
    /// unit is frozen once stock exists and this is what proves it.
    pub unit_id: Uuid,
    pub state: MoveState,
    /// The date the journal takes, which is not always today: goods that
    /// arrived on Friday and were keyed on Monday belong to Friday.
    pub moved_on: NaiveDate,
    /// What one unit was worth to the workspace, in its base currency.
    pub unit_cost: Money,
    /// `quantity * unit_cost`, rounded once. Stored rather than derived so a
    /// report does not re-round it a second time per row.
    pub value: Money,
    pub reference: Option<String>,
    pub source: Option<MoveSource>,
    pub journal: JournalOutcome,
}

impl StockMove {
    /// What this move amounts to, given what its two ends are.
    ///
    /// Derived, never stored - see [`MoveKind`]. Storing it is how a receipt
    /// posted as an adjustment comes to be valued as one.
    pub const fn kind(from: LocationKind, to: LocationKind) -> MoveKind {
        MoveKind::between(from, to)
    }
}

/// What a document hands over to make one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRequest {
    pub variant_id: Uuid,
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub quantity: Quantity,
    pub moved_on: NaiveDate,
    /// What one unit is worth. `None` lets the costing method decide, which is
    /// what everything but a receipt at a stated price wants.
    pub unit_cost: Option<Money>,
    pub reference: Option<String>,
    pub source: Option<MoveSource>,
    /// Which cost centre the other side of the journal is charged to, where
    /// there is another side. Resolved through the `CostCentres` port before a
    /// posting is built.
    pub cost_centre_id: Option<Uuid>,
}

impl MoveRequest {
    pub fn new(
        variant_id: Uuid,
        from_location_id: Uuid,
        to_location_id: Uuid,
        quantity: Quantity,
        moved_on: NaiveDate,
    ) -> Self {
        Self {
            variant_id,
            from_location_id,
            to_location_id,
            lot_id: None,
            quantity,
            moved_on,
            unit_cost: None,
            reference: None,
            source: None,
            cost_centre_id: None,
        }
    }

    /// Everything about a request that can be decided without the database.
    pub fn check(&self) -> Result<Self, MoveError> {
        if self.quantity.is_zero() {
            return Err(MoveError::QuantityRequired);
        }
        if self.quantity.is_negative() {
            return Err(MoveError::QuantityNegative);
        }
        if self.from_location_id == self.to_location_id {
            return Err(MoveError::SameBothEnds);
        }

        let reference = match self.reference.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(text) if text.chars().count() > MAX_REFERENCE_LEN => {
                return Err(MoveError::ReferenceTooLong);
            }
            Some(text) => Some(text.to_owned()),
        };

        Ok(Self {
            reference,
            ..self.clone()
        })
    }
}

/// Which two accounts a move between these ends debits and credits.
///
/// Debit where the value arrived, credit where it left. That is the whole rule,
/// and it means a receipt, a delivery, a write-off, a supplier return and a
/// despatch into transit are one line of code rather than five document types
/// each with its own opinion about accounting.
///
/// `None` where no journal is wanted: the two ends stand for the same account,
/// which is an internal rearrangement, or one of them stands for none - a
/// grouping, or production, which waits on works orders.
pub fn posting_roles(
    from: LocationKind,
    to: LocationKind,
) -> Option<(phonix_ports::ledger::AccountRole, phonix_ports::ledger::AccountRole)> {
    let debit = to.account_role()?;
    let credit = from.account_role()?;

    (debit != credit).then_some((debit, credit))
}

/// Whether stock may move between two kinds of place at all.
///
/// Both ends have to be able to hold stock, and at least one of them has to be
/// ours - a move between two outsiders is somebody else's business and is
/// refused rather than recorded.
pub fn check_ends(from: LocationKind, to: LocationKind) -> Result<MoveKind, MoveError> {
    if !from.can_hold_stock() || !to.can_hold_stock() {
        return Err(MoveError::GroupingHoldsNothing);
    }

    match MoveKind::between(from, to) {
        MoveKind::Neither => Err(MoveError::NeitherEndIsOurs),
        kind => Ok(kind),
    }
}

/// What a move needs to know about the thing being moved.
///
/// One read model rather than four lookups, because every one of these is
/// needed for every movement and fetching them separately means a receipt of
/// forty lines is a hundred and sixty queries. The costing policy comes from
/// the category and the tracking rules from the item, which is where each of
/// them belongs - this is only the place they are read together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveContext {
    pub variant_id: Uuid,
    pub variant_code: String,
    pub item_id: Uuid,
    pub item_name: String,
    /// False for a service, and for goods nobody counts. Neither may move.
    pub is_tracked: bool,
    pub tracking: crate::item::Tracking,
    pub uses_expiry: bool,
    pub stock_unit_id: Uuid,
    pub stock_unit_code: String,
    pub category_id: Uuid,
    pub costing_method: crate::category::CostingMethod,
    pub valuation: crate::category::Valuation,
    pub removal_strategy: crate::category::RemovalStrategy,
    /// The item's standing cost plus this variant's `cost_extra`. What a move
    /// with no price of its own is valued at.
    pub cost: Money,
    /// The item's own cost, without the variant's extra.
    ///
    /// What the running average is kept on. The average is an item-level number
    /// because the cost column is, and `cost_extra` stays what it says it is -
    /// what this combination adds on top - rather than being blended away the
    /// first time a red one is received.
    pub item_cost: Money,
}

impl MoveContext {
    /// Whether a movement may name this at all.
    pub const fn holds_stock(&self) -> bool {
        self.is_tracked
    }

    /// Whether a move out of here has to read the cost layers, or whether one
    /// standing number answers for every unit.
    pub const fn needs_layers(&self) -> bool {
        self.costing_method.needs_layers()
    }
}

/// One row of the movement grid, with the names a person reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveSummary {
    pub id: Uuid,
    pub moved_on: NaiveDate,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub item_name: String,
    pub lot_number: Option<String>,
    pub from_path: String,
    pub to_path: String,
    pub from_kind: LocationKind,
    pub to_kind: LocationKind,
    pub quantity: Quantity,
    pub unit_code: String,
    pub value: Money,
    pub state: MoveState,
    pub reference: Option<String>,
    pub journal: JournalOutcome,
}

impl MoveSummary {
    pub const fn kind(&self) -> MoveKind {
        MoveKind::between(self.from_kind, self.to_kind)
    }
}

/// What a movement screen may be narrowed by.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveFilter {
    pub variant_id: Option<Uuid>,
    pub item_id: Option<Uuid>,
    pub location_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub state: Option<MoveState>,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    #[error("a movement needs a quantity")]
    QuantityRequired,
    #[error("a quantity is positive - the two ends say which way it went")]
    QuantityNegative,
    #[error("a movement needs two different locations")]
    SameBothEnds,
    #[error("a grouping location holds nothing itself")]
    GroupingHoldsNothing,
    #[error("both ends of that move belong to somebody else")]
    NeitherEndIsOurs,
    #[error("a reference is at most 120 characters")]
    ReferenceTooLong,
    #[error("there is not that much there")]
    NotEnoughStock,
    #[error("a movement that has happened cannot be changed")]
    AlreadyDone,
    #[error("this location is closed")]
    LocationInactive,
    #[error("a quantity of this item is not kept")]
    ItemHoldsNoStock,
}

impl MoveError {
    pub fn field(self) -> &'static str {
        match self {
            Self::QuantityRequired | Self::QuantityNegative | Self::NotEnoughStock => "quantity",
            Self::SameBothEnds | Self::GroupingHoldsNothing | Self::LocationInactive => {
                "to_location_id"
            }
            Self::NeitherEndIsOurs => "from_location_id",
            Self::ReferenceTooLong => "reference",
            Self::AlreadyDone => "state",
            Self::ItemHoldsNoStock => "variant_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::QuantityRequired => msg!("moves.error.quantity_required"),
            Self::QuantityNegative => msg!("moves.error.quantity_negative"),
            Self::SameBothEnds => msg!("moves.error.same_both_ends"),
            Self::GroupingHoldsNothing => msg!("moves.error.grouping_holds_nothing"),
            Self::NeitherEndIsOurs => msg!("moves.error.neither_end_is_ours"),
            Self::ReferenceTooLong => msg!("moves.error.reference_too_long"),
            Self::NotEnoughStock => msg!("moves.error.not_enough_stock"),
            Self::AlreadyDone => msg!("moves.error.already_done"),
            Self::LocationInactive => msg!("moves.error.location_inactive"),
            Self::ItemHoldsNoStock => msg!("moves.error.item_holds_no_stock"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use LocationKind as K;

    fn request() -> MoveRequest {
        MoveRequest::new(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Quantity::from_units(5).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
        )
    }

    #[test]
    fn a_quantity_carries_no_sign() {
        // Minus three received is not the same fact as three returned, and a
        // signed quantity makes every report guess which one it is looking at.
        let negative = MoveRequest {
            quantity: Quantity::parse("-3").unwrap(),
            ..request()
        };

        assert_eq!(negative.check(), Err(MoveError::QuantityNegative));
        assert_eq!(
            MoveRequest {
                quantity: Quantity::ZERO,
                ..request()
            }
            .check(),
            Err(MoveError::QuantityRequired)
        );
    }

    #[test]
    fn a_move_needs_somewhere_else_to_go() {
        let nowhere = MoveRequest {
            to_location_id: Uuid::from_u128(2),
            ..request()
        };

        assert_eq!(nowhere.check(), Err(MoveError::SameBothEnds));
    }

    #[test]
    fn a_grouping_is_never_an_end_of_a_movement() {
        // Posting to a grouping and to its children is how a report counts the
        // same pallet twice.
        assert_eq!(
            check_ends(K::View, K::Internal),
            Err(MoveError::GroupingHoldsNothing)
        );
        assert_eq!(
            check_ends(K::Internal, K::View),
            Err(MoveError::GroupingHoldsNothing)
        );
    }

    #[test]
    fn a_move_between_two_outsiders_is_refused_rather_than_stored() {
        assert_eq!(
            check_ends(K::Vendor, K::Customer),
            Err(MoveError::NeitherEndIsOurs)
        );
    }

    #[test]
    fn the_ordinary_moves_are_told_apart_by_their_ends() {
        assert_eq!(check_ends(K::Vendor, K::Internal), Ok(MoveKind::Receipt));
        assert_eq!(check_ends(K::Internal, K::Customer), Ok(MoveKind::Delivery));
        assert_eq!(check_ends(K::Internal, K::Transit), Ok(MoveKind::Internal));
        assert_eq!(
            check_ends(K::Internal, K::InventoryLoss),
            Ok(MoveKind::Adjustment)
        );
    }

    #[test]
    fn a_blank_reference_is_stored_as_nothing_at_all() {
        let blanked = MoveRequest {
            reference: Some("   ".to_owned()),
            ..request()
        };

        assert_eq!(blanked.check().unwrap().reference, None);
    }

    #[test]
    fn an_outcome_round_trips_through_its_three_columns() {
        let journal_id = Uuid::from_u128(44);
        let posted = JournalOutcome::Posted {
            journal_id,
            number: "GL-000123".to_owned(),
        };

        assert_eq!(
            JournalOutcome::parse("posted", Some(journal_id), Some("GL-000123".to_owned())),
            Some(posted.clone())
        );
        assert_eq!(posted.journal_id(), Some(journal_id));

        // The two that carry no journal, and a row with neither columns nor a
        // state this build knows.
        assert_eq!(
            JournalOutcome::parse("no_ledger", None, None),
            Some(JournalOutcome::NoLedger)
        );
        assert_eq!(JournalOutcome::parse("posted", None, None), None);
        assert_eq!(JournalOutcome::parse("exploded", None, None), None);
    }

    #[test]
    fn a_journal_falls_out_of_the_two_ends_rather_than_a_document_type() {
        use phonix_ports::ledger::AccountRole as Role;

        // Debit where it arrived, credit where it left. Five document types,
        // one rule.
        assert_eq!(
            posting_roles(K::Vendor, K::Internal),
            Some((Role::Inventory, Role::GoodsReceivedNotInvoiced))
        );
        assert_eq!(
            posting_roles(K::Internal, K::Customer),
            Some((Role::CostOfSales, Role::Inventory))
        );
        assert_eq!(
            posting_roles(K::Internal, K::InventoryLoss),
            Some((Role::InventoryAdjustment, Role::Inventory))
        );
        // A supplier return is the receipt run backwards, and comes out that
        // way without a rule of its own.
        assert_eq!(
            posting_roles(K::Internal, K::Vendor),
            Some((Role::GoodsReceivedNotInvoiced, Role::Inventory))
        );
    }

    #[test]
    fn stock_on_a_lorry_moves_between_two_accounts_without_leaving_the_business() {
        use phonix_ports::ledger::AccountRole as Role;

        // The third state a transfer needs: still ours, still on the balance
        // sheet, and in a different account from the shelf it left.
        assert_eq!(
            posting_roles(K::Internal, K::Transit),
            Some((Role::InventoryInTransit, Role::Inventory))
        );
        assert_eq!(
            posting_roles(K::Transit, K::Internal),
            Some((Role::Inventory, Role::InventoryInTransit))
        );
    }

    #[test]
    fn moving_a_pallet_across_the_aisle_posts_nothing() {
        // Both ends stand for the same account, so there is nothing to say.
        assert_eq!(posting_roles(K::Internal, K::Internal), None);
        // And production has no role until works orders exist.
        assert_eq!(posting_roles(K::Internal, K::Production), None);
    }

    #[test]
    fn only_a_move_that_is_done_counts_towards_what_is_on_hand() {
        assert!(MoveState::Done.counts());
        assert!(!MoveState::Draft.counts());
        assert!(!MoveState::Cancelled.counts());
        assert!(!MoveState::Draft.is_final());
    }
}
