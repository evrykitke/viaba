//! Why the shelf disagreed with the record.
//!
//! # The type is the whole of this module
//!
//! An adjustment is one movement into or out of the inventory-loss location -
//! see [`crate::location`] - and that movement already knows how to value
//! itself and how to post. What it does not know is *why*, and why is the only
//! thing anybody asks about it afterwards.
//!
//! A workspace that posts a count difference, a smashed pallet, a case that
//! went out of date and a sample handed to a rep all to one "inventory
//! adjustment" account ends the year with a number that grows and explains
//! nothing. Theft, a warehouse that cannot count, and a perishable range with
//! the wrong shelf life have three different answers and none of them is
//! visible in one total. So a type names its own account, and the movement
//! remembers which type it was.
//!
//! # Direction is a fact about the reason, not about the entry
//!
//! Damage, expiry and theft take stock off a shelf and can never put it back.
//! Found stock only goes the other way. Only a count difference genuinely runs
//! both ways, and letting somebody book stock *in* as damage is how a stock
//! account comes to hold a credit nobody can account for.
//!
//! # Approval is asked of a person, not of a queue
//!
//! [`AdjustmentType::needs_approval`] does not put the adjustment into a second
//! state waiting for somebody: an adjustment is one movement, and a movement
//! that has half happened is the thing the whole stock ledger is built to make
//! impossible. It asks for a second permission at the moment the button is
//! pressed - so a storekeeper may book a count difference and only a manager
//! may write forty thousand pounds of stock off, which is what "needs approval"
//! means in a warehouse.

use chrono::NaiveDate;
use phonix_core::i18n::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::accounts::AccountRef;
use crate::quantity::Quantity;

pub const MAX_CODE_LEN: usize = 24;
pub const MAX_NAME_LEN: usize = 120;
pub const MAX_NOTE_LEN: usize = 2000;
pub const MAX_REASON_LEN: usize = 120;

/// Which way stock may move under a reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Stock appears: found in a corner, a count that came out high.
    In,
    /// Stock goes: damage, expiry, theft, a sample.
    Out,
    /// A count difference, which is the only honest both.
    #[default]
    Both,
}

impl Direction {
    pub const ALL: &'static [Self] = &[Self::Both, Self::Out, Self::In];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Both => "both",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|one| one.as_str() == raw)
    }

    /// Whether stock may move this way under this reason. `found` is the
    /// direction the screen asks for: true books stock in, false writes it out.
    pub const fn allows(self, found: bool) -> bool {
        match self {
            Self::Both => true,
            Self::In => found,
            Self::Out => !found,
        }
    }

    pub fn label(self) -> Message {
        match self {
            Self::In => msg!("adjustment_types.direction.in"),
            Self::Out => msg!("adjustment_types.direction.out"),
            Self::Both => msg!("adjustment_types.direction.both"),
        }
    }
}

/// One reason a stock figure changed by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustmentType {
    pub id: Uuid,
    /// `COUNT`, `DAMAGE`, `EXPIRY`. Upper case, unique in the workspace.
    pub code: String,
    pub name: String,
    pub direction: Direction,
    /// Where the non-stock half of the journal lands. `None` falls back to
    /// whatever the workspace has mapped `InventoryAdjustment` to - which is
    /// what every adjustment did before types existed.
    pub account: Option<AccountRef>,
    pub needs_approval: bool,
    pub is_active: bool,
    /// Seeded from `config/defaults/inventory.toml`. Editable, never deletable
    /// - see [`AdjustmentType::is_deletable`].
    pub is_system: bool,
    pub note: Option<String>,
}

impl AdjustmentType {
    /// `DAMAGE · Damage`, for a picker.
    pub fn label(&self) -> String {
        format!("{} · {}", self.code, self.name)
    }

    /// Whether this type may be thrown away.
    ///
    /// A seeded type may not: the workspace did not create it, movements point
    /// at it, and one that deleted "Count difference" would have nowhere to put
    /// the next count difference. Deactivating is the way to retire one, and it
    /// leaves the history readable.
    ///
    /// Whether a workspace's own type is in use is a question for the store;
    /// this is the half that can be answered from the row.
    pub const fn is_deletable(&self) -> bool {
        !self.is_system
    }
}

