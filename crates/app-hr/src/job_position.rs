//! A role, which exists whether or not anybody holds it.
//!
//! That is the whole reason it is a table rather than a string on an
//! assignment: an unfilled position is a vacancy, and a system where a job
//! title is text typed onto a person can never answer "what are we recruiting
//! for" - it has no row for the job nobody is doing.
//!
//! It is also what stops "Senior Nurse", "Sr. Nurse" and "senior nurse" being
//! three roles in the same report.
//!
//! The department is optional. A Health and Safety Officer may sit across the
//! whole organization, and forcing every role into one department would make
//! the org chart lie about who it answers to.

use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longest code the column holds. Matches `job_positions_code_format`.
pub const MAX_JOB_CODE_LEN: usize = 40;

/// Longest title the column holds. Matches `job_positions_title_present`.
pub const MAX_JOB_TITLE_LEN: usize = 120;

pub const MAX_JOB_DESCRIPTION_LEN: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPosition {
    pub id: Uuid,
    pub code: String,
    pub title: String,
    pub department_id: Option<Uuid>,
    pub description: Option<String>,
    pub is_active: bool,
}

/// One row of the grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPositionSummary {
    pub id: Uuid,
    pub code: String,
    pub title: String,
    pub department_id: Option<Uuid>,
    /// Resolved for display. `None` where the role belongs to no department.
    pub department_name: Option<String>,
    pub is_active: bool,
    /// How many people currently hold it. Zero is a vacancy, which is the
    /// number this screen exists to make visible.
    pub filled: i64,
}

impl JobPositionSummary {
    /// A role nobody currently holds.
    pub const fn is_vacant(&self) -> bool {
        self.filled == 0
    }
}

/// The editable part of a role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPositionInput {
    pub id: Option<Uuid>,
    /// Empty on create means "allocate one"; typed means use it as typed.
    pub code: String,
    pub title: String,
    pub department_id: Option<Uuid>,
    pub description: String,
    pub is_active: bool,
}

impl JobPositionInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            title: String::new(),
            department_id: None,
            description: String::new(),
            is_active: true,
        }
    }

    pub fn from_position(position: &JobPosition) -> Self {
        Self {
            id: Some(position.id),
            code: position.code.clone(),
            title: position.title.clone(),
            department_id: position.department_id,
            description: position.description.clone().unwrap_or_default(),
            is_active: position.is_active,
        }
    }

    pub fn check(&self) -> Result<CheckedJobPosition, JobPositionError> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err(JobPositionError::TitleRequired);
        }
        if title.chars().count() > MAX_JOB_TITLE_LEN {
            return Err(JobPositionError::TitleTooLong);
        }

        let code = self.code.trim();
        if code.chars().count() > MAX_JOB_CODE_LEN {
            return Err(JobPositionError::CodeTooLong);
        }
        if !code.is_empty() && !is_code_shaped(code) {
            return Err(JobPositionError::CodeMalformed);
        }

        if self.description.chars().count() > MAX_JOB_DESCRIPTION_LEN {
            return Err(JobPositionError::DescriptionTooLong);
        }

        Ok(CheckedJobPosition {
            id: self.id,
            code: code.to_owned(),
            title: title.to_owned(),
            department_id: self.department_id,
            description: non_empty(&self.description),
            is_active: self.is_active,
        })
    }
}

/// A role whose shape has been checked. The code may still be empty, which
/// means the store allocates one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedJobPosition {
    pub id: Option<Uuid>,
    pub code: String,
    pub title: String,
    pub department_id: Option<Uuid>,
    pub description: Option<String>,
    pub is_active: bool,
}

/// The same rule `job_positions_code_format` applies, so a code this accepts is
/// one the column will take.
fn is_code_shaped(code: &str) -> bool {
    let mut chars = code.chars();

    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JobPositionError {
    #[error("a role needs a title")]
    TitleRequired,
    #[error("a title is at most 120 characters")]
    TitleTooLong,
    #[error("a code is at most 40 characters")]
    CodeTooLong,
    #[error("a code may hold only letters, digits, hyphens and underscores")]
    CodeMalformed,
    #[error("that code is already in use")]
    CodeTaken,
    #[error("a description is at most 2000 characters")]
    DescriptionTooLong,
    #[error("that role still has people assigned to it")]
    StillHeld,
}

impl JobPositionError {
    pub fn field(self) -> &'static str {
        match self {
            Self::TitleRequired | Self::TitleTooLong => "title",
            Self::CodeTooLong | Self::CodeMalformed | Self::CodeTaken => "code",
            Self::DescriptionTooLong => "description",
            Self::StillHeld => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::TitleRequired => msg!("job_positions.error.title_required"),
            Self::TitleTooLong => msg!("job_positions.error.title_too_long"),
            Self::CodeTooLong => msg!("job_positions.error.code_too_long"),
            Self::CodeMalformed => msg!("job_positions.error.code_malformed"),
            Self::CodeTaken => msg!("job_positions.error.code_taken"),
            Self::DescriptionTooLong => msg!("job_positions.error.description_too_long"),
            Self::StillHeld => msg!("job_positions.error.still_held"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> JobPositionInput {
        JobPositionInput {
            title: "Staff Nurse".to_owned(),
            ..JobPositionInput::blank()
        }
    }

    #[test]
    fn a_role_needs_a_title() {
        let draft = JobPositionInput {
            title: "   ".to_owned(),
            ..input()
        };

        assert_eq!(draft.check(), Err(JobPositionError::TitleRequired));
    }

    #[test]
    fn a_blank_code_is_allowed_because_the_store_allocates_one() {
        let checked = input().check().expect("a role with a title is enough");

        assert!(checked.code.is_empty());
    }

    #[test]
    fn a_typed_code_is_kept_and_a_malformed_one_is_refused() {
        let good = JobPositionInput {
            code: " JOB-NURSE ".to_owned(),
            ..input()
        };
        assert_eq!(good.check().expect("well formed").code, "JOB-NURSE");

        let bad = JobPositionInput {
            code: "-leads-with-a-hyphen".to_owned(),
            ..input()
        };
        assert_eq!(bad.check(), Err(JobPositionError::CodeMalformed));
    }

    #[test]
    fn the_title_is_trimmed_rather_than_stored_with_its_spaces() {
        let draft = JobPositionInput {
            title: "  Staff Nurse  ".to_owned(),
            ..input()
        };

        assert_eq!(draft.check().expect("valid").title, "Staff Nurse");
    }

    #[test]
    fn a_role_nobody_holds_reads_as_vacant() {
        let summary = JobPositionSummary {
            id: Uuid::from_u128(1),
            code: "JOB-001".to_owned(),
            title: "Staff Nurse".to_owned(),
            department_id: None,
            department_name: None,
            is_active: true,
            filled: 0,
        };

        assert!(summary.is_vacant());
        assert!(!JobPositionSummary {
            filled: 3,
            ..summary
        }
        .is_vacant());
    }
}
