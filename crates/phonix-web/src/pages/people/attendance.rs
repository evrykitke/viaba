//! One person's month, every day of it resolved against their calendar.
//!
//! A month rather than a list of records, because the interesting days are the
//! ones with no record on them. A list would show what was keyed; this shows
//! what each day came to, which is a different answer on every day nobody
//! keyed anything — and those are the days somebody is looking for.
//!
//! One screen rather than two. Keying a day, reading the month and correcting a
//! record are the same act from the reader's side: they are looking at a month
//! and one row of it is wrong.

use app_hr::attendance::{
    AttendanceInput, AttendanceSource, AttendanceStatus, DayOutcome, TimesheetDay,
};
use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::form::Submission;
use phonix_core::permissions;
use uuid::Uuid;

use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{
    attendance_edit, blank_attendance, delete_attendance, employed_people, employee_timesheet,
    save_attendance,
};
use crate::ui::alert::{Alert, Alerts};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

/// The first of the month a date falls in, and the first of the next one.
///
/// Returned as a pair because the service takes an inclusive span and the last
/// day of a month is the one piece of date arithmetic worth not writing twice.
fn month_span(anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    let from = anchor.with_day(1).unwrap_or(anchor);

    let to = match from.month() {
        12 => from
            .with_year(from.year() + 1)
            .and_then(|d| d.with_month(1)),
        month => from.with_month(month + 1),
    }
    .and_then(|next| next.pred_opt())
    .unwrap_or(anchor);

    (from, to)
}

/// What a day's answer looks like, and how loudly.
///
/// Every one of the seven is drawn. `NotRecorded` and `Unknown` are the two
/// that would otherwise be blanks, and a blank is exactly what somebody reading
/// a timesheet must not see: it reads as "fine" and means "nobody knows".
fn outcome_badge(outcome: &DayOutcome) -> (String, Tone) {
    let tone = match outcome {
        DayOutcome::Present => Tone::Success,
        DayOutcome::HalfDay => Tone::Brand,
        DayOutcome::Absent => Tone::Danger,
        DayOutcome::NotRecorded => Tone::Warning,
        DayOutcome::WorkedDayOff { .. } => Tone::Brand,
        DayOutcome::DayOff { .. } => Tone::Neutral,
        DayOutcome::Unknown => Tone::Warning,
    };

    (crate::i18n::t(&outcome.label()), tone)
}

/// The day off's own name, where the answer carries one.
fn day_off_name(outcome: &DayOutcome) -> Option<String> {
    match outcome {
        DayOutcome::DayOff { name } | DayOutcome::WorkedDayOff { name } => Some(name.clone()),
        _ => None,
    }
}

#[component]
pub fn attendance_page() -> impl IntoView {
    let today = chrono::Local::now().date_naive();

    let person = RwSignal::new(None::<Uuid>);
    let anchor = RwSignal::new(today);
    // Bumped after every write, so the month reloads without the person or the
    // date having changed.
    let revision = RwSignal::new(0_u32);
    let editing = RwSignal::new(None::<AttendanceInput>);
    let rejected = RwSignal::new(None::<String>);

    let people = Resource::new(|| (), |()| async move { employed_people().await });

    let month = Resource::new(
        move || (person.get(), anchor.get(), revision.get()),
        |(person, anchor, _)| async move {
            let who = person?;
            let (from, to) = month_span(anchor);
            employee_timesheet(who, from, to).await.ok()
        },
    );

    let shift_month = move |by: i32| {
        anchor.update(|at| {
            let (from, _) = month_span(*at);
            *at = if by < 0 {
                from.pred_opt().unwrap_or(from)
            } else {
                let (_, to) = month_span(from);
                to.succ_opt().unwrap_or(from)
            };
        });
    };

    view! {
        <Title text=format!("{} | Phonix", l!("attendance.title")) />

        <PageHeader
            title=l!("attendance.title")
            subtitle=l!("attendance.subtitle")
            icon=Icon::ListChecks
        />

        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Panel>
                <div class="flex flex-wrap items-end gap-3">
                    <Transition fallback=|| ()>
                        {move || Suspend::new(async move {
                            let options = people
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .map(|(id, code, name)| {
                                    Choice::new(id.to_string(), name).detail(code)
                                })
                                .collect::<Vec<_>>();

                            view! {
                                <div class="min-w-[16rem] space-y-1">
                                    <label
                                        for="attendance-person"
                                        class="block text-xs font-medium text-content-muted"
                                    >
                                        {l!("attendance.person")}
                                    </label>
                                    <SelectField
                                        id="attendance-person"
                                        value=Signal::derive(move || {
                                            person
                                                .get()
                                                .map(|id| id.to_string())
                                                .unwrap_or_default()
                                        })
                                        on_change=Callback::new(move |chosen: String| {
                                            person.set(chosen.parse::<Uuid>().ok());
                                            editing.set(None);
                                        })
                                        options=options
                                        placeholder=l!("common.not_set")
                                        label=l!("attendance.person")
                                    />
                                </div>
                            }
                        })}
                    </Transition>

                    <div class="flex items-center gap-1">
                        <GhostButton
                            label=l!("attendance.month.previous")
                            icon=Icon::ChevronLeft
                            on_click=Callback::new(move |()| shift_month(-1))
                        />
                        <span class="min-w-[9rem] text-center text-sm font-medium text-content tabular-nums">
                            {move || {
                                let (from, _) = month_span(anchor.get());
                                from.format("%Y-%m").to_string()
                            }}
                        </span>
                        <GhostButton
                            label=l!("attendance.month.next")
                            icon=Icon::ChevronRight
                            on_click=Callback::new(move |()| shift_month(1))
                        />
                    </div>
                </div>
            </Panel>

            <Show
                when=move || person.get().is_some()
                fallback=move || {
                    view! {
                        <Panel>
                            <p class="py-6 text-center text-sm text-content-subtle">
                                {l!("attendance.choose_somebody")}
                            </p>
                        </Panel>
                    }
                }
            >
                <Panel>
                    <Transition fallback=|| {
                        view! {
                            <p class="text-sm text-content-subtle">{l!("common.loading")}</p>
                        }
                    }>
                        {move || Suspend::new(async move {
                            let days = month.await.unwrap_or_default();

                            view! {
                                <TimesheetTable
                                    days=days
                                    person=person
                                    editing=editing
                                />
                            }
                        })}
                    </Transition>
                </Panel>
            </Show>

            <Show when=move || editing.with(|draft| draft.is_some()) fallback=|| ()>
                <DayEditor editing=editing rejected=rejected revision=revision />
            </Show>
        </div>
    }
}