/// A row of the list screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustmentTypeSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub direction: Direction,
    /// The account's number, or `None` where this type takes the default.
    pub account_number: Option<String>,
    pub account_name: Option<String>,
    pub needs_approval: bool,
    pub is_active: bool,
    pub is_system: bool,
    /// How many movements have been booked under it. What makes "is this safe
    /// to delete" answerable on the row rather than after pressing the button.
    pub move_count: i64,
}

impl AdjustmentTypeSummary {
    pub const fn is_in_use(&self) -> bool {
        self.move_count > 0
    }
}

// --- Editing a type --------------------------------------------------------

/// A type as a form holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustmentTypeInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub direction: Direction,
    /// The chosen account, as three fields. All three or none: half an account
    /// is a picker that renders a blank row.
    pub account_id: Option<Uuid>,
    pub account_number: String,
    pub account_name: String,
    pub needs_approval: bool,
    pub is_active: bool,
    pub note: String,
    /// Not editable, and carried so the form can refuse the delete rather than
    /// offer it and fail.
    pub is_system: bool,
}

impl Default for AdjustmentTypeInput {
    fn default() -> Self {
        Self::blank()
    }
}

impl AdjustmentTypeInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            direction: Direction::Both,
            account_id: None,
            account_number: String::new(),
            account_name: String::new(),
            needs_approval: false,
            is_active: true,
            note: String::new(),
            is_system: false,
        }
    }

    pub fn from_type(kind: &AdjustmentType) -> Self {
        Self {
            id: Some(kind.id),
            code: kind.code.clone(),
            name: kind.name.clone(),
            direction: kind.direction,
            account_id: kind.account.as_ref().map(|account| account.account_id),
            account_number: kind
                .account
                .as_ref()
                .map(|account| account.number.clone())
                .unwrap_or_default(),
            account_name: kind
                .account
                .as_ref()
                .map(|account| account.name.clone())
                .unwrap_or_default(),
            needs_approval: kind.needs_approval,
            is_active: kind.is_active,
            note: kind.note.clone().unwrap_or_default(),
            is_system: kind.is_system,
        }
    }

    /// Trim, upper-case the code, and say what is still wrong.
    pub fn check(&self) -> Result<CheckedAdjustmentType, AdjustmentError> {
        let code = self.code.trim().to_uppercase();
        let name = self.name.trim();
        let note = self.note.trim();

        if code.is_empty() {
            return Err(AdjustmentError::CodeRequired);
        }
        if code.chars().count() > MAX_CODE_LEN {
            return Err(AdjustmentError::CodeTooLong);
        }
        if !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(AdjustmentError::CodeShape);
        }

        if name.is_empty() {
            return Err(AdjustmentError::NameRequired);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(AdjustmentError::NameTooLong);
        }
        if note.chars().count() > MAX_NOTE_LEN {
            return Err(AdjustmentError::NoteTooLong);
        }

        // Refused rather than filled in: an id with no number beside it is a
        // row the account screen would draw blank, and guessing the number
        // here would be guessing at another app's data.
        let account = match self.account_id {
            None => None,
            Some(account_id) => {
                let number = self.account_number.trim();
                let name = self.account_name.trim();

                if number.is_empty() || name.is_empty() {
                    return Err(AdjustmentError::AccountIncomplete);
                }

                Some(AccountRef {
                    account_id,
                    number: number.to_owned(),
                    name: name.to_owned(),
                })
            }
        };

        Ok(CheckedAdjustmentType {
            id: self.id,
            code,
            name: name.to_owned(),
            direction: self.direction,
            account,
            needs_approval: self.needs_approval,
            is_active: self.is_active,
            note: (!note.is_empty()).then(|| note.to_owned()),
        })
    }
}

