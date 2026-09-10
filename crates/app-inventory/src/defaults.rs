//! What an inventory looks like on the first morning.
//!
//! The shape of `config/defaults/inventory.toml`. It lives here rather than in
//! `phonix-config` because the rules about what makes it valid are Inventory's
//! rules; that crate knows only how to find the file and parse it into whatever
//! type the app asked for.
//!
//! # What is seeded, and why each piece has to be
//!
//! * **Units.** An item needs something to be counted in, and nobody wants to
//!   invent the kilogram.
//! * **The counterpart locations.** Vendors, customers, inventory loss,
//!   production and transit. Without them there is no such thing as a receipt,
//!   because a receipt is a move *from* somewhere. These are the pieces a
//!   workspace could not know it needed.
//! * **One warehouse**, with its view node and its stock location, so a box can
//!   be received on day one.
//! * **A category**, so an item has somewhere to be filed and a costing method
//!   without anybody opening the accounting screen.
//! * **The adjustment types.** A count difference, damage, expiry, a sample,
//!   a write-off. Not a gap a workspace can be expected to notice, and the
//!   argument for each having its own account is in
//!   [`crate::adjustment`].
//!
//! Everything is inserted `ON CONFLICT DO NOTHING`, so a redeploy can neither
//! put back a row somebody deleted nor overwrite one they edited.

use serde::Deserialize;

use crate::adjustment::Direction;
use crate::category::{CostingMethod, RemovalStrategy, Valuation};
use crate::location::LocationKind;
use crate::unit::{self, UnitClass};

/// The whole file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default)]
    pub unit: Vec<DefaultUnit>,
    #[serde(default)]
    pub location: Vec<DefaultLocation>,
    #[serde(default)]
    pub warehouse: Vec<DefaultWarehouse>,
    #[serde(default)]
    pub category: Vec<DefaultCategory>,
    #[serde(default)]
    pub adjustment_type: Vec<DefaultAdjustmentType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultUnit {
    pub code: String,
    pub name: String,
    pub class: UnitClass,
    /// As typed: `1`, `1000`, `0.453592`. One means this is the class's base.
    pub factor: String,
}

/// One of the locations that is not a place.
///
/// Only the counterparts are declared here. A warehouse's own locations are
/// derived from its step counts - see
/// [`required_sublocations`](crate::warehouse::required_sublocations) - because
/// a file that listed them could disagree with the warehouse it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultLocation {
    pub name: String,
    pub kind: LocationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultWarehouse {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultCategory {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    pub costing_method: CostingMethod,
    pub valuation: Valuation,
    pub removal_strategy: RemovalStrategy,
}

/// One reason a stock figure may be corrected by hand.
///
/// No account: the chart of accounts belongs to Books and does not exist yet
/// when this file is installed. A seeded type takes the workspace's
/// `InventoryAdjustment` default until somebody gives it one of its own, which
/// is the screen's job and not this file's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultAdjustmentType {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub needs_approval: bool,
}

impl Defaults {
    /// Everything that has to be true before any of this is worth installing.
    ///
    /// Checked at load, so a broken file stops a deployment where somebody is
    /// watching rather than half-installing a warehouse an administrator then
    /// unpicks by hand in a live workspace.
    pub fn check(&self) -> Result<(), DefaultsError> {
        self.check_units()?;
        self.check_locations()?;
        self.check_warehouses()?;
        self.check_categories()?;
        self.check_adjustment_types()
    }

    fn check_adjustment_types(&self) -> Result<(), DefaultsError> {
        let mut seen: Vec<String> = Vec::new();

        for entry in &self.adjustment_type {
            let code = entry.code.trim().to_uppercase();
            if code.is_empty()
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            {
                return Err(DefaultsError::AdjustmentTypeCode(entry.code.clone()));
            }
            if seen.contains(&code) {
                return Err(DefaultsError::DuplicateAdjustmentType(code));
            }
            if entry.name.trim().is_empty() {
                return Err(DefaultsError::AdjustmentTypeName(code));
            }
            seen.push(code);
        }

        Ok(())
    }