#[component]
fn timesheet_table(
    days: Vec<TimesheetDay>,
    person: RwSignal<Option<Uuid>>,
    editing: RwSignal<Option<AttendanceInput>>,
) -> impl IntoView {
    let viewer = crate::ui::viewer::Viewer::get();
    let may_record = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::ATTENDANCE_RECORD))
        })
    });

    if days.is_empty() {
        return view! {
            <p class="py-6 text-center text-sm text-content-subtle">
                {l!("attendance.empty")}
            </p>
        }
        .into_any();
    }

    view! {
        <div class="overflow-x-auto">
            <table class="w-full min-w-[36rem] text-sm">
                <thead>
                    <tr class="border-b border-edge text-left text-xs text-content-muted">
                        <th class="py-1.5 font-medium">{l!("attendance.date")}</th>
                        <th class="py-1.5 font-medium">{l!("attendance.day")}</th>
                        <th class="py-1.5 font-medium">{l!("attendance.in")}</th>
                        <th class="py-1.5 font-medium">{l!("attendance.out")}</th>
                        <th class="py-1.5 font-medium">{l!("attendance.source")}</th>
                        <th class="py-1.5"></th>
                    </tr>
                </thead>
                <tbody>
                    {days
                        .into_iter()
                        .map(|day| {
                            let (label, tone) = outcome_badge(&day.outcome);
                            let named = day_off_name(&day.outcome);
                            let on_date = day.on_date;
                            let record_id = day.record.as_ref().map(|record| record.id);
                            let checked_in = day
                                .record
                                .as_ref()
                                .and_then(|record| record.checked_in_at)
                                .map(|at| at.format("%H:%M").to_string())
                                .unwrap_or_default();
                            let checked_out = day
                                .record
                                .as_ref()
                                .and_then(|record| record.checked_out_at)
                                .map(|at| at.format("%H:%M").to_string())
                                .unwrap_or_default();
                            let source = day
                                .record
                                .as_ref()
                                .map(|record| crate::i18n::t(&record.source.label()))
                                .unwrap_or_default();

                            view! {
                                <tr class="border-b border-edge/60">
                                    <td class="py-1.5 tabular-nums text-content">
                                        {on_date.to_string()}
                                    </td>
                                    <td class="py-1.5">
                                        <div class="flex flex-wrap items-center gap-1.5">
                                            <Badge label=label tone=tone />
                                            {named
                                                .map(|named| {
                                                    view! {
                                                        <span class="text-2xs text-content-subtle">
                                                            {named}
                                                        </span>
                                                    }
                                                })}
                                        </div>
                                    </td>
                                    <td class="py-1.5 tabular-nums text-content-muted">
                                        {checked_in}
                                    </td>
                                    <td class="py-1.5 tabular-nums text-content-muted">
                                        {checked_out}
                                    </td>
                                    <td class="py-1.5 text-xs text-content-muted">{source}</td>
                                    <td class="py-1.5 text-right">
                                        <Show when=move || may_record.get() fallback=|| ()>
                                            <GhostButton
                                                label=if record_id.is_some() {
                                                    l!("common.edit")
                                                } else {
                                                    l!("attendance.record")
                                                }
                                                icon=Icon::Pencil
                                                on_click=Callback::new(move |()| {
                                                    let Some(who) = person.get_untracked() else {
                                                        return;
                                                    };

                                                    leptos::task::spawn_local(async move {
                                                        // An existing row is fetched rather than
                                                        // rebuilt from the table: the row carries
                                                        // what the month needed, and the form needs
                                                        // the note as well.
                                                        let draft = match record_id {
                                                            Some(id) => attendance_edit(id).await.ok(),
                                                            None => {
                                                                blank_attendance(who, on_date).await.ok()
                                                            }
                                                        };

                                                        if let Some(draft) = draft {
                                                            editing.set(Some(draft));
                                                        }
                                                    });
                                                })
                                            />
                                        </Show>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

#[component]
fn day_editor(
    editing: RwSignal<Option<AttendanceInput>>,
    rejected: RwSignal<Option<String>>,
    revision: RwSignal<u32>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let saving = RwSignal::new(false);

    let status_options = AttendanceStatus::ALL
        .iter()
        .map(|status| Choice::new(status.as_str(), crate::i18n::t(&status.label())))
        .collect::<Vec<_>>();

    let source_options = AttendanceSource::ALL
        .iter()
        .map(|source| Choice::new(source.as_str(), crate::i18n::t(&source.label())))
        .collect::<Vec<_>>();

    let save = move || {
        let Some(draft) = editing.get_untracked() else {
            return;
        };

        saving.set(true);
        rejected.set(None);

        leptos::task::spawn_local(async move {
            let result = save_attendance(draft).await;
            saving.set(false);

            match result {
                Ok(Submission::Saved(_)) => {
                    alerts.post(Alert::success(l!("attendance.saved")));
                    editing.set(None);
                    revision.update(|count| *count = count.wrapping_add(1));
                }
                Ok(Submission::Rejected(errors)) => {
                    rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                }
                Err(err) => rejected.set(Some(err.to_string())),
            }
        });
    };

    let remove = move || {
        let Some(id) = editing.with_untracked(|draft| draft.as_ref().and_then(|d| d.id)) else {
            return;
        };

        leptos::task::spawn_local(async move {
            match delete_attendance(id).await {
                Ok(Submission::Saved(())) => {
                    alerts.post(Alert::success(l!("attendance.deleted")));
                    editing.set(None);
                    revision.update(|count| *count = count.wrapping_add(1));
                }
                Ok(Submission::Rejected(errors)) => {
                    if let Some(error) = errors.first() {
                        alerts.post(Alert::warning(crate::i18n::t(&error.message)));
                    }
                }
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
        });
    };

    let dated = move || {
        editing
            .with(|draft| draft.as_ref().and_then(|d| d.on_date))
            .map(|on| on.to_string())
            .unwrap_or_default()
    };

    let stored = move || editing.with(|draft| draft.as_ref().is_some_and(|d| d.id.is_some()));

    view! {
        <Panel>
            <Section title=Signal::derive(dated).get()>
                <div class="grid gap-3 sm:grid-cols-2">
                    <div class="block space-y-1">
                        <label
                            for="attendance-status"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("attendance.status")}
                        </label>
                        <SelectField
                            id="attendance-status"
                            value=Signal::derive(move || {
                                editing
                                    .with(|draft| {
                                        draft
                                            .as_ref()
                                            .map(|d| d.status.as_str().to_owned())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |value: String| {
                                if let Some(status) = AttendanceStatus::parse(&value) {
                                    editing
                                        .update(|draft| {
                                            if let Some(draft) = draft.as_mut() {
                                                draft.status = status;
                                            }
                                        });
                                }
                            })
                            options=status_options
                            placeholder=l!("common.not_set")
                            label=l!("attendance.status")
                        />
                    </div>

                    <div class="block space-y-1">
                        <label
                            for="attendance-source"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("attendance.source")}
                        </label>
                        <SelectField
                            id="attendance-source"
                            value=Signal::derive(move || {
                                editing
                                    .with(|draft| {
                                        draft
                                            .as_ref()
                                            .map(|d| d.source.as_str().to_owned())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |value: String| {
                                if let Some(source) = AttendanceSource::parse(&value) {
                                    editing
                                        .update(|draft| {
                                            if let Some(draft) = draft.as_mut() {
                                                draft.source = source;
                                            }
                                        });
                                }
                            })
                            options=source_options
                            placeholder=l!("common.not_set")
                            label=l!("attendance.source")
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("attendance.source.help")}
                        </span>
                    </div>
                </div>

                <label class="mt-3 block space-y-1">
                    <span class="text-xs font-medium text-content-muted">
                        {l!("attendance.note")}
                    </span>
                    <textarea
                        class="w-full"
                        rows="2"
                        prop:value=move || {
                            editing
                                .with(|draft| {
                                    draft.as_ref().map(|d| d.note.clone()).unwrap_or_default()
                                })
                        }
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            editing
                                .update(|draft| {
                                    if let Some(draft) = draft.as_mut() {
                                        draft.note = value;
                                    }
                                });
                        }
                    />
                </label>

                <div class="mt-3 flex flex-wrap items-center justify-end gap-2">
                    <GhostButton
                        label=l!("common.cancel")
                        icon=Icon::X
                        on_click=Callback::new(move |()| editing.set(None))
                    />

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
        </Panel>
    }
}
