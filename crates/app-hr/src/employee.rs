//! A person, one or more periods of employment, and what they were doing
//! during each.
//!
//! # Why this is three types and not one
//!
//! The single most common failure in HR systems is carrying the *current
//! state* - job title, department, manager - as fields on the person, and then
//! having no record of when any of it changed. The current answer stays right,
//! which is what makes it dangerous: tenure analysis, cost-centre reporting and
//! every "what did this team look like last March" question break silently.
//!
//! So [`Employee`] holds only what stays true when somebody is promoted, and
//! everything that moves is a dated [`Assignment`].
//!
//! # An engagement is not an assignment
//!
//! An [`Engagement`] is a period of employment - hired on a date, ended on a
//! date, for a reason. An [`Assignment`] is what somebody was doing during part
//! of one.
//!
//! Fusing them is how systems get rehires wrong: model employment as a flag on
//! the person and somebody who leaves and comes back either loses their first
//! stint or becomes a second person - two national insurance numbers for one
//! human being, and a tenure figure that restarts at zero. Here a rehire is a
//! second engagement, so the history is continuous and the person stays one row.
//!
//! # "Is this person employed" is a question, not a flag
//!
//! It is [`Engagement::is_open`] - no end date. There is deliberately no
//! `is_active` on an employee: a flag and a set of dates are two facts about
//! one thing, and the first time somebody backdates a leaving date without
//! clearing the flag they disagree for ever.

use chrono::NaiveDate;
use phonix_core::Message;
use phonix_core::identity::UserId;
use phonix_core::msg;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_EMPLOYEE_CODE_LEN: usize = 40;
pub const MAX_NAME_LEN: usize = 100;
pub const MAX_EMAIL_LEN: usize = 320;
pub const MAX_PHONE_LEN: usize = 40;
pub const MAX_NATIONAL_ID_LEN: usize = 60;
pub const MAX_NOTE_LEN: usize = 2000;
pub const MAX_REASON_LEN: usize = 500;

/// How deep a reporting line may go before this refuses to walk it further.
///
/// A cycle - A reports to B who reports to A - cannot be seen from one row, so
/// the service walks the chain when a manager is chosen. Sixteen is already an
/// organization with more layers than anybody defends.
pub const MAX_REPORTING_DEPTH: usize = 16;

// ---------------------------------------------------------------------------
// The person
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Employee {
    pub id: Uuid,
    pub code: String,
    pub given_name: String,
    pub family_name: String,
    pub preferred_name: Option<String>,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    /// The login, where there is one. See the module docs of the migration for
    /// why this is optional in both directions and unique.
    pub user_id: Option<UserId>,
    pub date_of_birth: Option<NaiveDate>,
    pub national_id: Option<String>,
    pub note: Option<String>,
    /// Newest first, so `engagements.first()` is the current or most recent.
    pub engagements: Vec<Engagement>,
}

impl Employee {
    /// What to call them: the name they go by, falling back to the given name.
    pub fn display_name(&self) -> String {
        let first = self
            .preferred_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.given_name);

        format!("{first} {}", self.family_name)
    }

    /// How a list sorts and a report cites them.
    pub fn sort_name(&self) -> String {
        format!("{}, {}", self.family_name, self.given_name)
    }

    /// The period of employment they are in now, if any.
    pub fn current_engagement(&self) -> Option<&Engagement> {
        self.engagements.iter().find(|engagement| engagement.is_open())
    }

    /// Whether they work here now.
    pub fn is_employed(&self) -> bool {
        self.current_engagement().is_some()
    }

    /// What they are doing now.
    pub fn current_assignment(&self) -> Option<&Assignment> {
        self.current_engagement()
            .and_then(Engagement::current_assignment)
    }

    /// Whether a login may be created for them.
    ///
    /// Three conditions, and each refuses a different mistake: somebody who has
    /// already got one must not get a second (one login is one person), a
    /// former employee must not be given fresh access on their way out, and an
    /// invitation needs an address to go to.
    pub fn can_be_invited(&self) -> bool {
        self.user_id.is_none() && self.is_employed() && self.work_email.is_some()
    }

    /// Why they cannot be invited, for a screen that has to say so.
    pub fn invitation_blocker(&self) -> Option<EmployeeError> {
        if self.user_id.is_some() {
            return Some(EmployeeError::AlreadyHasLogin);
        }
        if !self.is_employed() {
            return Some(EmployeeError::NotEmployed);
        }
        if self.work_email.is_none() {
            return Some(EmployeeError::WorkEmailRequiredForLogin);
        }

        None
    }

    /// Whole years since they first started here, counting every engagement's
    /// span - which is the point of keeping them.
    ///
    /// `as_at` rather than "today" because this crate compiles to wasm and a
    /// domain type that reads the clock is one that cannot be tested.
    pub fn service_days(&self, as_at: NaiveDate) -> i64 {
        self.engagements
            .iter()
            .map(|engagement| engagement.days(as_at))
            .sum()
    }
}

