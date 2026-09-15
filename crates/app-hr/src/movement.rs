//! The document behind a promotion, a transfer or an exit.
//!
//! [`crate::employee::Assignment`] and [`crate::employee::Engagement`] record
//! what became true. This records the act that made it true: who decided, when,
//! and why. Confirming a movement is what writes those rows — through the same
//! service functions a hand-written move already uses, so there is exactly one
//! way an assignment is made.
//!
//! Onboarding is deliberately not a kind here: hiring already has a document,
//! and a movement recording somebody's arrival would have nobody to point at
//! until after they existed.

use chrono::{DateTime, NaiveDate, Utc};
use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::employee::{AssignmentInput, EndReason, LeavingInput};

pub const MAX_REASON_LEN: usize = 500;

/// Which kind of movement it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementKind {
    /// A step up: usually the role, sometimes the reporting line with it.
    Promotion,
    /// A step sideways: department, place, shift.
    Transfer,
    /// Employment ends.
    Exit,
}

impl MovementKind {
    pub const ALL: &'static [Self] = &[Self::Promotion, Self::Transfer, Self::Exit];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Promotion => "promotion",
            Self::Transfer => "transfer",
            Self::Exit => "exit",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    /// Whether confirming it opens a new assignment.
    ///
    /// Promotion and transfer differ in what a reader calls them and in nothing
    /// else this crate does — which is why they share a table and why the
    /// distinction is a word rather than a branch.
    pub const fn opens_an_assignment(self) -> bool {
        matches!(self, Self::Promotion | Self::Transfer)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Promotion => msg!("movements.kind.promotion"),
            Self::Transfer => msg!("movements.kind.transfer"),
            Self::Exit => msg!("movements.kind.exit"),
        }
    }
}

/// Where a movement is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementStatus {
    Draft,
    Confirmed,
    Cancelled,
}

impl MovementStatus {
    pub const ALL: &'static [Self] = &[Self::Draft, Self::Confirmed, Self::Cancelled];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Confirmed => "confirmed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    /// Whether it may still be edited.
    pub const fn is_editable(self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Draft => msg!("movements.status.draft"),
            Self::Confirmed => msg!("movements.status.confirmed"),
            Self::Cancelled => msg!("movements.status.cancelled"),
        }
    }
}

/// One movement, whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    pub id: Uuid,
    /// `None` while it is a draft. Taken at confirm, so a discarded draft
    /// leaves no gap in the sequence.
    pub number: Option<String>,
    pub kind: MovementKind,
    pub status: MovementStatus,
    pub employee_id: Uuid,
    pub employee_name: String,
    pub effective_on: NaiveDate,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub holiday_list_id: Option<Uuid>,
    pub shift_type_id: Option<Uuid>,
    pub end_reason: Option<EndReason>,
    pub reason: Option<String>,
    /// The assignment this document opened, where it opened one.
    pub assignment_id: Option<Uuid>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

impl Movement {
    /// The assignment this movement would write.
    ///
    /// `None` for an exit, which opens none. Built here rather than in the
    /// service so that the document and the row it becomes cannot drift: what a
    /// reader sees on the draft is what confirming writes.
    pub fn as_assignment(&self) -> Option<AssignmentInput> {
        self.kind.opens_an_assignment().then(|| AssignmentInput {
            effective_from: self.effective_on,
            department_id: self.department_id,
            job_position_id: self.job_position_id,
            work_location_id: self.work_location_id,
            manager_id: self.manager_id,
            holiday_list_id: self.holiday_list_id,
            shift_type_id: self.shift_type_id,
            reason: self.reason.clone().unwrap_or_default(),
        })
    }

    /// The leaving this movement would record.
    ///
    /// `None` for anything that is not an exit.
    pub fn as_leaving(&self) -> Option<LeavingInput> {
        match (self.kind, self.end_reason) {
            (MovementKind::Exit, Some(reason)) => Some(LeavingInput {
                ended_on: self.effective_on,
                reason,
                note: self.reason.clone().unwrap_or_default(),
            }),
            _ => None,
        }
    }
}

/// A list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementSummary {
    pub id: Uuid,
    pub number: Option<String>,
    pub kind: MovementKind,
    pub status: MovementStatus,
    pub employee_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub effective_on: NaiveDate,
    pub end_reason: Option<EndReason>,
}

