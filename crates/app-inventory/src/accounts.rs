//! Which account a stock posting lands on, when the workspace wants to say.
//!
//! # Three places to look, in this order
//!
//! ```text
//!   the item        empty for almost every item
//!   its category    where a workspace that cares sets it
//!   the role        Books' own default, in books.account_roles
//! ```
//!
//! Category first with an item override is the arrangement every serious
//! system converges on, and both extremes are worse. Item-only means somebody
//! has to set three accounts on four thousand items, and nobody does it, so the
//! ones they miss post nowhere. Category-only means the single item that needs
//! its own revenue account forces a whole category into existence to hold it.
//!
//! The default is that **nothing here is set**, and every posting falls through
//! to the role. A workspace that never opens this screen is not broken; it is
//! using the chart Books seeded.
//!
//! # Why an account id may sit in this schema at all
//!
//! It looks like Inventory holding a key into Books. It is not, and the
//! distinction is the one ADR 0001 draws: the column is a **bare id with no
//! foreign key**, exactly as `books.invoices` carries a `master.parties` id.
//! Nothing in this schema joins to `books.accounts`, the ledger verifies the id
//! when a posting arrives, and dropping the `books` schema leaves rows that
//! resolve to nothing rather than a database that will not drop.
//!
//! The name beside each id is a **snapshot**, for a screen that has to draw the
//! mapping without asking Books for a row per line. It is refreshed when the
//! mapping is edited and never trusted for anything but display.

use phonix_ports::ledger::AccountRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One override: an account id, and what it was called when it was chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRef {
    pub account_id: Uuid,
    /// Snapshot. For display only - see the module header.
    pub number: String,
    /// Snapshot. For display only.
    pub name: String,
}

/// The accounts a category or an item may name for itself.
///
/// Six, and they are the six every system in this space has. Each is `None`
/// unless somebody set it, and `None` means "whatever the role resolves to".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountOverrides {
    /// Stock on hand, at cost. The control account this item reconciles to.
    pub stock_valuation: Option<AccountRef>,
    /// Where a receipt's other side sits until the supplier's invoice arrives.
    pub stock_input: Option<AccountRef>,
    /// Where a delivery's other side sits until the customer is invoiced.
    pub stock_output: Option<AccountRef>,
    /// Ordered price against billed price.
    pub price_difference: Option<AccountRef>,
    /// What this sells for.
    pub revenue: Option<AccountRef>,
    /// What it cost when it was sold.
    pub cost_of_sales: Option<AccountRef>,
}

impl AccountOverrides {
    /// Whether anybody has set anything here. Drives the "using the defaults"
    /// note on the screen, which is what most rows say.
    pub const fn is_empty(&self) -> bool {
        self.stock_valuation.is_none()
            && self.stock_input.is_none()
            && self.stock_output.is_none()
            && self.price_difference.is_none()
            && self.revenue.is_none()
            && self.cost_of_sales.is_none()
    }

    /// The override for one role, or `None` where this level does not care.
    ///
    /// The roles with no slot fall through deliberately. Accounts payable is
    /// the supplier's, not the item's; landed cost, inventory adjustment and
    /// in-transit are workspace-wide policy and would be a different number per
    /// item for no reason anybody could explain afterwards.
    pub fn for_role(&self, role: AccountRole) -> Option<&AccountRef> {
        match role {
            AccountRole::Inventory => self.stock_valuation.as_ref(),
            AccountRole::GoodsReceivedNotInvoiced => self.stock_input.as_ref(),
            AccountRole::GoodsDeliveredNotInvoiced => self.stock_output.as_ref(),
            AccountRole::PurchasePriceVariance => self.price_difference.as_ref(),
            AccountRole::Revenue => self.revenue.as_ref(),
            AccountRole::CostOfSales => self.cost_of_sales.as_ref(),
            AccountRole::AccountsPayable
            | AccountRole::LandedCost
            | AccountRole::InventoryAdjustment
            | AccountRole::InventoryInTransit => None,
        }
    }

