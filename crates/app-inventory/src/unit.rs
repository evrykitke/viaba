//! Units of measure, and what it means to convert between two of them.
//!
//! # A unit belongs to a class, and only a class converts
//!
//! Kilograms convert to grams because both measure weight. Kilograms do not
//! convert to litres, and a system that lets somebody try has a stock ledger
//! that can be talked into anything. So every unit names a [`UnitClass`] and a
//! factor to that class's base unit, and [`Conversion`] refuses a pair from two
//! classes rather than inventing a factor.
//!
//! The one exception is deliberate and lives elsewhere: a case of twelve *is* a
//! conversion between two count units, but "twelve" is a fact about that item's
//! packaging rather than about cases in general. Item packaging is on the item.
//!
//! # Why a factor rather than a formula
//!
//! Because every unit conversion that matters in a warehouse is linear, and the
//! one that is not - temperature - is not a stock unit. A factor is a number an
//! auditor can check.

use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::quantity::{Quantity, QuantityError};

/// Decimal places a conversion factor is stored at. `NUMERIC(19, 6)`.
///
/// Six, so 1/3 of a drum is 0.333333 and a pound is 0.453592 kg exactly as
/// written down, rather than to whatever precision a float happened to keep.
pub const FACTOR_SCALE: u32 = 6;

const FACTOR_ONE: i128 = 1_000_000;

pub const MAX_UNIT_CODE_LEN: usize = 12;
pub const MAX_UNIT_NAME_LEN: usize = 60;

/// What a unit measures.
///
/// A closed set, because it is the thing that decides whether a conversion is
/// allowed at all. Adding one is a deliberate change; a free-text "category"
/// would let two spellings of "weight" become two incompatible classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitClass {
    /// Things counted one by one. The base is the each.
    Count,
    Weight,
    Volume,
    Length,
    Area,
    /// Hours and days, for a service item on a bill.
    Time,
}

impl UnitClass {
    pub const ALL: &'static [Self] = &[
        Self::Count,
        Self::Weight,
        Self::Volume,
        Self::Length,
        Self::Area,
        Self::Time,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Weight => "weight",
            Self::Volume => "volume",
            Self::Length => "length",
            Self::Area => "area",
            Self::Time => "time",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|class| class.as_str() == raw)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Count => msg!("units.class.count"),
            Self::Weight => msg!("units.class.weight"),
            Self::Volume => msg!("units.class.volume"),
            Self::Length => msg!("units.class.length"),
            Self::Area => msg!("units.class.area"),
            Self::Time => msg!("units.class.time"),
        }
    }
}

/// One unit of measure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: Uuid,
    /// `EA`, `KG`, `L`. Upper case, unique within the workspace.
    pub code: String,
    pub name: String,
    pub class: UnitClass,
    /// How many of this class's base unit one of these is, scaled at
    /// [`FACTOR_SCALE`]. A kilogram against a base of grams is 1 000 000 000.
    pub factor_scaled: i128,
    /// The one unit of its class everything else is measured against. Exactly
    /// one per class, and its factor is one.
    pub is_base: bool,
    pub is_active: bool,
}

impl Unit {
    /// Whether this is the class's base, by the factor rather than the flag.
    /// Used where the flag has not been read - a default file, say.
    pub const fn factor_is_one(&self) -> bool {
        self.factor_scaled == FACTOR_ONE
    }
}

/// A conversion between two units, built only where one is possible.
///
/// The check is in the constructor rather than at the call site, so a caller
/// holding one of these already knows the two units agree about what they
/// measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conversion {
    from_scaled: i128,
    to_scaled: i128,
}

impl Conversion {
    /// Build a conversion from `from` to `to`, or say why there is none.
    pub fn between(from: &Unit, to: &Unit) -> Result<Self, UnitError> {
        if from.class != to.class {
            return Err(UnitError::DifferentClasses);
        }
        if to.factor_scaled <= 0 || from.factor_scaled <= 0 {
            return Err(UnitError::FactorNotPositive);
        }

        Ok(Self {
            from_scaled: from.factor_scaled,
            to_scaled: to.factor_scaled,
        })
    }

