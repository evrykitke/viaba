//! The two reference records an employee's assignment points at: a role, and a
//! place.
//!
//! Both are small forms with the same shape - a generated code, a name, a
//! delete refused once anybody has ever been assigned. They share a file
//! because separating them would be two files of the same eighty lines, and
//! neither is where a decision lives.

use app_hr::job_position::JobPositionInput;
use app_hr::work_location::{LocationKind, WorkLocationInput};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{GhostButton, Notice, PageHeader, Panel, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{
    blank_job_position, blank_work_location, delete_job_position, delete_work_location,
    job_position_edit, list_departments, save_job_position, save_work_location, work_location_edit,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const ROLES: &str = "/people/roles";
const PLACES: &str = "/people/places";

// --- roles -----------------------------------------------------------------

#[component]
pub fn job_position_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(raw, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => job_position_edit(id).await,
            Err(_) => blank_job_position().await,
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.job_position.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => {
                        let heading = if draft.id.is_some() {
                            draft.title.clone()
                        } else {
                            l!("job_positions.new")
                        };

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    icon=Icon::ListChecks
                                    back=(ROLES, l!("job_positions.title"))
                                />
                                <JobPositionForm draft=draft />
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.job_position.singular")
                                    icon=Icon::ListChecks
                                    back=(ROLES, l!("job_positions.title"))
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
fn job_position_form(draft: JobPositionInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let departments = Resource::new(|| (), |()| async move { list_departments().await });

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_job_position(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("job_positions.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/people/roles/{id}"),
                                leptos_router::NavigateOptions {
                                    replace: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                    Ok(Submission::Rejected(errors)) => {
                        rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                    }
                    Err(err) => rejected.set(Some(err.to_string())),
                }
            });
        }
    };

    let remove = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("job_positions.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_job_position(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("job_positions.deleted")));
                                navigate(ROLES, leptos_router::NavigateOptions::default());
                            }
                            Ok(Submission::Rejected(errors)) => {
                                if let Some(error) = errors.first() {
                                    alerts.post(Alert::warning(crate::i18n::t(&error.message)));
                                }
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(l!("common.delete"))
                .confirm_label(l!("common.delete")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());
    let id = move || draft.with(|d| d.id);

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Panel title=l!("entity.job_position.singular")>
                <div class="grid gap-3 sm:grid-cols-2">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("job_positions.job_title")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.title.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.title = value);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("job_positions.code")}
                        </span>
                        <input
                            type="text"
                            class="w-full font-mono"
                            prop:value=move || draft.with(|d| d.code.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.code = value);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("employees.code.help")}
                        </span>
                    </label>

                    <div class="block space-y-1">
                        <label
                            for="role-department"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("job_positions.department")}
                        </label>
                        <Transition fallback=|| ()>
                            {move || Suspend::new(async move {
                                let options = departments
                                    .await
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter(|department| department.is_active)
                                    .map(|department| {
                                        Choice::new(department.id.to_string(), department.name)
                                            .detail(department.code)
                                    })
                                    .collect::<Vec<_>>();

                                view! {
                                    <SelectField
                                        id="role-department"
                                        value=Signal::derive(move || {
                                            draft
                                                .with(|d| {
                                                    d.department_id
                                                        .map(|id| id.to_string())
                                                        .unwrap_or_default()
                                                })
                                        })
                                        on_change=Callback::new(move |value: String| {
                                            let chosen = value.parse::<Uuid>().ok();
                                            draft.update(|d| d.department_id = chosen);
                                        })
                                        options=options
                                        placeholder=l!("common.not_set")
                                        clearable=true
                                        label=l!("job_positions.department")
                                    />
                                }
                            })}
                        </Transition>
                        <span class="block text-2xs text-content-subtle">
                            {l!("job_positions.department.help")}
                        </span>
                    </div>

                    <label class="flex items-center gap-2 self-end pb-2">
                        <input
                            type="checkbox"
                            prop:checked=move || draft.with(|d| d.is_active)
                            on:change=move |ev| {
                                let on = event_target_checked(&ev);
                                draft.update(|d| d.is_active = on);
                            }
                        />
                        <span class="text-sm text-content">{l!("common.active")}</span>
                    </label>
                </div>

                <label class="mt-3 block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("job_positions.description")}
                    </span>
                    <textarea
                        class="w-full"
                        rows="3"
                        prop:value=move || draft.with(|d| d.description.clone())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            draft.update(|d| d.description = value);
                        }
                    />
                </label>
            </Panel>

            <div class="flex flex-wrap items-center justify-end gap-2">
                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("common.delete")
                        icon=Icon::Trash2
                        on_click=Callback::new({
                            let remove = remove.clone();
                            move |()| remove()
                        })
                    />
                </Show>

                <PrimaryButton
                    label=l!("common.save")
                    icon=Icon::Save
                    pending=Signal::derive(move || saving.get())
                    on_click=Callback::new({
                        let save = save.clone();
                        move |()| save()
                    })
                />
            </div>

            <Show when=saved fallback=|| ()>
                <Panel title=l!("common.history")>
                    <RecordHistory
                        kind=kinds::JOB_POSITION
                        id=id().map(|id| id.to_string())
                    />
                </Panel>
            </Show>
        </div>
    }
}

