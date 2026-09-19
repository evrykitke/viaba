//! One movement: drafted, reviewed, and then made true.
//!
//! Two screens in one file because they are two states of one thing. A draft is
//! a form; a confirmed movement is a document, read-only, pointing at the
//! assignment it wrote. What decides which is rendered is the status, not the
//! address — a link somebody sent last week should still open the thing they
//! meant, which is the rule `pages::sales::invoice` follows for the same
//! reason.

use app_hr::employee::EndReason;
use app_hr::movement::{Movement, MovementInput, MovementKind, MovementStatus};
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
    blank_movement, cancel_movement, confirm_movement, employed_people, list_departments,
    movement_detail, movement_edit, save_movement, selectable_holiday_lists,
    selectable_job_positions, selectable_shift_types, selectable_work_locations,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const MOVEMENTS: &str = "/people/movements";

const fn status_tone(status: MovementStatus) -> Tone {
    match status {
        MovementStatus::Confirmed => Tone::Success,
        MovementStatus::Draft => Tone::Warning,
        MovementStatus::Cancelled => Tone::Neutral,
    }
}

/// Everything the pickers on this screen need, fetched once.
#[derive(Clone, Default)]
struct Choices {
    people: Vec<Choice>,
    departments: Vec<Choice>,
    roles: Vec<Choice>,
    places: Vec<Choice>,
    calendars: Vec<Choice>,
    shifts: Vec<Choice>,
}

#[component]
pub fn movement_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    // Bumped by confirming, so the page redraws as the document it became.
    let revision = RwSignal::new(0_u32);

    let movement = Resource::new(
        move || (raw(), revision.get()),
        |(raw, _)| async move {
            match raw.parse::<Uuid>() {
                Ok(id) => movement_detail(id).await.ok(),
                Err(_) => None,
            }
        },
    );

    view! {
        <Title text=format!("{} | Evrykit", l!("entity.movement.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match movement.await {
                    // Confirmed or cancelled: the document, read-only.
                    Some(movement) if !movement.status.is_editable() => {
                        view! { <MovementDocument movement=movement /> }.into_any()
                    }
                    // A draft, or a new one: the form.
                    found => {
                        let id = found.map(|movement| movement.id);
                        view! { <MovementDraft id=id revision=revision /> }.into_any()
                    }
                }
            })}
        </Transition>
    }
}

/// A confirmed or withdrawn movement: what was decided, and what it did.
#[component]
fn movement_document(movement: Movement) -> impl IntoView {
    let kind = crate::i18n::t(&movement.kind.label());
    let status = crate::i18n::t(&movement.status.label());
    let tone = status_tone(movement.status);
    let number = movement.number.clone().unwrap_or_default();
    let person = movement.employee_name.clone();
    let employee_id = movement.employee_id;
    let effective = movement.effective_on.to_string();
    let reason = movement.reason.clone();
    let end_reason = movement
        .end_reason
        .map(|reason| crate::i18n::t(&reason.label()));
    let assignment = movement.assignment_id;
    let id = movement.id;

    view! {
        <PageHeader
            title=format!("{kind} · {person}")
            icon=Icon::ArrowRight
            back=(MOVEMENTS, l!("movements.title"))
        />

        <div class="space-y-3">
            <Panel>
                <div class="flex flex-wrap items-center justify-between gap-2">
                    <code class="font-mono text-xs text-content-muted">{number}</code>
                    <Badge label=status tone=tone />
                </div>

                <dl class="mt-3 space-y-1 text-sm">
                    <div class="flex justify-between gap-4">
                        <dt class="text-content-muted">{l!("movements.person")}</dt>
                        <dd>
                            <a
                                class="text-brand hover:underline"
                                href=format!("/people/employees/{employee_id}")
                            >
                                {person.clone()}
                            </a>
                        </dd>
                    </div>
                    <div class="flex justify-between gap-4">
                        <dt class="text-content-muted">{l!("movements.effective_on")}</dt>
                        <dd class="tabular-nums text-content">{effective}</dd>
                    </div>
                    {end_reason
                        .map(|end_reason| {
                            view! {
                                <div class="flex justify-between gap-4">
                                    <dt class="text-content-muted">
                                        {l!("movements.end_reason")}
                                    </dt>
                                    <dd class="text-content">{end_reason}</dd>
                                </div>
                            }
                        })}
                    // The row it wrote. A document that says what it did beats
                    // one a reader has to match up by date.
                    {assignment
                        .map(|_| {
                            view! {
                                <div class="flex justify-between gap-4">
                                    <dt class="text-content-muted">
                                        {l!("movements.wrote")}
                                    </dt>
                                    <dd>
                                        <a
                                            class="text-brand hover:underline"
                                            href=format!("/people/employees/{employee_id}")
                                        >
                                            {l!("movements.wrote.assignment")}
                                        </a>
                                    </dd>
                                </div>
                            }
                        })}
                </dl>

                {reason
                    .map(|reason| {
                        view! {
                            <Section title=l!("movements.reason")>
                                <p class="whitespace-pre-wrap text-sm text-content-muted">
                                    {reason}
                                </p>
                            </Section>
                        }
                    })}
            </Panel>

            <CollapsibleCard title=l!("common.history") icon=Icon::Clock>
                <RecordHistory kind=kinds::MOVEMENT id=Some(id.to_string()) />
            </CollapsibleCard>
        </div>
    }
}