    /// The identity, for a line already in the unit it is being asked about.
    pub const IDENTITY: Self = Self {
        from_scaled: FACTOR_ONE,
        to_scaled: FACTOR_ONE,
    };

    pub const fn is_identity(self) -> bool {
        self.from_scaled == self.to_scaled
    }

    /// Convert a quantity, rounding half away from zero at the quantity's own
    /// six decimal places.
    pub fn apply(self, quantity: Quantity) -> Result<Quantity, UnitError> {
        if self.is_identity() {
            return Ok(quantity);
        }

        // One step: multiplied by what this unit is worth in the class base,
        // divided by what the target is. Going through the base as two roundings
        // would turn a third of a drum into 0.333333 and then back into 0.999999.
        quantity
            .scale_by_ratio(self.from_scaled, self.to_scaled)
            .map_err(UnitError::Factor)
    }
}

/// The editable part of a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub class: UnitClass,
    /// As typed: `1000`, `0.453592`. Parsed at [`FACTOR_SCALE`].
    pub factor: String,
    pub is_active: bool,
}

impl UnitInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            class: UnitClass::Count,
            factor: "1".to_owned(),
            is_active: true,
        }
    }

    pub fn from_unit(unit: &Unit) -> Self {
        Self {
            id: Some(unit.id),
            code: unit.code.clone(),
            name: unit.name.clone(),
            class: unit.class,
            factor: factor_to_string(unit.factor_scaled),
            is_active: unit.is_active,
        }
    }

    /// Trim, upper-case the code, and say what is still wrong.
    pub fn check(&self) -> Result<Checked, UnitError> {
        let code = self.code.trim().to_uppercase();
        let name = self.name.trim();

        if code.is_empty() {
            return Err(UnitError::CodeRequired);
        }
        if code.chars().count() > MAX_UNIT_CODE_LEN {
            return Err(UnitError::CodeTooLong);
        }
        if !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(UnitError::CodeShape);
        }

        if name.is_empty() {
            return Err(UnitError::NameRequired);
        }
        if name.chars().count() > MAX_UNIT_NAME_LEN {
            return Err(UnitError::NameTooLong);
        }

        let factor = parse_factor(&self.factor)?;

        Ok(Checked {
            id: self.id,
            code,
            name: name.to_owned(),
            class: self.class,
            factor_scaled: factor,
            is_active: self.is_active,
        })
    }
}

/// A unit somebody typed, after checking. A separate type from [`UnitInput`]
/// so the factor is a number by the time it reaches the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub class: UnitClass,
    pub factor_scaled: i128,
    pub is_active: bool,
}

/// Parse a factor at [`FACTOR_SCALE`], refusing zero and anything negative -
/// a unit that is nothing of its base converts every quantity to nothing.
pub fn parse_factor(raw: &str) -> Result<i128, UnitError> {
    let quantity = Quantity::parse(raw).map_err(|err| match err {
        QuantityError::Empty => UnitError::FactorRequired,
        other => UnitError::Factor(other),
    })?;

    if !quantity.is_positive() {
        return Err(UnitError::FactorNotPositive);
    }

    Ok(quantity.scaled())
}