/// A type somebody typed, after checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedAdjustmentType {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub direction: Direction,
    pub account: Option<AccountRef>,
    pub needs_approval: bool,
    pub is_active: bool,
    pub note: Option<String>,
}

// --- Making an adjustment --------------------------------------------------

/// What the adjust screen hands over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustmentInput {
    pub location_id: Option<Uuid>,
    pub variant_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    /// As typed. Parsed at [`Quantity`]'s scale.
    pub quantity: String,
    /// True books stock in from inventory loss, false writes it out to it.
    pub found: bool,
    pub type_id: Option<Uuid>,
    pub moved_on: NaiveDate,
    pub reason: String,
}

impl AdjustmentInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            location_id: None,
            variant_id: None,
            lot_id: None,
            quantity: String::new(),
            found: false,
            type_id: None,
            moved_on: today,
            reason: String::new(),
        }
    }

    /// Everything about an adjustment that can be decided without the database.
    ///
    /// The type is checked against here rather than looked up again later, so
    /// that "damage cannot bring stock in" is answered before anything moves.
    pub fn check(&self, kind: Option<&AdjustmentType>) -> Result<CheckedAdjustment, AdjustmentError> {
        let location_id = self
            .location_id
            .ok_or(AdjustmentError::LocationRequired)?;
        let variant_id = self.variant_id.ok_or(AdjustmentError::ItemRequired)?;

        let quantity = match self.quantity.trim() {
            "" => return Err(AdjustmentError::QuantityRequired),
            typed => Quantity::parse(typed).map_err(|_| AdjustmentError::QuantityRequired)?,
        };

        if !quantity.is_positive() {
            return Err(AdjustmentError::QuantityNotPositive);
        }

        let kind = kind.ok_or(AdjustmentError::TypeRequired)?;

        if !kind.is_active {
            return Err(AdjustmentError::TypeInactive);
        }
        if !kind.direction.allows(self.found) {
            return Err(AdjustmentError::WrongDirection);
        }

        let reason = self.reason.trim();
        if reason.chars().count() > MAX_REASON_LEN {
            return Err(AdjustmentError::ReasonTooLong);
        }

        Ok(CheckedAdjustment {
            location_id,
            variant_id,
            lot_id: self.lot_id,
            quantity,
            found: self.found,
            type_id: kind.id,
            account_id: kind.account.as_ref().map(|account| account.account_id),
            needs_approval: kind.needs_approval,
            moved_on: self.moved_on,
            reason: (!reason.is_empty()).then(|| reason.to_owned()),
        })
    }
}

/// An adjustment somebody asked for, after checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedAdjustment {
    pub location_id: Uuid,
    pub variant_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub quantity: Quantity,
    pub found: bool,
    pub type_id: Uuid,
    /// The account the discrepancy is charged to, from the type. `None` takes
    /// the workspace default for `InventoryAdjustment`.
    pub account_id: Option<Uuid>,
    pub needs_approval: bool,
    pub moved_on: NaiveDate,
    pub reason: Option<String>,
}

// --- Errors ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdjustmentError {
    #[error("an adjustment type needs a code")]
    CodeRequired,
    #[error("an adjustment type code is at most twenty-four characters")]
    CodeTooLong,
    #[error("a code may contain only letters, digits, hyphens and underscores")]
    CodeShape,
    #[error("an adjustment type needs a name")]
    NameRequired,
    #[error("an adjustment type name is at most a hundred and twenty characters")]
    NameTooLong,
    #[error("a note is at most two thousand characters")]
    NoteTooLong,
    #[error("that code is already in use")]
    CodeTaken,
    #[error("an account needs its number and name as well as its id")]
    AccountIncomplete,
    #[error("that adjustment type no longer exists")]
    TypeGone,
    #[error("an adjustment needs a reason")]
    TypeRequired,
    #[error("that adjustment type has been retired")]
    TypeInactive,
    #[error("that reason does not move stock in that direction")]
    WrongDirection,
    #[error("this adjustment needs somebody who may approve one")]
    NeedsApproval,
    #[error("a seeded adjustment type cannot be deleted")]
    SystemType,
    #[error("stock has already been adjusted under this type")]
    TypeInUse,
    #[error("an adjustment needs a location")]
    LocationRequired,
    #[error("an adjustment needs an item")]
    ItemRequired,
    #[error("an adjustment needs a quantity")]
    QuantityRequired,
    #[error("a quantity has to be greater than zero")]
    QuantityNotPositive,
    #[error("a reason is at most a hundred and twenty characters")]
    ReasonTooLong,
}

