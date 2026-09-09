//! Stock transfers: moving what we own from one of our places to another.
//!
//! # The middle is the whole point
//!
//! A lorry that left on Tuesday and arrives on Thursday is carrying stock that
//! is at neither address on Wednesday, and it is still ours. ADR 0006 section 7
//! is explicit about it: this is one document with two movements, not two
//! adjustments, because "subtract here, add there" has nowhere to put
//! Wednesday - either the pallet is counted twice or it does not exist for two
//! days, and a stock count finds both.
//!
//! So the stock goes out to a `transit` location and comes back off it. Neither
//! movement is built here; both are [`crate::movement::MoveRequest`] handed to
//! `stock::apply`, which values them and files the journals. What this module
//! adds is the pairing and the arithmetic of how far each line has got.
//!
//! # What is on the lorry is `despatched - received`
//!
//! Not a state, a subtraction. A line where the two are equal has arrived; a
//! transfer where every line has is `Done`. Something that left and never
//! turned up stays a positive difference rather than disappearing, and is
//! written off from the transit location by an adjustment - which is the right
//! account for goods lost in carriage, and not the destination's shrinkage.
//!
//! # Cancelling is only possible before anything has moved
//!
//! Once stock has left the shelf the document cannot be un-made, per ADR 0006
//! section 6.3. A load that turned back at the gate is *received* - back into
//! the location it came from, by a second transfer - rather than cancelled.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

pub const MAX_TRANSFER_NOTE_LEN: usize = 2000;
pub const MAX_TRANSFER_REFERENCE_LEN: usize = 120;
pub const MAX_TRANSFER_DESCRIPTION_LEN: usize = 200;

/// Where a journey is.
///
/// Four, and the third is reachable only through the second: nothing arrives
/// that did not leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    /// A plan. Nothing has moved and the document may still be abandoned.
    Draft,
    /// The stock has left and not all of it has arrived. The state that only
    /// exists because the transit location does.
    InTransit,
    /// Everything that left has arrived.
    Done,
    /// Abandoned before anything moved. Only reachable from [`Self::Draft`].
    Cancelled,
}

impl TransferState {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::InTransit, Self::Done, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InTransit => "in_transit",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|state| state.as_str() == raw)
    }

    /// Only a draft may be edited, cancelled or deleted.
    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Whether stock has left the origin.
    pub const fn has_left(self) -> bool {
        matches!(self, Self::InTransit | Self::Done)
    }

    pub const fn is_done(self) -> bool {
        matches!(self, Self::Done)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("transfers.state.draft"),
            Self::InTransit => msg!("transfers.state.in_transit"),
            Self::Done => msg!("transfers.state.done"),
            Self::Cancelled => msg!("transfers.state.cancelled"),
        }
    }
}

/// One item on the journey, and how far it has got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferLine {
    pub id: Uuid,
    pub line_no: i32,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub lot_id: Option<Uuid>,
    pub lot_number: Option<String>,
    pub description: String,
    pub quantity: Quantity,
    pub despatched: Quantity,
    pub received: Quantity,
}

impl TransferLine {
    /// What is on the lorry: gone from the origin, not yet at the destination.
    pub fn in_transit(&self) -> Quantity {
        self.despatched
            .checked_sub(self.received)
            .unwrap_or(Quantity::ZERO)
    }

    /// What has not left yet.
    pub fn to_despatch(&self) -> Quantity {
        self.quantity
            .checked_sub(self.despatched)
            .unwrap_or(Quantity::ZERO)
    }

