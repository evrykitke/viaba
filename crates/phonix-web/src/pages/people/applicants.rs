//! One application: the form, the stage it is at, and the act of hiring.
//!
//! Hiring is a button rather than a stage on the picker. It opens an
//! engagement, and a form that reached "hired" by choosing a word from a list
//! would open none while the record claimed otherwise — which is why
//! `ApplicantInput::check` refuses it and why this screen has to offer it
//! separately.

use app_hr::applicant::{ApplicantInput, Stage};
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
    applicant_detail, applicant_edit, blank_applicant, delete_applicant, hire_applicant,
    save_applicant, selectable_job_positions,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const APPLICANTS: &str = "/people/applicants";

const fn stage_tone(stage: Stage) -> Tone {
    match stage {
        Stage::Hired => Tone::Success,
        Stage::Offer => Tone::Brand,
        Stage::Rejected | Stage::Withdrawn => Tone::Neutral,
        Stage::Applied | Stage::Screening | Stage::Interview => Tone::Warning,
    }
}

#[component]
pub fn applicant_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    // Bumped by hiring, which closes the application and changes what the page
    // may do with it.
    let revision = RwSignal::new(0_u32);

    let draft = Resource::new(
        move || (raw(), revision.get()),
        |(raw, _)| async move {
            match raw.parse::<Uuid>() {
                // A hired application is no longer editable, so the detail is
                // what to show. `edit` refuses it; `detail` does not.
                Ok(id) => match applicant_detail(id).await {
                    Ok(found) if found.stage.is_movable() => applicant_edit(id).await.ok(),
                    Ok(found) => Some(ApplicantInput::from_applicant(&found)),
                    Err(_) => None,
                },
                Err(_) => blank_applicant().await.ok(),
            }
        },
    );

    let jobs = Resource::new(|| (), |()| async move { selectable_job_positions().await });

    view! {
        <Title text=format!("{} | Evrykit", l!("entity.applicant.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let options = jobs
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|job| Choice::new(job.id.to_string(), job.title).detail(job.code))
                    .collect::<Vec<_>>();

                match draft.await {
                    Some(found) => {
                        let heading = if found.id.is_some() {
                            format!(
                                "{} {}",
                                found.given_name.trim(),
                                found.family_name.trim(),
                            )
                        } else {
                            l!("applicants.new")
                        };

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    icon=Icon::UserPlus
                                    back=(APPLICANTS, l!("applicants.title"))
                                />
                                <ApplicantForm
                                    draft=found
                                    jobs=options
                                    revision=revision
                                />
                            </>
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.applicant.singular")
                                    icon=Icon::UserPlus
                                    back=(APPLICANTS, l!("applicants.title"))
                                />
                                <Notice
                                    message=Signal::derive(move || {
                                        Some(l!("applicants.error.gone"))
                                    })
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
fn applicant_form(
    draft: ApplicantInput,
    jobs: Vec<Choice>,
    revision: RwSignal<u32>,
) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);
    let starting_on = RwSignal::new(None::<NaiveDate>);
    let alerts = Alerts::get();
    let navigate = StoredValue::new(leptos_router::hooks::use_navigate());
    let viewer = crate::ui::viewer::Viewer::get();

    let may_hire = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::APPLICANTS_HIRE))
        })
    });

    // Every stage but the one that opens an engagement. `check` refuses it as a
    // typed value, so offering it here would offer a refusal.
    let stage_options = Stage::ALL
        .iter()
        .filter(|stage| **stage != Stage::Hired)
        .map(|stage| Choice::new(stage.as_str(), crate::i18n::t(&stage.label())))
        .collect::<Vec<_>>();

    let save = move || {
        saving.set(true);
        rejected.set(None);
        let submission = draft.get_untracked();

        leptos::task::spawn_local(async move {
            let result = save_applicant(submission).await;
            saving.set(false);

            match result {
                Ok(Submission::Saved(stored)) => {
                    let id = stored.id;
                    draft.set(stored);
                    alerts.post(Alert::success(l!("applicants.saved")));

                    if let Some(id) = id {
                        navigate.with_value(|go| {
                            go(
                                &format!("/people/applicants/{id}"),
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

    // The act with the consequence. Says which record it landed on, because
    // somebody hiring a returning employee needs to know they did not make a
    // second one - and somebody hiring a stranger needs to know they did.
    let hire = move || {
        let Some(id) = draft.with_untracked(|d| d.id) else {
            return;
        };
        let Some(on) = starting_on.get_untracked() else {
            alerts.post(Alert::warning(l!("applicants.hire.needs_a_date")));
            return;
        };

        alerts.ask(
            Confirm::new(l!("applicants.hire.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match hire_applicant(id, on).await {
                        Ok(Submission::Saved(hired)) => {
                            alerts.post(Alert::success(if hired.rejoined {
                                l!("applicants.hired.rejoined")
                            } else {
                                l!("applicants.hired.new")
                            }));

                            revision.update(|count| *count = count.wrapping_add(1));

                            navigate.with_value(|go| {
                                go(
                                    &format!("/people/employees/{}", hired.employee_id),
                                    leptos_router::NavigateOptions::default(),
                                );
                            });
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
            .titled(l!("applicants.hire"))
            .confirm_label(l!("applicants.hire")),
        );
    };

    let remove = move || {
        let Some(id) = draft.with_untracked(|d| d.id) else {
            return;
        };

        alerts.ask(
            Confirm::new(l!("applicants.delete.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match delete_applicant(id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("applicants.deleted")));
                            navigate.with_value(|go| {
                                go(APPLICANTS, leptos_router::NavigateOptions::default());
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
            .titled(l!("common.delete"))
            .confirm_label(l!("common.delete")),
        );
    };

    let stored = move || draft.with(|d| d.id.is_some());
    let id = move || draft.with(|d| d.id);
    let hired = move || draft.with(|d| d.stage == Stage::Hired);
    let closed = move || draft.with(|d| !d.stage.is_open());

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            // A hired application is a record of what happened, not a form.
            <Show when=hired fallback=|| ()>
                <Panel>
                    <div class="flex flex-wrap items-center gap-2">
                        <Badge
                            label=crate::i18n::t(&Stage::Hired.label())
                            tone=stage_tone(Stage::Hired)
                        />
                        <span class="text-sm text-content-muted">
                            {l!("applicants.hired.note")}
                        </span>
                    </div>
                </Panel>
            </Show>

            <Panel>
                <div class="grid gap-3 sm:grid-cols-2">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("applicants.given_name")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:disabled=hired
                            prop:value=move || draft.with(|d| d.given_name.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.given_name = value);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("applicants.family_name")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:disabled=hired
                            prop:value=move || draft.with(|d| d.family_name.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.family_name = value);
                            }
                        />
                    </label>

                    <div class="block space-y-1">
                        <label
                            for="applicant-job"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("applicants.job")}
                        </label>
                        <SelectField
                            id="applicant-job"
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.job_position_id
                                            .map(|id| id.to_string())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |value: String| {
                                let chosen = value.parse::<Uuid>().ok();
                                draft.update(|d| d.job_position_id = chosen);
                            })
                            options=jobs.clone()
                            placeholder=l!("common.not_set")
                            label=l!("applicants.job")
                        />
                    </div>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("applicants.applied_on")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:disabled=hired
                            prop:value=move || {
                                draft
                                    .with(|d| d.applied_on.map(|on| on.to_string()))
                                    .unwrap_or_default()
                            }
                            on:input=move |ev| {
                                let parsed = event_target_value(&ev).parse::<NaiveDate>().ok();
                                draft.update(|d| d.applied_on = parsed);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("field.email")}
                        </span>
                        <input
                            type="email"
                            class="w-full"
                            prop:disabled=hired
                            prop:value=move || draft.with(|d| d.email.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.email = value);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("applicants.email.help")}
                        </span>
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("applicants.phone")}
                        </span>
                        <input
                            type="tel"
                            class="w-full"
                            prop:disabled=hired
                            prop:value=move || draft.with(|d| d.phone.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.phone = value);
                            }
                        />
                    </label>

                    <div class="block space-y-1">
                        <label
                            for="applicant-stage"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("applicants.stage")}
                        </label>
                        <SelectField
                            id="applicant-stage"
                            value=Signal::derive(move || {
                                draft.with(|d| d.stage.as_str().to_owned())
                            })
                            on_change=Callback::new(move |value: String| {
                                if let Some(stage) = Stage::parse(&value) {
                                    draft.update(|d| d.stage = stage);
                                }
                            })
                            options=stage_options.clone()
                            placeholder=l!("common.not_set")
                            label=l!("applicants.stage")
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("applicants.stage.help")}
                        </span>
                    </div>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("applicants.source")}
                        </span>
                        <input
                            type="text"
                            class="w-full"
                            prop:disabled=hired
                            prop:value=move || draft.with(|d| d.source.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.source = value);
                            }
                        />
                    </label>
                </div>

                <label class="mt-3 block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("applicants.note")}
                    </span>
                    <textarea
                        class="w-full"
                        rows="3"
                        prop:disabled=hired
                        prop:value=move || draft.with(|d| d.note.clone())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            draft.update(|d| d.note = value);
                        }
                    />
                </label>

                <Show when=move || !hired() fallback=|| ()>
                    <Section>
                        <div class="flex flex-wrap items-center justify-end gap-2">
                            <Show when=stored fallback=|| ()>
                                <GhostButton
                                    label=l!("common.delete")
                                    icon=Icon::Trash2
                                    on_click=Callback::new(move |()| remove())
                                />
                            </Show>

                            <PrimaryButton
                                label=l!("common.save")
                                icon=Icon::Save
                                pending=Signal::derive(move || saving.get())
                                on_click=Callback::new(move |()| save())
                            />
                        </div>
                    </Section>
                </Show>
            </Panel>

            // Hiring, on its own, because it is not a save. Offered only on an
            // application still in play and only to whoever may.
            <Show
                when=move || stored() && !closed() && may_hire.get()
                fallback=|| ()
            >
                <Panel>
                    <Section title=l!("applicants.hire")>
                        <div class="flex flex-wrap items-end gap-3">
                            <label class="block space-y-1">
                                <span class="text-xs font-medium text-content-muted">
                                    {l!("applicants.hire.starts_on")}
                                </span>
                                <input
                                    type="date"
                                    class="w-full"
                                    prop:value=move || {
                                        starting_on
                                            .get()
                                            .map(|on| on.to_string())
                                            .unwrap_or_default()
                                    }
                                    on:input=move |ev| {
                                        let parsed = event_target_value(&ev)
                                            .parse::<NaiveDate>()
                                            .ok();
                                        starting_on.set(parsed);
                                    }
                                />
                            </label>

                            <PrimaryButton
                                label=l!("applicants.hire")
                                icon=Icon::UserPlus
                                on_click=Callback::new(move |()| hire())
                            />
                        </div>

                        <span class="mt-1 block text-2xs text-content-subtle">
                            {l!("applicants.hire.help")}
                        </span>
                    </Section>
                </Panel>
            </Show>

            <Show when=stored fallback=|| ()>
                <CollapsibleCard title=l!("common.history") icon=Icon::Clock>
                    <RecordHistory kind=kinds::APPLICANT id=id().map(|id| id.to_string()) />
                </CollapsibleCard>
            </Show>
        </div>
    }
}
