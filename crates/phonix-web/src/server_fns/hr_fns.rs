//! People: how the workspace is arranged, and what it charges to.
//!
//! There is deliberately no `list_cost_centres` endpoint, though Books and
//! Inventory both want that list: they reach `phonix_ports::CostCentres`
//! instead. An endpoint here would be the browser crossing an app boundary, and
//! the boundary would then exist only on the server. See ADR 0006 section 2.

use app_hr::department::DeleteOutcome;
use app_hr::department::{Department, DepartmentInput, DepartmentSummary};
use app_hr::employee::{
    AssignmentInput, Employee, EmployeeInput, EmployeeSummary, LeavingInput,
};
use app_hr::job_position::{JobPosition, JobPositionInput, JobPositionSummary};
use app_hr::work_location::{WorkLocation, WorkLocationInput, WorkLocationSummary};
use leptos::prelude::*;
use phonix_core::form::Submission;
use phonix_core::identity::InvitationIssued;
use uuid::Uuid;

/// Every department, already arranged into the tree.
#[server(name = ListDepartments, prefix = "/api", endpoint = "hr/departments")]
pub async fn list_departments() -> Result<Vec<DepartmentSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// One department.
#[server(name = DepartmentDetail, prefix = "/api", endpoint = "hr/departments/detail")]
pub async fn department_detail(department_id: Uuid) -> Result<Department, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::detail(&pool, &caller, department_id)
        .await
        .map_err(service_error)
}

/// The editable part of one, for the form to open on.
#[server(name = DepartmentEdit, prefix = "/api", endpoint = "hr/departments/edit")]
pub async fn department_edit(department_id: Uuid) -> Result<DepartmentInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::edit(&pool, &caller, department_id)
        .await
        .map_err(service_error)
}

/// Create a department or change one. Which, comes from the draft's own `id`.
#[server(name = SaveDepartment, prefix = "/api", endpoint = "hr/departments/save")]
pub async fn save_department(
    draft: DepartmentInput,
) -> Result<Submission<DepartmentInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Remove one. Comes back as `Ok(DeleteOutcome)`: two of the three answers are
/// things the screen renders beside the button, not faults.
#[server(name = DeleteDepartment, prefix = "/api", endpoint = "hr/departments/delete")]
pub async fn delete_department(department_id: Uuid) -> Result<DeleteOutcome, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::delete(&pool, &caller, department_id)
        .await
        .map_err(service_error)
}

/// How many departments there are, and how many are chargeable.
#[server(name = DepartmentCounts, prefix = "/api", endpoint = "hr/departments/counts")]
pub async fn department_counts() -> Result<(i64, i64), ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::counts(&pool, &caller)
        .await
        .map_err(service_error)
}

/// Who the manager picker may offer: a name and an id.
#[server(name = ManagerCandidates, prefix = "/api", endpoint = "hr/managers")]
pub async fn manager_candidates() -> Result<Vec<(Uuid, String)>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::department::manager_candidates(&pool, &caller)
        .await
        .map_err(service_error)
}

// --- People ----------------------------------------------------------------
//
// An employee record is a person plus a dated chain: engagements, and the
// assignments inside them. Nothing here edits that chain by accident - moving
// somebody, recording a leaver and rehiring them are three endpoints, because
// they are three acts with three different consequences.

#[server(name = ListEmployees, prefix = "/api", endpoint = "hr/employees")]
pub async fn list_employees() -> Result<Vec<EmployeeSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// One person, with every engagement and every assignment.
///
/// Personal details are stripped by the service for a caller without
/// `Employees.Personal`, rather than hidden by the screen - a field the browser
/// merely does not draw is one that was still sent to it.
#[server(name = EmployeeDetail, prefix = "/api", endpoint = "hr/employees/detail")]
pub async fn employee_detail(employee_id: Uuid) -> Result<Employee, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::detail(&pool, &caller, employee_id)
        .await
        .map_err(service_error)
}

