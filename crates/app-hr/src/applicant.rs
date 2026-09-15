//! Who has applied, against the vacancies [`crate::job_position`] already
//! makes queryable.
//!
//! An applicant is not an employee. Most never become one, so they are not
//! [`crate::employee::Employee`] rows with a flag on them — and the one who
//! *is* hired becomes an engagement on whichever record they already have,
//! rather than a second person. That is the duplicate-identity case the whole
//! of `0002_people.sql` was written to refuse.

use chrono::NaiveDate;
use phonix_core::Message;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_NAME_LEN: usize = 100;
pub const MAX_EMAIL_LEN: usize = 320;
pub const MAX_PHONE_LEN: usize = 40;
pub const MAX_SOURCE_LEN: usize = 200;
pub const MAX_NOTE_LEN: usize = 2000;

/// Where somebody is in the pipeline.
///
/// A closed list, named for what is happening rather than for how it feels. A
/// workspace wanting its own stages wants a table of them and a per-stage
/// order, which is a bigger thing — and having the closed list first is what
/// makes the bigger thing answerable rather than a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Applied,
    Screening,
    Interview,
    Offer,
    Hired,
    Rejected,
    Withdrawn,
}

impl Stage {
    pub const ALL: &'static [Self] = &[
        Self::Applied,
        Self::Screening,
        Self::Interview,
        Self::Offer,
        Self::Hired,
        Self::Rejected,
        Self::Withdrawn,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Screening => "screening",
            Self::Interview => "interview",
            Self::Offer => "offer",
            Self::Hired => "hired",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|it| it.as_str() == raw)
    }

    /// Whether this application is still in play.
    ///
    /// The three that are not are the three that end it, however they end it.
    pub const fn is_open(self) -> bool {
        !matches!(self, Self::Hired | Self::Rejected | Self::Withdrawn)
    }

    /// Whether somebody may still be moved out of this stage by hand.
    ///
    /// A hire is the one that cannot: it opened an engagement, and undoing it
    /// by changing a word on a form would leave the employment behind.
    pub const fn is_movable(self) -> bool {
        !matches!(self, Self::Hired)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Applied => msg!("applicants.stage.applied"),
            Self::Screening => msg!("applicants.stage.screening"),
            Self::Interview => msg!("applicants.stage.interview"),
            Self::Offer => msg!("applicants.stage.offer"),
            Self::Hired => msg!("applicants.stage.hired"),
            Self::Rejected => msg!("applicants.stage.rejected"),
            Self::Withdrawn => msg!("applicants.stage.withdrawn"),
        }
    }
}

/// One application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applicant {
    pub id: Uuid,
    pub job_position_id: Uuid,
    pub job_title: String,
    pub given_name: String,
    pub family_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub stage: Stage,
    pub source: Option<String>,
    pub applied_on: NaiveDate,
    pub note: Option<String>,
    /// Who they became, once hired. `None` until then.
    pub employee_id: Option<Uuid>,
}

impl Applicant {
    /// What to call them.
    pub fn display_name(&self) -> String {
        format!("{} {}", self.given_name.trim(), self.family_name.trim())
    }
}

/// What a hire did.
///
/// `rejoined` is the fact the screen has to say out loud: somebody hiring a
/// returning employee needs to know they did not create a second record, and
/// somebody hiring a stranger needs to know they did. Silence reads as the
/// first and is wrong half the time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hired {
    pub employee_id: Uuid,
    /// True where the application matched an employee already on file, so
    /// this opened a second engagement rather than a second person.
    pub rejoined: bool,
}

/// A list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicantSummary {
    pub id: Uuid,
    pub job_position_id: Uuid,
    pub job_title: String,
    pub given_name: String,
    pub family_name: String,
    pub email: Option<String>,
    pub stage: Stage,
    pub applied_on: NaiveDate,
    pub employee_id: Option<Uuid>,
}

impl ApplicantSummary {
    pub fn display_name(&self) -> String {
        format!("{} {}", self.given_name.trim(), self.family_name.trim())
    }
}

/// An application being written on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicantInput {
    pub id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub given_name: String,
    pub family_name: String,
    pub email: String,
    pub phone: String,
    pub stage: Stage,
    pub source: String,
    pub applied_on: Option<NaiveDate>,
    pub note: String,
}

