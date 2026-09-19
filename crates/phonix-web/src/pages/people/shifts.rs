//! One shift: when it runs, and how much lateness it forgives.

use app_hr::shift::ShiftTypeInput;
use chrono::NaiveTime;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{
    blank_shift_type, delete_shift_type, save_shift_type, shift_type_edit,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::card::CollapsibleCard;

const SHIFTS: &str = "/people/shifts";

/// `<input type="time">` hands back `HH:MM`, and sometimes `HH:MM:SS`.
fn parse_time(raw: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(raw, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(raw, "%H:%M:%S"))
        .ok()
}

fn shown(at: Option<NaiveTime>) -> String {
    at.map(|at| at.format("%H:%M").to_string())
        .unwrap_or_default()
}

#[component]
pub fn shift_type_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(raw, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => shift_type_edit(id).await,
            Err(_) => blank_shift_type().await,
        }
    });

    view! {
        <Title text=format!("{} | Evrykit", l!("entity.shift_type.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => {
                        let heading = if draft.id.is_some() {
                            draft.name.clone()
                        } else {
                            l!("shifts.new")
                        };

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    icon=Icon::Clock
                                    back=(SHIFTS, l!("shifts.title"))
                                />
                                <ShiftTypeForm draft=draft />
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.shift_type.singular")
                                    icon=Icon::Clock
                                    back=(SHIFTS, l!("shifts.title"))
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
fn shift_type_form(draft: ShiftTypeInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_shift_type(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("shifts.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/people/shifts/{id}"),
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
                Confirm::new(l!("shifts.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_shift_type(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("shifts.deleted")));
                                navigate(SHIFTS, leptos_router::NavigateOptions::default());
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

    // A shift whose end is not after its start runs past midnight. Said on the
    // screen rather than refused, because it is ordinary - and somebody who
    // typed it by accident should be told what they have made.
    let overnight = move || {
        draft.with(|d| match (d.starts_at, d.ends_at) {
            (Some(starts), Some(ends)) => ends < starts,
            _ => false,
        })
    };

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Panel>
                <div class="grid gap-3 sm:grid-cols-2">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("field.name")}
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
                            {l!("field.code")}
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

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("shifts.starts_at")}
                        </span>
                        <input
                            type="time"
                            class="w-full"
                            prop:value=move || draft.with(|d| shown(d.starts_at))
                            on:input=move |ev| {
                                let parsed = parse_time(&event_target_value(&ev));
                                draft.update(|d| d.starts_at = parsed);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("shifts.ends_at")}
                        </span>
                        <input
                            type="time"
                            class="w-full"
                            prop:value=move || draft.with(|d| shown(d.ends_at))
                            on:input=move |ev| {
                                let parsed = parse_time(&event_target_value(&ev));
                                draft.update(|d| d.ends_at = parsed);
                            }
                        />
                    </label>
                </div>

                <Show when=overnight fallback=|| ()>
                    <p class="mt-1 text-2xs text-content-subtle">
                        {l!("shifts.overnight")}
                    </p>
                </Show>

                <div class="mt-3 grid gap-3 sm:grid-cols-2">
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("shifts.late_grace")}
                        </span>
                        <input
                            type="number"
                            min="0"
                            max="1440"
                            class="w-full tabular-nums"
                            prop:value=move || draft.with(|d| d.late_grace_minutes.to_string())
                            on:input=move |ev| {
                                let parsed = event_target_value(&ev).parse::<i64>().unwrap_or(0);
                                draft.update(|d| d.late_grace_minutes = parsed);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("shifts.late_grace.help")}
                        </span>
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("shifts.early_grace")}
                        </span>
                        <input
                            type="number"
                            min="0"
                            max="1440"
                            class="w-full tabular-nums"
                            prop:value=move || {
                                draft.with(|d| d.early_exit_grace_minutes.to_string())
                            }
                            on:input=move |ev| {
                                let parsed = event_target_value(&ev).parse::<i64>().unwrap_or(0);
                                draft.update(|d| d.early_exit_grace_minutes = parsed);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("shifts.early_grace.help")}
                        </span>
                    </label>
                </div>

                <label class="mt-3 flex items-center gap-2">
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

                <Section>
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
                </Section>
            </Panel>

            <Show when=saved fallback=|| ()>
                <CollapsibleCard title=l!("common.history") icon=Icon::Clock>
                    <RecordHistory kind=kinds::SHIFT_TYPE id=id().map(|id| id.to_string()) />
                </CollapsibleCard>
            </Show>
        </div>
    }
}