#[server(name = EmployeeEdit, prefix = "/api", endpoint = "hr/employees/edit")]
pub async fn employee_edit(employee_id: Uuid) -> Result<EmployeeInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::edit(&pool, &caller, employee_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankEmployee, prefix = "/api", endpoint = "hr/employees/blank")]
pub async fn blank_employee() -> Result<EmployeeInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (_pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::blank(&caller).map_err(service_error)
}

/// Hire somebody, or correct who they are.
///
/// On create this writes the person, their first engagement and their first
/// assignment together, because hiring somebody is one act.
#[server(name = SaveEmployee, prefix = "/api", endpoint = "hr/employees/save")]
pub async fn save_employee(
    draft: EmployeeInput,
) -> Result<Submission<EmployeeInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

/// Somebody's current assignment, for the move form to open on.
#[server(name = EmployeeAssignment, prefix = "/api", endpoint = "hr/employees/assignment")]
pub async fn employee_assignment(employee_id: Uuid) -> Result<AssignmentInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::current_assignment(&pool, &caller, employee_id)
        .await
        .map_err(service_error)
}

/// Move somebody. Closes the open assignment and opens a new one.
#[server(name = MoveEmployee, prefix = "/api", endpoint = "hr/employees/move")]
pub async fn move_employee(
    employee_id: Uuid,
    draft: AssignmentInput,
) -> Result<Submission<Employee>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::move_to(&pool, &caller, employee_id, draft)
        .await
        .map_err(service_error)
}

/// End somebody's employment. The record is kept, so a rehire is a second
/// period rather than a second person.
#[server(name = RecordLeaver, prefix = "/api", endpoint = "hr/employees/leave")]
pub async fn record_leaver(
    employee_id: Uuid,
    draft: LeavingInput,
) -> Result<Submission<Employee>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::record_leaver(&pool, &caller, employee_id, draft)
        .await
        .map_err(service_error)
}

#[server(name = RehireEmployee, prefix = "/api", endpoint = "hr/employees/rehire")]
pub async fn rehire_employee(
    employee_id: Uuid,
    draft: EmployeeInput,
) -> Result<Submission<Employee>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::rehire(&pool, &caller, employee_id, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteEmployee, prefix = "/api", endpoint = "hr/employees/delete")]
pub async fn delete_employee(employee_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::delete(&pool, &caller, employee_id)
        .await
        .map_err(service_error)
}

/// Everybody currently employed, for a manager picker.
///
/// Employees rather than users: most managers never sign in, and a reporting
/// line that only exists for people with accounts is an org chart with holes.
#[server(name = EmployedPeople, prefix = "/api", endpoint = "hr/employees/employed")]
pub async fn employed_people() -> Result<Vec<(Uuid, String, String)>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::employed(&pool, &caller)
        .await
        .map_err(service_error)
}

/// How many people currently report to somebody. Read before a leaver is
/// recorded, so the screen can say who has to be reassigned.
#[server(name = DirectReports, prefix = "/api", endpoint = "hr/employees/reports")]
pub async fn direct_reports(employee_id: Uuid) -> Result<i64, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::direct_reports(&pool, &caller, employee_id)
        .await
        .map_err(service_error)
}

// --- The login -------------------------------------------------------------
//
// Not everybody who works here signs in, so this is a deliberate per-person
// act rather than something that happens when an employee is created. It goes
// through the ordinary invitation flow: the person sets their own password,
// and `Users.Create` is checked on top of the HR permission - so an HR grant by
// itself can never let anybody into the system.