/// A movement being written on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementInput {
    pub id: Option<Uuid>,
    pub kind: MovementKind,
    pub employee_id: Option<Uuid>,
    pub effective_on: Option<NaiveDate>,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub holiday_list_id: Option<Uuid>,
    pub shift_type_id: Option<Uuid>,
    pub end_reason: Option<EndReason>,
    pub reason: String,
}

impl MovementInput {
    pub fn blank(kind: MovementKind) -> Self {
        Self {
            id: None,
            kind,
            employee_id: None,
            effective_on: None,
            department_id: None,
            job_position_id: None,
            work_location_id: None,
            manager_id: None,
            holiday_list_id: None,
            shift_type_id: None,
            end_reason: None,
            reason: String::new(),
        }
    }

    pub fn from_movement(movement: &Movement) -> Self {
        Self {
            id: Some(movement.id),
            kind: movement.kind,
            employee_id: Some(movement.employee_id),
            effective_on: Some(movement.effective_on),
            department_id: movement.department_id,
            job_position_id: movement.job_position_id,
            work_location_id: movement.work_location_id,
            manager_id: movement.manager_id,
            holiday_list_id: movement.holiday_list_id,
            shift_type_id: movement.shift_type_id,
            end_reason: movement.end_reason,
            reason: movement.reason.clone().unwrap_or_default(),
        }
    }

    /// Pre-filled from what somebody is doing now, so a promotion only has to
    /// change the thing that moved.
    ///
    /// The same courtesy [`AssignmentInput::next`] does, and for the same
    /// reason: a form that opens empty invites somebody to blank four fields by
    /// forgetting them.
    pub fn moving(kind: MovementKind, employee_id: Uuid, current: &AssignmentInput) -> Self {
        Self {
            employee_id: Some(employee_id),
            department_id: current.department_id,
            job_position_id: current.job_position_id,
            work_location_id: current.work_location_id,
            manager_id: current.manager_id,
            holiday_list_id: current.holiday_list_id,
            shift_type_id: current.shift_type_id,
            ..Self::blank(kind)
        }
    }

