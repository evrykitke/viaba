//! People: how the workspace is arranged, and what it charges to.
//!
//! There is deliberately no `list_cost_centres` endpoint, though Books and
//! Inventory both want that list: they reach `phonix_ports::CostCentres`
//! instead. An endpoint here would be the browser crossing an app boundary, and
//! the boundary would then exist only on the server. See ADR 0006 section 2.

use app_hr::department::DeleteOutcome;
use app_hr::department::{Department, DepartmentInput, DepartmentSummary};
use leptos::prelude::*;
use phonix_core::form::Submission;
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
