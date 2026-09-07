//! The pictures tab.
//!
//! # Why an item has photographs at all
//!
//! A point-of-sale screen is a grid of pictures, and somebody serving a queue
//! picks by sight. They earn their place before any till exists too: a goods-in
//! screen where the receiver can see what should be in the box catches the
//! wrong delivery at the door rather than three weeks later during a count.
//!
//! # Three steps, not one
//!
//! ```text
//!   POST /files/upload?bucket=item-images   the bytes; answers with an id
//!   upload_status(id)                       polled until the job has decided
//!   attach_item_image(...)                  only then is it the item's
//! ```
//!
//! An upload is a job, so the response to the POST says "received", not
//! "accepted". Nothing is shown on the item until something has looked at the
//! file - which is what the quarantine exists for. The bucket's own limits are
//! the control; nothing in this file is.

use leptos::prelude::*;
use uuid::Uuid;

use app_inventory::image::{Gallery, Image};

use crate::components::page::{Badge, GhostButton, Notice, Panel, Tone};
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::server_fns::file_fns::content_url;
use crate::server_fns::inventory_fns::{detach_item_image, item_gallery};
use crate::ui::alert::{Alert, Alerts};

#[component]
pub fn pictures_panel(item_id: Uuid) -> impl IntoView {
    let gallery = RwSignal::new(Gallery::default());
    let busy = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);
    let alerts = Alerts::get();

    let reload = Callback::new(move |()| {
        leptos::task::spawn_local(async move {
            if let Ok(fresh) = item_gallery(item_id).await {
                gallery.set(fresh);
            }
        });
    });

    Effect::new(move |_| reload.run(()));

    let remove = Callback::new(move |image_id: Uuid| {
        leptos::task::spawn_local(async move {
            match detach_item_image(image_id).await {
                Ok(_) => reload.run(()),
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
        });
    });

    view! {
        <Panel title=l!("images.title") description=l!("images.help")>
            <div class="space-y-3">
                <Notice message=message tone=Tone::Danger />

                <label class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-control border border-edge px-3 text-sm text-content-muted hover:bg-surface-hover hover:text-content">
                    <Icon icon=Icon::Upload size=IconSize::Xs />
                    {l!("images.add")}
                    <input
                        type="file"
                        accept="image/*"
                        class="sr-only"
                        disabled=move || busy.get()
                        on:change=move |ev| upload(ev, item_id, busy, message, reload)
                    />
                </label>

                <Show
                    when=move || !gallery.get().is_empty()
                    fallback=|| {
                        view! { <p class="text-sm text-content-subtle">{l!("images.none")}</p> }
                    }
                >
                    <ul class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
                        {move || {
                            gallery
                                .get()
                                .images
                                .into_iter()
                                .map(|image| {
                                    view! { <PictureTile image=image remove=remove /> }
                                })
                                .collect_view()
                        }}
                    </ul>
                </Show>
            </div>
        </Panel>
    }
}

#[component]
fn picture_tile(image: Image, remove: Callback<Uuid>) -> impl IntoView {
    let id = image.id;
    let alt = image.alt_text.clone().unwrap_or_default();
    let is_the_items_own = image.is_the_items_own();

    view! {
        <li class="space-y-1.5 rounded-card border border-edge p-2">
            <img
                src=content_url(image.file_id)
                alt=alt
                class="aspect-square w-full rounded-control bg-surface-sunken object-cover"
            />
            <div class="flex items-center justify-between gap-2">
                <Badge label=if is_the_items_own {
                    l!("images.item_own")
                } else {
                    l!("images.variant_own")
                } />
                <GhostButton
                    label=l!("common.remove")
                    icon=Icon::Trash2
                    tone=Tone::Danger
                    on_click=Callback::new(move |()| remove.run(id))
                />
            </div>
        </li>
    }
}

// ---------------------------------------------------------------------------
// The browser half.
//
// Uploading needs `web_sys`, which exists only in the wasm build. Declared
// twice - once for the browser and once as a no-op - so the same markup
// compiles on the server, rather than a `cfg` inside the event handler.
// ---------------------------------------------------------------------------

#[cfg(feature = "hydrate")]
mod browser;

#[cfg(feature = "hydrate")]
use browser::upload;

#[cfg(not(feature = "hydrate"))]
fn upload(
    _ev: leptos::ev::Event,
    _item_id: Uuid,
    _busy: RwSignal<bool>,
    _message: RwSignal<Option<String>>,
    _reload: Callback<()>,
) {
}
