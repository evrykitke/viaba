//! Requisitions: a department asking, before anybody commits.
//!
//! # A request is not a small purchase order
//!
//! The two carry similar-looking lines and are answered by different people. An
//! order is a promise to a supplier made by whoever may commit the workspace's
//! money; a requisition is a request from somebody who may not. So it names no
//! supplier, no price and no currency - the person raising it knows none of the
//! three and asking them to invent one produces a number that then has to be
//! ignored. ADR 0006 section 7.
//!
//! Nothing here reaches the ledger. There is no journal on a requisition and no
//! valuation on its lines, because a request that moved an account would be a
//! commitment under another name.
//!
//! # Four things are required, and each of them costs something
//!
//! The first version of this document made all four optional and every one was
//! overruled. They are grouped here because the trade is the same each time: a
//! field that may be left blank is a field that is left blank, and the cost of
//! that lands on somebody further down the chain than the person who skipped it.
//!
//! **A cost centre.** Resolved through the `CostCentres` port and snapshotted
//! beside its id the way every port result is - see
//! [`phonix_ports::cost_centre`]. Required, which means **a workspace without
//! the HR app cannot raise a requisition at all**: `NoCostCentres` answers an
//! empty list and there is nothing to charge. That is a real dependency between
//! two apps and a departure from the spirit of ADR 0006 section 2, taken with
//! that understood. The reasoning is that "who is paying for this" answered at
//! the invoice is answered by whoever argues least, and a requisition that
//! cannot say is the document this one exists to replace.
//!
//! **An item on every line.** A line may not merely describe something in
//! words. Such a line cannot be grouped with anybody else's, cannot be priced,
//! and cannot become an order line without somebody retyping it - and no string
//! match may decide on a requester's behalf that "printer paper" and "A4 paper,
//! white" are the same thing. The cost is that needing something the workspace
//! does not stock is now two steps: create the item, then ask for it. The
//! payoff is that consolidation has one case instead of two.
//!
//! **A unit on every line**, which follows from the item.
//!
//! **A reason on every decision - including an approval.** The asymmetry the
//! first version had, where only a rejection had to explain itself, was
//! overruled: an approval nobody had to justify is the one that gets given
//! without being read, and the note is what somebody reads a year later when
//! the spend is queried. So there is no `approving` flag in
//! [`DecisionInput::check`] any more; there is one rule.
//!
//! # The number arrives at submit
//!
//! Not at create, on the same terms as the order's at confirm and the receipt's
//! at post: a draft somebody abandoned must not leave a hole in a series. Until
//! then a requisition is a shopping list and has no number.
//!
//! Rejected and cancelled requisitions keep theirs. Somebody was quoted that
//! number and has to be able to look it up, and a series that reuses a number
//! once a request is turned down is a series that cannot be cited.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use phonix_ports::cost_centre::CostCentre;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

pub const MAX_NOTE_LEN: usize = 2000;
pub const MAX_JUSTIFICATION_LEN: usize = 2000;
pub const MAX_LINE_DESCRIPTION_LEN: usize = 400;

/// Where a requisition is in its life.
///
/// `Ordered` and `PartiallyOrdered` are *not* here, for the reason
/// [`crate::purchase::OrderState`] has no `Received`: how much has been ordered
/// is arithmetic over the lines, and a stored copy is a second fact that stops
/// agreeing the first time an order is cancelled. See
/// [`Requisition::order_progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequisitionState {
    /// Being written. Editable, deletable, and carries no number.
    Draft,
    /// Asked for. Numbered, no longer editable by the requester, and waiting on
    /// somebody who may answer it.
    Submitted,
    /// Agreed. The only state consolidation reads, because buying on the
    /// strength of a request that could still be refused is buying on a guess.
    Approved,
    /// Turned down, with a reason on the record.
    Rejected,
    /// Withdrawn by the person who raised it, before anybody answered.
    Cancelled,
}

