//! The attachments section: the paperwork a record came with.
//!
//! Drop `<Attachments record=RecordRef::new(kinds::RECEIPT, id) />` inside a
//! [`Panel`](crate::components::page::Panel) and the record has attachments.
//! Nothing else is needed - the addressing is the same two values a record's
//! history already uses, and the permission is resolved on the server from the
//! entity kind, so a screen cannot grant itself access by asking nicely.
//!
//! # Two steps, because an upload is a job
//!
//! ```text
//!   POST /files/upload?bucket=attachments   the bytes; answers with an id
//!   upload_status(id)                       polled until the job has decided
//!   attach_file(input)                      the link, once it is stored
//! ```
//!
//! The same shape as the item pictures tab, and for the same reason: the
//! request that carries the bytes returns as soon as they are safely in
//! quarantine, and what they *are* is decided afterwards. Nothing is linked to
//! a record until that verdict is in, so the list never holds a row whose
//! download would fail.
//!
//! # A record has to exist first
//!
//! An attachment names a record, so there has to be one. A screen editing an
//! unsaved draft should not render this at all - see `attachments.save_first`
//! for the sentence to show instead.

use leptos::prelude::*;
use phonix_core::audit::EntityKind;
use phonix_core::files::attachment::{Attachment, RecordRef};
use uuid::Uuid;

use crate::icons::{Icon, IconSize};
use crate::l;
use crate::components::page::{Notice, Section, Tone};
use crate::components::preview::{PreviewButton, Previewable, Previews};
use crate::server_fns::file_fns::record_attachments;
use crate::ui::alert::{Alert, Alerts, Confirm};

/// Everything filed against one record, with a control to add another.
#[component]
pub fn attachments(
    record: RecordRef,
    /// Start open. Closed is right for a form somebody is keying; open is
    /// right for a document somebody is reading back.
    #[prop(optional)]
    open: bool,
) -> impl IntoView {
    let record = StoredValue::new(record);
    let rows = RwSignal::new(Vec::<Attachment>::new());
    let busy = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);

    let alerts = Alerts::get();

    let reload = Callback::new(move |()| {
        leptos::task::spawn_local(async move {
            if let Ok(fresh) = record_attachments(record.get_value()).await {
                rows.set(fresh);
            }
        });
    });

    Effect::new(move |_| reload.run(()));

    let remove = Callback::new(move |id: Uuid| {
        alerts.ask(
            Confirm::new(l!("attachments.remove.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match crate::server_fns::file_fns::detach_file(id).await {
                        Ok(phonix_core::form::Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("attachments.removed")));
                            let _ = reload.try_run(());
                        }
                        Ok(phonix_core::form::Submission::Rejected(errors)) => {
                            if let Some(error) = errors.first() {
                                alerts.post(Alert::warning(crate::i18n::t(&error.message)));
                            }
                        }
                        Err(err) => alerts.post(Alert::failure(err.to_string())),
                    }
                });
            })
            .titled(l!("attachments.remove"))
            .confirm_label(l!("attachments.remove")),
        );
    });

    view! {
        <Section
            title=l!("attachments.title")
            description=l!("attachments.help")
            collapsible=true
            open=open
        >
            <div class="space-y-2">
                <Notice message=Signal::derive(move || message.get()) tone=Tone::Danger />

                <label class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-control border border-edge px-3 text-sm text-content-muted hover:bg-surface-hover hover:text-content">
                    <Icon icon=Icon::Upload size=IconSize::Xs />
                    {move || {
                        if busy.get() { l!("attachments.uploading") } else { l!("attachments.add") }
                    }}
                    <input
                        type="file"
                        class="sr-only"
                        disabled=move || busy.get()
                        on:change=move |ev| {
                            upload(ev, record.get_value(), busy, message, reload)
                        }
                    />
                </label>

                <Show
                    when=move || !rows.get().is_empty()
                    fallback=|| {
                        view! {
                            <p class="text-sm text-content-subtle">{l!("attachments.none")}</p>
                        }
                    }
                >
                    <ul class="divide-y divide-edge/60 rounded-control border border-edge">
                        {move || {
                            rows.get()
                                .into_iter()
                                .map(|row| view! { <AttachmentRow row=row remove=remove /> })
                                .collect_view()
                        }}
                    </ul>
                </Show>
            </div>
        </Section>
    }
}

#[component]
fn attachment_row(row: Attachment, remove: Callback<Uuid>) -> impl IntoView {
    let id = row.id;
    let label = row.label();
    let href = row.href();
    let size = phonix_core::files::human_size(row.file.byte_size);
    let kind = row.file.type_label().map(|kind| crate::i18n::t(&kind));
    let filed_by = row.attached_by_name.clone();

    // A scan, an invoice and a spreadsheet all end up on the same record, and
    // only the first two can be shown. `Previewable` answers which this is; the
    // name below opens the pane when it can and saves the file when it cannot,
    // so the row never offers something that will not happen.
    let item = Previewable::attachment(&row);
    let showable = item.is_showable();
    let opened = item.clone();
    let previews = Previews::get();

    view! {
        <li class="flex items-center gap-2 px-2 py-1.5">
            <span class="shrink-0 text-content-subtle">
                <Icon icon=Icon::FileText size=IconSize::Xs />
            </span>

            <div class="min-w-0 flex-1">
                // `download` on the anchor for anything with no preview, so a
                // spreadsheet is saved rather than handed to a viewer this page
                // does not control. What can be previewed opens the pane
                // instead, and the download stays one button along.
                <a
                    href=href
                    download=(!showable).then(|| label.clone())
                    class="block truncate text-left text-sm text-content hover:text-brand"
                    title=label.clone()
                    on:click=move |event| {
                        if showable {
                            event.prevent_default();
                            previews.open(opened.clone());
                        }
                    }
                >
                    {label.clone()}
                </a>
                <p class="truncate text-2xs text-content-subtle">
                    {[kind, Some(size), filed_by]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ")}
                </p>
            </div>

            <PreviewButton item=item />

            <button
                type="button"
                class="shrink-0 rounded-control p-1 text-content-subtle hover:bg-surface-hover hover:text-danger"
                title=l!("attachments.remove")
                aria-label=l!("attachments.remove")
                on:click=move |_| {
                    let _ = remove.try_run(id);
                }
            >
                <Icon icon=Icon::Trash2 size=IconSize::Xs />
            </button>
        </li>
    }
}

/// What to show where a record does not exist yet.
///
/// A separate component rather than a branch inside [`Attachments`]: the
/// address is the whole of what this needs, and a component that renders
/// itself as "you cannot use me" is one that has to be given an id it does not
/// have.
#[component]
pub fn attachments_pending(#[prop(optional)] kind: Option<EntityKind>) -> impl IntoView {
    let _ = kind;

    view! {
        <Section title=l!("attachments.title") collapsible=true>
            <p class="text-sm text-content-subtle">{l!("attachments.save_first")}</p>
        </Section>
    }
}

// ---------------------------------------------------------------------------

#[cfg(feature = "hydrate")]
mod browser;

#[cfg(feature = "hydrate")]
use browser::upload;

#[cfg(not(feature = "hydrate"))]
fn upload(
    _ev: leptos::ev::Event,
    _record: RecordRef,
    _busy: RwSignal<bool>,
    _message: RwSignal<Option<String>>,
    _reload: Callback<()>,
) {
}
