//! Consolidation: eleven departments wanting printer paper, bought once.
//!
//! # Why this is a document and not a button
//!
//! Grouping demand by item is supplier-agnostic; deciding who to buy each item
//! from is not. Between those two facts sits a decision somebody makes, and one
//! consolidation routinely becomes *several* purchase orders because no single
//! supplier stocks everything eleven departments asked for.
//!
//! That is the failure the Dynamics 365 literature describes: consolidation
//! that merges across vendors and delivery addresses, leaving a buyer to
//! rebuild the request by hand late in the process and a receiving end that
//! cannot split what arrives. So the **supplier is chosen per line**, the
//! **warehouse is fixed for the document**, and confirming splits the thing
//! into one order per supplier.
//!
//! # The buyer may order more than was asked for, and it is visible
//!
//! Demand is 47 reams and a case is 50. Rounding up is the ordinary case, and
//! the three extra reams belong to no requisition - they are stock. Systems
//! that refuse the rounding make the buyer lie; systems that quietly attach the
//! excess to the last requisition charge a department for something it did not
//! ask for.
//!
//! Here the difference is carried explicitly: [`ConsolidationLine::demand`] is
//! what was outstanding when the line was drawn, `quantity` is what will be
//! bought, and [`ConsolidationLine::beyond_demand`] is the gap. After confirm
//! the same fact lives in `purchase_order_line_allocation.unallocated`.
//!
//! # The allocation is worked out at confirm, never remembered
//!
//! A consolidation drafted last week and confirmed today must allocate against
//! the demand outstanding *now*. Requisitions get approved and withdrawn in
//! between, and a draft that remembered its inputs would order things nobody is
//! waiting for any more. `demand` is therefore a *snapshot for the screen* -
//! it is what makes a stale draft visible - and never the basis of the write.
//!
//! Oldest request first, because the department that has waited longest should
//! be the one served by the first delivery.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::money::{Money, MoneyError};
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::purchase::SupplierSnapshot;
use crate::quantity::{Quantity, QuantityError};

pub const MAX_NOTE_LEN: usize = 2000;
pub const MAX_LINE_DESCRIPTION_LEN: usize = 400;

/// Where a consolidation is in its life.
///
/// There is no `ordered` state. Confirming *is* the ordering, and what came of
/// it is the orders themselves - which name this document, so "what did this
/// become" is a query rather than a column that can disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationState {
    Draft,
    Confirmed,
    Cancelled,
}

