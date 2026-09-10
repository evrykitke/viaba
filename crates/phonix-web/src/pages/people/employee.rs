//! One person: who they are, what they have done here, and the login some of
//! them get.
//!
//! # The form edits the person; the buttons edit the history
//!
//! Saving changes a name, an address, a phone number. Moving somebody between
//! departments, recording a leaver and rehiring them are three separate acts
//! with three separate panels, because each writes a dated row and none of them
//! should be reachable by somebody correcting a typo.
//!
//! That is the whole reason this screen is not one form. A system where
//! changing a department is a dropdown on the edit page is a system where the
//! history is one careless save away from being wrong - and being wrong
//! silently, because the current answer still looks right.
//!
//! # Not everybody can sign in
//!
//! Most people who work somewhere never do. The login panel is a deliberate,
//! per-person act; it is absent from the create form entirely, and it says why
//! it cannot be used when it cannot rather than being hidden.
//!
//! The invitation goes to the person's work email and they set their own
//! password, so nobody - including whoever pressed the button - ever knows it.

use app_hr::employee::{
    Assignment, AssignmentInput, Employee, EmployeeInput, EmploymentType, Engagement, EndReason,
    LeavingInput,
};
use chrono::NaiveDate;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::permissions;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{
    blank_employee, create_employee_login, delete_employee, direct_reports, employed_people,
    employee_assignment, employee_detail, list_departments, move_employee, record_leaver,
    rehire_employee, save_employee, selectable_job_positions, selectable_work_locations,
    unlink_employee_login,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;
use crate::ui::tabs::{Tab, TabbedPanel};

const BACK: &str = "/people/employees";

/// Everything the pickers on this screen need, fetched once.
#[derive(Clone, Default)]
struct Choices {
    departments: Vec<Choice>,
    roles: Vec<Choice>,
    places: Vec<Choice>,
    managers: Vec<Choice>,
}

#[component]
pub fn employee_new_page() -> impl IntoView {
    let blank = Resource::new(|| (), |()| async move { blank_employee().await });

    view! {
        <Title text=format!("{} | Phonix", l!("employees.new")) />

        <PageHeader
            title=l!("employees.new")
            subtitle=l!("employees.new.subtitle")
            icon=Icon::UserPlus
            back=(BACK, l!("employees.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match blank.await {
                    Ok(draft) => view! { <EmployeeForm draft=draft hiring=true /> }.into_any(),
                    Err(err) => {
                        view! {
                            <Notice
                                message=Signal::derive(move || Some(err.to_string()))
                                tone=Tone::Danger
                            />
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
    }
}

#[component]
pub fn employee_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    let employee = Resource::new(raw, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => employee_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not an employee id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.employee.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match employee.await {
                    Ok(stored) => {
                        let heading = stored.display_name();
                        let subtitle = stored.code.clone();
                        let employed = stored.is_employed();

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=subtitle
                                    icon=Icon::User
                                    back=(BACK, l!("employees.title"))
                                >
                                    <Badge
                                        label=if employed {
                                            l!("employees.state.employed")
                                        } else {
                                            l!("employees.state.left")
                                        }
                                        tone=if employed { Tone::Success } else { Tone::Neutral }
                                    />
                                </PageHeader>

                                <EmployeeRecord
                                    employee=stored
                                    reload=Callback::new(move |()| employee.refetch())
                                />
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.employee.singular")
                                    icon=Icon::User
                                    back=(BACK, l!("employees.title"))
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

// --- the record ------------------------------------------------------------

#[component]
fn employee_record(employee: Employee, reload: Callback<()>) -> impl IntoView {
    let id = employee.id;
    let employed = employee.is_employed();
    let has_login = employee.user_id.is_some();
    let can_invite = employee.can_be_invited();
    // Only a record with no employment at all - which in practice means one
    // saved by mistake and caught immediately. Anybody who has ever worked here
    // is recorded as a leaver, because deleting them would take their
    // assignment history with them.
    let deletable = employee.engagements.is_empty();

    // Held rather than moved: a tab renders every time it is shown, so what it
    // draws from has to survive being read more than once.
    let draft = StoredValue::new(EmployeeInput::from_employee(&employee, today()));
    let engagements = StoredValue::new(employee.engagements.clone());
    let blocker = StoredValue::new(
        employee
            .invitation_blocker()
            .map(|err| crate::i18n::t(&err.message())),
    );

    // Tabs, for the reason [`crate::ui::tabs`] gives: five panels stacked is a
    // screen whose history is a scroll away from the name it belongs to, with
    // two thirds of the viewport empty beside a narrow form.
    //
    // The grouping is by what the reader came to do, which on this screen is
    // also the line the module docs draw: correcting a record, changing where
    // somebody works, giving them a way in, and reading what has happened.
    let details_tab = Tab::new("details", l!("common.details"), move || {
        view! {
            <div class="space-y-3">
                <EmployeeForm draft=draft.get_value() hiring=false />

                <Show when=move || deletable fallback=|| ()>
                    <div class="flex flex-wrap items-center justify-end gap-2">
                        <DeleteButton employee_id=id />
                    </div>
                </Show>
            </div>
        }
        .into_any()
    })
    .icon(Icon::User);

    let employment_tab = Tab::new("employment", l!("employees.employment"), move || {
        view! {
            // Moving somebody and reading what they have done here are two
            // sections of one subject, not two cards.
            <Panel>
                <Show when=move || employed fallback=|| ()>
                    <MovePanel employee_id=id reload=reload />
                </Show>

                <EmploymentPanel
                    employee_id=id
                    engagements=engagements.get_value()
                    employed=employed
                    reload=reload
                />
            </Panel>
        }
        .into_any()
    })
    .icon(Icon::Building2);

    let login_tab = Tab::new("login", l!("employees.login"), move || {
        view! {
            <LoginPanel
                employee_id=id
                has_login=has_login
                can_invite=can_invite
                blocker=blocker.get_value()
                reload=reload
            />
        }
        .into_any()
    })
    .icon(Icon::KeyRound);

    let history_tab = Tab::new("history", l!("common.history"), move || {
        view! { <RecordHistory kind=kinds::EMPLOYEE id=Some(id.to_string()) /> }.into_any()
    })
    .icon(Icon::Clock)
    .require(permissions::AUDIT_LOGS);

    view! {
        <TabbedPanel
            id="employee"
            tabs=vec![details_tab, employment_tab, login_tab, history_tab]
        />
    }
}

/// Throw away a record created in error.
#[component]
fn delete_button(employee_id: Uuid) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let remove = move || {
        let navigate = navigate.clone();

        alerts.ask(
            Confirm::new(l!("employees.delete.confirm"), move || {
                let navigate = navigate.clone();

                leptos::task::spawn_local(async move {
                    match delete_employee(employee_id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("employees.deleted")));
                            navigate(BACK, leptos_router::NavigateOptions::default());
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
    };

    view! {
        <GhostButton
            label=l!("common.delete")
            icon=Icon::Trash2
            on_click=Callback::new(move |()| remove())
        />
    }
}

// --- the person ------------------------------------------------------------

#[component]
fn employee_form(draft: EmployeeInput, hiring: bool) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let departments = Resource::new(|| (), |()| async move { list_departments().await });
    let roles = Resource::new(|| (), |()| async move { selectable_job_positions().await });
    let places = Resource::new(|| (), |()| async move { selectable_work_locations().await });
    let managers = Resource::new(|| (), |()| async move { employed_people().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let choices = Choices {
                        departments: departments
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|department| department.is_active)
                            .map(|department| {
                                Choice::new(department.id.to_string(), department.name)
                                    .detail(department.code)
                            })
                            .collect(),
                        roles: roles
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|role| {
                                Choice::new(role.id.to_string(), role.title).detail(role.code)
                            })
                            .collect(),
                        places: places
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|place| {
                                Choice::new(place.id.to_string(), place.name).detail(place.code)
                            })
                            .collect(),
                        managers: managers
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(id, code, name)| Choice::new(id.to_string(), name).detail(code))
                            .collect(),
                    };

                    view! {
                        <EmployeeFields
                            draft=draft
                            hiring=hiring
                            choices=choices
                            saving=saving
                            rejected=rejected
                        />
                    }
                })}
            </Transition>
        </div>
    }
}

#[component]
fn employee_fields(
    draft: RwSignal<EmployeeInput>,
    hiring: bool,
    choices: Choices,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();
    let viewer = crate::ui::viewer::Viewer::get();

    // The one permission in this app that hides fields rather than buttons. A
    // rota needs to know who works here; it does not need a date of birth.
    let may_see_personal = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::EMPLOYEES_PERSONAL))
        })
    });

    let types = StoredValue::new(
        EmploymentType::ALL
            .iter()
            .map(|kind| Choice::new(kind.as_str(), crate::i18n::t(&kind.label())))
            .collect::<Vec<_>>(),
    );
    let choices = StoredValue::new(choices);

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_employee(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("employees.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/people/employees/{id}"),
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

    view! {
        // One card, sections inside: see `components::page`. Four bordered
        // blocks put four borders and eight edges of padding between a name
        // and a note, and a reader pays for all of it in scrolling.
        <Panel>
            <Section title=l!("employees.identity")>
                <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.given_name")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.given_name.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.given_name = value);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.family_name")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.family_name.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.family_name = value);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.preferred_name")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.preferred_name.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.preferred_name = value);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("employees.preferred_name.help")}
                        </span>
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.code")}
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

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.work_email")}
                        </span>
                        <input
                            type="email"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.work_email.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.work_email = value);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("employees.work_email.help")}
                        </span>
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.work_phone")}
                        </span>
                        <input
                            type="tel"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.work_phone.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.work_phone = value);
                            }
                        />
                    </label>
                </div>
            </Section>

            // Absent, not merely disabled, for a caller without the permission:
            // the service strips these fields before they reach the browser, so
            // drawing an empty box would be drawing a lie.
            <Show when=move || may_see_personal.get() fallback=|| ()>
                <Section title=l!("employees.personal")>
                    <div class="grid gap-3 sm:grid-cols-2">
                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("employees.date_of_birth")}
                            </span>
                            <input
                                type="date"
                                class="w-full"
                                prop:value=move || {
                                    draft
                                        .with(|d| {
                                            d.date_of_birth
                                                .map(|on| on.to_string())
                                                .unwrap_or_default()
                                        })
                                }
                                on:change=move |ev| {
                                    let value = event_target_value(&ev);
                                    draft.update(|d| d.date_of_birth = value.parse().ok());
                                }
                            />
                        </label>

                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("employees.national_id")}
                            </span>
                            <input
                                type="text"
                                class="w-full"
                                prop:value=move || draft.with(|d| d.national_id.clone())
                                on:input=move |ev| {
                                    let value = event_target_value(&ev);
                                    draft.update(|d| d.national_id = value);
                                }
                            />
                            <span class="block text-2xs text-content-subtle">
                                {l!("employees.national_id.help")}
                            </span>
                        </label>
                    </div>
                </Section>
            </Show>

            // Only when hiring. On an existing record these are the current
            // assignment's, and changing them is a move - see the module docs.
            <Show when=move || hiring fallback=|| ()>
                <Section
                    title=l!("employees.assignment")
                    description=l!("employees.assignment.help")
                >
                    <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("employees.started_on")}
                            </span>
                            <input
                                type="date"
                                class="w-full"
                                prop:value=move || {
                                    draft
                                        .with(|d| {
                                            d.started_on.map(|on| on.to_string()).unwrap_or_default()
                                        })
                                }
                                on:change=move |ev| {
                                    let value = event_target_value(&ev);
                                    draft.update(|d| d.started_on = value.parse().ok());
                                }
                            />
                        </label>

                        <div class="block space-y-1">
                            <label
                                for="emp-type"
                                class="block text-xs font-medium text-content-muted"
                            >
                                {l!("employees.employment_type")}
                            </label>
                            <SelectField
                                id="emp-type"
                                value=Signal::derive(move || {
                                    draft.with(|d| d.employment_type.as_str().to_owned())
                                })
                                on_change=Callback::new(move |value: String| {
                                    if let Some(kind) = EmploymentType::parse(&value) {
                                        draft.update(|d| d.employment_type = kind);
                                    }
                                })
                                options=types.get_value()
                                placeholder=l!("common.not_set")
                                label=l!("employees.employment_type")
                            />
                        </div>

                        <Picker
                            id="emp-department"
                            label=l!("employees.department")
                            options=choices.with_value(|c| c.departments.clone())
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.department_id.map(|id| id.to_string()).unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |chosen: Option<Uuid>| {
                                draft.update(|d| d.department_id = chosen);
                            })
                        />

                        <Picker
                            id="emp-role"
                            label=l!("employees.job_position")
                            options=choices.with_value(|c| c.roles.clone())
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.job_position_id
                                            .map(|id| id.to_string())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |chosen: Option<Uuid>| {
                                draft.update(|d| d.job_position_id = chosen);
                            })
                        />

                        <Picker
                            id="emp-place"
                            label=l!("employees.work_location")
                            options=choices.with_value(|c| c.places.clone())
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.work_location_id
                                            .map(|id| id.to_string())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |chosen: Option<Uuid>| {
                                draft.update(|d| d.work_location_id = chosen);
                            })
                        />

                        <Picker
                            id="emp-manager"
                            label=l!("employees.manager")
                            options=choices.with_value(|c| c.managers.clone())
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.manager_id.map(|id| id.to_string()).unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |chosen: Option<Uuid>| {
                                draft.update(|d| d.manager_id = chosen);
                            })
                        />
                    </div>
                </Section>
            </Show>

            <Section title=l!("employees.note")>
                <textarea
                    class="w-full"
                    rows="3"
                    prop:value=move || draft.with(|d| d.note.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.note = value);
                    }
                />
            </Section>

            <Section>
                <div class="flex flex-wrap items-center justify-end gap-2">
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
            </Section>
        </Panel>
    }
}

