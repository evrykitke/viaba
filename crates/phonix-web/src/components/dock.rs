//! The docked header: what a document is, on one strip that stays put.
//!
//! # The problem this solves
//!
//! A document screen - a receipt, an order, a bill - opens with a page header
//! and then a header *section*: a title block of twenty-odd lines, then eight
//! or ten fields laid out three to a row, each one a label, a control and
//! sometimes a line of help. Between the top of the screen and the first line
//! of the thing somebody actually came to key there can be four hundred
//! pixels, and every one of them is spent restating facts that were settled
//! before the lorry arrived.
//!
//! Worse, it scrolls away. The moment the line grid is long enough to be worth
//! having, Save and Post are off the top of the window.
//!
//! # What a dock is
//!
//! One strip, stuck to the top of the scroll area, carrying the three things
//! that stay true for the whole document:
//!
//! * **what it is** - the number, or "New receipt" before there is one, with
//!   the state beside it;
//! * **what you can do to it** - the buttons, which are now always reachable;
//! * **what it says** - the header fields, which fold away.
//!
//! Folded is the state a document spends most of its life in. Supplier,
//! warehouse and date are chosen once and then read at a glance, so the fold
//! leaves a `summary` line in their place - `ACME · Main store · 2026-09-10` -
//! and gives the screen back to the lines. Nothing is hidden that the strip
//! does not still say in shorter words.
//!
//! # `<details>`, again
//!
//! For the reasons in [`ui::card`](crate::ui::card): the fold is one node's
//! business, the element brings the keyboard and the accessibility tree with
//! it, and the markup is identical on the server and in the browser, so there
//! is no hydration question to get wrong. The cost is the same too - nothing
//! outside can open it - and it is the right cost here, because the one thing
//! that would want to (a rejected field inside the fold) is answered by
//! `problems`, which colours the strip and unfolds nothing.
//!
//! # Fields inside it
//!
//! [`DockField`] is the header's field: a label at `2xs`, the control directly
//! under it, and the help text moved into a `title` so it costs a hover
//! instead of a third line. A form field's job is different - it is read while
//! being filled in - which is why this is a second component and not a prop on
//! the first.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::icons::{Icon, IconSize};