impl ConsolidationState {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Confirmed, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Confirmed => "confirmed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == raw)
    }

    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    pub const fn is_confirmed(self) -> bool {
        matches!(self, Self::Confirmed)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("consolidations.state.draft"),
            Self::Confirmed => msg!("consolidations.state.confirmed"),
            Self::Cancelled => msg!("consolidations.state.cancelled"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consolidation {
    pub id: Uuid,
    /// `CON-2026-00042`. Empty until it is confirmed.
    pub number: String,
    pub state: ConsolidationState,
    pub warehouse_id: Uuid,
    pub warehouse_name: String,
    pub raised_on: NaiveDate,
    pub note: Option<String>,
    pub raised_by_name: Option<String>,
    pub lines: Vec<ConsolidationLine>,
    /// The orders confirming it produced. Empty while it is a draft, and read
    /// back rather than stored - see [`ConsolidationState`].
    pub orders: Vec<RaisedOrder>,
}

impl Consolidation {
    /// How many orders confirming this would raise: one per distinct supplier.
    ///
    /// Shown before the act, because "this will create three orders" is the
    /// thing a buyer most wants to know and the thing a single Confirm button
    /// otherwise hides.
    pub fn supplier_count(&self) -> usize {
        let mut seen: Vec<Uuid> = Vec::new();

        for line in &self.lines {
            if let Some(supplier) = &line.supplier {
                if !seen.contains(&supplier.party_id) {
                    seen.push(supplier.party_id);
                }
            }
        }

        seen.len()
    }

    /// Lines with nobody to buy them from. Confirming is refused while any
    /// remain, because an order cannot be raised for them and silently dropping
    /// them is how demand disappears.
    pub fn unsourced(&self) -> Vec<&ConsolidationLine> {
        self.lines
            .iter()
            .filter(|line| line.supplier.is_none())
            .collect()
    }

    pub fn can_be_confirmed(&self) -> bool {
        self.state.is_editable() && !self.lines.is_empty() && self.unsourced().is_empty()
    }

    /// What a document calls this.
    pub fn label(&self) -> String {
        if self.number.is_empty() {
            format!("{} · {}", self.raised_on, self.warehouse_name)
        } else {
            self.number.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationLine {
    pub id: Uuid,
    pub line_no: i32,
    pub variant_id: Uuid,
    pub variant_code: String,
    pub description: String,
    /// What will be bought, in the item's stock unit.
    pub quantity: Quantity,
    /// What was outstanding when the line was drawn - a snapshot for the
    /// screen, never the basis of the allocation. See the module docs.
    pub demand: Quantity,
    pub unit_code: String,
    pub supplier: Option<SupplierSnapshot>,
    pub unit_price: Option<Money>,
    pub note: Option<String>,
    /// What is outstanding *now*, read back beside the snapshot so a stale
    /// draft shows as one. `None` on a confirmed document, where the question
    /// has stopped being interesting.
    pub demand_now: Option<Quantity>,
}

impl ConsolidationLine {
    /// How much of this line nobody asked for: the buyer's own decision.
    ///
    /// Zero rather than negative when the buyer is ordering short of demand -
    /// that is [`Self::short_of_demand`], and the two are different facts.
    pub fn beyond_demand(&self) -> Quantity {
        match self.quantity.checked_sub(self.demand) {
            Ok(extra) if extra.is_positive() => extra,
            _ => Quantity::ZERO,
        }
    }

    /// How much of the demand this line is leaving behind, which stays
    /// outstanding for a later consolidation. Not an error: buying half of what
    /// was asked for is an ordinary purchasing decision.
    pub fn short_of_demand(&self) -> Quantity {
        match self.demand.checked_sub(self.quantity) {
            Ok(short) if short.is_positive() => short,
            _ => Quantity::ZERO,
        }
    }

    /// Whether the demand has moved since this line was drawn.
    ///
    /// The stale-draft warning. A requisition approved or withdrawn between the
    /// draft and the confirm changes what the allocation will do, and the buyer
    /// should see that before pressing the button rather than afterwards.
    pub fn demand_has_moved(&self) -> bool {
        self.demand_now
            .is_some_and(|now| !now.compare(self.demand).is_eq())
    }

    /// `quantity * unit_price`, where the buyer priced it.
    pub fn net(&self) -> Option<Money> {
        let price = self.unit_price?;

        price
            .scale_by(
                self.quantity.scaled(),
                crate::quantity::SCALE_FACTOR,
                phonix_core::money::Rounding::HalfUp,
            )
            .ok()
    }
}

/// An order a consolidation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaisedOrder {
    pub id: Uuid,
    pub number: String,
    pub supplier_name: String,
    pub currency: String,
    pub net: Money,
    pub line_count: i64,
}

/// One row of the consolidations grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationSummary {
    pub id: Uuid,
    pub number: String,
    pub state: ConsolidationState,
    pub warehouse_name: String,
    pub raised_on: NaiveDate,
    pub raised_by_name: Option<String>,
    pub line_count: i64,
    /// How many suppliers the lines name, which is how many orders confirming
    /// it did or would raise.
    pub supplier_count: i64,
    pub order_count: i64,
}

/// The editable part of a consolidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationInput {
    pub id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub raised_on: NaiveDate,
    pub note: String,
    pub lines: Vec<ConsolidationLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationLineInput {
    pub id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub description: String,
    pub quantity: String,
    /// Carried through the form untouched so the screen can keep showing what
    /// was outstanding when the line was drawn.
    pub demand: String,
    pub supplier_id: Option<Uuid>,
    pub unit_price: String,
    pub note: String,
}

impl ConsolidationLineInput {
    /// A line drawn from a demand row: buy exactly what was asked for, from
    /// nobody in particular yet.
    pub fn from_demand(
        variant_id: Uuid,
        description: String,
        outstanding: Quantity,
    ) -> Self {
        Self {
            id: None,
            variant_id: Some(variant_id),
            description,
            quantity: outstanding.to_display_string(),
            demand: outstanding.to_display_string(),
            supplier_id: None,
            unit_price: String::new(),
            note: String::new(),
        }
    }
}

impl ConsolidationInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            warehouse_id: None,
            raised_on: today,
            note: String::new(),
            lines: Vec::new(),
        }
    }

    pub fn from_consolidation(consolidation: &Consolidation) -> Self {
        Self {
            id: Some(consolidation.id),
            warehouse_id: Some(consolidation.warehouse_id),
            raised_on: consolidation.raised_on,
            note: consolidation.note.clone().unwrap_or_default(),
            lines: consolidation
                .lines
                .iter()
                .map(|line| ConsolidationLineInput {
                    id: Some(line.id),
                    variant_id: Some(line.variant_id),
                    description: line.description.clone(),
                    quantity: line.quantity.to_display_string(),
                    demand: line.demand.to_display_string(),
                    supplier_id: line.supplier.as_ref().map(|s| s.party_id),
                    unit_price: line
                        .unit_price
                        .map(|price| price.to_storage_string())
                        .unwrap_or_default(),
                    note: line.note.clone().unwrap_or_default(),
                })
                .collect(),
        }
    }

    /// Everything that can be decided without the database.
    pub fn check(&self) -> Result<Checked, ConsolidationError> {
        let warehouse_id = self
            .warehouse_id
            .ok_or(ConsolidationError::WarehouseRequired)?;

        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(ConsolidationError::NoteTooLong);
        }

        let mut lines = Vec::new();
        let mut seen: Vec<Uuid> = Vec::new();

        for line in &self.lines {
            if is_blank(line) {
                continue;
            }

            let variant_id = line.variant_id.ok_or(ConsolidationError::ItemRequired)?;

            // One item once. Two lines for the same thing is the keying mistake
            // that produces two orders to two suppliers for one requirement -
            // and the schema refuses it too.
            if seen.contains(&variant_id) {
                return Err(ConsolidationError::ItemTwice);
            }
            seen.push(variant_id);

            let description = line.description.trim();
            if description.is_empty() {
                return Err(ConsolidationError::DescriptionRequired);
            }
            if description.chars().count() > MAX_LINE_DESCRIPTION_LEN {
                return Err(ConsolidationError::DescriptionTooLong);
            }

            let quantity = Quantity::parse(&line.quantity)?;
            if !quantity.is_positive() {
                return Err(ConsolidationError::QuantityRequired);
            }

            // A blank demand is a line the buyer added by hand rather than drew
            // from the demand screen: nobody asked for it, so all of it is
            // beyond demand.
            let demand = if line.demand.trim().is_empty() {
                Quantity::ZERO
            } else {
                Quantity::parse(&line.demand)?
            };

            if line.note.chars().count() > MAX_NOTE_LEN {
                return Err(ConsolidationError::NoteTooLong);
            }

            lines.push(CheckedLine {
                id: line.id,
                variant_id,
                description: description.to_owned(),
                quantity,
                demand,
                supplier_id: line.supplier_id,
                unit_price: line.unit_price.trim().to_owned(),
                note: non_empty(&line.note),
            });
        }

        if lines.is_empty() {
            return Err(ConsolidationError::NoLines);
        }

        Ok(Checked {
            id: self.id,
            warehouse_id,
            raised_on: self.raised_on,
            note: non_empty(&self.note),
            lines,
        })
    }
}

