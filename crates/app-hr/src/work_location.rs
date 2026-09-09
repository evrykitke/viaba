//! Where somebody works.
//!
//! Three kinds, and the distinction that earns its place is whether the
//! workspace controls the premises: it decides who is covered by the building's
//! insurance, who is in scope for a fire drill, and who has to be asked about
//! their own desk rather than told.
//!
//! Not `inventory.warehouses`, which is where *stock* is. The two overlap in a
//! small workspace and diverge immediately in a large one - most people do not
//! work at a warehouse, and a dark store has nobody assigned to it at all.

use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_LOCATION_CODE_LEN: usize = 40;
pub const MAX_LOCATION_NAME_LEN: usize = 120;
pub const MAX_LOCATION_ADDRESS_LEN: usize = 500;

/// What kind of place it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    /// Premises the workspace controls.
    Office,
    /// The person's own home.
    Home,
    /// A client site, a vehicle, a field.
    Other,
}

impl LocationKind {
    pub const ALL: &'static [Self] = &[Self::Office, Self::Home, Self::Other];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Office => "office",
            Self::Home => "home",
            Self::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == raw)
    }

    /// Whether the workspace controls the premises. What the fire register and
    /// the insurance schedule are drawn from.
    pub const fn is_ours(self) -> bool {
        matches!(self, Self::Office)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Office => msg!("work_locations.kind.office"),
            Self::Home => msg!("work_locations.kind.home"),
            Self::Other => msg!("work_locations.kind.elsewhere"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkLocation {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: LocationKind,
    pub address: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkLocationSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: LocationKind,
    pub address: Option<String>,
    pub is_active: bool,
    /// How many people currently work there.
    pub headcount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkLocationInput {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub kind: LocationKind,
    pub address: String,
    pub is_active: bool,
}

impl WorkLocationInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            kind: LocationKind::Office,
            address: String::new(),
            is_active: true,
        }
    }

    pub fn from_location(location: &WorkLocation) -> Self {
        Self {
            id: Some(location.id),
            code: location.code.clone(),
            name: location.name.clone(),
            kind: location.kind,
            address: location.address.clone().unwrap_or_default(),
            is_active: location.is_active,
        }
    }

    pub fn check(&self) -> Result<CheckedWorkLocation, WorkLocationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(WorkLocationError::NameRequired);
        }
        if name.chars().count() > MAX_LOCATION_NAME_LEN {
            return Err(WorkLocationError::NameTooLong);
        }

        let code = self.code.trim();
        if code.chars().count() > MAX_LOCATION_CODE_LEN {
            return Err(WorkLocationError::CodeTooLong);
        }
        if !code.is_empty() && !crate::is_code_shaped(code) {
            return Err(WorkLocationError::CodeMalformed);
        }

        if self.address.chars().count() > MAX_LOCATION_ADDRESS_LEN {
            return Err(WorkLocationError::AddressTooLong);
        }

        Ok(CheckedWorkLocation {
            id: self.id,
            code: code.to_owned(),
            name: name.to_owned(),
            kind: self.kind,
            address: crate::non_empty(&self.address),
            is_active: self.is_active,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedWorkLocation {
    pub id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub kind: LocationKind,
    pub address: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorkLocationError {
    #[error("a place needs a name")]
    NameRequired,
    #[error("a name is at most 120 characters")]
    NameTooLong,
    #[error("a code is at most 40 characters")]
    CodeTooLong,
    #[error("a code may hold only letters, digits, hyphens and underscores")]
    CodeMalformed,
    #[error("that code is already in use")]
    CodeTaken,
    #[error("an address is at most 500 characters")]
    AddressTooLong,
    #[error("that place still has people working at it")]
    StillUsed,
}

impl WorkLocationError {
    pub fn field(self) -> &'static str {
        match self {
            Self::NameRequired | Self::NameTooLong => "name",
            Self::CodeTooLong | Self::CodeMalformed | Self::CodeTaken => "code",
            Self::AddressTooLong => "address",
            Self::StillUsed => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::NameRequired => msg!("work_locations.error.name_required"),
            Self::NameTooLong => msg!("work_locations.error.name_too_long"),
            Self::CodeTooLong => msg!("work_locations.error.code_too_long"),
            Self::CodeMalformed => msg!("work_locations.error.code_malformed"),
            Self::CodeTaken => msg!("work_locations.error.code_taken"),
            Self::AddressTooLong => msg!("work_locations.error.address_too_long"),
            Self::StillUsed => msg!("work_locations.error.still_used"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_office_is_premises_the_workspace_controls() {
        assert!(LocationKind::Office.is_ours());
        assert!(!LocationKind::Home.is_ours());
        assert!(!LocationKind::Other.is_ours());
    }

    #[test]
    fn every_kind_survives_a_round_trip_through_the_column() {
        for kind in LocationKind::ALL {
            assert_eq!(LocationKind::parse(kind.as_str()), Some(*kind));
        }

        assert_eq!(LocationKind::parse("spaceship"), None);
    }

    #[test]
    fn a_place_needs_a_name() {
        let draft = WorkLocationInput {
            name: "  ".to_owned(),
            ..WorkLocationInput::blank()
        };

        assert_eq!(draft.check(), Err(WorkLocationError::NameRequired));
    }

    #[test]
    fn a_blank_address_is_stored_as_nothing_rather_than_as_an_empty_string() {
        let draft = WorkLocationInput {
            name: "Head office".to_owned(),
            address: "   ".to_owned(),
            ..WorkLocationInput::blank()
        };

        assert_eq!(draft.check().expect("valid").address, None);
    }
}