/// A draft: the form, and the two acts that end its life.
#[component]
fn movement_draft(id: Option<Uuid>, revision: RwSignal<u32>) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = StoredValue::new(leptos_router::hooks::use_navigate());
    let viewer = crate::ui::viewer::Viewer::get();

    let may_confirm = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::MOVEMENTS_CONFIRM))
        })
    });

    // A new movement needs a kind and a person before the server can pre-fill
    // it, so those two are picked here and the draft is fetched once both are.
    let kind = RwSignal::new(MovementKind::Promotion);
    let person = RwSignal::new(None::<Uuid>);
    let draft = RwSignal::new(None::<MovementInput>);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let existing = Resource::new(
        move || id,
        |id| async move {
            match id {
                Some(id) => movement_edit(id).await.ok(),
                None => None,
            }
        },
    );

    // One resource per list, assembled into `Choices` inside the transition
    // below. A resource's output crosses the SSR boundary and has to
    // serialise; a list of `Choice` is a view concern and does not. The
    // employee form makes the same split for the same reason.
    let people = Resource::new(|| (), |()| async move { employed_people().await });
    let departments = Resource::new(|| (), |()| async move { list_departments().await });
    let roles = Resource::new(|| (), |()| async move { selectable_job_positions().await });
    let places = Resource::new(|| (), |()| async move { selectable_work_locations().await });
    let calendars = Resource::new(|| (), |()| async move { selectable_holiday_lists().await });
    let shifts = Resource::new(|| (), |()| async move { selectable_shift_types().await });

    // Picking a person on a new movement fetches what they are on now, so the
    // form opens on their current assignment rather than empty.
    let prefill = move || {
        let (Some(who), chosen) = (person.get_untracked(), kind.get_untracked()) else {
            return;
        };

        leptos::task::spawn_local(async move {
            if let Ok(prefilled) = blank_movement(chosen, who).await {
                draft.set(Some(prefilled));
            }
        });
    };

    let save = move || {
        let Some(submission) = draft.get_untracked() else {
            return;
        };

        saving.set(true);
        rejected.set(None);

        leptos::task::spawn_local(async move {
            let result = save_movement(submission).await;
            saving.set(false);

            match result {
                Ok(Submission::Saved(stored)) => {
                    let saved_id = stored.id;
                    draft.set(Some(stored));
                    alerts.post(Alert::success(l!("movements.saved")));

                    if let Some(saved_id) = saved_id {
                        navigate.with_value(|go| {
                            go(
                                &format!("/people/movements/{saved_id}"),
                                leptos_router::NavigateOptions {
                                    replace: true,
                                    ..Default::default()
                                },
                            );
                        });
                    }
                }
                Ok(Submission::Rejected(errors)) => {
                    rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                }
                Err(err) => rejected.set(Some(err.to_string())),
            }
        });
    };

    // Confirming can refuse — a promotion that moves nobody, an engagement
    // that has since ended — and the refusal is the reader's answer rather
    // than something to swallow.
    let confirm = move || {
        let Some(movement_id) = draft.with_untracked(|d| d.as_ref().and_then(|d| d.id)) else {
            return;
        };

        alerts.ask(
            Confirm::new(l!("movements.confirm.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match confirm_movement(movement_id).await {
                        Ok(Submission::Saved(_)) => {
                            alerts.post(Alert::success(l!("movements.confirmed")));
                            revision.update(|count| *count = count.wrapping_add(1));
                        }
                        Ok(Submission::Rejected(errors)) => {
                            if let Some(error) = errors.first() {
                                rejected.set(Some(crate::i18n::t(&error.message)));
                                alerts.post(Alert::warning(crate::i18n::t(&error.message)));
                            }
                        }
                        Err(err) => alerts.post(Alert::failure(err.to_string())),
                    }
                });
            })
            .titled(l!("movements.confirm"))
            .confirm_label(l!("movements.confirm")),
        );
    };

    let withdraw = move || {
        let Some(movement_id) = draft.with_untracked(|d| d.as_ref().and_then(|d| d.id)) else {
            return;
        };

        alerts.ask(
            Confirm::new(l!("movements.cancel.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match cancel_movement(movement_id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("movements.cancelled")));
                            navigate.with_value(|go| {
                                go(MOVEMENTS, leptos_router::NavigateOptions::default());
                            });
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
            .titled(l!("movements.cancel"))
            .confirm_label(l!("movements.cancel")),
        );
    };

    let is_exit = move || draft.with(|d| d.as_ref().is_some_and(|d| d.kind == MovementKind::Exit));
    let started = move || draft.with(|d| d.is_some());
    let stored = move || draft.with(|d| d.as_ref().is_some_and(|d| d.id.is_some()));

    view! {
        <PageHeader
            title=l!("movements.new")
            icon=Icon::ArrowRight
            back=(MOVEMENTS, l!("movements.title"))
        />

        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    // An existing draft arrives whole; a new one is built once
                    // somebody has picked a kind and a person.
                    if let Some(found) = existing.await
                        && draft.with_untracked(|d| d.is_none())
                    {
                        kind.set(found.kind);
                        person.set(found.employee_id);
                        draft.set(Some(found));
                    }

                    let kind_options = MovementKind::ALL
                        .iter()
                        .map(|kind| {
                            Choice::new(kind.as_str(), crate::i18n::t(&kind.label()))
                        })
                        .collect::<Vec<_>>();

                    let reason_options = EndReason::ALL
                        .iter()
                        .map(|reason| {
                            Choice::new(reason.as_str(), crate::i18n::t(&reason.label()))
                        })
                        .collect::<Vec<_>>();

                    let map_id = |(id, code, name): (Uuid, String, String)| {
                        Choice::new(id.to_string(), name).detail(code)
                    };

                    let picks = Choices {
                        people: people.await.unwrap_or_default().into_iter().map(map_id).collect(),
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
                        calendars: calendars
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(map_id)
                            .collect(),
                        shifts: shifts
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|shift| {
                                Choice::new(shift.id.to_string(), shift.name).detail(shift.code)
                            })
                            .collect(),
                    };

                    view! {
                        <Panel>
                            <div class="grid gap-3 sm:grid-cols-2">
                                <div class="block space-y-1">
                                    <label
                                        for="movement-kind"
                                        class="block text-xs font-medium text-content-muted"
                                    >
                                        {l!("movements.kind")}
                                    </label>
                                    <SelectField
                                        id="movement-kind"
                                        value=Signal::derive(move || {
                                            kind.get().as_str().to_owned()
                                        })
                                        on_change=Callback::new(move |value: String| {
                                            if let Some(chosen) = MovementKind::parse(&value) {
                                                kind.set(chosen);
                                                prefill();
                                            }
                                        })
                                        options=kind_options.clone()
                                        placeholder=l!("common.not_set")
                                        label=l!("movements.kind")
                                    />
                                </div>

                                <div class="block space-y-1">
                                    <label
                                        for="movement-person"
                                        class="block text-xs font-medium text-content-muted"
                                    >
                                        {l!("movements.person")}
                                    </label>
                                    <SelectField
                                        id="movement-person"
                                        value=Signal::derive(move || {
                                            person
                                                .get()
                                                .map(|id| id.to_string())
                                                .unwrap_or_default()
                                        })
                                        on_change=Callback::new(move |value: String| {
                                            person.set(value.parse::<Uuid>().ok());
                                            prefill();
                                        })
                                        options=picks.people.clone()
                                        placeholder=l!("common.not_set")
                                        label=l!("movements.person")
                                    />
                                </div>
                            </div>

                            <Show
                                when=started
                                fallback=move || {
                                    view! {
                                        <p class="py-6 text-center text-sm text-content-subtle">
                                            {l!("movements.choose_somebody")}
                                        </p>
                                    }
                                }
                            >
                                <MovementFields
                                    draft=draft
                                    choices=picks.clone()
                                    reason_options=reason_options.clone()
                                    is_exit=Signal::derive(is_exit)
                                />

                                <Section>
                                    <div class="flex flex-wrap items-center justify-end gap-2">
                                        <Show when=stored fallback=|| ()>
                                            <GhostButton
                                                label=l!("movements.cancel")
                                                icon=Icon::Ban
                                                on_click=Callback::new(move |()| withdraw())
                                            />
                                        </Show>

                                        <PrimaryButton
                                            label=l!("common.save")
                                            icon=Icon::Save
                                            pending=Signal::derive(move || saving.get())
                                            on_click=Callback::new(move |()| save())
                                        />

                                        // Confirming is the act with
                                        // consequences, so it is offered only
                                        // once there is something to confirm
                                        // and only to whoever may.
                                        <Show
                                            when=move || stored() && may_confirm.get()
                                            fallback=|| ()
                                        >
                                            <PrimaryButton
                                                label=l!("movements.confirm")
                                                icon=Icon::Check
                                                on_click=Callback::new(move |()| confirm())
                                            />
                                        </Show>
                                    </div>
                                </Section>
                            </Show>
                        </Panel>
                    }
                })}
            </Transition>
        </div>
    }
}