/// A labelled select over a list of ids. Four of these on one screen is three
/// too many to write out.
#[component]
fn picker(
    id: &'static str,
    label: String,
    options: Vec<Choice>,
    value: Signal<String>,
    on_change: Callback<Option<Uuid>>,
) -> impl IntoView {
    let heading = label.clone();

    view! {
        <div class="block space-y-1">
            <label for=id class="block text-xs font-medium text-content-muted">
                {heading}
            </label>
            <SelectField
                id=id
                value=value
                on_change=Callback::new(move |raw: String| {
                    on_change.run(raw.parse::<Uuid>().ok());
                })
                options=options
                placeholder=l!("common.not_set")
                clearable=true
                label=label
            />
        </div>
    }
}

// --- moving somebody -------------------------------------------------------

#[component]
fn move_panel(employee_id: Uuid, reload: Callback<()>) -> impl IntoView {
    let open = RwSignal::new(false);
    let saving = RwSignal::new(false);

    let current = Resource::new(
        move || (employee_id, open.get()),
        |(employee_id, _)| async move { employee_assignment(employee_id).await },
    );

    let departments = Resource::new(|| (), |()| async move { list_departments().await });
    let roles = Resource::new(|| (), |()| async move { selectable_job_positions().await });
    let places = Resource::new(|| (), |()| async move { selectable_work_locations().await });
    let managers = Resource::new(|| (), |()| async move { employed_people().await });

    view! {
        <Section title=l!("employees.move.title") description=l!("employees.move.subtitle")>
            <Show
                when=move || open.get()
                fallback=move || {
                    view! {
                        <GhostButton
                            label=l!("employees.move")
                            icon=Icon::ArrowRight
                            on_click=Callback::new(move |()| open.set(true))
                        />
                    }
                }
            >
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        let Ok(prefilled) = current.await else {
                            return ().into_any();
                        };

                        let choices = Choices {
                            departments: departments
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|department| department.is_active)
                                .map(|department| {
                                    Choice::new(department.id.to_string(), department.name)
                                        .detail(department.code)
                                })
                                .collect(),
                            roles: roles
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .map(|role| {
                                    Choice::new(role.id.to_string(), role.title).detail(role.code)
                                })
                                .collect(),
                            places: places
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .map(|place| {
                                    Choice::new(place.id.to_string(), place.name)
                                        .detail(place.code)
                                })
                                .collect(),
                            managers: managers
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|(id, _, _)| *id != employee_id)
                                .map(|(id, code, name)| {
                                    Choice::new(id.to_string(), name).detail(code)
                                })
                                .collect(),
                        };

                        view! {
                            <MoveFields
                                employee_id=employee_id
                                draft=prefilled
                                choices=choices
                                open=open
                                saving=saving
                                reload=reload
                            />
                        }
                            .into_any()
                    })}
                </Transition>
            </Show>
        </Section>
    }
}