    fn check_units(&self) -> Result<(), DefaultsError> {
        let mut seen: Vec<String> = Vec::new();
        let mut bases: Vec<UnitClass> = Vec::new();

        for entry in &self.unit {
            let code = entry.code.trim().to_uppercase();
            if code.is_empty() {
                return Err(DefaultsError::UnitCode(entry.code.clone()));
            }
            if seen.contains(&code) {
                return Err(DefaultsError::DuplicateUnit(code));
            }

            let factor = unit::parse_factor(&entry.factor)
                .map_err(|_| DefaultsError::UnitFactor(code.clone()))?;

            // A class with two bases is a class with two answers to "one what?",
            // and a class with none has nothing for its factors to be against.
            if factor == 1_000_000 {
                if bases.contains(&entry.class) {
                    return Err(DefaultsError::TwoBases(entry.class));
                }
                bases.push(entry.class);
            }

            seen.push(code);
        }

        for entry in &self.unit {
            if !bases.contains(&entry.class) {
                return Err(DefaultsError::NoBase(entry.class));
            }
        }

        Ok(())
    }

    fn check_locations(&self) -> Result<(), DefaultsError> {
        let mut seen: Vec<String> = Vec::new();

        for entry in &self.location {
            let name = entry.name.trim();
            if name.is_empty() || name.contains('/') {
                return Err(DefaultsError::LocationName(entry.name.clone()));
            }
            if seen.contains(&name.to_owned()) {
                return Err(DefaultsError::DuplicateLocation(name.to_owned()));
            }
            // The file declares the counterparts. Internal and view nodes come
            // from the warehouse, and one declared here would be a location in
            // no building that the warehouse screen could never explain.
            if matches!(entry.kind, LocationKind::Internal | LocationKind::View) {
                return Err(DefaultsError::LocationKind(name.to_owned()));
            }
            seen.push(name.to_owned());
        }

        // Without these a receipt has nowhere to come from and a count
        // difference has nowhere to go, which is not a gap a workspace can be
        // expected to notice.
        for required in [
            LocationKind::Vendor,
            LocationKind::Customer,
            LocationKind::InventoryLoss,
            LocationKind::Production,
            LocationKind::Transit,
        ] {
            if !self.location.iter().any(|entry| entry.kind == required) {
                return Err(DefaultsError::MissingCounterpart(required));
            }
        }

        Ok(())
    }

    fn check_warehouses(&self) -> Result<(), DefaultsError> {
        let mut seen: Vec<String> = Vec::new();

        for entry in &self.warehouse {
            let code = entry.code.trim().to_uppercase();
            if code.is_empty() || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err(DefaultsError::WarehouseCode(entry.code.clone()));
            }
            if seen.contains(&code) {
                return Err(DefaultsError::DuplicateWarehouse(code));
            }
            if entry.name.trim().is_empty() {
                return Err(DefaultsError::WarehouseName(code));
            }
            seen.push(code);
        }