    pub fn has_arrived(&self) -> bool {
        !self.in_transit().is_positive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    pub id: Uuid,
    pub number: String,
    pub state: TransferState,
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub transit_location_id: Uuid,
    pub from_path: String,
    pub to_path: String,
    pub planned_on: NaiveDate,
    pub despatched_on: Option<NaiveDate>,
    pub arrived_on: Option<NaiveDate>,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<TransferLine>,
}

impl Transfer {
    /// What to call it before it has a number.
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} → {}", self.from_path, self.to_path)
        } else {
            self.number.clone()
        }
    }

    pub fn has_lines(&self) -> bool {
        !self.lines.is_empty()
    }

    /// Whether anything on this document is still on the road.
    pub fn is_carrying(&self) -> bool {
        self.lines.iter().any(|line| !line.has_arrived())
    }

    /// Total still on the road. Only meaningful where every line counts in the
    /// same unit, which is why the screen shows it per line as well.
    pub fn in_transit(&self) -> Quantity {
        self.lines.iter().fold(Quantity::ZERO, |running, line| {
            running.checked_add(line.in_transit()).unwrap_or(running)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSummary {
    pub id: Uuid,
    pub number: String,
    pub state: TransferState,
    pub from_path: String,
    pub to_path: String,
    pub planned_on: NaiveDate,
    pub despatched_on: Option<NaiveDate>,
    pub reference: Option<String>,
    pub line_count: i64,
    pub in_transit: Quantity,
}

// --- What somebody keys ---------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferInput {
    pub id: Option<Uuid>,
    pub from_location_id: Option<Uuid>,
    pub to_location_id: Option<Uuid>,
    pub planned_on: NaiveDate,
    pub reference: String,
    pub note: String,
    pub lines: Vec<TransferLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferLineInput {
    pub id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub description: String,
    /// As typed. A blank row is dropped rather than refused.
    pub quantity: String,
}

impl TransferLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            variant_id: None,
            lot_id: None,
            description: String::new(),
            quantity: String::new(),
        }
    }

    fn is_blank(&self) -> bool {
        self.variant_id.is_none()
            && self.description.trim().is_empty()
            && self.quantity.trim().is_empty()
    }
}

