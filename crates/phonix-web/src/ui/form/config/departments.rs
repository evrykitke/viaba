//! The department form.
//!
//! The code field is offered empty: blank means the service allocates one,
//! typed means it is used as typed. It is not previewed — promising `DEPT-004`
//! before the row exists promises a number somebody else may take first (ADR
//! 0001 section 5 rule 3).
//!
//! The parent picker only leaves out the row being edited. Cycles and depth
//! need the whole tree, so the service checks those.

use app_hr::department::{DepartmentInput, DepartmentSummary};
use phonix_core::identity::UserId;
use phonix_core::permissions;
use uuid::Uuid;

use super::FormConfig;
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::save_department;
use crate::ui::form::{Choice, Field, FieldValue, FormAction, Then};

/// What "not chosen" is worth, as a select option.
///
/// An empty value rather than a sentinel word: it round-trips through `parse`
/// as a failure, which is exactly `None`.
const NOT_SET: &str = "";

/// Where a department sits, and whether anything is charged to it.
///
/// `departments` is the tree as the screen has it, for the parent picker, and
/// `managers` is who could run it. Passing both in rather than fetching here
/// keeps this a pure description of a form.
pub fn department_form(
    editing: Option<Uuid>,
    departments: Vec<DepartmentSummary>,
    managers: Vec<(UserId, String)>,
) -> FormConfig<DepartmentInput> {
    FormConfig::new("department", |draft: DepartmentInput| async move {
        save_department(draft).await
    })
    .field(
        Field::text("name", l!("field.name"), |m: &DepartmentInput| {
            FieldValue::text(&m.name)
        })
        .writing(|m, value| m.name = value.as_input())
        .placeholder("Finance")
        .require(permissions::DEPARTMENTS_EDIT)
        .required(),
    )
    .field(
        Field::text("code", l!("field.code"), |m: &DepartmentInput| {
            FieldValue::text(&m.code)
        })
        .writing(|m, value| m.code = value.as_input())
        // No placeholder: grey `DEPT-004` reads as a promise.
        .help(l!("departments.code_help"))
        .require(permissions::DEPARTMENTS_EDIT),
    )
    .field(
        Field::select(
            "parent_id",
            l!("departments.parent"),
            parent_choices(editing, &departments),
            |m: &DepartmentInput| {
                FieldValue::choice(
                    m.parent_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| NOT_SET.to_owned()),
                )
            },
        )
        // Anything unparseable becomes top level, which is the empty choice.
        .writing(|m, value| {
            m.parent_id = value
                .as_choice()
                .filter(|raw| !raw.is_empty())
                .and_then(|raw| Uuid::parse_str(raw).ok());
        })
        .none_label(l!("departments.parent.none"))
        .require(permissions::DEPARTMENTS_EDIT),
    )
    .field(
        Field::toggle(
            "is_cost_centre",
            l!("departments.cost_centre"),
            |m: &DepartmentInput| FieldValue::Bool(m.is_cost_centre),
        )
        .writing(|m, value| m.is_cost_centre = value.as_bool())
        // The one control here that changes what other apps may do.
        .help(l!("departments.cost_centre.hint"))
        .require(permissions::DEPARTMENTS_EDIT),
    )
    .field(
        Field::select(
            "manager_user_id",
            l!("departments.manager"),
            manager_choices(&managers),
            |m: &DepartmentInput| {
                FieldValue::choice(
                    m.manager_user_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| NOT_SET.to_owned()),
                )
            },
        )
        .writing(|m, value| {
            m.manager_user_id = value
                .as_choice()
                .filter(|raw| !raw.is_empty())
                .and_then(|raw| Uuid::parse_str(raw).ok())
                .map(UserId::from);
        })
        .none_label(l!("departments.manager.none"))
        .require(permissions::DEPARTMENTS_EDIT),
    )
    .field(
        Field::toggle("is_active", l!("field.status"), |m: &DepartmentInput| {
            FieldValue::Bool(m.is_active)
        })
        .writing(|m, value| m.is_active = value.as_bool())
        .require(permissions::DEPARTMENTS_EDIT),
    )
    .action(
        FormAction::submit(l!("common.save"))
            .icon(Icon::Save)
            .then(Then::Say("Department saved."))
            .require(permissions::DEPARTMENTS_EDIT),
    )
}

/// Everywhere a department could sit, indented by depth. Inactive rows are
/// kept: a live department under a retired division is an ordinary state.
fn parent_choices(editing: Option<Uuid>, departments: &[DepartmentSummary]) -> Vec<Choice> {
    let mut choices = Vec::new();

    // Its descendants are not left out — the service checks those against the
    // tree, and a picker that half-enforced the rule would be trusted.
    choices.extend(
        departments
            .iter()
            .filter(|row| Some(row.id) != editing)
            .map(|row| {
                // Non-breaking: a select collapses ordinary whitespace runs.
                let indent = "\u{00a0}\u{00a0}".repeat(row.depth.min(4) as usize);
                Choice::new(row.id.to_string(), format!("{indent}{}", row.name))
            }),
    );

    choices
}

/// Who could run it.
fn manager_choices(managers: &[(UserId, String)]) -> Vec<Choice> {
    let mut choices = Vec::new();

    choices.extend(
        managers
            .iter()
            .map(|(id, name)| Choice::new(id.to_string(), name.clone())),
    );

    choices
}
