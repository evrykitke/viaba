//! Cards: a bordered block that holds one thing.
//!
//! # The collapsible one
//!
//! [`CollapsibleCard`] wears the same clothes as the shortcut tiles on an app's
//! front page - icon, title, a line of explanation - but instead of going
//! somewhere it opens. That resemblance is the point: a page can mix "go here"
//! and "the rest of it is in here" without the eye having to learn two shapes.
//!
//! It is **closed when it arrives**, always. A stack of cards that opened
//! itself would be a stack of headings with a page of prose between each pair,
//! which is the layout the card exists to avoid; and the one thing somebody
//! wants is the one they click.
//!
//! # Why `<details>` and not a signal
//!
//! Openness here is a property of one node with no consequences anywhere else,
//! and the platform already has that element. Taking it means the keyboard, the
//! accessibility tree, `Ctrl+F` in Chromium and the `open` attribute in a
//! printed page all work without a line of code - and, the reason that decided
//! it, nothing about the markup differs between the server's render and the
//! browser's. A `RwSignal<bool>` starting `false` on both sides would hydrate
//! correctly too, but it is a hydration question that has to keep being
//! answered correctly, and this one cannot be got wrong.
//!
//! The cost is that a caller cannot open a card from outside - no "expand all",
//! no opening one because a search matched inside it. When something needs
//! that, it wants a different component and not a prop on this one: a
//! controlled disclosure and an uncontrolled one behave differently under
//! every interaction, and one component pretending to be both is how a kit
//! grows a `controlled: bool`.
//!
//! ```ignore
//! <CollapsibleCard
//!     title=l!("ui.editor.title")
//!     detail=l!("ui.editor.detail")
//!     icon=Icon::Pencil
//!     meta="v0".to_owned()
//! >
//!     <p>"Whatever the card is hiding."</p>
//! </CollapsibleCard>
//! ```
//!
//! # Leaving the title off
//!
//! A card with no `title` is *headless*: a slim strip with the chevron on it
//! and nothing else, and the contents directly beneath.
//!
//! That is the right shape for the single card that is the whole page. The page
//! already has a heading, an icon and a line of explanation at the top, and a
//! card underneath repeating all three says the same thing twice and pushes the
//! first field further down for the privilege. What is still worth having is
//! the collapse - a long form somebody wants out of the way while they read
//! what is under it - so that is what is kept.
//!
//! It is not the shape for a card in a stack. There the header is how somebody
//! decides which one to open, and a stack of identical strips is a stack of
//! things nobody can choose between.

use leptos::prelude::*;

use crate::components::page::{Badge, Tone};
use crate::icons::{Icon, IconSize};

/// A card that opens.
///
/// See the [module documentation](self) for why openness is an attribute
/// rather than a signal, and why there is no way to open it from outside.
#[component]
pub fn collapsible_card(
    /// Left off for a headless card: a strip with the chevron and no header.
    /// See the module documentation for when that is right.
    #[prop(optional, into)]
    title: Option<String>,
    /// The line under the title. A sentence, not a label - this is the part
    /// somebody reads to decide whether to open it.
    #[prop(optional, into)]
    detail: Option<String>,
    #[prop(optional)] icon: Option<Icon>,
    /// A short word at the right of the header: a count, a version, a state.
    #[prop(optional, into)]
    meta: Option<String>,
    /// Start open. The default - closed - is the one to reach for; see the
    /// module docs. This exists for the single card on a page that *is* the
    /// page, where arriving closed would be a page with nothing on it.
    #[prop(optional)]
    open: bool,
    /// How many things inside are wrong.
    ///
    /// A closed card hides its contents from the eye and from nothing else. A
    /// field that failed validation inside one still submits, still draws its
    /// error, and still does it where nobody is looking - so a stack of closed
    /// cards would answer a rejected save by appearing to do nothing at all.
    /// This is the header saying which card to open.
    #[prop(optional, into)]
    problems: Signal<u32>,
    children: Children,
) -> impl IntoView {
    // No title is the whole of what makes a card headless: there is nothing for
    // the icon to sit beside and nothing for the detail to explain, so both are
    // dropped rather than drawn on their own.
    let headless = title.is_none();

    view! {
        // `group` so the chevron and the icon tile can answer to the card's
        // own `[open]` rather than each carrying state.
        //
        // The border is the only part of a closed card big enough to find
        // without reading it, which is why the problem count colours it as
        // well as adding a badge.
        <details
            class=move || {
                let edge = if problems.get() > 0 { "border-danger" } else { "border-edge" };
                format!("group rounded-card border {edge} bg-surface-raised")
            }
            open=open
        >
            <summary class=if headless {
                "flex cursor-pointer items-center justify-end gap-2 rounded-card px-3 py-1.5 hover:bg-surface-hover group-open:rounded-b-none"
            } else {
                "flex cursor-pointer items-start gap-3 rounded-card p-4 hover:bg-surface-hover group-open:rounded-b-none"
            }>
                {icon
                    .filter(|_| !headless)
                    .map(|icon| {
                        view! {
                            <span class="grid size-9 shrink-0 place-items-center rounded-control bg-surface-sunken text-content-muted transition-colors group-open:bg-brand-subtle group-open:text-brand">
                                <Icon icon=icon size=IconSize::Sm />
                            </span>
                        }
                    })}

                {title
                    .map(|title| {
                        view! {
                            <span class="min-w-0 flex-1">
                                <span class="block text-sm font-medium text-content">{title}</span>
                                {detail
                                    .map(|detail| {
                                        view! {
                                            <span class="mt-0.5 block text-xs leading-relaxed text-content-muted">
                                                {detail}
                                            </span>
                                        }
                                    })}
                            </span>
                        }
                    })}

                {move || {
                    let count = problems.get();
                    (count > 0)
                        .then(|| {
                            view! {
                                <span class="mt-0.5 shrink-0">
                                    <Badge
                                        label=crate::lp!("ui.card.problems", count)
                                        tone=Tone::Danger
                                        icon=Icon::TriangleAlert
                                    />
                                </span>
                            }
                        })
                }}

                {meta
                    .map(|meta| {
                        view! {
                            <span class="mt-0.5 shrink-0 font-mono text-2xs text-content-subtle">
                                {meta}
                            </span>
                        }
                    })}

                // Decoration: the summary is already announced as a disclosure
                // and already says whether it is expanded.
                <span
                    class=if headless {
                        "shrink-0 text-content-subtle transition-transform duration-150 group-open:rotate-180"
                    } else {
                        "mt-0.5 shrink-0 text-content-subtle transition-transform duration-150 group-open:rotate-180"
                    }
                    aria-hidden="true"
                >
                    <Icon icon=Icon::ChevronDown size=IconSize::Xs />
                </span>
            </summary>

            // Divided from the header rather than floated below it: the border
            // is what stops an open card reading as two cards. A headless card
            // has no header to be divided from, and a rule under a strip of
            // nothing would draw the eye to the one part with nothing in it.
            <div class=if headless { "px-4 pb-4" } else { "border-t border-edge px-4 py-4" }>
                {children()}
            </div>
        </details>
    }
}
