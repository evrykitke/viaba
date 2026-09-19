//! One holiday calendar: the span it covers, and the days off on it.
//!
//! Its own file rather than beside the role and place forms in
//! [`super::reference`], which share one because they are the same eighty
//! lines. This is not: a calendar carries child rows, and the weekly-off
//! generator is the part of the screen anybody actually uses — nobody types out
//! fifty-two Saturdays.

use app_hr::holiday::{HolidayInput, HolidayListInput};
use chrono::{NaiveDate, Weekday};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::hr_fns::{
    blank_holiday_list, delete_holiday_list, holiday_list_edit, save_holiday_list,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::card::CollapsibleCard;
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const CALENDARS: &str = "/people/holidays";

/// The weekdays the generator offers, in the order a week runs.
const WEEK: &[Weekday] = &[
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

/// What a weekday is called, from the catalogue rather than from chrono - whose
/// `Display` is English and would put "Saturday" on a French screen.
fn weekday_label(weekday: Weekday) -> String {
    match weekday {
        Weekday::Mon => l!("weekday.monday"),
        Weekday::Tue => l!("weekday.tuesday"),
        Weekday::Wed => l!("weekday.wednesday"),
        Weekday::Thu => l!("weekday.thursday"),
        Weekday::Fri => l!("weekday.friday"),
        Weekday::Sat => l!("weekday.saturday"),
        Weekday::Sun => l!("weekday.sunday"),
    }
}

fn weekday_of(raw: &str) -> Option<Weekday> {
    WEEK.iter()
        .copied()
        .find(|weekday| weekday.to_string() == raw)
}

#[component]
pub fn holiday_list_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let raw = move || params.with(|params| params.get("id").unwrap_or_default());

    let draft = Resource::new(raw, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => holiday_list_edit(id).await,
            Err(_) => blank_holiday_list().await,
        }
    });

    view! {
        <Title text=format!("{} | Evrykit", l!("entity.holiday_list.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(draft) => {
                        let heading = if draft.id.is_some() {
                            draft.name.clone()
                        } else {
                            l!("holidays.new")
                        };

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    icon=Icon::Calendar
                                    back=(CALENDARS, l!("holidays.title"))
                                />
                                <HolidayListForm draft=draft />
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.holiday_list.singular")
                                    icon=Icon::Calendar
                                    back=(CALENDARS, l!("holidays.title"))
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
fn holiday_list_form(draft: HolidayListInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);
    let weekday = RwSignal::new(Weekday::Sat);
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let weekday_options = WEEK
        .iter()
        .map(|day| Choice::new(day.to_string(), weekday_label(*day)))
        .collect::<Vec<_>>();

    // Replaces the generated rows rather than adding to them, so pressing the
    // button twice does not put every Saturday on the calendar twice - which
    // `check` would refuse, with the screen unable to say which day. The days
    // somebody named by hand are left alone, which is what `is_weekly_off` is
    // in the schema for.
    let generate = move || {
        let chosen = weekday.get_untracked();
        let called = weekday_label(chosen);
        let generated = draft.with_untracked(|d| d.weekly_offs(chosen, &called));

        if generated.is_empty() {
            alerts.post(Alert::warning(l!("holidays.generate.needs_a_span")));
            return;
        }

        draft.update(|d| {
            d.holidays.retain(|day| !day.is_weekly_off);
            d.holidays.extend(generated);
            d.holidays.sort_by_key(|day| day.observed_on);
        });
    };

    let add_day = move || {
        draft.update(|d| {
            d.holidays.push(HolidayInput {
                observed_on: d.valid_from,
                name: String::new(),
                is_weekly_off: false,
            });
        });
    };

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_holiday_list(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("holidays.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/people/holidays/{id}"),
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
                Confirm::new(l!("holidays.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_holiday_list(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("holidays.deleted")));
                                navigate(CALENDARS, leptos_router::NavigateOptions::default());
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
                            {l!("holidays.valid_from")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:value=move || {
                                draft.with(|d| d.valid_from.map(|on| on.to_string()))
                                    .unwrap_or_default()
                            }
                            on:input=move |ev| {
                                let parsed = event_target_value(&ev).parse::<NaiveDate>().ok();
                                draft.update(|d| d.valid_from = parsed);
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("holidays.valid_to")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:value=move || {
                                draft.with(|d| d.valid_to.map(|on| on.to_string()))
                                    .unwrap_or_default()
                            }
                            on:input=move |ev| {
                                let parsed = event_target_value(&ev).parse::<NaiveDate>().ok();
                                draft.update(|d| d.valid_to = parsed);
                            }
                        />
                    </label>
                </div>

                <span class="mt-1 block text-2xs text-content-subtle">
                    {l!("holidays.span.help")}
                </span>

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

                <Section title=l!("holidays.days")>
                    <div class="flex flex-wrap items-end gap-2">
                        <div class="block space-y-1">
                            <label
                                for="weekly-off"
                                class="block text-xs font-medium text-content-muted"
                            >
                                {l!("holidays.generate.weekly")}
                            </label>
                            <SelectField
                                id="weekly-off"
                                value=Signal::derive(move || weekday.get().to_string())
                                on_change=Callback::new(move |value: String| {
                                    if let Some(chosen) = weekday_of(&value) {
                                        weekday.set(chosen);
                                    }
                                })
                                options=weekday_options
                                placeholder=l!("common.not_set")
                                label=l!("holidays.generate.weekly")
                            />
                        </div>

                        <GhostButton
                            label=l!("holidays.generate")
                            icon=Icon::RefreshCw
                            on_click=Callback::new(move |()| generate())
                        />

                        <GhostButton
                            label=l!("holidays.day.add")
                            icon=Icon::Plus
                            on_click=Callback::new(move |()| add_day())
                        />
                    </div>

                    <span class="mt-1 block text-2xs text-content-subtle">
                        {l!("holidays.generate.help")}
                    </span>

                    <Show
                        when=move || !draft.with(|d| d.holidays.is_empty())
                        fallback=move || {
                            view! {
                                <p class="py-3 text-sm text-content-subtle">
                                    {l!("holidays.days.none")}
                                </p>
                            }
                        }
                    >
                        <div class="mt-3 overflow-x-auto">
                            <table class="w-full min-w-[28rem] text-sm">
                                <tbody>
                                    <For
                                        each=move || {
                                            draft.with(|d| (0..d.holidays.len()).collect::<Vec<_>>())
                                        }
                                        key=|index| *index
                                        let:index
                                    >
                                        <tr class="border-t border-edge">
                                            <td class="py-1.5 pr-2">
                                                <input
                                                    type="date"
                                                    class="w-full"
                                                    prop:value=move || {
                                                        draft
                                                            .with(|d| {
                                                                d.holidays
                                                                    .get(index)
                                                                    .and_then(|day| day.observed_on)
                                                                    .map(|on| on.to_string())
                                                            })
                                                            .unwrap_or_default()
                                                    }
                                                    on:input=move |ev| {
                                                        let parsed = event_target_value(&ev)
                                                            .parse::<NaiveDate>()
                                                            .ok();
                                                        draft
                                                            .update(|d| {
                                                                if let Some(day) = d.holidays.get_mut(index) {
                                                                    day.observed_on = parsed;
                                                                }
                                                            });
                                                    }
                                                />
                                            </td>
                                            <td class="py-1.5 pr-2">
                                                <input
                                                    type="text"
                                                    class="w-full"
                                                    prop:value=move || {
                                                        draft
                                                            .with(|d| {
                                                                d.holidays
                                                                    .get(index)
                                                                    .map(|day| day.name.clone())
                                                            })
                                                            .unwrap_or_default()
                                                    }
                                                    on:input=move |ev| {
                                                        let value = event_target_value(&ev);
                                                        draft
                                                            .update(|d| {
                                                                if let Some(day) = d.holidays.get_mut(index) {
                                                                    day.name = value;
                                                                }
                                                            });
                                                    }
                                                />
                                            </td>
                                            <td class="py-1.5 pr-2">
                                                <Show
                                                    when=move || {
                                                        draft
                                                            .with(|d| {
                                                                d.holidays
                                                                    .get(index)
                                                                    .is_some_and(|day| day.is_weekly_off)
                                                            })
                                                    }
                                                    fallback=|| ()
                                                >
                                                    <Badge
                                                        label=l!("holidays.weekly_off")
                                                        tone=Tone::Neutral
                                                    />
                                                </Show>
                                            </td>
                                            <td class="py-1.5 text-right">
                                                <GhostButton
                                                    label=l!("common.remove")
                                                    icon=Icon::Trash2
                                                    on_click=Callback::new(move |()| {
                                                        draft
                                                            .update(|d| {
                                                                if index < d.holidays.len() {
                                                                    d.holidays.remove(index);
                                                                }
                                                            });
                                                    })
                                                />
                                            </td>
                                        </tr>
                                    </For>
                                </tbody>
                            </table>
                        </div>
                    </Show>
                </Section>

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
                    <RecordHistory kind=kinds::HOLIDAY_LIST id=id().map(|id| id.to_string()) />
                </CollapsibleCard>
            </Show>
        </div>
    }
}
