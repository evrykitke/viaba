//! What all four statements need: a span, and the controls that change it.
//!
//! It held the money column and the table classes too, until the statements
//! moved onto the report engine and stopped drawing tables of their own.

use chrono::{Days, NaiveDate};
use leptos::prelude::*;
use phonix_core::query::DateRange;

use crate::l;
use crate::server_fns::books_fns::report_span;
use crate::ui::table::DatePreset;
use crate::ui::table::date::midnight;
use crate::ui::table::date_picker::{DatePointPicker, DateSpanPicker, last_day};

/// The named spans a financial report is read over.
///
/// Not [`DatePreset::COMMON`]. A list of what happened is narrowed to today or
/// this week; a profit and loss is drawn for a month or a year, and offering
/// "yesterday" beside them would be offering a statement nobody runs.
const SPANS: &[DatePreset] = &[
    DatePreset::ThisMonth,
    DatePreset::LastMonth,
    DatePreset::ThisYear,
    DatePreset::LastYear,
];

/// The span a report opens on, asked of the server.
///
/// `None` until it answers, which is what the screen waits on. The server is
/// asked rather than the browser told, for two reasons: it is the only side
/// that knows when this workspace's financial year began, and a date worked
/// out in the browser as well as during the server's render is a hydration
/// mismatch on any night the two disagree about what day it is.
pub fn opening_span() -> RwSignal<Option<(NaiveDate, NaiveDate)>> {
    let span = RwSignal::new(None);

    let fetched = Resource::new(|| (), |()| async move { report_span().await.ok() });

    Effect::new(move |_| {
        // Only the opening. Once somebody has chosen dates, a late answer
        // must not move them back.
        if span.get_untracked().is_none()
            && let Some(Some(answer)) = fetched.get()
        {
            span.set(Some(answer));
        }
    });

    span
}

/// The span a report is read over, as the control the grids are narrowed with.
///
/// # Two days, and the picker's two instants
///
/// A grid narrows to a half-open range of instants - midnight to the midnight
/// after the last day - because that is what a `WHERE` clause on a timestamp
/// needs. A report is read over two dates, both included, because that is what
/// is printed at the top of it. The conversion between them lives here and
/// nowhere else.
///
/// # An open end keeps the one already in force
///
/// The panel can leave either end unset. A statement cannot be drawn over an
/// unbounded span, so an end that came back empty is ignored rather than
/// stored - which is also why the control is not offered the clearing button.
///
/// # It is not rendered until the span is known
///
/// The dates come from the server, in an effect, so on the server's render
/// there is nothing here at all. That is deliberate: the button names the span
/// it is showing, naming it means asking what day it is, and a clock read
/// during both renders is how the two come out different.
#[component]
pub fn span_picker(span: RwSignal<Option<(NaiveDate, NaiveDate)>>) -> impl IntoView {
    let range = Signal::derive(move || match span.get() {
        Some((from, to)) => DateRange::new(
            Some(midnight(from)),
            to.checked_add_days(Days::new(1)).map(midnight),
        ),
        None => DateRange::ANY,
    });

    let on_change = Callback::new(move |chosen: DateRange| {
        span.update(|span| {
            let Some((from, to)) = span.as_mut() else {
                return;
            };

            if let Some(start) = chosen.from {
                *from = start.date_naive();
            }

            if let Some(end) = last_day(chosen) {
                *to = end;
            }
        });
    });

    view! {
        <Show when=move || span.get().is_some() fallback=|| ()>
            <DateSpanPicker
                key="report-span"
                label=l!("reports.span")
                presets=SPANS
                range=range
                on_change=on_change
            />
        </Show>
    }
}

/// The date a report is drawn *at*, for the one statement that is a photograph.
#[component]
pub fn as_at_picker(span: RwSignal<Option<(NaiveDate, NaiveDate)>>) -> impl IntoView {
    let day = Signal::derive(move || span.get().map(|(_, to)| to));

    view! {
        <Show when=move || span.get().is_some() fallback=|| ()>
            <DatePointPicker
                label=l!("reports.as_at")
                day=day
                on_pick=Callback::new(move |picked: NaiveDate| {
                    span.update(|span| {
                        if let Some((_, to)) = span.as_mut() {
                            *to = picked;
                        }
                    });
                })
            />
        </Show>
    }
}