impl RequisitionState {
    pub const ALL: &'static [Self] = &[
        Self::Draft,
        Self::Submitted,
        Self::Approved,
        Self::Rejected,
        Self::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == raw)
    }

    /// Whether the lines may still be changed.
    ///
    /// A draft only. Once it is submitted the requester has asked a question,
    /// and editing the question after somebody started answering it is how an
    /// approval ends up attached to a request nobody approved.
    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Whether it is waiting on a decision.
    pub const fn awaits_decision(self) -> bool {
        matches!(self, Self::Submitted)
    }

    /// Whether an order may be raised from it.
    pub const fn can_be_ordered(self) -> bool {
        matches!(self, Self::Approved)
    }

    /// Whether it has been decided, either way.
    pub const fn is_decided(self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }

    /// Whether a row in this state carries a number.
    pub const fn is_numbered(self) -> bool {
        !matches!(self, Self::Draft)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("requisitions.state.draft"),
            Self::Submitted => msg!("requisitions.state.submitted"),
            Self::Approved => msg!("requisitions.state.approved"),
            Self::Rejected => msg!("requisitions.state.rejected"),
            Self::Cancelled => msg!("requisitions.state.cancelled"),
        }
    }
}

/// How much of a requisition has been put on an order, worked out rather than
/// stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderProgress {
    Nothing,
    Partly,
    Everything,
}

impl OrderProgress {
    pub fn label(self) -> Message {
        match self {
            Self::Nothing => msg!("requisitions.ordered.nothing"),
            Self::Partly => msg!("requisitions.ordered.partly"),
            Self::Everything => msg!("requisitions.ordered.everything"),
        }
    }

    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Everything)
    }
}

/// Who decided, when, and why.
///
/// One value rather than four loose columns, because the four are one fact: the
/// schema's `requisitions_decided_when_settled` says a decided requisition has
/// all of it and an undecided one has none. The reason is not optional - see the
/// module documentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub at: chrono::DateTime<chrono::Utc>,
    /// The person. `None` where the account has since been removed - the
    /// decision still happened, and blanking the whole record because the
    /// decider left would lose the part that matters.
    pub by: Option<Uuid>,
    pub by_name: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requisition {
    pub id: Uuid,
    /// `REQ-2026-00042`. Empty until it is submitted.
    pub number: String,
    pub state: RequisitionState,
    /// Who is paying. Required - see the module documentation.
    pub cost_centre: CostCentre,
    /// Where the goods are wanted. An order raised from this inherits it.
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    pub raised_on: NaiveDate,
    pub needed_by: Option<NaiveDate>,
    /// Why they want it, in their words. The field an approver actually reads.
    pub justification: Option<String>,
    pub note: Option<String>,
    pub decision: Option<Decision>,
    pub raised_by: Option<Uuid>,
    pub raised_by_name: Option<String>,
    pub lines: Vec<RequisitionLine>,
}

impl Requisition {
    /// How much of this has been put on an order.
    ///
    /// Derived from the lines every time it is asked, for the reason
    /// [`crate::purchase::PurchaseOrder::receipt_state`] is.
    ///
    /// There is no `Over` case, unlike a receipt's: the schema refuses
    /// `ordered > quantity` on a line, because ordering more than a department
    /// asked for is a purchasing decision that belongs on the order rather than
    /// smuggled back onto somebody else's request.
    pub fn order_progress(&self) -> OrderProgress {
        if self.lines.is_empty() {
            return OrderProgress::Nothing;
        }

        let mut any = false;
        let mut all = true;

        for line in &self.lines {
            if line.ordered.is_positive() {
                any = true;
            }
            if line.ordered.compare(line.quantity).is_lt() {
                all = false;
            }
        }

        match (any, all) {
            (_, true) => OrderProgress::Everything,
            (true, _) => OrderProgress::Partly,
            _ => OrderProgress::Nothing,
        }
    }

    /// Whether anything is still to be ordered, which is what decides whether
    /// the document offers to raise one.
    pub fn has_outstanding(&self) -> bool {
        self.lines.iter().any(RequisitionLine::is_outstanding)
    }

    pub fn can_raise_an_order(&self) -> bool {
        self.state.can_be_ordered() && self.has_outstanding()
    }

    /// What the requester thinks the whole thing costs.
    ///
    /// `None` unless *every* line carries an estimate. A partial total is worse
    /// than none: an approver reading "£240" under a request where three of the
    /// eight lines were priced is being shown a number that invites the wrong
    /// decision.
    pub fn estimate(&self) -> Option<Money> {
        let mut total: Option<Money> = None;

        for line in &self.lines {
            let value = line.estimated_value()?;

            total = Some(match total {
                None => value,
                Some(running) => running.checked_add(value).ok()?,
            });
        }

        total
    }