impl TransferInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            from_location_id: None,
            to_location_id: None,
            planned_on: today,
            reference: String::new(),
            note: String::new(),
            lines: vec![TransferLineInput::blank()],
        }
    }

    /// Reopen a draft for editing.
    pub fn from_document(document: &Transfer) -> Self {
        Self {
            id: Some(document.id),
            from_location_id: Some(document.from_location_id),
            to_location_id: Some(document.to_location_id),
            planned_on: document.planned_on,
            reference: document.reference.clone().unwrap_or_default(),
            note: document.note.clone().unwrap_or_default(),
            lines: document
                .lines
                .iter()
                .map(|line| TransferLineInput {
                    id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    lot_id: line.lot_id,
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                })
                .collect(),
        }
    }

    /// Everything decidable without the database.
    ///
    /// Whether the two ends can hold stock, and whether there is enough on the
    /// shelf, are not: both need a lookup, and both are decided at despatch,
    /// where the answer is still true a moment later.
    pub fn check(&self) -> Result<CheckedTransfer, TransferError> {
        let from_location_id = self.from_location_id.ok_or(TransferError::OriginRequired)?;
        let to_location_id = self
            .to_location_id
            .ok_or(TransferError::DestinationRequired)?;

        if from_location_id == to_location_id {
            return Err(TransferError::EndsAreTheSame);
        }

        if self.reference.chars().count() > MAX_TRANSFER_REFERENCE_LEN {
            return Err(TransferError::ReferenceTooLong);
        }
        if self.note.chars().count() > MAX_TRANSFER_NOTE_LEN {
            return Err(TransferError::NoteTooLong);
        }

        let mut lines = Vec::new();

        for line in &self.lines {
            if line.is_blank() {
                continue;
            }

            let variant_id = line.variant_id.ok_or(TransferError::ItemRequired)?;

            let description = line.description.trim();
            if description.is_empty() {
                return Err(TransferError::DescriptionRequired);
            }
            if description.chars().count() > MAX_TRANSFER_DESCRIPTION_LEN {
                return Err(TransferError::DescriptionTooLong);
            }

            if line.quantity.trim().is_empty() {
                return Err(TransferError::QuantityRequired);
            }

            let quantity = Quantity::parse(line.quantity.trim())?;
            if !quantity.is_positive() {
                return Err(TransferError::QuantityNotPositive);
            }

            lines.push(CheckedTransferLine {
                id: line.id,
                variant_id,
                lot_id: line.lot_id,
                description: description.to_owned(),
                quantity,
            });
        }

        if lines.is_empty() {
            return Err(TransferError::NothingToMove);
        }

        Ok(CheckedTransfer {
            id: self.id,
            from_location_id,
            to_location_id,
            planned_on: self.planned_on,
            reference: non_empty(&self.reference),
            note: non_empty(&self.note),
            lines,
        })
    }
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedTransfer {
    pub id: Option<Uuid>,
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub planned_on: NaiveDate,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<CheckedTransferLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedTransferLine {
    pub id: Option<Uuid>,
    pub variant_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub description: String,
    pub quantity: Quantity,
}

/// How much of one line is arriving now.
///
/// Keyed at the destination, per line, because a part-load is ordinary: two
/// pallets of three turned up and the third is on the next van.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrivalInput {
    pub transfer_id: Uuid,
    pub arrived_on: NaiveDate,
    pub lines: Vec<ArrivalLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrivalLineInput {
    pub line_id: Uuid,
    /// As typed. Blank means none of this line arrived.
    pub quantity: String,
}

impl ArrivalInput {
    /// Everything on the road, pre-filled - the common case, where the whole
    /// load turned up.
    pub fn everything(document: &Transfer, today: NaiveDate) -> Self {
        Self {
            transfer_id: document.id,
            arrived_on: today,
            lines: document
                .lines
                .iter()
                .filter(|line| !line.has_arrived())
                .map(|line| ArrivalLineInput {
                    line_id: line.id,
                    quantity: line.in_transit().to_display_string(),
                })
                .collect(),
        }
    }

    /// What is actually arriving, against what is on the road.
    ///
    /// A line arriving more than was sent is refused here rather than absorbed:
    /// more turning up than left is a count error at one end, and finding it is
    /// the whole reason the two numbers are kept apart.
    pub fn check(&self, document: &Transfer) -> Result<Vec<Arriving>, TransferError> {
        let mut arriving = Vec::new();

        for line in &self.lines {
            if line.quantity.trim().is_empty() {
                continue;
            }

            let quantity = Quantity::parse(line.quantity.trim())?;
            if !quantity.is_positive() {
                continue;
            }

            let Some(stored) = document.lines.iter().find(|stored| stored.id == line.line_id)
            else {
                return Err(TransferError::LineNotOnTransfer);
            };

            if quantity.compare(stored.in_transit()).is_gt() {
                return Err(TransferError::MoreThanLeft);
            }

            arriving.push(Arriving {
                line_id: stored.id,
                variant_id: stored.variant_id,
                lot_id: stored.lot_id,
                quantity,
            });
        }

        if arriving.is_empty() {
            return Err(TransferError::NothingArriving);
        }

        Ok(arriving)
    }
}

/// One line's arrival, checked against what is on the road.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arriving {
    pub line_id: Uuid,
    pub variant_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    #[error("a transfer needs somewhere to move stock from")]
    OriginRequired,
    #[error("a transfer needs somewhere to move stock to")]
    DestinationRequired,
    #[error("a transfer between one place and itself moves nothing")]
    EndsAreTheSame,
    #[error("a transfer needs at least one line")]
    NothingToMove,
    #[error("a line needs an item")]
    ItemRequired,
    #[error("a line needs a description")]
    DescriptionRequired,
    #[error("a description is at most 200 characters")]
    DescriptionTooLong,
    #[error("a line needs a quantity")]
    QuantityRequired,
    #[error("a quantity has to be more than nothing")]
    QuantityNotPositive,
    #[error("a reference is at most 120 characters")]
    ReferenceTooLong,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("stock only moves between two places that can hold it")]
    EndsCannotHoldStock,
    #[error("only a draft transfer can be changed")]
    NotEditable,
    #[error("this transfer has not been despatched")]
    NotDespatched,
    #[error("this transfer has already been despatched")]
    AlreadyDespatched,
    #[error("nothing on this transfer is still on its way")]
    NothingArriving,
    #[error("that line is not on this transfer")]
    LineNotOnTransfer,
    #[error("more cannot arrive than left")]
    MoreThanLeft,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
}

