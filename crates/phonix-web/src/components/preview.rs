//! Looking at a file without leaving the page.
//!
//! # The problem with a download link
//!
//! A goods receipt has four supplier documents on it and somebody wants to know
//! which one is the delivery note. With a download link that is four saves into
//! the downloads folder, four windows, and four files to delete afterwards -
//! and on a phone it is worse, because a downloaded PDF leaves the browser
//! altogether and coming back means finding the tab again.
//!
//! # The shape
//!
//! Three things are needed to show a file, and only three: an address, a
//! [`Preview`] saying which viewer can draw it, and a name. That is
//! [`Previewable`], and it is deliberately not "a file row" - a PDF this
//! application *generated* has an address and a kind too, and rendering a
//! report ought to be the same component as reading an attachment rather than a
//! second one that looks like it.
//!
//! ```ignore
//! // An attachment somebody filed.
//! Previews::get().open(Previewable::attachment(&row));
//!
//! // A report this application just produced. Same pane, same dialog.
//! Previews::get().open(Previewable::pdf("/reports/stock-count.pdf", "Stock count"));
//! ```
//!
//! # Two surfaces, one pane
//!
//! [`FilePreview`] is the pane and can be dropped into any panel - a document
//! screen that wants its scan beside its lines uses it directly.
//! [`PreviewLayer`] is the same pane in a modal, mounted once at the root
//! beside the alert layer and opened from anywhere through [`Previews`]. The
//! reason for the root mount is the one `components::user_link` gives: an
//! attachment row lives inside a scrolling panel, and an overlay drawn there is
//! clipped by it.
//!
//! # What is not decided here
//!
//! Whether a file may be shown at all. The pane asks for `/files/{id}/preview`
//! and that route serves only what it can render safely, refusing everything
//! else with a `415` - see `phonix_server::files` for what that means for a
//! PDF, which is the interesting case. This module trusts [`Preview`] and draws
//! a download link wherever it says `None`.

use leptos::prelude::*;
use phonix_core::files::attachment::Attachment;
use phonix_core::files::{FileSummary, Preview};

use crate::i18n::t;
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::server_fns::file_fns::{content_url, preview_url};

/// A file, as a preview pane needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Previewable {
    /// What the pane loads. Not necessarily a stored file: a `blob:` URL held
    /// by the page works exactly as well, which is the point of the field being
    /// a string rather than an id.
    pub src: String,
    /// Which viewer can draw it.
    pub kind: Preview,
    /// What to call it, on the dialog's title bar and on the saved copy.
    pub name: String,
    /// Where *Download* points, when that is a different address from `src`. A
    /// stored file is served twice over, once for showing and once for saving,
    /// and the two have different dispositions.
    pub download: Option<String>,
    /// A line under the name - "PDF document · 240 KB". Absent for anything
    /// with no file row behind it.
    pub detail: Option<String>,
}

