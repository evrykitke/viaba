//! Item categories: where costing, valuation and picking policy are decided.
//!
//! # Why the policy is on the category and not on the item
//!
//! Because it is a decision about a *kind* of stock, and a workspace that sets
//! it per item ends up with two items in the same warehouse valued two
//! different ways and a stock account nobody can tie out. A category is where
//! an accountant says "raw materials are FIFO" once.
//!
//! # The three costing methods, and what each one is for
//!
//! * [`CostingMethod::Standard`] - a cost somebody sets and reviews. Every
//!   receipt at any other price posts the difference to purchase price
//!   variance, which is the point: the variance is a number you can look at,
//!   rather than a drift you cannot.
//! * [`CostingMethod::Average`] - the running weighted average. What most
//!   distributors want and what most of them are already doing in a
//!   spreadsheet.
//! * [`CostingMethod::Fifo`] - layers, consumed oldest first. The truest, the
//!   most work, and the one an auditor asks for.
//!
//! # Automated valuation, and why it is the default
//!
//! Under [`Valuation::Manual`] the stock account moves only when somebody posts
//! a journal at period end, so the balance sheet is wrong for most of every
//! month and right for one afternoon of it. Under [`Valuation::Automated`]
//! every move that changes what the workspace holds posts its own journal
//! through the `Ledger` port as it happens, and the stock account agrees with
//! the stock report at every moment.
//!
//! The second is what a modern system should do and the first is what most of
//! them default to. This defaults to automated, and a workspace with no ledger
//! at all simply records that no journal was posted - see
//! `phonix_ports::ledger::NoLedger`.

use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_CATEGORY_NAME_LEN: usize = 120;
pub const MAX_CATEGORY_DEPTH: usize = 8;

/// How the cost of a unit is worked out when it leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostingMethod {
    /// A cost somebody sets. Differences at receipt become purchase price
    /// variance rather than quietly changing the stock value.
    Standard,
    /// Weighted average, recomputed on every receipt.
    Average,
    /// Layers, oldest consumed first.
    Fifo,
}

impl CostingMethod {
    pub const ALL: &'static [Self] = &[Self::Standard, Self::Average, Self::Fifo];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Average => "average",
            Self::Fifo => "fifo",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.as_str() == raw)
    }

    /// Whether the cost of what is on hand changes when a receipt arrives at a
    /// different price.
    ///
    /// Not under `Standard`: that is what makes the standard a standard, and
    /// what sends the difference to variance instead.
    pub const fn revalues_on_receipt(self) -> bool {
        matches!(self, Self::Average | Self::Fifo)
    }

    /// Whether the workspace has to keep a layer per receipt to answer what a
    /// unit cost. Only FIFO does; the others are one number per item.
    pub const fn needs_layers(self) -> bool {
        matches!(self, Self::Fifo)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Standard => msg!("categories.costing.standard"),
            Self::Average => msg!("categories.costing.average"),
            Self::Fifo => msg!("categories.costing.fifo"),
        }
    }
}

/// When the stock account is told what happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Valuation {
    /// Somebody posts a journal at period end. The balance sheet is right once
    /// a month.
    Manual,
    /// Every move that changes what is held posts its own journal as it
    /// happens.
    Automated,
}

impl Valuation {
    pub const ALL: &'static [Self] = &[Self::Automated, Self::Manual];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Automated => "automated",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.as_str() == raw)
    }

    pub const fn posts_a_journal(self) -> bool {
        matches!(self, Self::Automated)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Manual => msg!("categories.valuation.manual"),
            Self::Automated => msg!("categories.valuation.automated"),
        }
    }
}