/// The strip at the top of a document screen.
///
/// See the [module docs](self). The short version: title, state and actions
/// stay; the fields fold away behind a one-line digest.
#[component]
pub fn dock_header(
    /// The document's own name: its number, or what it will be called.
    #[prop(into)]
    title: String,
    /// What kind of document this is. A word, under the title.
    ///
    /// Kept an `Option` rather than stripped to a `String`: a screen forwards
    /// what it was given, and a document with no second line should draw none
    /// rather than draw an empty one.
    #[prop(optional_no_strip)]
    subtitle: Option<String>,
    #[prop(optional)] icon: Option<Icon>,
    /// A link back to the list this was opened from.
    #[prop(optional)]
    back: Option<(&'static str, String)>,
    /// The header's fields in one line, for when they are folded away.
    ///
    /// Reactive: it is a reading of the same draft the fields are editing, and
    /// it has to change as they do. ` · ` between the parts, the spelling
    /// `Lot::label` already uses.
    #[prop(optional, into)]
    summary: Signal<String>,
    /// How many fields inside the fold are wrong.
    ///
    /// A folded strip hides its fields from the eye and from nothing else, so
    /// a save rejected on a field nobody can see would look like a save that
    /// did nothing. This colours the strip instead.
    #[prop(optional, into)]
    problems: Signal<u32>,
    /// Start folded. A document being keyed for the first time wants its
    /// fields; one being read back does not.
    #[prop(optional)]
    folded: bool,
    /// The state badge, beside the title.
    ///
    /// Erased rather than a second `Children`: a component takes one set of
    /// children, and the fields are already it. `view! { ... }.into_any()`.
    #[prop(optional, into)]
    status: Option<AnyView>,
    /// The buttons for the whole document. Always on screen, which is the
    /// other half of what docking buys.
    #[prop(optional, into)]
    actions: Option<AnyView>,
    /// The header fields. [`DockField`] each.
    children: Children,
) -> impl IntoView {
    view! {
        <details
            class=move || {
                let edge = if problems.get() > 0 { "border-danger" } else { "border-edge" };
                format!(
                    "group sticky top-0 z-20 rounded-card border {edge} bg-surface-raised/95 \
                     backdrop-blur",
                )
            }
            open=!folded
        >
            <summary class="flex cursor-pointer list-none flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2">
                {icon
                    .map(|icon| {
                        view! {
                            <span class="grid size-7 shrink-0 place-items-center rounded-control bg-brand-subtle text-brand">
                                <Icon icon=icon size=IconSize::Sm />
                            </span>
                        }
                    })}

                <div class="min-w-0 flex-1">
                    <div class="flex min-w-0 items-center gap-2">
                        {back
                            .map(|(href, label)| {
                                view! {
                                    // Stops the click reaching the summary: a
                                    // link that also folded the header would
                                    // leave the page mid-animation.
                                    <A
                                        href=href
                                        attr:class="inline-flex shrink-0 items-center text-content-subtle hover:text-content"
                                        attr:title=label
                                        on:click=|ev| ev.stop_propagation()
                                    >
                                        <Icon icon=Icon::ArrowLeft size=IconSize::Xs />
                                    </A>
                                }
                            })}
                        <h1 class="truncate text-base font-semibold tracking-tight text-content">
                            {title}
                        </h1>
                        {status}
                    </div>

                    // Two lines that swap: the document's kind while the fields
                    // are open, and what they say once they are folded. One of
                    // them is always the more useful, and it is never both.
                    <p class="truncate text-2xs text-content-subtle group-open:hidden">
                        {move || summary.get()}
                    </p>
                    {subtitle
                        .map(|subtitle| {
                            view! {
                                <p class="hidden truncate text-2xs text-content-subtle group-open:block">
                                    {subtitle}
                                </p>
                            }
                        })}
                </div>

                {actions
                    .map(|actions| {
                        view! {
                            // The buttons are inside the summary so they stay
                            // on the strip when it folds; the click must not
                            // travel on to it, or saving would also fold the
                            // header under the cursor.
                            <div
                                class="flex shrink-0 items-center gap-1.5"
                                on:click=|ev| ev.stop_propagation()
                            >
                                {actions}
                            </div>
                        }
                    })}

                <span class="shrink-0 text-content-subtle transition-transform group-open:rotate-180">
                    <Icon icon=Icon::ChevronDown size=IconSize::Xs />
                </span>
            </summary>

            <div class="grid gap-x-3 gap-y-2 border-t border-edge px-3 py-2.5 sm:grid-cols-3 lg:grid-cols-4">
                {children()}
            </div>
        </details>
    }
}

/// One field on a [`DockHeader`].
///
/// Tighter than a form field on purpose: the label is `2xs`, there is no gap
/// worth naming between it and the control, and the help is a `title` rather
/// than a third line. Ten of these are a strip; ten form fields are a screen.
#[component]
pub fn dock_field(
    #[prop(into)] label: String,
    /// Ties the label to the control. Worth passing wherever the child is a
    /// component rather than a bare `<input>`, since only a `for` reaches one.
    #[prop(optional, into)]
    id: Option<String>,
    /// A hover, not a line. Anything a reader must see before typing belongs
    /// in the label instead.
    #[prop(optional, into)]
    help: Option<String>,
    children: Children,
) -> impl IntoView {
    let text = view! {
        <span class="block truncate text-2xs font-medium uppercase tracking-wide text-content-muted">
            {label}
        </span>
    };

    view! {
        <div class="min-w-0 space-y-0.5" title=help>
            {match id {
                // A `<label>` wrapping the control is what makes the text click
                // into it; where the control is a component with an id of its
                // own, `for` is the only thing that reaches it.
                None => view! { <label class="block">{text}</label> }.into_any(),
                Some(id) => view! { <label for=id>{text}</label> }.into_any(),
            }}
            {children()}
        </div>
    }
}
