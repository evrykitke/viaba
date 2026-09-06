//! One department: its details, and what has been done to it.

use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::permissions;
use uuid::Uuid;

use app_hr::department::DepartmentInput;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, Notice, PageHeader, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{department_edit, list_departments, manager_candidates};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::EntityForm;
use crate::ui::form::config::departments::department_form;
use crate::ui::tabs::{Tab, TabbedPanel};

#[component]
pub fn department_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let department_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(department_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => department_edit(id).await,
            Err(_) => Err(ServerFnError::new("That is not a department id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.department.singular")) />

        // Transition, not Suspense: moving between departments re-suspends, and
        // a fallback would blank the screen somebody is looking at.
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => view! { <DepartmentEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.department.singular")
                                    icon=Icon::Building2
                                    back=("/people/departments", l!("departments.title"))
                                />
                                <Notice
                                    message=Signal::derive(move || Some(err.to_string()))
                                    tone=Tone::Danger
                                />
                            </>
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
    }
}

#[component]
fn department_editor(draft: DepartmentInput) -> impl IntoView {
    let department_id = draft.id.unwrap_or_else(Uuid::nil);
    let title = draft.name.clone();
    let is_cost_centre = draft.is_cost_centre;
    let is_active = draft.is_active;

    // Hoisted above the tab strip: a tab's render closure runs again each time
    // it comes back on screen.
    let departments = Resource::new(|| (), |()| async move { list_departments().await });
    let managers = Resource::new(|| (), |()| async move { manager_candidates().await });
    let value = RwSignal::new(draft);

    let details_tab = Tab::new("details", "Details", move || {
        view! {
            <div class="max-w-3xl">
                <CollapsibleCard title=l!("departments.edit") icon=Icon::Building2 open=true>
                    <Transition fallback=|| {
                        view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                    }>
                        {move || Suspend::new(async move {
                            let departments = departments.await.unwrap_or_default();
                            let managers = managers.await.unwrap_or_default();

                            view! {
                                <EntityForm
                                    config=department_form(
                                        Some(department_id),
                                        departments,
                                        managers,
                                    )
                                    value=value.get_untracked()
                                />
                            }
                        })}
                    </Transition>
                </CollapsibleCard>
            </div>
        }
        .into_any()
    })
    .icon(Icon::SlidersHorizontal);

    let history_tab = Tab::new("history", "History", move || {
        view! { <RecordHistory kind=kinds::DEPARTMENT id=Some(department_id.to_string()) /> }
            .into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <PageHeader
            title=title
            icon=Icon::Building2
            back=("/people/departments", l!("departments.title"))
        >
            <div class="flex flex-wrap items-center gap-1.5">
                {is_cost_centre
                    .then(|| {
                        view! {
                            <Badge label=l!("departments.cost_centre.yes") tone=Tone::Success />
                        }
                    })}
                {(!is_active).then(|| view! { <Badge label=l!("common.inactive") /> })}
            </div>
        </PageHeader>

        <TabbedPanel id="department" tabs=vec![details_tab, history_tab] />
    }
}