impl TransferError {
    pub fn field(self) -> &'static str {
        match self {
            Self::OriginRequired | Self::EndsAreTheSame | Self::EndsCannotHoldStock => {
                "from_location_id"
            }
            Self::DestinationRequired => "to_location_id",
            Self::NothingToMove
            | Self::ItemRequired
            | Self::DescriptionRequired
            | Self::DescriptionTooLong
            | Self::LineNotOnTransfer => "lines",
            Self::QuantityRequired
            | Self::QuantityNotPositive
            | Self::MoreThanLeft
            | Self::NothingArriving
            | Self::Quantity(_) => "quantity",
            Self::ReferenceTooLong => "reference",
            Self::NoteTooLong => "note",
            Self::NotEditable | Self::NotDespatched | Self::AlreadyDespatched => "state",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::OriginRequired => msg!("transfers.error.origin_required"),
            Self::DestinationRequired => msg!("transfers.error.destination_required"),
            Self::EndsAreTheSame => msg!("transfers.error.ends_are_the_same"),
            Self::NothingToMove => msg!("transfers.error.nothing_to_move"),
            Self::ItemRequired => msg!("transfers.error.item_required"),
            Self::DescriptionRequired => msg!("transfers.error.description_required"),
            Self::DescriptionTooLong => msg!("transfers.error.description_too_long"),
            Self::QuantityRequired => msg!("transfers.error.quantity_required"),
            Self::QuantityNotPositive => msg!("transfers.error.quantity_not_positive"),
            Self::ReferenceTooLong => msg!("transfers.error.reference_too_long"),
            Self::NoteTooLong => msg!("transfers.error.note_too_long"),
            Self::EndsCannotHoldStock => msg!("transfers.error.ends_cannot_hold_stock"),
            Self::NotEditable => msg!("transfers.error.not_editable"),
            Self::NotDespatched => msg!("transfers.error.not_despatched"),
            Self::AlreadyDespatched => msg!("transfers.error.already_despatched"),
            Self::NothingArriving => msg!("transfers.error.nothing_arriving"),
            Self::LineNotOnTransfer => msg!("transfers.error.line_not_on_transfer"),
            Self::MoreThanLeft => msg!("transfers.error.more_than_left"),
            Self::Quantity(err) => err.message(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 4).expect("a date")
    }

    fn qty(raw: &str) -> Quantity {
        Quantity::parse(raw).expect("a quantity")
    }

    fn line(quantity: &str, despatched: &str, received: &str) -> TransferLine {
        TransferLine {
            id: Uuid::from_u128(1),
            line_no: 1,
            variant_id: Uuid::from_u128(2),
            variant_code: "SAMPLE-GLOVE-M".to_owned(),
            lot_id: None,
            lot_number: None,
            description: "Gloves".to_owned(),
            quantity: qty(quantity),
            despatched: qty(despatched),
            received: qty(received),
        }
    }

    fn transfer(lines: Vec<TransferLine>, state: TransferState) -> Transfer {
        Transfer {
            id: Uuid::from_u128(9),
            number: String::new(),
            state,
            from_location_id: Uuid::from_u128(3),
            to_location_id: Uuid::from_u128(4),
            transit_location_id: Uuid::from_u128(5),
            from_path: "WH/Stock".to_owned(),
            to_path: "WH2/Stock".to_owned(),
            planned_on: today(),
            despatched_on: None,
            arrived_on: None,
            reference: None,
            note: None,
            lines,
        }
    }

    fn draft() -> TransferInput {
        TransferInput {
            from_location_id: Some(Uuid::from_u128(3)),
            to_location_id: Some(Uuid::from_u128(4)),
            lines: vec![TransferLineInput {
                id: None,
                variant_id: Some(Uuid::from_u128(2)),
                lot_id: None,
                description: "Gloves".to_owned(),
                quantity: "40".to_owned(),
            }],
            ..TransferInput::blank(today())
        }
    }

    #[test]
    fn what_is_on_the_lorry_is_what_left_less_what_arrived() {
        let line = line("40", "40", "15");

        assert_eq!(line.in_transit(), qty("25"));
        assert!(!line.has_arrived());
    }