    /// Every role this type can carry an override for, for a form to draw.
    pub const OVERRIDABLE: &'static [AccountRole] = &[
        AccountRole::Inventory,
        AccountRole::GoodsReceivedNotInvoiced,
        AccountRole::GoodsDeliveredNotInvoiced,
        AccountRole::PurchasePriceVariance,
        AccountRole::Revenue,
        AccountRole::CostOfSales,
    ];
}

/// Where a posting for `role` should land, given what the item and its category
/// say.
///
/// `None` means nobody overrode it, and the caller passes the role on its own
/// so Books resolves it from `books.account_roles`. That is the ordinary case
/// and the one the whole design is arranged around.
pub fn resolve<'a>(
    role: AccountRole,
    item: &'a AccountOverrides,
    category: &'a AccountOverrides,
) -> Option<&'a AccountRef> {
    item.for_role(role).or_else(|| category.for_role(role))
}

/// Build the `account_id` a [`Posting`](phonix_ports::ledger::Posting) carries.
///
/// The one function a caller assembling a journal needs, so the fallback order
/// lives here rather than being re-implemented per document type.
pub fn account_for(
    role: AccountRole,
    item: &AccountOverrides,
    category: &AccountOverrides,
) -> Option<Uuid> {
    resolve(role, item, category).map(|chosen| chosen.account_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(n: u128, number: &str) -> AccountRef {
        AccountRef {
            account_id: Uuid::from_u128(n),
            number: number.to_owned(),
            name: format!("Account {number}"),
        }
    }

    #[test]
    fn nothing_set_anywhere_falls_through_to_the_role() {
        // The ordinary case, and the one that has to keep working: a workspace
        // that never opens the mapping screen posts to Books' own defaults.
        let empty = AccountOverrides::default();

        assert!(empty.is_empty());
        assert_eq!(account_for(AccountRole::Inventory, &empty, &empty), None);
    }

    #[test]
    fn a_category_answers_for_every_item_filed_under_it() {
        let category = AccountOverrides {
            stock_valuation: Some(account(1, "1200")),
            ..AccountOverrides::default()
        };

        assert_eq!(
            account_for(AccountRole::Inventory, &AccountOverrides::default(), &category),
            Some(Uuid::from_u128(1))
        );
    }

    #[test]
    fn an_item_overrides_its_category() {
        let category = AccountOverrides {
            stock_valuation: Some(account(1, "1200")),
            revenue: Some(account(2, "4000")),
            ..AccountOverrides::default()
        };
        let item = AccountOverrides {
            revenue: Some(account(3, "4010")),
            ..AccountOverrides::default()
        };

        // The one it set.
        assert_eq!(
            account_for(AccountRole::Revenue, &item, &category),
            Some(Uuid::from_u128(3))
        );
        // The one it did not.
        assert_eq!(
            account_for(AccountRole::Inventory, &item, &category),
            Some(Uuid::from_u128(1))
        );
    }

    #[test]
    fn the_roles_that_are_not_the_items_business_never_resolve_here() {
        // Accounts payable belongs to the supplier and in-transit is workspace
        // policy. An item-level answer to either would be a number nobody could
        // explain the following March.
        let everything = AccountOverrides {
            stock_valuation: Some(account(1, "1200")),
            stock_input: Some(account(2, "2010")),
            stock_output: Some(account(3, "2020")),
            price_difference: Some(account(4, "5230")),
            revenue: Some(account(5, "4000")),
            cost_of_sales: Some(account(6, "5000")),
        };

        for role in [
            AccountRole::AccountsPayable,
            AccountRole::LandedCost,
            AccountRole::InventoryAdjustment,
            AccountRole::InventoryInTransit,
        ] {
            assert_eq!(everything.for_role(role), None, "{role:?}");
        }

        // And every role a form offers does resolve, so the two lists agree.
        for role in AccountOverrides::OVERRIDABLE {
            assert!(everything.for_role(*role).is_some(), "{role:?}");
        }
    }
}