#[component]
fn move_fields(
    employee_id: Uuid,
    draft: AssignmentInput,
    choices: Choices,
    open: RwSignal<bool>,
    saving: RwSignal<bool>,
    reload: Callback<()>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let draft = RwSignal::new(draft);

    let record = move || {
        alerts.ask(
            Confirm::new(l!("employees.move.confirm"), move || {
                saving.set(true);
                let submission = draft.get_untracked();

                leptos::task::spawn_local(async move {
                    let result = move_employee(employee_id, submission).await;
                    saving.set(false);

                    match result {
                        Ok(Submission::Saved(_)) => {
                            alerts.post(Alert::success(l!("employees.moved")));
                            open.set(false);
                            let _ = reload.try_run(());
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
            .titled(l!("employees.move"))
            .confirm_label(l!("employees.move")),
        );
    };

    view! {
        <div class="space-y-3">
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                <label class="block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("employees.assignment.from")}
                    </span>
                    <input
                        type="date"
                        class="w-full"
                        prop:value=move || draft.with(|d| d.effective_from.to_string())
                        on:change=move |ev| {
                            if let Ok(on) = event_target_value(&ev).parse() {
                                draft.update(|d| d.effective_from = on);
                            }
                        }
                    />
                </label>

                <Picker
                    id="move-department"
                    label=l!("employees.department")
                    options=choices.departments
                    value=Signal::derive(move || {
                        draft.with(|d| d.department_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |chosen: Option<Uuid>| {
                        draft.update(|d| d.department_id = chosen);
                    })
                />

                <Picker
                    id="move-role"
                    label=l!("employees.job_position")
                    options=choices.roles
                    value=Signal::derive(move || {
                        draft
                            .with(|d| {
                                d.job_position_id.map(|id| id.to_string()).unwrap_or_default()
                            })
                    })
                    on_change=Callback::new(move |chosen: Option<Uuid>| {
                        draft.update(|d| d.job_position_id = chosen);
                    })
                />

                <Picker
                    id="move-place"
                    label=l!("employees.work_location")
                    options=choices.places
                    value=Signal::derive(move || {
                        draft
                            .with(|d| {
                                d.work_location_id.map(|id| id.to_string()).unwrap_or_default()
                            })
                    })
                    on_change=Callback::new(move |chosen: Option<Uuid>| {
                        draft.update(|d| d.work_location_id = chosen);
                    })
                />

                <Picker
                    id="move-manager"
                    label=l!("employees.manager")
                    options=choices.managers
                    value=Signal::derive(move || {
                        draft.with(|d| d.manager_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |chosen: Option<Uuid>| {
                        draft.update(|d| d.manager_id = chosen);
                    })
                />
            </div>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("employees.reason")}
                </span>
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.reason.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.reason = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("employees.reason.help")}
                </span>
            </label>

            <div class="flex flex-wrap items-center justify-end gap-2">
                <GhostButton
                    label=l!("common.cancel")
                    icon=Icon::X
                    on_click=Callback::new(move |()| open.set(false))
                />
                <PrimaryButton
                    label=l!("employees.move")
                    icon=Icon::ArrowRight
                    pending=Signal::derive(move || saving.get())
                    on_click=Callback::new(move |()| record())
                />
            </div>
        </div>
    }
}

// --- the login -------------------------------------------------------------

#[component]
fn login_panel(
    employee_id: Uuid,
    has_login: bool,
    can_invite: bool,
    /// Why a login cannot be created, where it cannot. Said rather than hidden:
    /// "no work email" is a thing somebody can fix.
    blocker: Option<String>,
    reload: Callback<()>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let viewer = crate::ui::viewer::Viewer::get();
    let working = RwSignal::new(false);
    let link = RwSignal::new(None::<String>);

    let may_invite = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::EMPLOYEES_INVITE))
        })
    });

    let invite = move || {
        alerts.ask(
            Confirm::new(l!("employees.login.create.confirm"), move || {
                working.set(true);

                leptos::task::spawn_local(async move {
                    // No roles from here. What somebody may do is decided under
                    // Users, by whoever owns access - an HR screen handing out
                    // permissions would be the wrong person choosing.
                    let result = create_employee_login(employee_id, Vec::new()).await;
                    working.set(false);

                    match result {
                        Ok(Submission::Saved(issued)) => {
                            let emailed = issued.delivery_note.is_none();

                            alerts.post(if emailed {
                                Alert::success(
                                    l!("employees.login.created", email = issued.email.clone()),
                                )
                            } else {
                                // The account exists and the link works; only
                                // the email failed. Showing the link is what
                                // makes a machine with no relay usable.
                                Alert::warning(l!(
                                    "employees.login.created_undelivered",
                                    email = issued.email.clone()
                                ))
                            });

                            if !emailed {
                                link.set(Some(issued.link.clone()));
                            }

                            let _ = reload.try_run(());
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
            .titled(l!("employees.login.create"))
            .confirm_label(l!("employees.login.create")),
        );
    };

    let unlink = move || {
        alerts.ask(
            Confirm::new(l!("employees.login.unlink.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match unlink_employee_login(employee_id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("employees.login.unlinked")));
                            let _ = reload.try_run(());
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
            .titled(l!("employees.login.unlink"))
            .confirm_label(l!("employees.login.unlink")),
        );
    };

    view! {
        <Panel title=l!("employees.login") description=l!("employees.login.help")>
            <div class="space-y-2">
                <p class="text-sm text-content-muted">
                    {if has_login {
                        l!("employees.login.has_one")
                    } else {
                        blocker.clone().unwrap_or_else(|| l!("employees.login.none"))
                    }}
                </p>

                // Only when the relay could not deliver it. Copying a link out
                // of a screen is a workaround, and offering it every time would
                // make it look like the normal path.
                <Show when=move || link.get().is_some() fallback=|| ()>
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.login.link")}
                        </span>
                        <input
                            type="text"
                            class="w-full font-mono text-xs"
                            readonly=true
                            prop:value=move || link.get().unwrap_or_default()
                        />
                    </label>
                </Show>

                <div class="flex flex-wrap items-center justify-end gap-2">
                    <Show
                        when=move || has_login && may_invite.get()
                        fallback=|| ()
                    >
                        <GhostButton
                            label=l!("employees.login.unlink")
                            icon=Icon::LockOpen
                            on_click=Callback::new(move |()| unlink())
                        />
                    </Show>

                    <Show
                        when=move || !has_login && can_invite && may_invite.get()
                        fallback=|| ()
                    >
                        <PrimaryButton
                            label=l!("employees.login.create")
                            icon=Icon::KeyRound
                            pending=Signal::derive(move || working.get())
                            on_click=Callback::new(move |()| invite())
                        />
                    </Show>
                </div>
            </div>
        </Panel>
    }
}

// --- employment history ----------------------------------------------------

#[component]
fn employment_panel(
    employee_id: Uuid,
    engagements: Vec<Engagement>,
    employed: bool,
    reload: Callback<()>,
) -> impl IntoView {
    let count = engagements.len();
    let rehired = count > 1;

    view! {
        <Section
            title=l!("employees.employment")
            description=l!("employees.employment.help")
        >
            <div class="space-y-4">
                <Show when=move || rehired fallback=|| ()>
                    <p class="text-xs text-content-subtle">
                        {l!("employees.service.rehired", count = count)}
                    </p>
                </Show>

                {engagements
                    .into_iter()
                    .map(|engagement| view! { <EngagementBlock engagement=engagement /> })
                    .collect_view()}

                <div class="flex flex-wrap items-center justify-end gap-2">
                    <Show when=move || employed fallback=|| ()>
                        <LeavingButton employee_id=employee_id reload=reload />
                    </Show>
                    <Show when=move || !employed fallback=|| ()>
                        <RehireButton employee_id=employee_id reload=reload />
                    </Show>
                </div>
            </div>
        </Section>
    }
}

#[component]
fn engagement_block(engagement: Engagement) -> impl IntoView {
    let started = engagement.started_on.to_string();
    let ended = engagement.ended_on.map(|on| on.to_string());
    let kind = crate::i18n::t(&engagement.employment_type.label());
    let reason = engagement.end_reason.map(|why| crate::i18n::t(&why.label()));
    let note = engagement.end_note.clone();
    let overrunning = engagement.is_overrunning(today());
    let assignments = StoredValue::new(engagement.assignments.clone());
    let any_assignments = !engagement.assignments.is_empty();
    let open = engagement.is_open();

    view! {
        <div class="rounded-control border border-edge p-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="flex items-center gap-2">
                    <span class="tabular-nums text-sm text-content">
                        {started.clone()} " — " {ended.clone().unwrap_or_else(|| "…".to_owned())}
                    </span>
                    <Badge
                        label=kind
                        tone=if open { Tone::Success } else { Tone::Neutral }
                    />
                </div>

                {reason
                    .map(|why| {
                        view! {
                            <span class="text-xs text-content-muted">
                                {l!("employees.end_reason")} ": " {why}
                            </span>
                        }
                    })}
            </div>

            <Show when=move || overrunning fallback=|| ()>
                <p class="mt-1 text-xs text-warning">{l!("employees.overrunning")}</p>
            </Show>

            {note
                .map(|note| {
                    view! {
                        <p class="mt-1 whitespace-pre-wrap text-xs text-content-muted">{note}</p>
                    }
                })}

            <Show when=move || any_assignments fallback=|| ()>
                <div class="mt-2 overflow-x-auto">
                    <table class="w-full min-w-[36rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="py-1 font-medium">
                                    {l!("employees.assignment.from")}
                                </th>
                                <th class="py-1 font-medium">{l!("employees.assignment.to")}</th>
                                <th class="py-1 font-medium">{l!("employees.department")}</th>
                                <th class="py-1 font-medium">{l!("employees.job_position")}</th>
                                <th class="py-1 font-medium">{l!("employees.manager")}</th>
                                <th class="py-1 font-medium">{l!("employees.reason")}</th>
                            </tr>
                        </thead>
                        <tbody>
                            {assignments
                                .get_value()
                                .into_iter()
                                .map(|assignment| {
                                    view! { <AssignmentRow assignment=assignment /> }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn assignment_row(assignment: Assignment) -> impl IntoView {
    let from = assignment.effective_from.to_string();
    let to = assignment
        .effective_to
        .map(|on| on.to_string())
        .unwrap_or_else(|| l!("employees.assignment.current"));
    let department = assignment.department_name.clone().unwrap_or_default();
    let role = assignment.job_title.clone().unwrap_or_default();
    let manager = assignment.manager_name.clone().unwrap_or_default();
    let reason = assignment.reason.clone().unwrap_or_default();
    let current = assignment.is_open();

    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1.5 tabular-nums text-content">{from}</td>
            <td class=if current {
                "py-1.5 text-xs text-brand"
            } else {
                "py-1.5 tabular-nums text-content-muted"
            }>{to}</td>
            <td class="py-1.5 text-content">{department}</td>
            <td class="py-1.5 text-content-muted">{role}</td>
            <td class="py-1.5 text-content-muted">{manager}</td>
            <td class="py-1.5 text-xs text-content-subtle">{reason}</td>
        </tr>
    }
}

#[component]
fn leaving_button(employee_id: Uuid, reload: Callback<()>) -> impl IntoView {
    let alerts = Alerts::get();
    let open = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let ended_on = RwSignal::new(today());
    let reason = RwSignal::new(EndReason::Resigned);
    let note = RwSignal::new(String::new());

    // Read so the screen can say who has to be reassigned, rather than leaving
    // it to be discovered when somebody's manager stops existing.
    let reports = Resource::new(
        move || (employee_id, open.get()),
        |(employee_id, _)| async move { direct_reports(employee_id).await },
    );

    let reasons = StoredValue::new(
        EndReason::ALL
            .iter()
            .map(|why| Choice::new(why.as_str(), crate::i18n::t(&why.label())))
            .collect::<Vec<_>>(),
    );

    let record = move || {
        alerts.ask(
            Confirm::new(l!("employees.leave.confirm"), move || {
                saving.set(true);

                let draft = LeavingInput {
                    ended_on: ended_on.get_untracked(),
                    reason: reason.get_untracked(),
                    note: note.get_untracked(),
                };

                leptos::task::spawn_local(async move {
                    let result = record_leaver(employee_id, draft).await;
                    saving.set(false);

                    match result {
                        Ok(Submission::Saved(_)) => {
                            alerts.post(Alert::success(l!("employees.left_note")));
                            open.set(false);
                            let _ = reload.try_run(());
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
            .titled(l!("employees.leave"))
            .confirm_label(l!("employees.leave")),
        );
    };

    view! {
        <Show
            when=move || open.get()
            fallback=move || {
                view! {
                    <GhostButton
                        label=l!("employees.leave")
                        icon=Icon::LogOut
                        on_click=Callback::new(move |()| open.set(true))
                    />
                }
            }
        >
            <div class="w-full space-y-3 rounded-control border border-edge p-3">
                <Transition fallback=|| ()>
                    {move || Suspend::new(async move {
                        let count = reports.await.unwrap_or(0);
                        if count == 0 {
                            return ().into_any();
                        }

                        view! {
                            <Notice
                                message=Signal::derive(move || {
                                    Some(l!("employees.manager.help"))
                                })
                                tone=Tone::Warning
                            />
                        }
                            .into_any()
                    })}
                </Transition>

                <div class="grid gap-3 sm:grid-cols-2">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("employees.ended_on")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:value=move || ended_on.get().to_string()
                            on:change=move |ev| {
                                if let Ok(on) = event_target_value(&ev).parse() {
                                    ended_on.set(on);
                                }
                            }
                        />
                    </label>

                    <div class="block space-y-1">
                        <label
                            for="leave-reason"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("employees.end_reason")}
                        </label>
                        <SelectField
                            id="leave-reason"
                            value=Signal::derive(move || reason.get().as_str().to_owned())
                            on_change=Callback::new(move |value: String| {
                                if let Some(why) = EndReason::parse(&value) {
                                    reason.set(why);
                                }
                            })
                            options=reasons.get_value()
                            placeholder=l!("common.not_set")
                            label=l!("employees.end_reason")
                        />
                    </div>
                </div>

                <label class="block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("employees.note")}
                    </span>
                    <textarea
                        class="w-full"
                        rows="2"
                        prop:value=move || note.get()
                        on:input=move |ev| note.set(event_target_value(&ev))
                    />
                </label>

                <div class="flex flex-wrap items-center justify-end gap-2">
                    <GhostButton
                        label=l!("common.cancel")
                        icon=Icon::X
                        on_click=Callback::new(move |()| open.set(false))
                    />
                    <PrimaryButton
                        label=l!("employees.leave")
                        icon=Icon::LogOut
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new(move |()| record())
                    />
                </div>
            </div>
        </Show>
    }
}

#[component]
fn rehire_button(employee_id: Uuid, reload: Callback<()>) -> impl IntoView {
    let alerts = Alerts::get();
    let saving = RwSignal::new(false);
    let started_on = RwSignal::new(today());

    let rehire = move || {
        alerts.ask(
            Confirm::new(l!("employees.rehire.confirm"), move || {
                saving.set(true);

                let draft = EmployeeInput {
                    started_on: Some(started_on.get_untracked()),
                    ..EmployeeInput::blank(today())
                };

                leptos::task::spawn_local(async move {
                    let result = rehire_employee(employee_id, draft).await;
                    saving.set(false);

                    match result {
                        Ok(Submission::Saved(_)) => {
                            alerts.post(Alert::success(l!("employees.rehired")));
                            let _ = reload.try_run(());
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
            .titled(l!("employees.rehire"))
            .confirm_label(l!("employees.rehire")),
        );
    };

    view! {
        <div class="flex flex-wrap items-end gap-2">
            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("employees.started_on")}
                </span>
                <input
                    type="date"
                    prop:value=move || started_on.get().to_string()
                    on:change=move |ev| {
                        if let Ok(on) = event_target_value(&ev).parse() {
                            started_on.set(on);
                        }
                    }
                />
            </label>

            <PrimaryButton
                label=l!("employees.rehire")
                icon=Icon::UserPlus
                pending=Signal::derive(move || saving.get())
                on_click=Callback::new(move |()| rehire())
            />
        </div>
    }
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}