impl ApplicantInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            job_position_id: None,
            given_name: String::new(),
            family_name: String::new(),
            email: String::new(),
            phone: String::new(),
            stage: Stage::Applied,
            source: String::new(),
            applied_on: Some(today),
            note: String::new(),
        }
    }

    pub fn from_applicant(applicant: &Applicant) -> Self {
        Self {
            id: Some(applicant.id),
            job_position_id: Some(applicant.job_position_id),
            given_name: applicant.given_name.clone(),
            family_name: applicant.family_name.clone(),
            email: applicant.email.clone().unwrap_or_default(),
            phone: applicant.phone.clone().unwrap_or_default(),
            stage: applicant.stage,
            source: applicant.source.clone().unwrap_or_default(),
            applied_on: Some(applicant.applied_on),
            note: applicant.note.clone().unwrap_or_default(),
        }
    }

    pub fn check(&self) -> Result<CheckedApplicant, ApplicantError> {
        let Some(job_position_id) = self.job_position_id else {
            return Err(ApplicantError::PositionRequired);
        };

        let Some(applied_on) = self.applied_on else {
            return Err(ApplicantError::DateRequired);
        };

        let given_name = self.given_name.trim();
        let family_name = self.family_name.trim();

        if given_name.is_empty() || family_name.is_empty() {
            return Err(ApplicantError::NameRequired);
        }
        if given_name.chars().count() > MAX_NAME_LEN || family_name.chars().count() > MAX_NAME_LEN {
            return Err(ApplicantError::NameTooLong);
        }

        let email = crate::non_empty(&self.email);

        // The column refuses a malformed one too. Checked here so the screen
        // can say which field rather than handing back a constraint.
        if let Some(email) = &email
            && (email.chars().count() > MAX_EMAIL_LEN || !is_email_shaped(email))
        {
            return Err(ApplicantError::EmailMalformed);
        }

        if self.phone.chars().count() > MAX_PHONE_LEN {
            return Err(ApplicantError::PhoneTooLong);
        }
        if self.source.chars().count() > MAX_SOURCE_LEN {
            return Err(ApplicantError::SourceTooLong);
        }
        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(ApplicantError::NoteTooLong);
        }

        // Hiring is its own act, with its own permission and its own
        // consequence: it opens an engagement. Reaching it by editing a form
        // would open none, and the record would claim an employment nobody has.
        if self.stage == Stage::Hired {
            return Err(ApplicantError::HireIsItsOwnAct);
        }

        Ok(CheckedApplicant {
            id: self.id,
            job_position_id,
            given_name: given_name.to_owned(),
            family_name: family_name.to_owned(),
            email,
            phone: crate::non_empty(&self.phone),
            stage: self.stage,
            source: crate::non_empty(&self.source),
            applied_on,
            note: crate::non_empty(&self.note),
        })
    }
}