/// Render a stored factor the way it was typed.
pub fn factor_to_string(factor_scaled: i128) -> String {
    Quantity::from_scaled(factor_scaled)
        .map(|quantity| quantity.to_display_string())
        .unwrap_or_else(|_| "1".to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnitError {
    #[error("a unit needs a code")]
    CodeRequired,
    #[error("a unit code is at most twelve characters")]
    CodeTooLong,
    #[error("a unit code may contain only letters, digits, hyphens and underscores")]
    CodeShape,
    #[error("a unit needs a name")]
    NameRequired,
    #[error("a unit name is at most sixty characters")]
    NameTooLong,
    #[error("a unit needs a factor")]
    FactorRequired,
    #[error("a factor has to be greater than zero")]
    FactorNotPositive,
    #[error("{0}")]
    Factor(QuantityError),
    #[error("those two units measure different things")]
    DifferentClasses,
    #[error("a class already has a base unit")]
    BaseAlreadySet,
}

impl UnitError {
    pub fn field(self) -> &'static str {
        match self {
            Self::CodeRequired | Self::CodeTooLong | Self::CodeShape => "code",
            Self::NameRequired | Self::NameTooLong => "name",
            Self::FactorRequired | Self::FactorNotPositive | Self::Factor(_) => "factor",
            Self::DifferentClasses | Self::BaseAlreadySet => "class",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CodeRequired => msg!("units.error.code_required"),
            Self::CodeTooLong => msg!("units.error.code_too_long"),
            Self::CodeShape => msg!("units.error.code_shape"),
            Self::NameRequired => msg!("units.error.name_required"),
            Self::NameTooLong => msg!("units.error.name_too_long"),
            Self::FactorRequired => msg!("units.error.factor_required"),
            Self::FactorNotPositive => msg!("units.error.factor_not_positive"),
            Self::Factor(err) => err.message(),
            Self::DifferentClasses => msg!("units.error.different_classes"),
            Self::BaseAlreadySet => msg!("units.error.base_already_set"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(code: &str, class: UnitClass, factor: i128) -> Unit {
        Unit {
            id: Uuid::from_u128(factor as u128),
            code: code.to_owned(),
            name: code.to_owned(),
            class,
            factor_scaled: factor,
            is_base: factor == FACTOR_ONE,
            is_active: true,
        }
    }

    #[test]
    fn a_conversion_within_a_class_is_exact_in_both_directions() {
        let gram = unit("G", UnitClass::Weight, FACTOR_ONE);
        let kilo = unit("KG", UnitClass::Weight, 1_000 * FACTOR_ONE);

        let two_and_a_half = Quantity::parse("2.5").unwrap();

        let grams = Conversion::between(&kilo, &gram)
            .unwrap()
            .apply(two_and_a_half)
            .unwrap();
        assert_eq!(grams.to_display_string(), "2500");

        let back = Conversion::between(&gram, &kilo).unwrap().apply(grams).unwrap();
        assert_eq!(back, two_and_a_half);
    }

    #[test]
    fn weight_does_not_convert_to_volume() {
        // The whole reason a class exists. A litre of mercury is not a
        // kilogram of anything, and a stock ledger that guesses is worse than
        // one that refuses.
        let kilo = unit("KG", UnitClass::Weight, FACTOR_ONE);
        let litre = unit("L", UnitClass::Volume, FACTOR_ONE);

        assert_eq!(
            Conversion::between(&kilo, &litre),
            Err(UnitError::DifferentClasses)
        );
    }

    #[test]
    fn the_identity_conversion_changes_nothing() {
        let each = unit("EA", UnitClass::Count, FACTOR_ONE);
        let quantity = Quantity::parse("7.25").unwrap();

        assert_eq!(
            Conversion::between(&each, &each).unwrap().apply(quantity),
            Ok(quantity)
        );
    }

    #[test]
    fn a_factor_of_zero_is_refused_rather_than_stored() {
        let input = UnitInput {
            code: "BAD".to_owned(),
            name: "Bad".to_owned(),
            factor: "0".to_owned(),
            ..UnitInput::blank()
        };

        assert_eq!(input.check(), Err(UnitError::FactorNotPositive));
    }

    #[test]
    fn a_unit_code_is_stored_upper_case() {
        let input = UnitInput {
            code: " kg ".to_owned(),
            name: " Kilogram ".to_owned(),
            class: UnitClass::Weight,
            factor: "1000".to_owned(),
            ..UnitInput::blank()
        };

        let checked = input.check().unwrap();
        assert_eq!(checked.code, "KG");
        assert_eq!(checked.name, "Kilogram");
        assert_eq!(checked.factor_scaled, 1_000 * FACTOR_ONE);
    }

    #[test]
    fn every_class_round_trips() {
        for class in UnitClass::ALL {
            assert_eq!(UnitClass::parse(class.as_str()), Some(*class));
        }
        assert_eq!(UnitClass::parse("mass"), None);
    }
}