    /// What a document calls this. The number once it has one, and something a
    /// person can still recognise before that.
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} · {}", self.raised_on, self.cost_centre.name)
        } else {
            self.number.clone()
        }
    }
}

/// One line of a requisition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequisitionLine {
    pub id: Uuid,
    /// Position on the printed request, from one.
    pub line_no: i32,
    /// What they want. Required - see the module documentation.
    pub variant_id: Uuid,
    pub variant_code: String,
    /// The requester's own words. Defaulted from the item and still editable,
    /// because "the 5ml ones, not the 10ml" is worth carrying even when the
    /// variant already says which.
    pub description: String,
    pub quantity: Quantity,
    pub unit_id: Uuid,
    pub unit_code: String,
    /// How much of this line has reached a purchase order.
    pub ordered: Quantity,
    /// What the requester thinks one unit costs, in the workspace's own
    /// currency. Advisory, never posted, and explicitly not what the order is
    /// priced at.
    pub estimate: Option<Money>,
    pub note: Option<String>,
}

impl RequisitionLine {
    /// What is still to be ordered. Never negative - the schema will not store
    /// `ordered` above `quantity`.
    pub fn outstanding(&self) -> Quantity {
        match self.quantity.checked_sub(self.ordered) {
            Ok(left) if left.is_positive() => left,
            _ => Quantity::ZERO,
        }
    }

    /// Whether anything on this line is still to be ordered.
    ///
    /// Also the answer to "can consolidation group this", which used to be a
    /// method of its own back when a line could name no item. Every line names
    /// one now, so the two questions have one answer.
    pub fn is_outstanding(&self) -> bool {
        self.outstanding().is_positive()
    }

    /// `quantity * estimate`, or `None` where nobody estimated.
    pub fn estimated_value(&self) -> Option<Money> {
        let each = self.estimate?;

        each.scale_by(
            self.quantity.scaled(),
            crate::quantity::SCALE_FACTOR,
            phonix_core::money::Rounding::HalfUp,
        )
        .ok()
    }
}

/// One row of the requisition grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequisitionSummary {
    pub id: Uuid,
    pub number: String,
    pub state: RequisitionState,
    pub order_progress: OrderProgress,
    pub cost_centre_name: String,
    pub warehouse_name: String,
    pub raised_on: NaiveDate,
    pub needed_by: Option<NaiveDate>,
    pub raised_by_name: Option<String>,
    pub line_count: i64,
    /// The sum of the lines' estimates, where every line carried one.
    pub estimate: Option<Money>,
}

/// One item that approved requisitions are still waiting for.
///
/// The unit consolidation works in: eleven departments asking for printer paper
/// is one row here, and one line of the order it becomes. Read from the
/// `requisition_demand` view rather than assembled in Rust, because grouping a
/// year of requisition lines in the browser to draw one screen is the query the
/// view exists to be.
///
/// Approved only. A submitted requisition is a question nobody has answered, and
/// buying against it is buying on the strength of a request that could still be
/// turned down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Demand {
    pub variant_id: Uuid,
    pub variant_code: String,
    pub item_name: String,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    /// The stock unit, which is what the order line will be raised in.
    pub unit_code: String,
    /// How many requisitions are waiting on this, and how many lines between
    /// them. Two numbers because they differ, and the difference is worth
    /// seeing: one requisition asking for the same thing on three lines is a
    /// keying mistake somebody should look at.
    pub requisitions: i64,
    pub lines: i64,
    pub outstanding: Quantity,
    /// The earliest date anybody said they needed it, which is what decides
    /// how urgent the consolidated order is.
    pub needed_by: Option<NaiveDate>,
    /// When the oldest of these requests was raised. The column that says how
    /// long somebody has been waiting, which no total can.
    pub oldest_request: NaiveDate,
}

/// The editable part of a requisition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequisitionInput {
    pub id: Option<Uuid>,
    pub cost_centre_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub raised_on: NaiveDate,
    pub needed_by: Option<NaiveDate>,
    pub justification: String,
    pub note: String,
    pub lines: Vec<RequisitionLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequisitionLineInput {
    pub id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    pub quantity: String,
    pub unit_id: Option<Uuid>,
    /// As typed. Parsed against the workspace's own currency by the service,
    /// which is the only place that knows what that is.
    pub estimate: String,
    pub note: String,
}

