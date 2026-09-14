//! What all four statements need: a span, a date field, and a money column.

use chrono::NaiveDate;
use leptos::prelude::*;
use phonix_core::money::Money;

use crate::l;
use crate::server_fns::books_fns::report_span;

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

/// One end of a span.
#[component]
pub fn date_field(
    #[prop(into)] label: String,
    value: Signal<Option<NaiveDate>>,
    on_pick: Callback<NaiveDate>,
) -> impl IntoView {
    view! {
        <label class="flex items-center gap-2 text-xs text-content-subtle">
            {label}
            <input
                type="date"
                class="h-8 rounded-control border border-edge bg-surface px-2 text-sm text-content"
                prop:value=move || value.get().map(|on| on.to_string()).unwrap_or_default()
                on:change=move |ev| {
                    if let Ok(picked) = event_target_value(&ev).parse::<NaiveDate>() {
                        on_pick.run(picked);
                    }
                }
            />
        </label>
    }
}

/// The strip under a report's title: what it is in, and over what.
#[component]
pub fn report_note(#[prop(into)] currency: String) -> impl IntoView {
    view! {
        <p class="text-xs text-content-subtle">{l!("reports.currency_note", currency = currency)}</p>
    }
}

/// An amount, right-aligned and in the figures a ledger is read in.
///
/// Negative stays negative and is not painted red: a credit balance in the
/// debit column is a fact about the account, not a warning.
pub fn amount(money: Money) -> String {
    money.to_display_string()
}

/// The classes every money cell carries, so two reports cannot line their
/// columns up differently.
pub const MONEY_CELL: &str = "py-1.5 pl-3 text-right tabular-nums text-content-muted";

/// The same, for a figure somebody is meant to read first.
pub const MONEY_TOTAL: &str = "py-1.5 pl-3 text-right tabular-nums font-medium text-content";

/// A heading cell over a money column.
pub const MONEY_HEAD: &str = "py-2 pl-3 text-right font-medium";