/// One row of the people grid, drawn from `current_staff` plus the leavers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmployeeSummary {
    pub id: Uuid,
    pub code: String,
    pub given_name: String,
    pub family_name: String,
    pub preferred_name: Option<String>,
    pub work_email: Option<String>,
    pub has_login: bool,
    /// `None` for somebody who has left, which is what makes the state column
    /// derivable rather than stored.
    pub started_on: Option<NaiveDate>,
    pub employment_type: Option<EmploymentType>,
    pub department_name: Option<String>,
    pub job_title: Option<String>,
    pub manager_name: Option<String>,
    pub work_location_name: Option<String>,
}

impl EmployeeSummary {
    pub fn display_name(&self) -> String {
        let first = self
            .preferred_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.given_name);

        format!("{first} {}", self.family_name)
    }

    pub const fn is_employed(&self) -> bool {
        self.started_on.is_some()
    }
}

// ---------------------------------------------------------------------------
// One period of employment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmploymentType {
    Permanent,
    FixedTerm,
    Contract,
    Intern,
    Casual,
    Apprentice,
}

impl EmploymentType {
    pub const ALL: &'static [Self] = &[
        Self::Permanent,
        Self::FixedTerm,
        Self::Contract,
        Self::Intern,
        Self::Casual,
        Self::Apprentice,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
            Self::FixedTerm => "fixed_term",
            Self::Contract => "contract",
            Self::Intern => "intern",
            Self::Casual => "casual",
            Self::Apprentice => "apprentice",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == raw)
    }

    /// Whether an end date was agreed at the start. What makes
    /// `expected_end_on` worth asking for.
    pub const fn is_time_limited(self) -> bool {
        matches!(self, Self::FixedTerm | Self::Contract | Self::Intern)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Permanent => msg!("employees.type.permanent"),
            Self::FixedTerm => msg!("employees.type.fixed_term"),
            Self::Contract => msg!("employees.type.contract"),
            Self::Intern => msg!("employees.type.intern"),
            Self::Casual => msg!("employees.type.casual"),
            Self::Apprentice => msg!("employees.type.apprentice"),
        }
    }
}

/// Why an engagement ended.
///
/// Required the moment there is an end date. "Fifteen leavers and no reason on
/// any of them" is a named symptom of an HR system nobody can report from, and
/// it is the rule this codebase already applies to a requisition's decision: an
/// outcome carries its reason or it is not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Resigned,
    Dismissed,
    Redundancy,
    EndOfContract,
    Retirement,
    Died,
    Transferred,
    Other,
}