#[server(name = CreateEmployeeLogin, prefix = "/api", endpoint = "hr/employees/login")]
pub async fn create_employee_login(
    employee_id: Uuid,
    roles: Vec<String>,
) -> Result<Submission<InvitationIssued>, ServerFnError> {
    use crate::state::{inviting_context, pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;
    let (state, tenant) = inviting_context().await?;

    phonix_services::hr::employee::create_login(
        &pool,
        &caller,
        &phonix_services::identity::invitation::Inviting {
            config: &state.config,
            hasher: &state.hasher,
            vault: &state.vault,
            workspace_slug: tenant.slug.as_str(),
            workspace_name: &tenant.display_name,
        },
        employee_id,
        roles,
    )
    .await
    .map_err(service_error)
}

/// Detach a login from a person, leaving the account alone.
///
/// For the link made against the wrong employee. Closing the account is a
/// different act under a different permission.
#[server(name = UnlinkEmployeeLogin, prefix = "/api", endpoint = "hr/employees/login/unlink")]
pub async fn unlink_employee_login(employee_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::employee::unlink_login(&pool, &caller, employee_id)
        .await
        .map_err(service_error)
}

// --- Roles -----------------------------------------------------------------

#[server(name = ListJobPositions, prefix = "/api", endpoint = "hr/roles")]
pub async fn list_job_positions() -> Result<Vec<JobPositionSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::job_position::list(&pool, &caller)
        .await
        .map_err(service_error)
}

/// The ones a form may offer.
#[server(name = SelectableJobPositions, prefix = "/api", endpoint = "hr/roles/selectable")]
pub async fn selectable_job_positions() -> Result<Vec<JobPosition>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::job_position::selectable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = JobPositionEdit, prefix = "/api", endpoint = "hr/roles/edit")]
pub async fn job_position_edit(position_id: Uuid) -> Result<JobPositionInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::job_position::edit(&pool, &caller, position_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankJobPosition, prefix = "/api", endpoint = "hr/roles/blank")]
pub async fn blank_job_position() -> Result<JobPositionInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (_pool, caller) = pool_and_caller().await?;

    phonix_services::hr::job_position::blank(&caller).map_err(service_error)
}

#[server(name = SaveJobPosition, prefix = "/api", endpoint = "hr/roles/save")]
pub async fn save_job_position(
    draft: JobPositionInput,
) -> Result<Submission<JobPositionInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::job_position::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteJobPosition, prefix = "/api", endpoint = "hr/roles/delete")]
pub async fn delete_job_position(position_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::job_position::delete(&pool, &caller, position_id)
        .await
        .map_err(service_error)
}

// --- Places ----------------------------------------------------------------

#[server(name = ListWorkLocations, prefix = "/api", endpoint = "hr/places")]
pub async fn list_work_locations() -> Result<Vec<WorkLocationSummary>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::work_location::list(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = SelectableWorkLocations, prefix = "/api", endpoint = "hr/places/selectable")]
pub async fn selectable_work_locations() -> Result<Vec<WorkLocation>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::work_location::selectable(&pool, &caller)
        .await
        .map_err(service_error)
}

#[server(name = WorkLocationEdit, prefix = "/api", endpoint = "hr/places/edit")]
pub async fn work_location_edit(location_id: Uuid) -> Result<WorkLocationInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::work_location::edit(&pool, &caller, location_id)
        .await
        .map_err(service_error)
}

#[server(name = BlankWorkLocation, prefix = "/api", endpoint = "hr/places/blank")]
pub async fn blank_work_location() -> Result<WorkLocationInput, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (_pool, caller) = pool_and_caller().await?;

    phonix_services::hr::work_location::blank(&caller).map_err(service_error)
}

#[server(name = SaveWorkLocation, prefix = "/api", endpoint = "hr/places/save")]
pub async fn save_work_location(
    draft: WorkLocationInput,
) -> Result<Submission<WorkLocationInput>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::work_location::save(&pool, &caller, draft)
        .await
        .map_err(service_error)
}

#[server(name = DeleteWorkLocation, prefix = "/api", endpoint = "hr/places/delete")]
pub async fn delete_work_location(location_id: Uuid) -> Result<Submission<()>, ServerFnError> {
    use crate::state::{pool_and_caller, service_error};

    let (pool, caller) = pool_and_caller().await?;

    phonix_services::hr::work_location::delete(&pool, &caller, location_id)
        .await
        .map_err(service_error)
}