/// Which units a pick reaches for first.
///
/// A policy rather than a preference: it decides what a batch of stock costs
/// under FIFO and which lot goes out under FEFO, so getting it wrong is a
/// margin error and, for anything with a shelf life, a recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalStrategy {
    /// Oldest received first. The default, and right for almost everything.
    Fifo,
    /// Newest first. For non-perishables where reaching the back of the shelf
    /// costs more than the ageing does.
    Lifo,
    /// Nearest expiry first. What anything with a shelf life needs, and it
    /// needs the item to be tracked by lot to work at all.
    Fefo,
    /// Whatever is nearest the picker. Fastest, and it makes no promise about
    /// age.
    ClosestLocation,
}

impl RemovalStrategy {
    pub const ALL: &'static [Self] = &[Self::Fifo, Self::Lifo, Self::Fefo, Self::ClosestLocation];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fifo => "fifo",
            Self::Lifo => "lifo",
            Self::Fefo => "fefo",
            Self::ClosestLocation => "closest_location",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == raw)
    }

    /// Whether this strategy can only be honoured for an item tracked by lot
    /// with an expiry date on it.
    pub const fn needs_expiry_dates(self) -> bool {
        matches!(self, Self::Fefo)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Fifo => msg!("categories.removal.fifo"),
            Self::Lifo => msg!("categories.removal.lifo"),
            Self::Fefo => msg!("categories.removal.fefo"),
            Self::ClosestLocation => msg!("categories.removal.closest"),
        }
    }
}

/// One item category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: Uuid,
    /// The full path: `All/Raw materials/Fasteners`. Derived from the tree and
    /// stored, for the same reason a location's is.
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub costing_method: CostingMethod,
    pub valuation: Valuation,
    pub removal_strategy: RemovalStrategy,
    pub is_active: bool,
}

/// One row of the category tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorySummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub costing_method: CostingMethod,
    pub valuation: Valuation,
    pub removal_strategy: RemovalStrategy,
    pub is_active: bool,
    pub depth: u16,
    pub item_count: i64,
    pub child_count: i64,
}

/// The editable part of a category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryInput {
    pub id: Option<Uuid>,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub costing_method: CostingMethod,
    pub valuation: Valuation,
    pub removal_strategy: RemovalStrategy,
    pub is_active: bool,
}

impl CategoryInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            name: String::new(),
            parent_id: None,
            costing_method: CostingMethod::Average,
            valuation: Valuation::Automated,
            removal_strategy: RemovalStrategy::Fifo,
            is_active: true,
        }
    }

    pub fn under(parent: &Category) -> Self {
        Self {
            parent_id: Some(parent.id),
            // Inherited as a starting point rather than enforced. A child that
            // must match its parent would make the tree a policy hierarchy,
            // and a workspace that wants one costing method for consumables
            // inside a category that uses another has a real reason to.
            costing_method: parent.costing_method,
            valuation: parent.valuation,
            removal_strategy: parent.removal_strategy,
            ..Self::blank()
        }
    }

    pub fn from_category(category: &Category) -> Self {
        Self {
            id: Some(category.id),
            name: category.name.clone(),
            parent_id: category.parent_id,
            costing_method: category.costing_method,
            valuation: category.valuation,
            removal_strategy: category.removal_strategy,
            is_active: category.is_active,
        }
    }

    pub fn check(&self) -> Result<Self, CategoryError> {
        let name = self.name.trim();

        if name.is_empty() {
            return Err(CategoryError::NameRequired);
        }
        if name.chars().count() > MAX_CATEGORY_NAME_LEN {
            return Err(CategoryError::NameTooLong);
        }
        if name.contains('/') {
            return Err(CategoryError::NameHasSeparator);
        }

        if let (Some(id), Some(parent_id)) = (self.id, self.parent_id)
            && id == parent_id
        {
            return Err(CategoryError::OwnParent);
        }

        Ok(Self {
            id: self.id,
            name: name.to_owned(),
            parent_id: self.parent_id,
            costing_method: self.costing_method,
            valuation: self.valuation,
            removal_strategy: self.removal_strategy,
            is_active: self.is_active,
        })
    }
}