fn is_blank(line: &ConsolidationLineInput) -> bool {
    line.variant_id.is_none()
        && line.description.trim().is_empty()
        && line.unit_price.trim().is_empty()
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub id: Option<Uuid>,
    pub warehouse_id: Uuid,
    pub raised_on: NaiveDate,
    pub note: Option<String>,
    pub lines: Vec<CheckedLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedLine {
    pub id: Option<Uuid>,
    pub variant_id: Uuid,
    pub description: String,
    pub quantity: Quantity,
    pub demand: Quantity,
    pub supplier_id: Option<Uuid>,
    /// Still text: parsing it needs the supplier's currency, which is the
    /// service's to establish.
    pub unit_price: String,
    pub note: Option<String>,
}

/// One requisition line an order line was raised for.
///
/// What `purchase_order_line_sources` holds, and what lets the cost of a
/// receipt be split back across the cost centres that asked for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    pub requisition_line_id: Uuid,
    pub requisition_id: Uuid,
    pub requisition_number: String,
    pub cost_centre_id: Uuid,
    pub cost_centre_name: String,
    pub quantity: Quantity,
}

/// How an order line's quantity divides between the requisitions it was raised
/// for and the part the buyer bought for stock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineAllocation {
    pub order_line_id: Uuid,
    pub description: String,
    pub quantity_stock: Quantity,
    pub allocated: Quantity,
    /// Charged to nobody. Not an error - see the module docs.
    pub unallocated: Quantity,
    pub sources: Vec<Allocation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConsolidationError {
    #[error("a consolidation needs a warehouse")]
    WarehouseRequired,
    #[error("a consolidation needs at least one line")]
    NoLines,
    #[error("a line needs an item")]
    ItemRequired,
    #[error("one item cannot be on two lines of the same consolidation")]
    ItemTwice,
    #[error("a line needs a description")]
    DescriptionRequired,
    #[error("a description is at most 400 characters")]
    DescriptionTooLong,
    #[error("a line needs a quantity above nothing")]
    QuantityRequired,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("every line needs a supplier before this can be ordered")]
    SupplierRequired,
    #[error("that party is not a supplier")]
    NotASupplier,
    #[error("that quantity is not a number")]
    Quantity(#[from] QuantityError),
    #[error("that price is not an amount")]
    Money(#[from] MoneyError),
    #[error("only a draft consolidation can be edited")]
    NotEditable,
    #[error("that item cannot be purchased")]
    NotPurchasable,
}

impl ConsolidationError {
    pub fn field(self) -> &'static str {
        match self {
            Self::WarehouseRequired => "warehouse_id",
            Self::NoLines
            | Self::ItemRequired
            | Self::ItemTwice
            | Self::NotPurchasable => "lines",
            Self::SupplierRequired | Self::NotASupplier => "supplier_id",
            Self::QuantityRequired | Self::Quantity(_) => "quantity",
            Self::Money(_) => "unit_price",
            Self::DescriptionRequired | Self::DescriptionTooLong => "description",
            Self::NoteTooLong => "note",
            Self::NotEditable => "state",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::WarehouseRequired => msg!("consolidations.error.warehouse_required"),
            Self::NoLines => msg!("consolidations.error.no_lines"),
            Self::ItemRequired => msg!("consolidations.error.item_required"),
            Self::ItemTwice => msg!("consolidations.error.item_twice"),
            Self::DescriptionRequired => msg!("consolidations.error.description_required"),
            Self::DescriptionTooLong => msg!("consolidations.error.description_too_long"),
            Self::QuantityRequired => msg!("consolidations.error.quantity_required"),
            Self::NoteTooLong => msg!("consolidations.error.note_too_long"),
            Self::SupplierRequired => msg!("consolidations.error.supplier_required"),
            Self::NotASupplier => msg!("consolidations.error.not_a_supplier"),
            Self::Quantity(_) => msg!("consolidations.error.quantity_not_a_number"),
            Self::Money(_) => msg!("consolidations.error.price_not_an_amount"),
            Self::NotEditable => msg!("consolidations.error.not_editable"),
            Self::NotPurchasable => msg!("consolidations.error.not_purchasable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quantity(units: i64) -> Quantity {
        Quantity::from_units(units).expect("a real quantity")
    }

    fn supplier(id: u128) -> SupplierSnapshot {
        SupplierSnapshot {
            party_id: Uuid::from_u128(id),
            code: format!("SUP{id}"),
            name: format!("Supplier {id}"),
        }
    }

    fn line(quantity_units: i64, demand_units: i64, from: Option<u128>) -> ConsolidationLine {
        ConsolidationLine {
            id: Uuid::from_u128(1),
            line_no: 1,
            variant_id: Uuid::from_u128(7),
            variant_code: "ITM-00001".to_owned(),
            description: "Printer paper".to_owned(),
            quantity: quantity(quantity_units),
            demand: quantity(demand_units),
            unit_code: "EA".to_owned(),
            supplier: from.map(supplier),
            unit_price: None,
            note: None,
            demand_now: None,
        }
    }

    fn consolidation(lines: Vec<ConsolidationLine>) -> Consolidation {
        Consolidation {
            id: Uuid::from_u128(2),
            number: String::new(),
            state: ConsolidationState::Draft,
            warehouse_id: Uuid::from_u128(3),
            warehouse_name: "WH".to_owned(),
            raised_on: NaiveDate::from_ymd_opt(2026, 9, 9).expect("a real date"),
            note: None,
            raised_by_name: None,
            lines,
            orders: Vec::new(),
        }
    }

    fn input() -> ConsolidationInput {
        ConsolidationInput {
            id: None,
            warehouse_id: Some(Uuid::from_u128(3)),
            raised_on: NaiveDate::from_ymd_opt(2026, 9, 9).expect("a real date"),
            note: String::new(),
            lines: vec![ConsolidationLineInput::from_demand(
                Uuid::from_u128(7),
                "Printer paper".to_owned(),
                quantity(47),
            )],
        }
    }

    #[test]
    fn rounding_a_case_up_is_beyond_demand_rather_than_an_error() {
        // 47 asked for, a case is 50. The three are stock, charged to nobody.
        let rounded = line(50, 47, Some(1));

        assert_eq!(rounded.beyond_demand(), quantity(3));
        assert_eq!(rounded.short_of_demand(), Quantity::ZERO);
    }

    #[test]
    fn buying_less_than_was_asked_for_leaves_the_rest_outstanding() {
        let partial = line(20, 47, Some(1));

        assert_eq!(partial.short_of_demand(), quantity(27));
        assert_eq!(partial.beyond_demand(), Quantity::ZERO);
    }

    #[test]
    fn one_consolidation_becomes_one_order_per_supplier() {
        let three = consolidation(vec![
            line(10, 10, Some(1)),
            line(4, 4, Some(2)),
            line(6, 6, Some(1)),
        ]);

        assert_eq!(three.supplier_count(), 2);
    }

    #[test]
    fn a_line_with_nobody_to_buy_it_from_stops_the_whole_document() {
        // Silently dropping it would be how demand disappears.
        let unsourced = consolidation(vec![line(10, 10, Some(1)), line(4, 4, None)]);

        assert_eq!(unsourced.unsourced().len(), 1);
        assert!(!unsourced.can_be_confirmed());

        let sourced = consolidation(vec![line(10, 10, Some(1))]);
        assert!(sourced.can_be_confirmed());
    }

    #[test]
    fn an_empty_consolidation_cannot_be_confirmed() {
        assert!(!consolidation(Vec::new()).can_be_confirmed());
    }

    #[test]
    fn a_draft_whose_demand_has_moved_says_so() {
        let steady = ConsolidationLine {
            demand_now: Some(quantity(47)),
            ..line(50, 47, Some(1))
        };
        assert!(!steady.demand_has_moved());

        // Somebody's requisition was approved after the draft was drawn.
        let moved = ConsolidationLine {
            demand_now: Some(quantity(61)),
            ..line(50, 47, Some(1))
        };
        assert!(moved.demand_has_moved());
    }

    #[test]
    fn one_item_may_not_appear_on_two_lines() {
        let twice = ConsolidationInput {
            lines: vec![
                ConsolidationLineInput::from_demand(
                    Uuid::from_u128(7),
                    "Printer paper".to_owned(),
                    quantity(10),
                ),
                ConsolidationLineInput::from_demand(
                    Uuid::from_u128(7),
                    "Printer paper again".to_owned(),
                    quantity(4),
                ),
            ],
            ..input()
        };

        assert_eq!(twice.check(), Err(ConsolidationError::ItemTwice));
    }

    #[test]
    fn a_line_added_by_hand_carries_no_demand_and_is_all_beyond_it() {
        let by_hand = ConsolidationInput {
            lines: vec![ConsolidationLineInput {
                variant_id: Some(Uuid::from_u128(7)),
                description: "Something nobody asked for".to_owned(),
                quantity: "5".to_owned(),
                demand: String::new(),
                ..ConsolidationLineInput::from_demand(
                    Uuid::from_u128(7),
                    String::new(),
                    Quantity::ZERO,
                )
            }],
            ..input()
        };

        let checked = by_hand.check().expect("checks");
        assert_eq!(checked.lines.len(), 1);
        assert_eq!(
            checked.lines.first().expect("one line").demand,
            Quantity::ZERO
        );
    }

    #[test]
    fn a_state_round_trips_through_the_column_it_is_stored_in() {
        for state in ConsolidationState::ALL {
            assert_eq!(ConsolidationState::parse(state.as_str()), Some(*state));
        }

        assert_eq!(ConsolidationState::parse("ordered"), None);
    }
}