        Ok(())
    }

    fn check_categories(&self) -> Result<(), DefaultsError> {
        let mut seen: Vec<String> = Vec::new();

        for entry in &self.category {
            let name = entry.name.trim();
            if name.is_empty() || name.contains('/') {
                return Err(DefaultsError::CategoryName(entry.name.clone()));
            }
            if seen.contains(&name.to_owned()) {
                return Err(DefaultsError::DuplicateCategory(name.to_owned()));
            }
            // Declared before it is referenced, so the file can be installed in
            // the order it is written without a second pass.
            if let Some(parent) = entry.parent.as_deref()
                && !seen.iter().any(|earlier| earlier == parent.trim())
            {
                return Err(DefaultsError::UnknownParent(parent.to_owned()));
            }
            seen.push(name.to_owned());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefaultsError {
    #[error("'{0}' is not a usable unit code")]
    UnitCode(String),
    #[error("unit {0} is declared twice")]
    DuplicateUnit(String),
    #[error("unit {0} has a factor that is not a positive number")]
    UnitFactor(String),
    #[error("{0:?} has two base units, so nothing says what one of them is")]
    TwoBases(UnitClass),
    #[error("{0:?} has no base unit, so its factors are against nothing")]
    NoBase(UnitClass),
    #[error("'{0}' is not a usable location name")]
    LocationName(String),
    #[error("location '{0}' is declared twice")]
    DuplicateLocation(String),
    #[error("location '{0}' is internal or a grouping; those come from a warehouse")]
    LocationKind(String),
    #[error("no {0:?} location is declared, so half of what a movement means is missing")]
    MissingCounterpart(LocationKind),
    #[error("'{0}' is not a usable warehouse code")]
    WarehouseCode(String),
    #[error("warehouse {0} has no name")]
    WarehouseName(String),
    #[error("warehouse {0} is declared twice")]
    DuplicateWarehouse(String),
    #[error("'{0}' is not a usable category name")]
    CategoryName(String),
    #[error("category '{0}' is declared twice")]
    DuplicateCategory(String),
    #[error("category parent '{0}' is not declared before it is used")]
    UnknownParent(String),
    #[error("'{0}' is not a usable adjustment type code")]
    AdjustmentTypeCode(String),
    #[error("adjustment type {0} has no name")]
    AdjustmentTypeName(String),
    #[error("adjustment type {0} is declared twice")]
    DuplicateAdjustmentType(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(code: &str, class: UnitClass, factor: &str) -> DefaultUnit {
        DefaultUnit {
            code: code.to_owned(),
            name: code.to_owned(),
            class,
            factor: factor.to_owned(),
        }
    }

    fn counterparts() -> Vec<DefaultLocation> {
        [
            ("Vendors", LocationKind::Vendor),
            ("Customers", LocationKind::Customer),
            ("Inventory loss", LocationKind::InventoryLoss),
            ("Production", LocationKind::Production),
            ("Transit", LocationKind::Transit),
        ]
        .into_iter()
        .map(|(name, kind)| DefaultLocation {
            name: name.to_owned(),
            kind,
        })
        .collect()
    }

    fn valid() -> Defaults {
        Defaults {
            unit: vec![unit("EA", UnitClass::Count, "1")],
            location: counterparts(),
            warehouse: vec![DefaultWarehouse {
                code: "WH".to_owned(),
                name: "Main".to_owned(),
            }],
            category: vec![DefaultCategory {
                name: "All".to_owned(),
                parent: None,
                costing_method: CostingMethod::Average,
                valuation: Valuation::Automated,
                removal_strategy: RemovalStrategy::Fifo,
            }],
            adjustment_type: vec![DefaultAdjustmentType {
                code: "COUNT".to_owned(),
                name: "Count difference".to_owned(),
                direction: Direction::Both,
                needs_approval: false,
            }],
        }
    }

    #[test]
    fn the_shipped_arrangement_is_valid() {
        assert_eq!(valid().check(), Ok(()));
    }

    #[test]
    fn a_class_needs_exactly_one_base_unit() {
        let two = Defaults {
            unit: vec![
                unit("EA", UnitClass::Count, "1"),
                unit("PC", UnitClass::Count, "1"),
            ],
            ..valid()
        };
        assert_eq!(two.check(), Err(DefaultsError::TwoBases(UnitClass::Count)));

        let none = Defaults {
            unit: vec![unit("KG", UnitClass::Weight, "1000")],
            ..valid()
        };
        assert_eq!(none.check(), Err(DefaultsError::NoBase(UnitClass::Weight)));
    }

    #[test]
    fn every_counterpart_has_to_be_there() {
        // Without a vendor location there is no such thing as a receipt, and
        // that is not a gap a workspace could be expected to notice.
        let missing = Defaults {
            location: counterparts()
                .into_iter()
                .filter(|entry| entry.kind != LocationKind::Vendor)
                .collect(),
            ..valid()
        };

        assert_eq!(
            missing.check(),
            Err(DefaultsError::MissingCounterpart(LocationKind::Vendor))
        );
    }

    #[test]
    fn a_warehouses_own_locations_are_not_declared_in_the_file() {
        // They come from its step counts, and a file that listed them could
        // disagree with the warehouse it belongs to.
        let mut declared = counterparts();
        declared.push(DefaultLocation {
            name: "Stock".to_owned(),
            kind: LocationKind::Internal,
        });

        assert_eq!(
            Defaults {
                location: declared,
                ..valid()
            }
            .check(),
            Err(DefaultsError::LocationKind("Stock".to_owned()))
        );
    }

    #[test]
    fn a_category_parent_has_to_be_declared_first() {
        let out_of_order = Defaults {
            category: vec![DefaultCategory {
                name: "Raw materials".to_owned(),
                parent: Some("All".to_owned()),
                costing_method: CostingMethod::Fifo,
                valuation: Valuation::Automated,
                removal_strategy: RemovalStrategy::Fifo,
            }],
            ..valid()
        };

        assert_eq!(
            out_of_order.check(),
            Err(DefaultsError::UnknownParent("All".to_owned()))
        );
    }

    #[test]
    fn a_duplicate_is_caught_before_a_unique_index_catches_it() {
        let twice = Defaults {
            unit: vec![
                unit("EA", UnitClass::Count, "1"),
                unit(" ea ", UnitClass::Count, "1"),
            ],
            ..valid()
        };

        assert_eq!(twice.check(), Err(DefaultsError::DuplicateUnit("EA".to_owned())));
    }
}