    pub fn check(&self) -> Result<CheckedMovement, MovementError> {
        let Some(employee_id) = self.employee_id else {
            return Err(MovementError::EmployeeRequired);
        };

        let Some(effective_on) = self.effective_on else {
            return Err(MovementError::DateRequired);
        };

        if self.reason.chars().count() > MAX_REASON_LEN {
            return Err(MovementError::ReasonTooLong);
        }

        // Nobody reports to themselves. The deeper case - A to B to A - needs
        // the whole tree and is the service's, exactly as it is for a move.
        if self.manager_id == Some(employee_id) {
            return Err(MovementError::ManagesThemself);
        }

        match self.kind {
            MovementKind::Exit => {
                if self.end_reason.is_none() {
                    return Err(MovementError::EndReasonRequired);
                }

                // The column refuses these too. Caught here so the screen can
                // say which field rather than handing back a constraint.
                if self.department_id.is_some()
                    || self.job_position_id.is_some()
                    || self.work_location_id.is_some()
                    || self.manager_id.is_some()
                    || self.holiday_list_id.is_some()
                    || self.shift_type_id.is_some()
                {
                    return Err(MovementError::ExitMovesNobody);
                }
            }
            MovementKind::Promotion | MovementKind::Transfer => {
                if self.end_reason.is_some() {
                    return Err(MovementError::OnlyAnExitEnds);
                }
            }
        }

        Ok(CheckedMovement {
            id: self.id,
            kind: self.kind,
            employee_id,
            effective_on,
            department_id: self.department_id,
            job_position_id: self.job_position_id,
            work_location_id: self.work_location_id,
            manager_id: self.manager_id,
            holiday_list_id: self.holiday_list_id,
            shift_type_id: self.shift_type_id,
            end_reason: self.end_reason,
            reason: crate::non_empty(&self.reason),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedMovement {
    pub id: Option<Uuid>,
    pub kind: MovementKind,
    pub employee_id: Uuid,
    pub effective_on: NaiveDate,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub holiday_list_id: Option<Uuid>,
    pub shift_type_id: Option<Uuid>,
    pub end_reason: Option<EndReason>,
    pub reason: Option<String>,
}

impl CheckedMovement {
    /// Whether this moves somebody anywhere they are not already.
    ///
    /// A promotion that changes nothing is a document recording that nothing
    /// happened, and somebody will read it as evidence that something did. The
    /// comparison is against what they are on now, which only the service
    /// knows - so it takes the current assignment rather than reaching for it.
    pub fn changes_anything(&self, current: Option<&AssignmentInput>) -> bool {
        let Some(current) = current else {
            // Nothing to compare against: whatever this says is new.
            return true;
        };

        self.department_id != current.department_id
            || self.job_position_id != current.job_position_id
            || self.work_location_id != current.work_location_id
            || self.manager_id != current.manager_id
            || self.holiday_list_id != current.holiday_list_id
            || self.shift_type_id != current.shift_type_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MovementError {
    #[error("a movement needs somebody it is about")]
    EmployeeRequired,
    #[error("a movement needs the day it takes effect")]
    DateRequired,
    #[error("an exit needs a reason")]
    EndReasonRequired,
    #[error("only an exit ends an engagement")]
    OnlyAnExitEnds,
    #[error("an exit moves nobody anywhere")]
    ExitMovesNobody,
    #[error("nobody reports to themself")]
    ManagesThemself,
    #[error("this moves them nowhere they are not already")]
    MovesNobody,
    #[error("a reason is at most 500 characters")]
    ReasonTooLong,
    #[error("only a draft can be edited")]
    NotEditable,
    #[error("only a draft can be confirmed")]
    NotConfirmable,
    #[error("that movement is not here any more")]
    Gone,
}

impl MovementError {
    pub fn field(self) -> &'static str {
        match self {
            Self::EmployeeRequired => "employee_id",
            Self::DateRequired => "effective_on",
            Self::EndReasonRequired | Self::OnlyAnExitEnds => "end_reason",
            Self::ExitMovesNobody | Self::MovesNobody => "department_id",
            Self::ManagesThemself => "manager_id",
            Self::ReasonTooLong => "reason",
            Self::NotEditable | Self::NotConfirmable | Self::Gone => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::EmployeeRequired => msg!("movements.error.employee_required"),
            Self::DateRequired => msg!("movements.error.date_required"),
            Self::EndReasonRequired => msg!("movements.error.end_reason_required"),
            Self::OnlyAnExitEnds => msg!("movements.error.only_an_exit_ends"),
            Self::ExitMovesNobody => msg!("movements.error.exit_moves_nobody"),
            Self::ManagesThemself => msg!("movements.error.manages_themself"),
            Self::MovesNobody => msg!("movements.error.moves_nobody"),
            Self::ReasonTooLong => msg!("movements.error.reason_too_long"),
            Self::NotEditable => msg!("movements.error.not_editable"),
            Self::NotConfirmable => msg!("movements.error.not_confirmable"),
            Self::Gone => msg!("movements.error.gone"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
    }

    fn somebody() -> Uuid {
        Uuid::from_u128(1)
    }

    fn promotion() -> MovementInput {
        MovementInput {
            employee_id: Some(somebody()),
            effective_on: Some(on(2026, 4, 1)),
            job_position_id: Some(Uuid::from_u128(9)),
            ..MovementInput::blank(MovementKind::Promotion)
        }
    }

    fn current() -> AssignmentInput {
        AssignmentInput {
            effective_from: on(2020, 1, 1),
            department_id: Some(Uuid::from_u128(4)),
            job_position_id: Some(Uuid::from_u128(5)),
            work_location_id: None,
            manager_id: None,
            holiday_list_id: None,
            shift_type_id: None,
            reason: String::new(),
        }
    }

    /// The document and the row it becomes are built from one place, so a
    /// reader reviewing a draft is reviewing what confirming will write.
    #[test]
    fn a_promotion_becomes_the_assignment_it_shows() {
        let movement = Movement {
            id: Uuid::nil(),
            number: None,
            kind: MovementKind::Promotion,
            status: MovementStatus::Draft,
            employee_id: somebody(),
            employee_name: "A Nurse".to_owned(),
            effective_on: on(2026, 4, 1),
            department_id: Some(Uuid::from_u128(4)),
            job_position_id: Some(Uuid::from_u128(9)),
            work_location_id: None,
            manager_id: None,
            holiday_list_id: None,
            shift_type_id: None,
            end_reason: None,
            reason: Some("Acting up since January".to_owned()),
            assignment_id: None,
            confirmed_at: None,
        };

        let assignment = movement.as_assignment().expect("a promotion opens one");

        assert_eq!(assignment.effective_from, on(2026, 4, 1));
        assert_eq!(assignment.job_position_id, Some(Uuid::from_u128(9)));
        assert_eq!(assignment.reason, "Acting up since January");
        assert!(movement.as_leaving().is_none());
    }

    #[test]
    fn an_exit_becomes_a_leaving_and_opens_nothing() {
        let movement = Movement {
            id: Uuid::nil(),
            number: None,
            kind: MovementKind::Exit,
            status: MovementStatus::Draft,
            employee_id: somebody(),
            employee_name: "A Nurse".to_owned(),
            effective_on: on(2026, 6, 30),
            department_id: None,
            job_position_id: None,
            work_location_id: None,
            manager_id: None,
            holiday_list_id: None,
            shift_type_id: None,
            end_reason: Some(EndReason::Resigned),
            reason: None,
            assignment_id: None,
            confirmed_at: None,
        };

        let leaving = movement.as_leaving().expect("an exit records one");

        assert_eq!(leaving.ended_on, on(2026, 6, 30));
        assert_eq!(leaving.reason, EndReason::Resigned);
        assert!(movement.as_assignment().is_none());
    }

    #[test]
    fn an_exit_needs_a_reason_and_moves_nobody() {
        let no_reason = MovementInput {
            employee_id: Some(somebody()),
            effective_on: Some(on(2026, 6, 30)),
            ..MovementInput::blank(MovementKind::Exit)
        };
        assert_eq!(no_reason.check(), Err(MovementError::EndReasonRequired));

        let also_moves = MovementInput {
            end_reason: Some(EndReason::Resigned),
            department_id: Some(Uuid::from_u128(4)),
            ..no_reason
        };
        assert_eq!(also_moves.check(), Err(MovementError::ExitMovesNobody));
    }

    #[test]
    fn a_promotion_cannot_carry_an_end_reason() {
        let draft = MovementInput {
            end_reason: Some(EndReason::Retirement),
            ..promotion()
        };

        assert_eq!(draft.check(), Err(MovementError::OnlyAnExitEnds));
    }

    #[test]
    fn nobody_is_promoted_into_reporting_to_themself() {
        let draft = MovementInput {
            manager_id: Some(somebody()),
            ..promotion()
        };

        assert_eq!(draft.check(), Err(MovementError::ManagesThemself));
    }

    /// A promotion that changes nothing is a document saying nothing happened,
    /// and somebody will read it as evidence that something did.
    #[test]
    fn a_movement_that_moves_nobody_is_recognised_as_such() {
        let current = current();

        let unchanged = MovementInput {
            department_id: current.department_id,
            job_position_id: current.job_position_id,
            ..promotion()
        }
        .check()
        .expect("valid");

        assert!(!unchanged.changes_anything(Some(&current)));

        let promoted = promotion().check().expect("valid");
        assert!(promoted.changes_anything(Some(&current)));

        // Nothing to compare against: whatever it says is new.
        assert!(unchanged.changes_anything(None));
    }

    #[test]
    fn only_a_draft_may_be_edited() {
        assert!(MovementStatus::Draft.is_editable());
        assert!(!MovementStatus::Confirmed.is_editable());
        assert!(!MovementStatus::Cancelled.is_editable());
    }

    #[test]
    fn every_kind_and_status_survives_a_round_trip_through_its_column() {
        for kind in MovementKind::ALL {
            assert_eq!(MovementKind::parse(kind.as_str()), Some(*kind));
        }
        for status in MovementStatus::ALL {
            assert_eq!(MovementStatus::parse(status.as_str()), Some(*status));
        }

        assert_eq!(MovementKind::parse("onboarding"), None);
        assert_eq!(MovementStatus::parse("pending"), None);
    }
}
