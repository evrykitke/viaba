//! The colour an account class is drawn in.
//!
//! Five classes, five fixed hues, defined once so the grid, the tree and the
//! detail header cannot disagree. Fixed rather than `var(--brand)` for the
//! reason [`Accent::swatch`](crate::theme::Accent::swatch) is: these appear
//! beside one another and each has to look like itself.
//!
//! Deliberately *not* [`Tone`](crate::components::page::Tone). That vocabulary
//! means success, warning and failure everywhere it is used, and painting
//! expenses in the danger red would teach the reader that an expense account is
//! a problem. Colour here classifies; it does not judge.
//!
//! The hues follow how the classes sit on a balance sheet and a profit and loss
//! account: what the business owns, what it owes, what is left over, what comes
//! in, what goes out.

use app_books::account::AccountClass;
use leptos::prelude::*;

use crate::i18n::t;

/// The swatch for a class. Mid-chroma oklch, legible on both themes.
pub const fn swatch(class: AccountClass) -> &'static str {
    match class {
        AccountClass::Asset => "oklch(0.58 0.19 255)",
        AccountClass::Liability => "oklch(0.68 0.16 55)",
        AccountClass::Equity => "oklch(0.58 0.22 292)",
        AccountClass::Revenue => "oklch(0.62 0.16 149)",
        AccountClass::Expense => "oklch(0.62 0.18 20)",
    }
}

/// A dot in the class colour. For a dense row, where a chip would be noise.
#[component]
pub fn class_dot(class: AccountClass) -> impl IntoView {
    view! {
        <span
            class="inline-block size-2 shrink-0 rounded-full"
            style=format!("background-color:{}", swatch(class))
            aria-hidden="true"
        />
    }
}

/// The class, named and coloured.
///
/// The word is always there: colour is never the only carrier of the meaning,
/// which is the same rule [`Tone::face`](crate::components::page::Tone) follows.
#[component]
pub fn class_chip(class: AccountClass) -> impl IntoView {
    let label = t(&class.label());
    let colour = swatch(class);

    view! {
        <span
            class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-2xs font-medium"
            style=format!("color:{colour};border-color:color-mix(in oklch, {colour} 40%, transparent);background-color:color-mix(in oklch, {colour} 12%, transparent)")
        >
            <span
                class="inline-block size-1.5 rounded-full"
                style=format!("background-color:{colour}")
                aria-hidden="true"
            />
            {label}
        </span>
    }
}