impl RequisitionLineInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            variant_id: None,
            description: String::new(),
            quantity: "1".to_owned(),
            unit_id: None,
            estimate: String::new(),
            note: String::new(),
        }
    }
}

impl RequisitionInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            cost_centre_id: None,
            warehouse_id: None,
            raised_on: today,
            needed_by: None,
            justification: String::new(),
            note: String::new(),
            lines: vec![RequisitionLineInput::blank()],
        }
    }

    pub fn from_requisition(requisition: &Requisition) -> Self {
        Self {
            id: Some(requisition.id),
            cost_centre_id: Some(requisition.cost_centre.id),
            warehouse_id: Some(requisition.warehouse_id),
            raised_on: requisition.raised_on,
            needed_by: requisition.needed_by,
            justification: requisition.justification.clone().unwrap_or_default(),
            note: requisition.note.clone().unwrap_or_default(),
            lines: requisition
                .lines
                .iter()
                .map(|line| RequisitionLineInput {
                    id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    unit_id: Some(line.unit_id),
                    estimate: line
                        .estimate
                        .map(|amount| amount.to_storage_string())
                        .unwrap_or_default(),
                    note: line.note.clone().unwrap_or_default(),
                })
                .collect(),
        }
    }

    /// Everything that can be decided without the database.
    ///
    /// Blank lines are dropped rather than refused, for the reason
    /// [`crate::purchase::OrderInput::check`] drops them: a form that always
    /// shows one empty row at the bottom would otherwise refuse every save.
    pub fn check(&self) -> Result<Checked, RequisitionError> {
        let cost_centre_id = self
            .cost_centre_id
            .ok_or(RequisitionError::CostCentreRequired)?;
        let warehouse_id = self
            .warehouse_id
            .ok_or(RequisitionError::WarehouseRequired)?;

        if self.needed_by.is_some_and(|needed| needed < self.raised_on) {
            return Err(RequisitionError::NeededBeforeRaised);
        }

        if self.justification.chars().count() > MAX_JUSTIFICATION_LEN {
            return Err(RequisitionError::JustificationTooLong);
        }

        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(RequisitionError::NoteTooLong);
        }

        let mut lines = Vec::new();

        for line in &self.lines {
            if is_blank(line) {
                continue;
            }

            let variant_id = line.variant_id.ok_or(RequisitionError::ItemRequired)?;
            let unit_id = line.unit_id.ok_or(RequisitionError::UnitRequired)?;

            let description = line.description.trim();
            if description.is_empty() {
                return Err(RequisitionError::DescriptionRequired);
            }
            if description.chars().count() > MAX_LINE_DESCRIPTION_LEN {
                return Err(RequisitionError::DescriptionTooLong);
            }

            let quantity = Quantity::parse(&line.quantity)?;
            if !quantity.is_positive() {
                return Err(RequisitionError::QuantityRequired);
            }

            if line.note.chars().count() > MAX_NOTE_LEN {
                return Err(RequisitionError::NoteTooLong);
            }

            lines.push(CheckedLine {
                id: line.id,
                variant_id,
                description: description.to_owned(),
                quantity,
                unit_id,
                estimate: line.estimate.trim().to_owned(),
                note: non_empty(&line.note),
            });
        }

        if lines.is_empty() {
            return Err(RequisitionError::NoLines);
        }

        Ok(Checked {
            id: self.id,
            cost_centre_id,
            warehouse_id,
            raised_on: self.raised_on,
            needed_by: self.needed_by,
            justification: non_empty(&self.justification),
            note: non_empty(&self.note),
            lines,
        })
    }
}