    #[test]
    fn a_line_that_all_arrived_is_carrying_nothing() {
        let line = line("40", "40", "40");

        assert_eq!(line.in_transit(), Quantity::ZERO);
        assert!(line.has_arrived());
    }

    #[test]
    fn a_transfer_is_carrying_while_any_line_is() {
        let document = transfer(
            vec![line("40", "40", "40"), line("10", "10", "4")],
            TransferState::InTransit,
        );

        assert!(document.is_carrying());
        assert_eq!(document.in_transit(), qty("6"));
    }

    #[test]
    fn a_transfer_to_where_it_already_is_is_refused() {
        let same = Uuid::from_u128(3);
        let input = TransferInput {
            to_location_id: Some(same),
            ..draft()
        };

        assert_eq!(input.check(), Err(TransferError::EndsAreTheSame));
    }

    #[test]
    fn a_half_typed_line_is_dropped_rather_than_refused() {
        let mut input = draft();
        input.lines.push(TransferLineInput::blank());

        let checked = input.check().expect("a checked transfer");

        assert_eq!(checked.lines.len(), 1);
    }

    #[test]
    fn a_transfer_of_nothing_is_refused() {
        let input = TransferInput {
            lines: vec![TransferLineInput::blank()],
            ..draft()
        };

        assert_eq!(input.check(), Err(TransferError::NothingToMove));
    }

    #[test]
    fn a_line_of_zero_is_refused_rather_than_moved() {
        let mut input = draft();
        input.lines[0].quantity = "0".to_owned();

        assert_eq!(input.check(), Err(TransferError::QuantityNotPositive));
    }

    #[test]
    fn more_cannot_arrive_than_left() {
        let document = transfer(vec![line("40", "40", "15")], TransferState::InTransit);
        let arrival = ArrivalInput {
            transfer_id: document.id,
            arrived_on: today(),
            lines: vec![ArrivalLineInput {
                line_id: document.lines[0].id,
                quantity: "26".to_owned(),
            }],
        };

        assert_eq!(arrival.check(&document), Err(TransferError::MoreThanLeft));
    }

    #[test]
    fn part_of_a_load_arriving_is_ordinary() {
        let document = transfer(vec![line("40", "40", "15")], TransferState::InTransit);
        let arrival = ArrivalInput {
            transfer_id: document.id,
            arrived_on: today(),
            lines: vec![ArrivalLineInput {
                line_id: document.lines[0].id,
                quantity: "10".to_owned(),
            }],
        };

        let arriving = arrival.check(&document).expect("an arrival");

        assert_eq!(arriving.len(), 1);
        assert_eq!(arriving[0].quantity, qty("10"));
    }

    #[test]
    fn prefilling_an_arrival_offers_what_is_still_on_the_road() {
        let document = transfer(
            vec![line("40", "40", "40"), line("10", "10", "4")],
            TransferState::InTransit,
        );

        let arrival = ArrivalInput::everything(&document, today());

        assert_eq!(arrival.lines.len(), 1);
        assert_eq!(arrival.lines[0].quantity, "6");
    }

    #[test]
    fn an_arrival_of_nothing_is_refused() {
        let document = transfer(vec![line("40", "40", "15")], TransferState::InTransit);
        let arrival = ArrivalInput {
            transfer_id: document.id,
            arrived_on: today(),
            lines: vec![ArrivalLineInput {
                line_id: document.lines[0].id,
                quantity: String::new(),
            }],
        };

        assert_eq!(arrival.check(&document), Err(TransferError::NothingArriving));
    }

    #[test]
    fn only_a_draft_may_be_changed() {
        assert!(TransferState::Draft.is_editable());
        assert!(!TransferState::InTransit.is_editable());
        assert!(!TransferState::Done.is_editable());
        assert!(TransferState::InTransit.has_left());
        assert!(TransferState::Done.has_left());
        assert!(!TransferState::Cancelled.has_left());
    }

    #[test]
    fn every_state_round_trips_through_its_stored_word() {
        for state in TransferState::ALL {
            assert_eq!(TransferState::parse(state.as_str()), Some(*state));
        }
    }
}
