//! The accounting calendar.
//!
//! Closing a period is the strongest routine control an accounting system has:
//! it is what makes a filed report stay filed. So the screen is deliberately
//! plain - a list of months, each with one button - and both directions ask
//! before they act. Reopening asks harder, because a period that was closed was
//! closed on purpose.

use app_books::period::Period;
use chrono::{Datelike, Utc};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::permissions;

use crate::components::page::{Badge, PageHeader, Tone};
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::server_fns::books_fns::{
    list_periods, next_year_to_open, open_financial_year, set_period_closed,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::viewer::Viewer;

#[component]
pub fn periods_page() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let periods = Resource::new(
        move || reload.get(),
        |_| async move { list_periods().await.unwrap_or_default() },
    );
    let refresh = Callback::new(move |()| reload.update(|count| *count += 1));

    view! {
        <Title text=format!("{} | Phonix", l!("periods.title")) />

        <PageHeader
            title=l!("periods.title")
            subtitle=l!("periods.subtitle")
            icon=Icon::Calendar
        >
            <OpenYearButton refresh=refresh />
        </PageHeader>

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let periods = periods.await;

                if periods.is_empty() {
                    return view! {
                        <div class="rounded-card border border-edge bg-surface-raised px-4 py-8 text-center">
                            <p class="text-sm font-medium text-content">
                                {l!("periods.empty.title")}
                            </p>
                            <p class="mt-1 text-sm text-content-muted">
                                {l!("periods.empty.detail")}
                            </p>
                        </div>
                    }
                        .into_any();
                }

                view! {
                    <div class="overflow-hidden rounded-card border border-edge bg-surface-raised">
                        <ul>
                            {periods
                                .into_iter()
                                .map(|period| {
                                    view! { <PeriodRow period=period refresh=refresh /> }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    </div>
                }
                    .into_any()
            })}
        </Transition>
    }
}

/// One month, and the one thing that can be done to it.
#[component]
fn period_row(period: Period, refresh: Callback<()>) -> impl IntoView {
    let viewer = Viewer::get();
    let alerts = Alerts::get();

    let id = period.id;
    let label = period.label.clone();
    let closed = period.is_closed;
    let span = format!("{} – {}", period.starts_on, period.ends_on);
    // The month somebody is posting into today, marked so the row they want is
    // the one their eye lands on.
    let is_current = period.covers(Utc::now().date_naive());

    let may_manage = move || {
        viewer
            .get()
            .is_some_and(|user| user.can(permissions::PERIODS_MANAGE))
    };

    let ask = {
        let label = label.clone();

        move |_| {
            let label = label.clone();
            let question = if closed {
                l!("periods.reopen.confirm", period = label.clone())
            } else {
                l!("periods.close.confirm", period = label.clone())
            };
            let heading = if closed {
                l!("periods.reopen")
            } else {
                l!("periods.close")
            };

            alerts.ask(
                Confirm::new(question, move || {
                    leptos::task::spawn_local(async move {
                        match set_period_closed(id, !closed).await {
                            Ok(period) => {
                                alerts
                                    .post(Alert::success(if period.is_closed {
                                        l!("periods.closed", period = period.label.clone())
                                    } else {
                                        l!("periods.reopened", period = period.label.clone())
                                    }));
                                refresh.run(());
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(heading.clone())
                .confirm_label(heading),
            );
        }
    };

    view! {
        <li class="flex items-center gap-3 border-b border-edge px-4 py-2.5 last:border-b-0">
            <span class=move || {
                if closed { "text-content-subtle" } else { "text-success" }
            }>
                <Icon
                    icon=if closed { Icon::Lock } else { Icon::LockOpen }
                    size=IconSize::Sm
                />
            </span>

            <span class="w-20 shrink-0 font-mono text-sm tabular-nums text-content">{label}</span>
            <span class="hidden flex-1 text-xs tabular-nums text-content-subtle sm:block">
                {span}
            </span>

            <div class="ml-auto flex items-center gap-2">
                {is_current.then(|| view! { <Badge label=l!("periods.current") tone=Tone::Brand /> })}
                {closed.then(|| view! { <Badge label=l!("periods.closed_badge") /> })}

                <Show when=may_manage fallback=|| ()>
                    <button
                        type="button"
                        class="rounded-control border border-edge px-2 py-1 text-xs text-content-muted hover:bg-surface-hover hover:text-content"
                        on:click=ask.clone()
                    >
                        {if closed { l!("periods.reopen") } else { l!("periods.close") }}
                    </button>
                </Show>
            </div>
        </li>
    }
}

/// Opening a financial year.
///
/// Offers the year the workspace is in until that year is open, and the one
/// after the calendar's far end afterwards. Those are two different questions
/// and the button answers whichever the workspace actually has: a calendar that
/// has never been opened is discovered by somebody who cannot post today, and
/// one running out is discovered by somebody trying to post into January.
#[component]
fn open_year_button(refresh: Callback<()>) -> impl IntoView {
    let viewer = Viewer::get();
    let alerts = Alerts::get();

    let offered = Resource::new(
        || (),
        |()| async move {
            next_year_to_open()
                .await
                .unwrap_or_else(|_| Utc::now().date_naive().year())
        },
    );

    let may_manage = move || {
        viewer
            .get()
            .is_some_and(|user| user.can(permissions::PERIODS_MANAGE))
    };

    view! {
        <Show when=may_manage fallback=|| ()>
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    let next = offered.await;

                    let open = move |_| {
                        leptos::task::spawn_local(async move {
                            match open_financial_year(next).await {
                                Ok(0) => {
                                    alerts
                                        .post(Alert::info(l!("periods.already_open", year = next)))
                                }
                                Ok(created) => {
                                    alerts
                                        .post(
                                            Alert::success(
                                                l!("periods.opened", year = next, count = created),
                                            ),
                                        );
                                    refresh.run(());
                                }
                                Err(err) => alerts.post(Alert::failure(err.to_string())),
                            }
                        });
                    };

                    view! {
                        <button
                            type="button"
                            class="flex h-8 items-center gap-1.5 rounded-control border border-edge px-2.5 text-sm text-content-muted hover:bg-surface-hover hover:text-content"
                            on:click=open
                        >
                            <Icon icon=Icon::Plus size=IconSize::Sm />
                            {l!("periods.open_year", year = next)}
                        </button>
                    }
                })}
            </Suspense>
        </Show>
    }
}