/// A line nobody filled in. Dropped rather than refused.
fn is_blank(line: &RequisitionLineInput) -> bool {
    line.variant_id.is_none()
        && line.description.trim().is_empty()
        && line.estimate.trim().is_empty()
        && line.note.trim().is_empty()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// A requisition that passed [`RequisitionInput::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub id: Option<Uuid>,
    pub cost_centre_id: Uuid,
    pub warehouse_id: Uuid,
    pub raised_on: NaiveDate,
    pub needed_by: Option<NaiveDate>,
    pub justification: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<CheckedLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedLine {
    pub id: Option<Uuid>,
    pub variant_id: Uuid,
    pub description: String,
    pub quantity: Quantity,
    pub unit_id: Uuid,
    /// Still text: parsing it needs the workspace's currency, which is the
    /// service's to establish.
    pub estimate: String,
    pub note: Option<String>,
}

/// What somebody answering a requisition says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionInput {
    pub note: String,
}

impl DecisionInput {
    /// Both answers have to say why.
    ///
    /// There is no `approving` argument. An earlier version required a reason
    /// only on a rejection, on the grounds that "yes" explains itself; that was
    /// overruled, because an approval nobody had to justify is the one given
    /// without being read, and the note is what somebody reads a year later when
    /// the spend is queried.
    pub fn check(&self) -> Result<String, RequisitionError> {
        let note = self.note.trim();

        if note.is_empty() {
            return Err(RequisitionError::DecisionNeedsAReason);
        }
        if note.chars().count() > MAX_NOTE_LEN {
            return Err(RequisitionError::NoteTooLong);
        }

        Ok(note.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RequisitionError {
    #[error("a requisition needs a cost centre")]
    CostCentreRequired,
    #[error("a requisition needs somewhere to deliver to")]
    WarehouseRequired,
    #[error("a requisition needs at least one line")]
    NoLines,
    #[error("a line needs an item")]
    ItemRequired,
    #[error("a line needs a unit")]
    UnitRequired,
    #[error("a line needs a description")]
    DescriptionRequired,
    #[error("a line needs a quantity above nothing")]
    QuantityRequired,
    #[error("a description is at most 400 characters")]
    DescriptionTooLong,
    #[error("a justification is at most 2000 characters")]
    JustificationTooLong,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("goods cannot be needed before they were asked for")]
    NeededBeforeRaised,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that estimate is not an amount")]
    Money(#[from] MoneyError),
    #[error("only a draft requisition can be edited")]
    NotEditable,
    #[error("only a submitted requisition can be decided")]
    NotDecidable,
    #[error("only an approved requisition can be ordered from")]
    NotOrderable,
    #[error("a decision has to say why")]
    DecisionNeedsAReason,
    #[error("that cost centre is not one this workspace charges to")]
    UnknownCostCentre,
    #[error("that item cannot be purchased")]
    NotPurchasable,
    #[error("nothing on this requisition is still to be ordered")]
    NothingOutstanding,
}

impl RequisitionError {
    pub fn field(self) -> &'static str {
        match self {
            Self::CostCentreRequired | Self::UnknownCostCentre => "cost_centre_id",
            Self::WarehouseRequired => "warehouse_id",
            Self::NoLines | Self::ItemRequired | Self::NotPurchasable | Self::NothingOutstanding => {
                "lines"
            }
            Self::UnitRequired => "unit_id",
            Self::QuantityRequired | Self::Quantity(_) => "quantity",
            Self::Money(_) => "estimate",
            Self::DescriptionRequired | Self::DescriptionTooLong => "description",
            Self::JustificationTooLong => "justification",
            Self::NoteTooLong | Self::DecisionNeedsAReason => "note",
            Self::NeededBeforeRaised => "needed_by",
            Self::NotEditable | Self::NotDecidable | Self::NotOrderable => "state",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CostCentreRequired => msg!("requisitions.error.cost_centre_required"),
            Self::WarehouseRequired => msg!("requisitions.error.warehouse_required"),
            Self::NoLines => msg!("requisitions.error.no_lines"),
            Self::ItemRequired => msg!("requisitions.error.item_required"),
            Self::UnitRequired => msg!("requisitions.error.unit_required"),
            Self::DescriptionRequired => msg!("requisitions.error.description_required"),
            Self::QuantityRequired => msg!("requisitions.error.quantity_required"),
            Self::DescriptionTooLong => msg!("requisitions.error.description_too_long"),
            Self::JustificationTooLong => msg!("requisitions.error.justification_too_long"),
            Self::NoteTooLong => msg!("requisitions.error.note_too_long"),
            Self::NeededBeforeRaised => msg!("requisitions.error.needed_before_raised"),
            Self::Quantity(_) => msg!("requisitions.error.quantity_not_a_number"),
            Self::Money(_) => msg!("requisitions.error.estimate_not_an_amount"),
            Self::NotEditable => msg!("requisitions.error.not_editable"),
            Self::NotDecidable => msg!("requisitions.error.not_decidable"),
            Self::NotOrderable => msg!("requisitions.error.not_orderable"),
            Self::DecisionNeedsAReason => msg!("requisitions.error.decision_needs_a_reason"),
            Self::UnknownCostCentre => msg!("requisitions.error.unknown_cost_centre"),
            Self::NotPurchasable => msg!("requisitions.error.not_purchasable"),
            Self::NothingOutstanding => msg!("requisitions.error.nothing_outstanding"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quantity(units: i64) -> Quantity {
        Quantity::from_units(units).expect("a real quantity")
    }

    fn centre() -> CostCentre {
        CostCentre {
            id: Uuid::from_u128(5),
            code: "DEPT-004".to_owned(),
            name: "Finance".to_owned(),
        }
    }

    fn line(quantity_units: i64, ordered_units: i64) -> RequisitionLine {
        RequisitionLine {
            id: Uuid::from_u128(1),
            line_no: 1,
            variant_id: Uuid::from_u128(7),
            variant_code: "ITM-00001".to_owned(),
            description: "Printer paper".to_owned(),
            quantity: quantity(quantity_units),
            unit_id: Uuid::from_u128(8),
            unit_code: "EA".to_owned(),
            ordered: quantity(ordered_units),
            estimate: None,
            note: None,
        }
    }

    fn requisition(lines: Vec<RequisitionLine>) -> Requisition {
        Requisition {
            id: Uuid::from_u128(2),
            number: String::new(),
            state: RequisitionState::Approved,
            cost_centre: centre(),
            warehouse_id: Uuid::from_u128(3),
            warehouse_name: "WH".to_owned(),
            raised_on: NaiveDate::from_ymd_opt(2026, 9, 9).expect("a real date"),
            needed_by: None,
            justification: None,
            note: None,
            decision: None,
            raised_by: None,
            raised_by_name: None,
            lines,
        }
    }

    fn filled_line() -> RequisitionLineInput {
        RequisitionLineInput {
            variant_id: Some(Uuid::from_u128(7)),
            unit_id: Some(Uuid::from_u128(8)),
            description: "Printer paper".to_owned(),
            ..RequisitionLineInput::blank()
        }
    }

    fn input() -> RequisitionInput {
        RequisitionInput {
            id: None,
            cost_centre_id: Some(Uuid::from_u128(5)),
            warehouse_id: Some(Uuid::from_u128(3)),
            raised_on: NaiveDate::from_ymd_opt(2026, 9, 9).expect("a real date"),
            needed_by: None,
            justification: String::new(),
            note: String::new(),
            lines: vec![filled_line()],
        }
    }

    #[test]
    fn how_much_has_been_ordered_is_read_off_the_lines() {
        assert_eq!(
            requisition(vec![line(10, 0), line(4, 0)]).order_progress(),
            OrderProgress::Nothing
        );
        assert_eq!(
            requisition(vec![line(10, 10), line(4, 1)]).order_progress(),
            OrderProgress::Partly
        );
        assert_eq!(
            requisition(vec![line(10, 10), line(4, 4)]).order_progress(),
            OrderProgress::Everything
        );
    }

    #[test]
    fn a_requisition_with_no_lines_has_had_nothing_ordered() {
        // Rather than "everything", which is what an `all()` over an empty list
        // says and which would show a blank request as fully bought.
        assert_eq!(
            requisition(Vec::new()).order_progress(),
            OrderProgress::Nothing
        );
    }

    #[test]
    fn only_an_approved_requisition_with_something_left_offers_an_order() {
        let mut open = requisition(vec![line(10, 4)]);
        assert!(open.can_raise_an_order());

        open.state = RequisitionState::Submitted;
        assert!(!open.can_raise_an_order());

        let done = requisition(vec![line(10, 10)]);
        assert!(!done.can_raise_an_order());
    }

    #[test]
    fn a_total_is_offered_only_when_every_line_was_estimated() {
        let priced = |amount: &str| {
            Money::parse(phonix_core::locale::Currency::USD, amount).expect("a real amount")
        };

        let both = requisition(vec![
            RequisitionLine {
                estimate: Some(priced("2.50")),
                ..line(4, 0)
            },
            RequisitionLine {
                estimate: Some(priced("10.00")),
                ..line(2, 0)
            },
        ]);
        assert_eq!(both.estimate(), Some(priced("30.00")));

        // One priced line out of two is a number that invites the wrong
        // decision, so there is no number.
        let half = requisition(vec![
            RequisitionLine {
                estimate: Some(priced("2.50")),
                ..line(4, 0)
            },
            line(2, 0),
        ]);
        assert_eq!(half.estimate(), None);
    }

    #[test]
    fn a_requisition_needs_a_cost_centre() {
        // The overruled decision, pinned: a workspace with no HR app cannot
        // raise one of these, and that is the intended behaviour rather than an
        // accident of a missing picker.
        let unpaid = RequisitionInput {
            cost_centre_id: None,
            ..input()
        };

        assert_eq!(unpaid.check(), Err(RequisitionError::CostCentreRequired));
    }

    #[test]
    fn every_line_names_an_item_and_a_unit() {
        let described = RequisitionInput {
            lines: vec![RequisitionLineInput {
                description: "A left-handed widget, about 30mm".to_owned(),
                ..RequisitionLineInput::blank()
            }],
            ..input()
        };
        assert_eq!(described.check(), Err(RequisitionError::ItemRequired));

        let unitless = RequisitionInput {
            lines: vec![RequisitionLineInput {
                unit_id: None,
                ..filled_line()
            }],
            ..input()
        };
        assert_eq!(unitless.check(), Err(RequisitionError::UnitRequired));
    }

    #[test]
    fn a_line_still_has_to_be_called_something() {
        // The item's name is defaulted in by the form, so this is the row where
        // somebody cleared it.
        let nameless = RequisitionInput {
            lines: vec![RequisitionLineInput {
                description: "   ".to_owned(),
                ..filled_line()
            }],
            ..input()
        };

        assert_eq!(nameless.check(), Err(RequisitionError::DescriptionRequired));
    }

    #[test]
    fn the_empty_row_a_form_always_shows_is_dropped_rather_than_refused() {
        let with_trailer = RequisitionInput {
            lines: vec![filled_line(), RequisitionLineInput::blank()],
            ..input()
        };

        assert_eq!(with_trailer.check().expect("checks").lines.len(), 1);
    }

    #[test]
    fn a_requisition_of_nothing_but_empty_rows_is_refused() {
        let blank = RequisitionInput {
            lines: vec![RequisitionLineInput::blank()],
            ..input()
        };

        assert_eq!(blank.check(), Err(RequisitionError::NoLines));
    }

    #[test]
    fn goods_cannot_be_needed_before_they_were_asked_for() {
        let backwards = RequisitionInput {
            needed_by: NaiveDate::from_ymd_opt(2026, 9, 8),
            ..input()
        };

        assert_eq!(backwards.check(), Err(RequisitionError::NeededBeforeRaised));
    }

    #[test]
    fn both_answers_have_to_say_why() {
        // The overruled asymmetry, pinned. An approval used to be allowed to
        // stay silent.
        let silent = DecisionInput {
            note: "   ".to_owned(),
        };
        assert_eq!(silent.check(), Err(RequisitionError::DecisionNeedsAReason));

        let spoken = DecisionInput {
            note: "  Budget is spent for this quarter.  ".to_owned(),
        };
        assert_eq!(
            spoken.check(),
            Ok("Budget is spent for this quarter.".to_owned())
        );
    }

    #[test]
    fn a_state_round_trips_through_the_column_it_is_stored_in() {
        for state in RequisitionState::ALL {
            assert_eq!(RequisitionState::parse(state.as_str()), Some(*state));
        }

        assert_eq!(RequisitionState::parse("ordered"), None);
    }

    #[test]
    fn every_state_but_draft_carries_a_number() {
        // The schema says the same thing in
        // `requisitions_numbered_when_submitted`, and a rejected requisition
        // keeps its number because somebody was quoted it.
        assert!(!RequisitionState::Draft.is_numbered());
        assert!(RequisitionState::Rejected.is_numbered());
        assert!(RequisitionState::Cancelled.is_numbered());
    }
}