/// What a delete answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    Deleted,
    HasChildren { count: i64 },
    /// Items are filed here. Moving them somewhere else would change how they
    /// are costed, which is not a thing a delete may do quietly.
    HasItems { count: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CategoryError {
    #[error("a category needs a name")]
    NameRequired,
    #[error("a category name is at most 120 characters")]
    NameTooLong,
    #[error("a category name may not contain a slash")]
    NameHasSeparator,
    #[error("a category cannot be its own parent")]
    OwnParent,
    #[error("that would put a category underneath itself")]
    Cycle,
    #[error("categories are at most eight levels deep")]
    TooDeep,
    #[error("changing the costing method of a category holding valued stock would restate it")]
    HasValuedStock,
}

impl CategoryError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NameRequired | Self::NameTooLong | Self::NameHasSeparator => "name",
            Self::OwnParent | Self::Cycle | Self::TooDeep => "parent_id",
            Self::HasValuedStock => "costing_method",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NameRequired => msg!("categories.error.name_required"),
            Self::NameTooLong => msg!("categories.error.name_too_long"),
            Self::NameHasSeparator => msg!("categories.error.name_has_separator"),
            Self::OwnParent => msg!("categories.error.own_parent"),
            Self::Cycle => msg!("categories.error.cycle"),
            Self::TooDeep => msg!("categories.error.too_deep"),
            Self::HasValuedStock => msg!("categories.error.has_valued_stock"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standard_cost_does_not_move_when_a_receipt_arrives_at_another_price() {
        // Which is the whole point of a standard: the difference becomes a
        // variance somebody can look at rather than a drift they cannot.
        assert!(!CostingMethod::Standard.revalues_on_receipt());
        assert!(CostingMethod::Average.revalues_on_receipt());
        assert!(CostingMethod::Fifo.revalues_on_receipt());
    }

    #[test]
    fn only_fifo_needs_a_layer_per_receipt() {
        assert!(CostingMethod::Fifo.needs_layers());
        assert!(!CostingMethod::Average.needs_layers());
        assert!(!CostingMethod::Standard.needs_layers());
    }

    #[test]
    fn a_new_category_is_automated_and_averaged() {
        // The defaults matter more than the options: this is what a workspace
        // that never opens this screen ends up with.
        let blank = CategoryInput::blank();
        assert_eq!(blank.valuation, Valuation::Automated);
        assert_eq!(blank.costing_method, CostingMethod::Average);
        assert_eq!(blank.removal_strategy, RemovalStrategy::Fifo);
        assert!(blank.valuation.posts_a_journal());
    }

    #[test]
    fn fefo_is_the_one_strategy_that_needs_dates_behind_it() {
        assert!(RemovalStrategy::Fefo.needs_expiry_dates());
        assert!(!RemovalStrategy::Fifo.needs_expiry_dates());
    }

    #[test]
    fn a_child_starts_from_its_parents_policy() {
        let parent = Category {
            id: Uuid::from_u128(1),
            code: "All".to_owned(),
            name: "All".to_owned(),
            parent_id: None,
            costing_method: CostingMethod::Fifo,
            valuation: Valuation::Automated,
            removal_strategy: RemovalStrategy::Fefo,
            is_active: true,
        };

        let child = CategoryInput::under(&parent);
        assert_eq!(child.costing_method, CostingMethod::Fifo);
        assert_eq!(child.removal_strategy, RemovalStrategy::Fefo);
        assert_eq!(child.parent_id, Some(parent.id));
    }

    #[test]
    fn every_policy_value_round_trips() {
        for method in CostingMethod::ALL {
            assert_eq!(CostingMethod::parse(method.as_str()), Some(*method));
        }
        for valuation in Valuation::ALL {
            assert_eq!(Valuation::parse(valuation.as_str()), Some(*valuation));
        }
        for strategy in RemovalStrategy::ALL {
            assert_eq!(RemovalStrategy::parse(strategy.as_str()), Some(*strategy));
        }
        assert_eq!(CostingMethod::parse("lifo"), None);
    }
}