impl AdjustmentError {
    pub const fn field(self) -> &'static str {
        match self {
            Self::CodeRequired | Self::CodeTooLong | Self::CodeShape | Self::CodeTaken => "code",
            Self::NameRequired | Self::NameTooLong => "name",
            Self::NoteTooLong => "note",
            Self::AccountIncomplete => "account_id",
            Self::TypeGone
            | Self::TypeRequired
            | Self::TypeInactive
            | Self::WrongDirection
            | Self::NeedsApproval
            | Self::SystemType
            | Self::TypeInUse => "type_id",
            Self::LocationRequired => "location_id",
            Self::ItemRequired => "variant_id",
            Self::QuantityRequired | Self::QuantityNotPositive => "quantity",
            Self::ReasonTooLong => "reason",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CodeRequired => msg!("adjustment_types.error.code_required"),
            Self::CodeTooLong => msg!("adjustment_types.error.code_too_long"),
            Self::CodeShape => msg!("adjustment_types.error.code_shape"),
            Self::NameRequired => msg!("adjustment_types.error.name_required"),
            Self::NameTooLong => msg!("adjustment_types.error.name_too_long"),
            Self::NoteTooLong => msg!("adjustment_types.error.note_too_long"),
            Self::CodeTaken => msg!("adjustment_types.error.code_taken"),
            Self::AccountIncomplete => msg!("adjustment_types.error.account_incomplete"),
            Self::TypeGone => msg!("adjustment_types.error.type_gone"),
            Self::TypeRequired => msg!("adjustments.error.type_required"),
            Self::TypeInactive => msg!("adjustments.error.type_inactive"),
            Self::WrongDirection => msg!("adjustments.error.wrong_direction"),
            Self::NeedsApproval => msg!("adjustments.error.needs_approval"),
            Self::SystemType => msg!("adjustment_types.error.system_type"),
            Self::TypeInUse => msg!("adjustment_types.error.type_in_use"),
            Self::LocationRequired => msg!("adjustments.error.location_required"),
            Self::ItemRequired => msg!("adjustments.error.item_required"),
            Self::QuantityRequired => msg!("adjustments.error.quantity_required"),
            Self::QuantityNotPositive => msg!("adjustments.error.quantity_not_positive"),
            Self::ReasonTooLong => msg!("adjustments.error.reason_too_long"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(code: &str, direction: Direction) -> AdjustmentType {
        AdjustmentType {
            id: Uuid::from_u128(7),
            code: code.to_owned(),
            name: code.to_owned(),
            direction,
            account: None,
            needs_approval: false,
            is_active: true,
            is_system: false,
            note: None,
        }
    }

    fn on(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, day).expect("a March date")
    }

    fn asking(found: bool) -> AdjustmentInput {
        AdjustmentInput {
            location_id: Some(Uuid::from_u128(1)),
            variant_id: Some(Uuid::from_u128(2)),
            lot_id: None,
            quantity: "3".to_owned(),
            found,
            type_id: Some(Uuid::from_u128(7)),
            moved_on: on(4),
            reason: String::new(),
        }
    }

    #[test]
    fn a_one_way_reason_refuses_the_other_way() {
        let damage = kind("DAMAGE", Direction::Out);

        assert!(asking(false).check(Some(&damage)).is_ok());
        assert_eq!(
            asking(true).check(Some(&damage)),
            Err(AdjustmentError::WrongDirection)
        );
    }

    #[test]
    fn found_stock_only_comes_in() {
        let found = kind("FOUND", Direction::In);

        assert!(asking(true).check(Some(&found)).is_ok());
        assert_eq!(
            asking(false).check(Some(&found)),
            Err(AdjustmentError::WrongDirection)
        );
    }

    #[test]
    fn a_count_difference_runs_both_ways() {
        let count = kind("COUNT", Direction::Both);

        assert!(asking(true).check(Some(&count)).is_ok());
        assert!(asking(false).check(Some(&count)).is_ok());
    }

    #[test]
    fn a_retired_reason_is_refused_rather_than_hidden() {
        let mut retired = kind("OLD", Direction::Both);
        retired.is_active = false;

        assert_eq!(
            asking(false).check(Some(&retired)),
            Err(AdjustmentError::TypeInactive)
        );
    }

    #[test]
    fn an_adjustment_without_a_reason_is_refused() {
        assert_eq!(
            asking(false).check(None),
            Err(AdjustmentError::TypeRequired)
        );
    }

    #[test]
    fn a_quantity_has_to_be_a_positive_number() {
        let count = kind("COUNT", Direction::Both);

        let mut empty = asking(false);
        empty.quantity = "  ".to_owned();
        assert_eq!(
            empty.check(Some(&count)),
            Err(AdjustmentError::QuantityRequired)
        );

        let mut zero = asking(false);
        zero.quantity = "0".to_owned();
        assert_eq!(
            zero.check(Some(&count)),
            Err(AdjustmentError::QuantityNotPositive)
        );
    }

    #[test]
    fn the_account_travels_from_the_type_onto_the_adjustment() {
        let mut damage = kind("DAMAGE", Direction::Out);
        let account_id = Uuid::from_u128(99);
        damage.account = Some(AccountRef {
            account_id,
            number: "5810".to_owned(),
            name: "Stock damage".to_owned(),
        });
        damage.needs_approval = true;

        let checked = asking(false).check(Some(&damage)).expect("a valid adjustment");

        assert_eq!(checked.account_id, Some(account_id));
        assert!(checked.needs_approval);
        assert_eq!(checked.type_id, damage.id);
    }

    #[test]
    fn a_code_is_upper_cased_and_a_blank_note_becomes_nothing() {
        let input = AdjustmentTypeInput {
            code: " damage ".to_owned(),
            name: "  Damage  ".to_owned(),
            note: "   ".to_owned(),
            ..AdjustmentTypeInput::blank()
        };

        let checked = input.check().expect("a valid type");

        assert_eq!(checked.code, "DAMAGE");
        assert_eq!(checked.name, "Damage");
        assert_eq!(checked.note, None);
    }

    #[test]
    fn an_account_is_all_three_fields_or_none() {
        let half = AdjustmentTypeInput {
            code: "DAMAGE".to_owned(),
            name: "Damage".to_owned(),
            account_id: Some(Uuid::from_u128(3)),
            ..AdjustmentTypeInput::blank()
        };

        assert_eq!(half.check(), Err(AdjustmentError::AccountIncomplete));

        let whole = AdjustmentTypeInput {
            account_number: "5810".to_owned(),
            account_name: "Stock damage".to_owned(),
            ..half
        };

        assert!(whole.check().expect("a valid type").account.is_some());
    }

    #[test]
    fn a_code_may_not_hold_punctuation() {
        let input = AdjustmentTypeInput {
            code: "WRITE OFF".to_owned(),
            name: "Write-off".to_owned(),
            ..AdjustmentTypeInput::blank()
        };

        assert_eq!(input.check(), Err(AdjustmentError::CodeShape));
    }

    #[test]
    fn a_seeded_type_cannot_be_deleted_and_a_workspaces_own_can() {
        let mut seeded = kind("COUNT", Direction::Both);
        seeded.is_system = true;

        assert!(!seeded.is_deletable());
        assert!(kind("OURS", Direction::Both).is_deletable());
    }

    #[test]
    fn a_direction_round_trips_through_its_wire_name() {
        for one in Direction::ALL {
            assert_eq!(Direction::parse(one.as_str()), Some(*one));
        }
        assert_eq!(Direction::parse("sideways"), None);
    }
}