// --- places ----------------------------------------------------------------

#[component]
pub fn work_location_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(raw, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => work_location_edit(id).await,
            Err(_) => blank_work_location().await,
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.work_location.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => {
                        let heading = if draft.id.is_some() {
                            draft.name.clone()
                        } else {
                            l!("work_locations.new")
                        };

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    icon=Icon::Warehouse
                                    back=(PLACES, l!("work_locations.title"))
                                />
                                <WorkLocationForm draft=draft />
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.work_location.singular")
                                    icon=Icon::Warehouse
                                    back=(PLACES, l!("work_locations.title"))
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
fn work_location_form(draft: WorkLocationInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let kinds_options = LocationKind::ALL
        .iter()
        .map(|kind| Choice::new(kind.as_str(), crate::i18n::t(&kind.label())))
        .collect::<Vec<_>>();

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_work_location(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("work_locations.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/people/places/{id}"),
                                leptos_router::NavigateOptions {
                                    replace: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                    Ok(Submission::Rejected(errors)) => {
                        rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                    }
                    Err(err) => rejected.set(Some(err.to_string())),
                }
            });
        }
    };

    let remove = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("work_locations.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_work_location(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("work_locations.deleted")));
                                navigate(PLACES, leptos_router::NavigateOptions::default());
                            }
                            Ok(Submission::Rejected(errors)) => {
                                if let Some(error) = errors.first() {
                                    alerts.post(Alert::warning(crate::i18n::t(&error.message)));
                                }
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(l!("common.delete"))
                .confirm_label(l!("common.delete")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());
    let id = move || draft.with(|d| d.id);

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Panel title=l!("entity.work_location.singular")>
                <div class="grid gap-3 sm:grid-cols-2">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("work_locations.name")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.name.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.name = value);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("work_locations.code")}
                        </span>
                        <input
                            type="text"
                            class="w-full font-mono"
                            prop:value=move || draft.with(|d| d.code.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.code = value);
                            }
                        />
                    </label>

                    <div class="block space-y-1">
                        <label
                            for="place-kind"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("work_locations.kind")}
                        </label>
                        <SelectField
                            id="place-kind"
                            value=Signal::derive(move || {
                                draft.with(|d| d.kind.as_str().to_owned())
                            })
                            on_change=Callback::new(move |value: String| {
                                if let Some(kind) = LocationKind::parse(&value) {
                                    draft.update(|d| d.kind = kind);
                                }
                            })
                            options=kinds_options
                            placeholder=l!("common.not_set")
                            label=l!("work_locations.kind")
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("work_locations.kind.help")}
                        </span>
                    </div>

                    <label class="flex items-center gap-2 self-end pb-2">
                        <input
                            type="checkbox"
                            prop:checked=move || draft.with(|d| d.is_active)
                            on:change=move |ev| {
                                let on = event_target_checked(&ev);
                                draft.update(|d| d.is_active = on);
                            }
                        />
                        <span class="text-sm text-content">{l!("common.active")}</span>
                    </label>
                </div>

                <label class="mt-3 block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("work_locations.address")}
                    </span>
                    <textarea
                        class="w-full"
                        rows="3"
                        prop:value=move || draft.with(|d| d.address.clone())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            draft.update(|d| d.address = value);
                        }
                    />
                </label>
            </Panel>

            <div class="flex flex-wrap items-center justify-end gap-2">
                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("common.delete")
                        icon=Icon::Trash2
                        on_click=Callback::new({
                            let remove = remove.clone();
                            move |()| remove()
                        })
                    />
                </Show>

                <PrimaryButton
                    label=l!("common.save")
                    icon=Icon::Save
                    pending=Signal::derive(move || saving.get())
                    on_click=Callback::new({
                        let save = save.clone();
                        move |()| save()
                    })
                />
            </div>

            <Show when=saved fallback=|| ()>
                <Panel title=l!("common.history")>
                    <RecordHistory
                        kind=kinds::WORK_LOCATION
                        id=id().map(|id| id.to_string())
                    />
                </Panel>
            </Show>
        </div>
    }
}