impl EndReason {
    pub const ALL: &'static [Self] = &[
        Self::Resigned,
        Self::Dismissed,
        Self::Redundancy,
        Self::EndOfContract,
        Self::Retirement,
        Self::Died,
        Self::Transferred,
        Self::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resigned => "resigned",
            Self::Dismissed => "dismissed",
            Self::Redundancy => "redundancy",
            Self::EndOfContract => "end_of_contract",
            Self::Retirement => "retirement",
            Self::Died => "died",
            Self::Transferred => "transferred",
            Self::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == raw)
    }

    /// Whether the person chose to go. The split every turnover figure is
    /// reported on, and getting it from a list rather than from free text is
    /// the only reason the list exists.
    pub const fn is_voluntary(self) -> bool {
        matches!(self, Self::Resigned | Self::Retirement)
    }

    pub fn label(self) -> Message {
        match self {
            Self::Resigned => msg!("employees.end_reason.resigned"),
            Self::Dismissed => msg!("employees.end_reason.dismissed"),
            Self::Redundancy => msg!("employees.end_reason.redundancy"),
            Self::EndOfContract => msg!("employees.end_reason.end_of_contract"),
            Self::Retirement => msg!("employees.end_reason.retirement"),
            Self::Died => msg!("employees.end_reason.died"),
            Self::Transferred => msg!("employees.end_reason.transferred"),
            Self::Other => msg!("employees.end_reason.unstated"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engagement {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub started_on: NaiveDate,
    /// `None` means still employed. The only place that fact lives.
    pub ended_on: Option<NaiveDate>,
    pub end_reason: Option<EndReason>,
    pub end_note: Option<String>,
    pub employment_type: EmploymentType,
    /// When a fixed term was agreed to run to, as distinct from when it did.
    pub expected_end_on: Option<NaiveDate>,
    pub note: Option<String>,
    /// Newest first.
    pub assignments: Vec<Assignment>,
}

impl Engagement {
    pub const fn is_open(&self) -> bool {
        self.ended_on.is_none()
    }

    /// What the person was doing at the end of this engagement, or is doing now.
    pub fn current_assignment(&self) -> Option<&Assignment> {
        self.assignments
            .iter()
            .find(|assignment| assignment.is_open())
            .or_else(|| self.assignments.first())
    }

    /// How long this engagement ran, in days, up to `as_at` for an open one.
    ///
    /// Zero rather than negative for an engagement that has not started: a
    /// future hire has served no time, and a negative figure summed across
    /// engagements would silently reduce somebody's tenure.
    pub fn days(&self, as_at: NaiveDate) -> i64 {
        let until = self.ended_on.unwrap_or(as_at).min(as_at);

        (until - self.started_on).num_days().max(0)
    }

    /// Whether a fixed term has run past the date it was agreed to end on.
    ///
    /// Worth a warning rather than a refusal: contracts get extended verbally
    /// and written up late, and refusing to show the person would not make the
    /// paperwork appear.
    pub fn is_overrunning(&self, as_at: NaiveDate) -> bool {
        self.is_open()
            .then(|| self.expected_end_on.is_some_and(|expected| expected < as_at))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// What somebody was doing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub effective_from: NaiveDate,
    /// `None` means current.
    pub effective_to: Option<NaiveDate>,
    pub department_id: Option<Uuid>,
    pub department_name: Option<String>,
    pub job_position_id: Option<Uuid>,
    pub job_title: Option<String>,
    pub work_location_id: Option<Uuid>,
    pub work_location_name: Option<String>,
    /// Who they report to, as an employee rather than a user: most managers do
    /// not have a login, and a reporting line that only exists for people with
    /// accounts is an org chart with holes in it.
    pub manager_id: Option<Uuid>,
    pub manager_name: Option<String>,
    pub reason: Option<String>,
}

impl Assignment {
    pub const fn is_open(&self) -> bool {
        self.effective_to.is_none()
    }
}

// ---------------------------------------------------------------------------
// The forms
// ---------------------------------------------------------------------------

/// The editable part of a person. Nothing dated is here - see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmployeeInput {
    pub id: Option<Uuid>,
    /// Empty on create means "allocate one".
    pub code: String,
    pub given_name: String,
    pub family_name: String,
    pub preferred_name: String,
    pub work_email: String,
    pub work_phone: String,
    pub date_of_birth: Option<NaiveDate>,
    pub national_id: String,
    pub note: String,
    /// Only read on create: the first engagement, so hiring somebody is one
    /// form rather than two. Ignored on edit, where the engagement has its own.
    pub started_on: Option<NaiveDate>,
    pub employment_type: EmploymentType,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
}

impl EmployeeInput {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            id: None,
            code: String::new(),
            given_name: String::new(),
            family_name: String::new(),
            preferred_name: String::new(),
            work_email: String::new(),
            work_phone: String::new(),
            date_of_birth: None,
            national_id: String::new(),
            note: String::new(),
            started_on: Some(today),
            employment_type: EmploymentType::Permanent,
            department_id: None,
            job_position_id: None,
            work_location_id: None,
            manager_id: None,
        }
    }

    pub fn from_employee(employee: &Employee, today: NaiveDate) -> Self {
        let current = employee.current_engagement();
        let assignment = employee.current_assignment();

        Self {
            id: Some(employee.id),
            code: employee.code.clone(),
            given_name: employee.given_name.clone(),
            family_name: employee.family_name.clone(),
            preferred_name: employee.preferred_name.clone().unwrap_or_default(),
            work_email: employee.work_email.clone().unwrap_or_default(),
            work_phone: employee.work_phone.clone().unwrap_or_default(),
            date_of_birth: employee.date_of_birth,
            national_id: employee.national_id.clone().unwrap_or_default(),
            note: employee.note.clone().unwrap_or_default(),
            started_on: current.map(|engagement| engagement.started_on),
            employment_type: current
                .map_or(EmploymentType::Permanent, |engagement| {
                    engagement.employment_type
                }),
            department_id: assignment.and_then(|a| a.department_id),
            job_position_id: assignment.and_then(|a| a.job_position_id),
            work_location_id: assignment.and_then(|a| a.work_location_id),
            manager_id: assignment.and_then(|a| a.manager_id),
            ..Self::blank(today)
        }
    }

    pub fn check(&self) -> Result<CheckedEmployee, EmployeeError> {
        let given_name = self.given_name.trim();
        if given_name.is_empty() {
            return Err(EmployeeError::GivenNameRequired);
        }
        if given_name.chars().count() > MAX_NAME_LEN {
            return Err(EmployeeError::NameTooLong);
        }

        let family_name = self.family_name.trim();
        if family_name.is_empty() {
            return Err(EmployeeError::FamilyNameRequired);
        }
        if family_name.chars().count() > MAX_NAME_LEN {
            return Err(EmployeeError::NameTooLong);
        }

        if self.preferred_name.trim().chars().count() > MAX_NAME_LEN {
            return Err(EmployeeError::NameTooLong);
        }

        let code = self.code.trim();
        if code.chars().count() > MAX_EMPLOYEE_CODE_LEN {
            return Err(EmployeeError::CodeTooLong);
        }
        if !code.is_empty() && !crate::is_code_shaped(code) {
            return Err(EmployeeError::CodeMalformed);
        }

        let work_email = crate::non_empty(&self.work_email);
        if let Some(email) = &work_email {
            if email.chars().count() > MAX_EMAIL_LEN || !is_email_shaped(email) {
                return Err(EmployeeError::EmailMalformed);
            }
        }

        if self.work_phone.trim().chars().count() > MAX_PHONE_LEN {
            return Err(EmployeeError::PhoneTooLong);
        }
        if self.national_id.trim().chars().count() > MAX_NATIONAL_ID_LEN {
            return Err(EmployeeError::NationalIdTooLong);
        }
        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(EmployeeError::NoteTooLong);
        }

        // Somebody born after today is a typing error, and it is one that
        // quietly produces a negative age on every report that shows one.
        // Checked here as well as by the column so the message names the field.
        if let (Some(born), Some(started)) = (self.date_of_birth, self.started_on) {
            if born >= started {
                return Err(EmployeeError::BornAfterStarting);
            }
        }

        // Only on create. An edit does not move the start date - that is the
        // engagement's own form, because moving it is a different act with
        // different consequences.
        let started_on = match (self.id, self.started_on) {
            (None, None) => return Err(EmployeeError::StartDateRequired),
            (_, started) => started,
        };

        Ok(CheckedEmployee {
            id: self.id,
            code: code.to_owned(),
            given_name: given_name.to_owned(),
            family_name: family_name.to_owned(),
            preferred_name: crate::non_empty(&self.preferred_name),
            work_email,
            work_phone: crate::non_empty(&self.work_phone),
            date_of_birth: self.date_of_birth,
            national_id: crate::non_empty(&self.national_id),
            note: crate::non_empty(&self.note),
            started_on,
            employment_type: self.employment_type,
            department_id: self.department_id,
            job_position_id: self.job_position_id,
            work_location_id: self.work_location_id,
            manager_id: self.manager_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedEmployee {
    pub id: Option<Uuid>,
    pub code: String,
    pub given_name: String,
    pub family_name: String,
    pub preferred_name: Option<String>,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub national_id: Option<String>,
    pub note: Option<String>,
    /// Present on create, where it opens the first engagement.
    pub started_on: Option<NaiveDate>,
    pub employment_type: EmploymentType,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
}

/// Moving somebody: a new assignment, which closes the one before it.
///
/// A separate form from the employee's because it is a separate act. Editing
/// the person's name is a correction; moving them to another department is a
/// fact with a date, and conflating the two is how the history gets overwritten
/// by somebody fixing a typo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentInput {
    pub effective_from: NaiveDate,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub reason: String,
}

impl AssignmentInput {
    /// Pre-filled from what somebody is doing now, so a move only has to change
    /// the thing that moved.
    pub fn next(current: Option<&Assignment>, on: NaiveDate) -> Self {
        Self {
            effective_from: on,
            department_id: current.and_then(|a| a.department_id),
            job_position_id: current.and_then(|a| a.job_position_id),
            work_location_id: current.and_then(|a| a.work_location_id),
            manager_id: current.and_then(|a| a.manager_id),
            reason: String::new(),
        }
    }

    pub fn check(&self) -> Result<CheckedAssignment, EmployeeError> {
        if self.reason.trim().chars().count() > MAX_REASON_LEN {
            return Err(EmployeeError::ReasonTooLong);
        }

        Ok(CheckedAssignment {
            effective_from: self.effective_from,
            department_id: self.department_id,
            job_position_id: self.job_position_id,
            work_location_id: self.work_location_id,
            manager_id: self.manager_id,
            reason: crate::non_empty(&self.reason),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedAssignment {
    pub effective_from: NaiveDate,
    pub department_id: Option<Uuid>,
    pub job_position_id: Option<Uuid>,
    pub work_location_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub reason: Option<String>,
}

/// Ending an engagement. Both fields, always - see [`EndReason`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeavingInput {
    pub ended_on: NaiveDate,
    pub reason: EndReason,
    pub note: String,
}

impl LeavingInput {
    pub fn check(&self, started_on: NaiveDate) -> Result<CheckedLeaving, EmployeeError> {
        if self.ended_on < started_on {
            return Err(EmployeeError::EndsBeforeStarting);
        }
        if self.note.chars().count() > MAX_NOTE_LEN {
            return Err(EmployeeError::NoteTooLong);
        }

        Ok(CheckedLeaving {
            ended_on: self.ended_on,
            reason: self.reason,
            note: crate::non_empty(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedLeaving {
    pub ended_on: NaiveDate,
    pub reason: EndReason,
    pub note: Option<String>,
}

/// The same rule `employees_work_email_shape` applies.
fn is_email_shaped(raw: &str) -> bool {
    let mut parts = raw.split('@');

    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();

    parts.next().is_none()
        && !local.is_empty()
        && !domain.is_empty()
        && !raw.chars().any(char::is_whitespace)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EmployeeError {
    #[error("a person needs a given name")]
    GivenNameRequired,
    #[error("a person needs a family name")]
    FamilyNameRequired,
    #[error("a name is at most 100 characters")]
    NameTooLong,
    #[error("a code is at most 40 characters")]
    CodeTooLong,
    #[error("a code may hold only letters, digits, hyphens and underscores")]
    CodeMalformed,
    #[error("that code is already in use")]
    CodeTaken,
    #[error("that is not an email address")]
    EmailMalformed,
    #[error("a phone number is at most 40 characters")]
    PhoneTooLong,
    #[error("an identifier is at most 60 characters")]
    NationalIdTooLong,
    #[error("that identifier already belongs to somebody else")]
    NationalIdTaken,
    #[error("a note is at most 2000 characters")]
    NoteTooLong,
    #[error("a reason is at most 500 characters")]
    ReasonTooLong,
    #[error("a new employee needs a start date")]
    StartDateRequired,
    #[error("somebody cannot be born on or after the day they started")]
    BornAfterStarting,
    #[error("an engagement cannot end before it started")]
    EndsBeforeStarting,
    #[error("that person does not work here")]
    NotEmployed,
    #[error("that person is already employed")]
    AlreadyEmployed,
    #[error("that person already has a login")]
    AlreadyHasLogin,
    #[error("a login needs a work email address to send the invitation to")]
    WorkEmailRequiredForLogin,
    #[error("that would make somebody their own manager, directly or through a chain")]
    ReportingCycle,
    #[error("that manager does not work here")]
    ManagerNotEmployed,
    #[error("an assignment cannot start before the engagement it belongs to")]
    AssignmentBeforeEngagement,
    #[error("that person has records against them and cannot be deleted")]
    HasHistory,
}

impl EmployeeError {
    pub fn field(self) -> &'static str {
        match self {
            Self::GivenNameRequired => "given_name",
            Self::FamilyNameRequired => "family_name",
            Self::NameTooLong => "given_name",
            Self::CodeTooLong | Self::CodeMalformed | Self::CodeTaken => "code",
            Self::EmailMalformed | Self::WorkEmailRequiredForLogin => "work_email",
            Self::PhoneTooLong => "work_phone",
            Self::NationalIdTooLong | Self::NationalIdTaken => "national_id",
            Self::NoteTooLong => "note",
            Self::ReasonTooLong => "reason",
            Self::StartDateRequired | Self::BornAfterStarting => "started_on",
            Self::EndsBeforeStarting => "ended_on",
            Self::ReportingCycle | Self::ManagerNotEmployed => "manager_id",
            Self::AssignmentBeforeEngagement => "effective_from",
            Self::NotEmployed
            | Self::AlreadyEmployed
            | Self::AlreadyHasLogin
            | Self::HasHistory => "id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::GivenNameRequired => msg!("employees.error.given_name_required"),
            Self::FamilyNameRequired => msg!("employees.error.family_name_required"),
            Self::NameTooLong => msg!("employees.error.name_too_long"),
            Self::CodeTooLong => msg!("employees.error.code_too_long"),
            Self::CodeMalformed => msg!("employees.error.code_malformed"),
            Self::CodeTaken => msg!("employees.error.code_taken"),
            Self::EmailMalformed => msg!("employees.error.email_malformed"),
            Self::PhoneTooLong => msg!("employees.error.phone_too_long"),
            Self::NationalIdTooLong => msg!("employees.error.national_id_too_long"),
            Self::NationalIdTaken => msg!("employees.error.national_id_taken"),
            Self::NoteTooLong => msg!("employees.error.note_too_long"),
            Self::ReasonTooLong => msg!("employees.error.reason_too_long"),
            Self::StartDateRequired => msg!("employees.error.start_date_required"),
            Self::BornAfterStarting => msg!("employees.error.born_after_starting"),
            Self::EndsBeforeStarting => msg!("employees.error.ends_before_starting"),
            Self::NotEmployed => msg!("employees.error.not_employed"),
            Self::AlreadyEmployed => msg!("employees.error.already_employed"),
            Self::AlreadyHasLogin => msg!("employees.error.already_has_login"),
            Self::WorkEmailRequiredForLogin => msg!("employees.error.work_email_required_for_login"),
            Self::ReportingCycle => msg!("employees.error.reporting_cycle"),
            Self::ManagerNotEmployed => msg!("employees.error.manager_not_employed"),
            Self::AssignmentBeforeEngagement => msg!("employees.error.assignment_before_engagement"),
            Self::HasHistory => msg!("employees.error.has_history"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
    }

    fn engagement(started: NaiveDate, ended: Option<NaiveDate>) -> Engagement {
        Engagement {
            id: Uuid::from_u128(1),
            employee_id: Uuid::from_u128(9),
            started_on: started,
            ended_on: ended,
            end_reason: ended.map(|_| EndReason::Resigned),
            end_note: None,
            employment_type: EmploymentType::Permanent,
            expected_end_on: None,
            note: None,
            assignments: Vec::new(),
        }
    }

    fn person(engagements: Vec<Engagement>) -> Employee {
        Employee {
            id: Uuid::from_u128(9),
            code: "EMP-0001".to_owned(),
            given_name: "Margaret".to_owned(),
            family_name: "Okonkwo".to_owned(),
            preferred_name: None,
            work_email: Some("margaret@example.test".to_owned()),
            work_phone: None,
            user_id: None,
            date_of_birth: None,
            national_id: None,
            note: None,
            engagements,
        }
    }

    #[test]
    fn a_preferred_name_is_what_somebody_is_called() {
        let plain = person(Vec::new());
        assert_eq!(plain.display_name(), "Margaret Okonkwo");

        let known_as = Employee {
            preferred_name: Some("Maggie".to_owned()),
            ..plain
        };
        assert_eq!(known_as.display_name(), "Maggie Okonkwo");
        // The sort name keeps the legal one: a list sorted by what people like
        // to be called is one nobody can find anybody in.
        assert_eq!(known_as.sort_name(), "Okonkwo, Margaret");
    }

    #[test]
    fn a_blank_preferred_name_falls_back_rather_than_showing_a_gap() {
        let known_as = Employee {
            preferred_name: Some("   ".to_owned()),
            ..person(Vec::new())
        };

        assert_eq!(known_as.display_name(), "Margaret Okonkwo");
    }

    #[test]
    fn employment_is_an_open_engagement_and_not_a_flag() {
        let leaver = person(vec![engagement(on(2020, 1, 1), Some(on(2022, 6, 30)))]);
        assert!(!leaver.is_employed());

        let current = person(vec![engagement(on(2020, 1, 1), None)]);
        assert!(current.is_employed());
    }

    #[test]
    fn a_rehire_keeps_both_stints_in_the_service_total() {
        let rehired = person(vec![
            // Newest first, as the store returns them.
            engagement(on(2024, 1, 1), None),
            engagement(on(2020, 1, 1), Some(on(2021, 1, 1))),
        ]);

        let today = on(2025, 1, 1);
        // 366 days for the first stint (2020 is a leap year) plus 366 for the
        // second up to today. The point is that the first is not lost.
        assert_eq!(rehired.service_days(today), 366 + 366);
        assert!(rehired.is_employed());
    }

    #[test]
    fn a_future_hire_has_served_no_time_rather_than_a_negative_amount() {
        let starting_next_month = engagement(on(2026, 12, 1), None);

        assert_eq!(starting_next_month.days(on(2026, 9, 9)), 0);
    }

    #[test]
    fn an_engagement_that_ended_stops_counting_at_its_end_date() {
        let past = engagement(on(2020, 1, 1), Some(on(2020, 1, 11)));

        assert_eq!(past.days(on(2026, 9, 9)), 10);
    }

    #[test]
    fn somebody_can_be_invited_only_when_employed_addressable_and_without_a_login() {
        let ready = person(vec![engagement(on(2020, 1, 1), None)]);
        assert!(ready.can_be_invited());
        assert_eq!(ready.invitation_blocker(), None);

        let has_one = Employee {
            user_id: Some(Uuid::from_u128(3)),
            ..person(vec![engagement(on(2020, 1, 1), None)])
        };
        assert!(!has_one.can_be_invited());
        assert_eq!(
            has_one.invitation_blocker(),
            Some(EmployeeError::AlreadyHasLogin)
        );

        let gone = person(vec![engagement(on(2020, 1, 1), Some(on(2021, 1, 1)))]);
        assert_eq!(gone.invitation_blocker(), Some(EmployeeError::NotEmployed));

        let unreachable = Employee {
            work_email: None,
            ..person(vec![engagement(on(2020, 1, 1), None)])
        };
        assert_eq!(
            unreachable.invitation_blocker(),
            Some(EmployeeError::WorkEmailRequiredForLogin)
        );
    }

    #[test]
    fn a_new_employee_needs_both_names() {
        let today = on(2026, 9, 9);

        let no_given = EmployeeInput {
            family_name: "Okonkwo".to_owned(),
            ..EmployeeInput::blank(today)
        };
        assert_eq!(no_given.check(), Err(EmployeeError::GivenNameRequired));

        let no_family = EmployeeInput {
            given_name: "Margaret".to_owned(),
            ..EmployeeInput::blank(today)
        };
        assert_eq!(no_family.check(), Err(EmployeeError::FamilyNameRequired));
    }

    #[test]
    fn a_new_employee_needs_a_start_date_and_an_edit_does_not() {
        let today = on(2026, 9, 9);

        let creating = EmployeeInput {
            given_name: "Margaret".to_owned(),
            family_name: "Okonkwo".to_owned(),
            started_on: None,
            ..EmployeeInput::blank(today)
        };
        assert_eq!(creating.check(), Err(EmployeeError::StartDateRequired));

        // Editing somebody who has left: there is no open engagement to take a
        // start date from, and the form must still save a corrected surname.
        let editing = EmployeeInput {
            id: Some(Uuid::from_u128(9)),
            ..creating
        };
        assert!(editing.check().is_ok());
    }

    #[test]
    fn somebody_cannot_be_born_after_they_started() {
        let today = on(2026, 9, 9);
        let draft = EmployeeInput {
            given_name: "Margaret".to_owned(),
            family_name: "Okonkwo".to_owned(),
            date_of_birth: Some(on(2026, 1, 1)),
            started_on: Some(on(2020, 1, 1)),
            ..EmployeeInput::blank(today)
        };

        assert_eq!(draft.check(), Err(EmployeeError::BornAfterStarting));
    }

    #[test]
    fn a_malformed_work_email_is_refused_and_a_blank_one_is_not() {
        let today = on(2026, 9, 9);
        let base = EmployeeInput {
            given_name: "Margaret".to_owned(),
            family_name: "Okonkwo".to_owned(),
            ..EmployeeInput::blank(today)
        };

        let bad = EmployeeInput {
            work_email: "not an address".to_owned(),
            ..base.clone()
        };
        assert_eq!(bad.check(), Err(EmployeeError::EmailMalformed));

        // Blank is allowed: most of a warehouse has no work address, and
        // demanding one would be demanding a fiction.
        assert_eq!(base.check().expect("valid").work_email, None);
    }

    #[test]
    fn ending_an_engagement_before_it_started_is_refused() {
        let leaving = LeavingInput {
            ended_on: on(2019, 1, 1),
            reason: EndReason::Resigned,
            note: String::new(),
        };

        assert_eq!(
            leaving.check(on(2020, 1, 1)),
            Err(EmployeeError::EndsBeforeStarting)
        );
        assert!(leaving.check(on(2018, 1, 1)).is_ok());
    }

    #[test]
    fn only_resignation_and_retirement_count_as_voluntary() {
        assert!(EndReason::Resigned.is_voluntary());
        assert!(EndReason::Retirement.is_voluntary());
        assert!(!EndReason::Dismissed.is_voluntary());
        assert!(!EndReason::Redundancy.is_voluntary());
    }

    #[test]
    fn every_stored_value_survives_a_round_trip_through_its_column() {
        for kind in EmploymentType::ALL {
            assert_eq!(EmploymentType::parse(kind.as_str()), Some(*kind));
        }
        for reason in EndReason::ALL {
            assert_eq!(EndReason::parse(reason.as_str()), Some(*reason));
        }

        assert_eq!(EmploymentType::parse("freelance"), None);
        assert_eq!(EndReason::parse("vanished"), None);
    }

    #[test]
    fn a_move_starts_from_what_somebody_is_doing_now() {
        let current = Assignment {
            id: Uuid::from_u128(2),
            engagement_id: Uuid::from_u128(1),
            effective_from: on(2020, 1, 1),
            effective_to: None,
            department_id: Some(Uuid::from_u128(4)),
            department_name: Some("Clinic A".to_owned()),
            job_position_id: Some(Uuid::from_u128(5)),
            job_title: Some("Staff Nurse".to_owned()),
            work_location_id: None,
            work_location_name: None,
            manager_id: Some(Uuid::from_u128(6)),
            manager_name: None,
            reason: None,
        };

        let next = AssignmentInput::next(Some(&current), on(2026, 9, 9));

        // Everything carried but the date, so a move only has to change what
        // moved.
        assert_eq!(next.department_id, current.department_id);
        assert_eq!(next.job_position_id, current.job_position_id);
        assert_eq!(next.manager_id, current.manager_id);
        assert_eq!(next.effective_from, on(2026, 9, 9));
        assert!(next.reason.is_empty());
    }

    #[test]
    fn a_fixed_term_past_its_agreed_end_is_flagged_but_an_ended_one_is_not() {
        let overrunning = Engagement {
            employment_type: EmploymentType::FixedTerm,
            expected_end_on: Some(on(2026, 1, 1)),
            ..engagement(on(2025, 1, 1), None)
        };
        assert!(overrunning.is_overrunning(on(2026, 9, 9)));

        let finished = Engagement {
            ended_on: Some(on(2026, 2, 1)),
            end_reason: Some(EndReason::EndOfContract),
            ..overrunning
        };
        assert!(!finished.is_overrunning(on(2026, 9, 9)));
    }

    #[test]
    fn only_a_time_limited_engagement_wants_an_expected_end_date() {
        assert!(EmploymentType::FixedTerm.is_time_limited());
        assert!(EmploymentType::Contract.is_time_limited());
        assert!(EmploymentType::Intern.is_time_limited());
        assert!(!EmploymentType::Permanent.is_time_limited());
        assert!(!EmploymentType::Casual.is_time_limited());
    }
}