impl Previewable {
    /// A stored file, addressed by its id.
    pub fn file(summary: &FileSummary) -> Self {
        let detail = [
            summary.type_label().map(|label| t(&label)),
            Some(summary.size_label()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");

        Self {
            src: preview_url(summary.id),
            kind: summary.preview(),
            name: summary.original_name.clone(),
            download: Some(content_url(summary.id)),
            detail: Some(detail),
        }
    }

    /// An attachment, which is a stored file under the caption somebody gave
    /// it.
    pub fn attachment(row: &Attachment) -> Self {
        Self {
            name: row.label(),
            ..Self::file(&row.file)
        }
    }

    /// A PDF this application produced, at whatever address it lives.
    ///
    /// The constructor that makes this module worth more than attachments: a
    /// report is shown by the same pane that shows a supplier's invoice, so
    /// there is no second viewer to keep in step with the first.
    pub fn pdf(src: impl Into<String>, name: impl Into<String>) -> Self {
        let src = src.into();

        Self {
            download: Some(src.clone()),
            src,
            kind: Preview::Pdf,
            name: name.into(),
            detail: None,
        }
    }

    /// Whether there is anything to draw.
    pub fn is_showable(&self) -> bool {
        self.kind.is_showable()
    }
}

/// Which file is open in the dialog, shared between whatever opened it and the
/// layer that draws it.
///
/// A context rather than a prop, for the reason
/// [`OpenCard`](crate::components::user_link::OpenCard) is one: the row with
/// the button and the overlay with the pane never meet in the tree.
#[derive(Copy, Clone)]
pub struct Previews(RwSignal<Option<Previewable>>);

impl Previews {
    /// Make the preview layer reachable from everything below. The host calls
    /// this once, beside `Alerts::provide`.
    pub fn provide() {
        provide_context(Self(RwSignal::new(None)));
    }

    /// The previews for this tree, or a detached one where no host provided
    /// any - a component in a test opens nothing rather than taking the page
    /// down.
    pub fn get() -> Self {
        use_context::<Self>().unwrap_or_else(|| Self(RwSignal::new(None)))
    }

    pub fn open(self, item: Previewable) {
        self.0.set(Some(item));
    }

    pub fn close(self) {
        self.0.set(None);
    }
}

/// The pane: whichever viewer this file's kind calls for.
///
/// `height` is a Tailwind class rather than a number because the pane is used
/// at two sizes - most of the screen in the dialog, and cropped inside a panel
/// - and a caller that could pass any number would eventually pass a pixel
/// value that does not survive a phone.
#[component]
pub fn file_preview(
    item: Previewable,
    #[prop(into, default = "h-[70vh]".to_owned())] height: String,
) -> impl IntoView {
    let frame = format!("w-full {height} rounded-control border border-edge bg-surface");

    match item.kind {
        // `object-contain` rather than a crop: this is the file, and a preview
        // that cuts the top off a scanned invoice is worse than none.
        Preview::Image => view! {
            <img
                src=item.src
                alt=item.name
                class=format!("{frame} object-contain")
                loading="lazy"
            />
        }
        .into_any(),

        // A frame, so the browser's own viewer draws it. The origin is taken
        // away by the response's own headers rather than by a `sandbox`
        // attribute here: a screen must not be the thing deciding how dangerous
        // a file is, and a header applies even when the address is opened
        // directly.
        Preview::Pdf => view! {
            <iframe src=item.src title=item.name class=frame referrerpolicy="no-referrer"></iframe>
        }
        .into_any(),

        Preview::None => view! { <NoPreview item=item /> }.into_any(),
    }
}

/// What stands where a preview would be, for a format nothing here can draw.
///
/// Not an error and not an empty box: the file is fine, it is simply a
/// spreadsheet. So it says what it is and offers the one thing that does work.
#[component]
fn no_preview(item: Previewable) -> impl IntoView {
    let href = item.download.unwrap_or(item.src);

    view! {
        <div class="grid place-items-center rounded-control border border-dashed border-edge bg-surface px-4 py-10 text-center">
            <div class="space-y-2">
                <span class="inline-flex text-content-subtle">
                    <Icon icon=Icon::FileText size=IconSize::Md />
                </span>
                <p class="text-sm text-content">{item.name}</p>
                <p class="text-2xs text-content-subtle">{l!("preview.none")}</p>
                <a
                    href=href
                    download=""
                    class="inline-flex h-8 items-center gap-1.5 rounded-control border border-edge px-3 text-sm text-content-muted hover:bg-surface-hover hover:text-content"
                >
                    <Icon icon=Icon::Download size=IconSize::Xs />
                    {l!("preview.download")}
                </a>
            </div>
        </div>
    }
}

/// The button that opens one, for a row that has a file on it.
///
/// Renders nothing where the file cannot be shown - the row keeps its download
/// link, and an eye that opens a box saying "no preview" is a promise broken
/// once per click.
#[component]
pub fn preview_button(item: Previewable) -> impl IntoView {
    if !item.is_showable() {
        return ().into_any();
    }

    let previews = Previews::get();
    let label = l!("preview.open", name = item.name.clone());

    view! {
        <button
            type="button"
            class="shrink-0 rounded-control p-1 text-content-subtle hover:bg-surface-hover hover:text-content"
            title=label.clone()
            aria-label=label
            on:click=move |event| {
                // The row this sits in is often clickable itself, and looking
                // at a document must not also navigate away from the record
                // holding it.
                event.stop_propagation();
                event.prevent_default();
                previews.open(item.clone());
            }
        >
            <Icon icon=Icon::Eye size=IconSize::Xs />
        </button>
    }
    .into_any()
}

/// The dialog, mounted once at the root of the application.
///
/// Nothing is rendered and nothing is fetched until something opens a file: the
/// server can open none, so both sides agree on the first frame and there is no
/// hydration question here.
#[component]
pub fn preview_layer() -> impl IntoView {
    let previews = Previews::get();
    let open = previews.0;

    // Escape closes it, on the window rather than on the dialog - focus may
    // never have reached the pane, and a frame swallows the key once it has.
    Effect::new(move |_| {
        let handle = window_event_listener(leptos::ev::keydown, move |event| {
            if event.key() == "Escape" {
                previews.close();
            }
        });

        on_cleanup(move || handle.remove());
    });

    view! {
        {move || {
            open.get()
                .map(|item| {
                    let name = item.name.clone();
                    let titled = item.name.clone();
                    let detail = item.detail.clone();
                    let save = item.download.clone().unwrap_or_else(|| item.src.clone());
                    let away = item.src.clone();

                    view! {
                        <div
                            class="fixed inset-0 z-[70] grid place-items-center bg-overlay p-4"
                            role="dialog"
                            aria-modal="true"
                            aria-label=titled
                            on:click=move |_| previews.close()
                        >
                            <div
                                class="flex max-h-full w-[min(64rem,100%)] flex-col overflow-hidden rounded-card border border-edge bg-surface-raised shadow-xl"
                                on:click=move |event| event.stop_propagation()
                            >
                                <div class="flex items-center gap-2 border-b border-edge px-3 py-2">
                                    <div class="min-w-0 flex-1">
                                        <p class="truncate text-sm font-medium text-content">
                                            {name}
                                        </p>
                                        {detail
                                            .map(|detail| {
                                                view! {
                                                    <p class="truncate text-2xs text-content-subtle">
                                                        {detail}
                                                    </p>
                                                }
                                            })}
                                    </div>

                                    // A tab of its own, for the person who wants
                                    // the viewer's own zoom and page controls
                                    // rather than a pane inside a dialog.
                                    <a
                                        href=away
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        class="shrink-0 rounded-control p-1.5 text-content-subtle hover:bg-surface-hover hover:text-content"
                                        title=l!("preview.new_tab")
                                        aria-label=l!("preview.new_tab")
                                    >
                                        <Icon icon=Icon::ExternalLink size=IconSize::Xs />
                                    </a>

                                    <a
                                        href=save
                                        download=""
                                        class="shrink-0 rounded-control p-1.5 text-content-subtle hover:bg-surface-hover hover:text-content"
                                        title=l!("preview.download")
                                        aria-label=l!("preview.download")
                                    >
                                        <Icon icon=Icon::Download size=IconSize::Xs />
                                    </a>

                                    <button
                                        type="button"
                                        class="shrink-0 rounded-control p-1.5 text-content-subtle hover:bg-surface-hover hover:text-content"
                                        title=l!("common.close")
                                        aria-label=l!("common.close")
                                        on:click=move |_| previews.close()
                                    >
                                        <Icon icon=Icon::X size=IconSize::Xs />
                                    </button>
                                </div>

                                <div class="min-h-0 flex-1 overflow-auto p-3">
                                    <FilePreview item=item />
                                </div>
                            </div>
                        </div>
                    }
                })
        }}
    }
}
