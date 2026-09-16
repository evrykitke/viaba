//! A dialog over the page, for the form somebody just opened.
//!
//! # Rendered is open
//!
//! There is no `open` prop. A caller that has something to edit renders one
//! and a caller that does not renders nothing, which is the shape every screen
//! that wanted this already had - `editing.get().map(...)`. It also means the
//! contents are built fresh each time, so controls that read their opening
//! value once are re-seeded rather than showing the row before last.
//!
//! # What it is for
//!
//! A settings tab that appends its editor below its grid puts the row somebody
//! just clicked off the screen, and grows the page instead of focusing it.
//!
//! Four dialogs in this codebase were hand-rolled before this existed, at
//! three widths, some closing on Escape and some on a click outside. Three
//! of them are this now. The fourth is the message box in
//! [`ui::alert::host`](crate::ui::alert::host), which is not opened by
//! anybody and has to sit *above* whatever is open - see the note there.

use leptos::html;
use leptos::prelude::*;

use crate::icons::{Icon, IconSize};
use crate::l;

/// How wide the panel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalSize {
    /// A list of one-line choices - which columns are on, which format to
    /// write. Narrow, because a column of tick boxes across a page is a
    /// line of empty space with a word at each end.
    Small,
    /// A form, a confirmation, a short list.
    #[default]
    Regular,
    /// A form with something beside it - a preview, a second column.
    Large,
}

impl ModalSize {
    const fn width(self) -> &'static str {
        match self {
            Self::Small => "max-w-sm",
            Self::Regular => "max-w-2xl",
            Self::Large => "max-w-5xl",
        }
    }
}

/// A panel over the page, with the page dimmed behind it.
///
/// ```ignore
/// {move || editing.get().map(|row| view! {
///     <Modal title=l!("documents.title") size=ModalSize::Large on_close=close>
///         <DocumentEditor settings=row />
///     </Modal>
/// })}
/// ```
#[component]
pub fn modal(
    #[prop(into)] title: String,
    /// Asked to be dismissed - by Escape, by the backdrop, or by the close
    /// button. Whether that actually closes it is the caller's to decide,
    /// because the caller is what renders it.
    on_close: Callback<()>,
    #[prop(optional)] size: ModalSize,
    children: Children,
) -> impl IntoView {
    let panel: NodeRef<html::Div> = NodeRef::new();
    let label = title.clone();

    // Escape, bound on the window rather than the panel so it works before
    // anything inside has been focused. The listener lives exactly as long as
    // the dialog does, because the dialog is only mounted while it is open.
    Effect::new(move |_| {
        let handle = window_event_listener(leptos::ev::keydown, move |event| {
            if event.key() == "Escape" {
                on_close.run(());
            }
        });

        on_cleanup(move || handle.remove());
    });

    // Focus goes into the panel when it opens and back where it was when it
    // closes, so a keyboard is not left behind a dialog it cannot see.
    Effect::new(move |_| {
        let returning = focused();

        if let Some(node) = panel.get() {
            let _ = node.focus();
        }

        on_cleanup(move || restore(returning));
    });

    view! {
        <div
            // `inset-0` sizes this to the viewport; the height cap belongs on
            // the panel, which is the note `table::toolbar` left behind.
            class="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-overlay px-4 py-4"
            role="dialog"
            aria-modal="true"
            aria-label=label
            on:click=move |_| on_close.run(())
        >
            <div
                node_ref=panel
                tabindex="-1"
                class=format!(
                    "flex max-h-[90dvh] w-full {} flex-col overflow-hidden rounded-pop border \
                     border-edge bg-surface-raised shadow-pop outline-none",
                    size.width(),
                )
                // The backdrop closes on a click; the panel must not, or
                // typing in a field would dismiss the form.
                on:click=|event| event.stop_propagation()
            >
                <div class="flex items-center justify-between gap-2 border-b border-edge px-4 py-2.5">
                    <h2 class="truncate text-sm font-medium text-content">{title}</h2>
                    <button
                        type="button"
                        class="grid size-7 shrink-0 place-items-center rounded-control text-content-muted hover:bg-surface-hover hover:text-content"
                        aria-label=l!("common.close")
                        on:click=move |_| on_close.run(())
                    >
                        <Icon icon=Icon::X size=IconSize::Sm />
                    </button>
                </div>

                <div class="min-h-0 flex-1 overflow-y-auto p-4">{children()}</div>
            </div>
        </div>
    }
}

/// What had focus before the dialog opened.
#[cfg(feature = "hydrate")]
type Focused = Option<web_sys::HtmlElement>;

/// Always nothing: the server renders no dialog anybody can focus. An
/// `Option` rather than a unit so both builds hand the same shape around.
#[cfg(not(feature = "hydrate"))]
type Focused = Option<()>;

#[cfg(feature = "hydrate")]
fn focused() -> Focused {
    use wasm_bindgen::JsCast;

    leptos::prelude::document()
        .active_element()
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
}

#[cfg(not(feature = "hydrate"))]
const fn focused() -> Focused {
    None
}

#[cfg(feature = "hydrate")]
fn restore(focused: Focused) {
    if let Some(element) = focused {
        let _ = element.focus();
    }
}

#[cfg(not(feature = "hydrate"))]
const fn restore(_focused: Focused) {}