/// The same shape the column's CHECK enforces: something, an at, something.
fn is_email_shaped(raw: &str) -> bool {
    match raw.split_once('@') {
        Some((before, after)) => !before.is_empty() && !after.is_empty(),
        None => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedApplicant {
    pub id: Option<Uuid>,
    pub job_position_id: Uuid,
    pub given_name: String,
    pub family_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub stage: Stage,
    pub source: Option<String>,
    pub applied_on: NaiveDate,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ApplicantError {
    #[error("an application needs the job it is for")]
    PositionRequired,
    #[error("an application needs the day it arrived")]
    DateRequired,
    #[error("an applicant needs a name")]
    NameRequired,
    #[error("a name is at most 100 characters")]
    NameTooLong,
    #[error("that is not an email address")]
    EmailMalformed,
    #[error("a phone number is at most 40 characters")]
    PhoneTooLong,
    #[error("a source is at most 200 characters")]
    SourceTooLong,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("hiring is its own act, not a stage to be typed")]
    HireIsItsOwnAct,
    #[error("this application has already been closed")]
    AlreadyClosed,
    #[error("somebody hired cannot be moved back")]
    NotMovable,
    #[error("that application is not here any more")]
    Gone,
}

impl ApplicantError {
    pub fn field(self) -> &'static str {
        match self {
            Self::PositionRequired => "job_position_id",
            Self::DateRequired => "applied_on",
            Self::NameRequired | Self::NameTooLong => "given_name",
            Self::EmailMalformed => "email",
            Self::PhoneTooLong => "phone",
            Self::SourceTooLong => "source",
            Self::NoteTooLong => "note",
            Self::HireIsItsOwnAct | Self::AlreadyClosed | Self::NotMovable => "stage",
            Self::Gone => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::PositionRequired => msg!("applicants.error.position_required"),
            Self::DateRequired => msg!("applicants.error.date_required"),
            Self::NameRequired => msg!("applicants.error.name_required"),
            Self::NameTooLong => msg!("applicants.error.name_too_long"),
            Self::EmailMalformed => msg!("applicants.error.email_malformed"),
            Self::PhoneTooLong => msg!("applicants.error.phone_too_long"),
            Self::SourceTooLong => msg!("applicants.error.source_too_long"),
            Self::NoteTooLong => msg!("applicants.error.note_too_long"),
            Self::HireIsItsOwnAct => msg!("applicants.error.hire_is_its_own_act"),
            Self::AlreadyClosed => msg!("applicants.error.already_closed"),
            Self::NotMovable => msg!("applicants.error.not_movable"),
            Self::Gone => msg!("applicants.error.gone"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).expect("a real date")
    }

    fn applied() -> ApplicantInput {
        ApplicantInput {
            job_position_id: Some(Uuid::from_u128(5)),
            given_name: "  Ada ".to_owned(),
            family_name: "Lovelace".to_owned(),
            email: "ada@example.com".to_owned(),
            ..ApplicantInput::blank(on(3, 1))
        }
    }

    /// The three stages that end an application end it however they end it,
    /// and every other one is still in play.
    #[test]
    fn an_open_stage_is_one_that_has_not_ended() {
        assert!(Stage::Applied.is_open());
        assert!(Stage::Screening.is_open());
        assert!(Stage::Interview.is_open());
        assert!(Stage::Offer.is_open());

        assert!(!Stage::Hired.is_open());
        assert!(!Stage::Rejected.is_open());
        assert!(!Stage::Withdrawn.is_open());
    }

    /// Hiring opened an engagement. Moving out of it by editing a form would
    /// leave the employment behind and the record claiming neither.
    #[test]
    fn a_hire_is_the_one_stage_nobody_is_moved_out_of() {
        assert!(!Stage::Hired.is_movable());

        for stage in Stage::ALL.iter().filter(|stage| **stage != Stage::Hired) {
            assert!(stage.is_movable(), "{stage:?} should be movable");
        }
    }

    /// And it cannot be reached by typing either, for the same reason.
    #[test]
    fn hiring_cannot_be_typed_into_the_form() {
        let draft = ApplicantInput {
            stage: Stage::Hired,
            ..applied()
        };

        assert_eq!(draft.check(), Err(ApplicantError::HireIsItsOwnAct));
    }

    #[test]
    fn a_name_is_trimmed_and_both_halves_are_required() {
        let checked = applied().check().expect("valid");
        assert_eq!(checked.given_name, "Ada");

        let half = ApplicantInput {
            family_name: "   ".to_owned(),
            ..applied()
        };
        assert_eq!(half.check(), Err(ApplicantError::NameRequired));
    }

    #[test]
    fn an_address_without_an_at_is_refused() {
        let draft = ApplicantInput {
            email: "ada.example.com".to_owned(),
            ..applied()
        };

        assert_eq!(draft.check(), Err(ApplicantError::EmailMalformed));

        // Absent is fine: a walk-in may have left a phone number instead.
        let none = ApplicantInput {
            email: "  ".to_owned(),
            ..applied()
        };
        assert_eq!(none.check().expect("valid").email, None);
    }

    #[test]
    fn an_application_needs_the_job_it_is_for() {
        let draft = ApplicantInput {
            job_position_id: None,
            ..applied()
        };

        assert_eq!(draft.check(), Err(ApplicantError::PositionRequired));
    }

    #[test]
    fn every_stage_survives_a_round_trip_through_the_column() {
        for stage in Stage::ALL {
            assert_eq!(Stage::parse(stage.as_str()), Some(*stage));
        }

        assert_eq!(Stage::parse("ghosted"), None);
    }
}