/// The body of the form: what the movement moves them to, or why they left.
#[component]
fn movement_fields(
    draft: RwSignal<Option<MovementInput>>,
    choices: Choices,
    reason_options: Vec<Choice>,
    is_exit: Signal<bool>,
) -> impl IntoView {
    /// One picker over a list of ids, bound to one field of the draft.
    ///
    /// Six of these written out is five too many, and the only thing that
    /// differs between them is which `Option<Uuid>` they set.
    macro_rules! picker {
        ($id:literal, $label:expr, $options:expr, $field:ident) => {{
            let options = $options;
            view! {
                <div class="block space-y-1">
                    <label for=$id class="block text-xs font-medium text-content-muted">
                        {$label}
                    </label>
                    <SelectField
                        id=$id
                        value=Signal::derive(move || {
                            draft
                                .with(|d| {
                                    d.as_ref()
                                        .and_then(|d| d.$field)
                                        .map(|id| id.to_string())
                                        .unwrap_or_default()
                                })
                        })
                        on_change=Callback::new(move |value: String| {
                            let chosen = value.parse::<Uuid>().ok();
                            draft
                                .update(|d| {
                                    if let Some(d) = d.as_mut() {
                                        d.$field = chosen;
                                    }
                                });
                        })
                        options=options
                        placeholder=l!("common.not_set")
                        label=$label
                    />
                </div>
            }
        }};
    }

    view! {
        <div class="mt-3 grid gap-3 sm:grid-cols-2">
            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("movements.effective_on")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || {
                        draft
                            .with(|d| {
                                d.as_ref()
                                    .and_then(|d| d.effective_on)
                                    .map(|on| on.to_string())
                            })
                            .unwrap_or_default()
                    }
                    on:input=move |ev| {
                        let parsed = event_target_value(&ev).parse::<NaiveDate>().ok();
                        draft
                            .update(|d| {
                                if let Some(d) = d.as_mut() {
                                    d.effective_on = parsed;
                                }
                            });
                    }
                />
            </label>

            // An exit names why from the closed list; a promotion names none,
            // and the service refuses one that carries it.
            <Show when=move || is_exit.get() fallback=|| ()>
                <div class="block space-y-1">
                    <label
                        for="movement-end-reason"
                        class="block text-xs font-medium text-content-muted"
                    >
                        {l!("movements.end_reason")}
                    </label>
                    <SelectField
                        id="movement-end-reason"
                        value=Signal::derive(move || {
                            draft
                                .with(|d| {
                                    d.as_ref()
                                        .and_then(|d| d.end_reason)
                                        .map(|reason| reason.as_str().to_owned())
                                        .unwrap_or_default()
                                })
                        })
                        on_change=Callback::new(move |value: String| {
                            let chosen = EndReason::parse(&value);
                            draft
                                .update(|d| {
                                    if let Some(d) = d.as_mut() {
                                        d.end_reason = chosen;
                                    }
                                });
                        })
                        options=reason_options.clone()
                        placeholder=l!("common.not_set")
                        label=l!("movements.end_reason")
                    />
                </div>
            </Show>
        </div>

        // Where they are moving to. Absent for an exit, which moves nobody
        // anywhere - the service refuses one that names any of these.
        <Show when=move || !is_exit.get() fallback=|| ()>
            <div class="mt-3 grid gap-3 sm:grid-cols-2">
                {picker!(
                    "movement-department", l!("employees.department"),
                    choices.departments.clone(), department_id
                )}
                {picker!(
                    "movement-role", l!("employees.job_position"),
                    choices.roles.clone(), job_position_id
                )}
                {picker!(
                    "movement-place", l!("employees.work_location"),
                    choices.places.clone(), work_location_id
                )}
                {picker!(
                    "movement-manager", l!("employees.manager"),
                    choices.people.clone(), manager_id
                )}
                {picker!(
                    "movement-calendar", l!("employees.holiday_list"),
                    choices.calendars.clone(), holiday_list_id
                )}
                {picker!(
                    "movement-shift", l!("employees.shift_type"),
                    choices.shifts.clone(), shift_type_id
                )}
            </div>
        </Show>

        <label class="mt-3 block space-y-1">
            <span class="text-xs font-medium text-content-muted">
                {l!("movements.reason")}
            </span>
            <textarea
                class="w-full"
                rows="2"
                prop:value=move || {
                    draft
                        .with(|d| d.as_ref().map(|d| d.reason.clone()))
                        .unwrap_or_default()
                }
                on:input=move |ev| {
                    let value = event_target_value(&ev);
                    draft
                        .update(|d| {
                            if let Some(d) = d.as_mut() {
                                d.reason = value;
                            }
                        });
                }
            />
            <span class="block text-2xs text-content-subtle">
                {l!("movements.reason.help")}
            </span>
        </label>
    }
}
