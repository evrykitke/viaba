//! The variants tab: what an item is offered in.
//!
//! # The plan is shown before it is applied
//!
//! Six colours, five sizes and three materials is ninety rows, and a workspace
//! that meant to add one colour should read that number before the rows appear.
//! Choosing values changes nothing; the plan says what applying would do, and
//! applying is a second, separate click.
//!
//! Dropping a value retires the variants it made rather than deleting them.
//! Stock has moved against them and the moves are the audit trail, so the panel
//! says "retire" and means it.

use leptos::prelude::*;
use phonix_core::i18n::Message;
use phonix_core::msg;
use uuid::Uuid;

use app_inventory::variant::{Attribute, Plan, Selection, SelectionLine, VariantSummary};

use crate::components::page::{Badge, GhostButton, Panel, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    item_selection, item_variants, list_attributes, plan_variants, save_variant, set_variants,
};
use crate::ui::alert::{Alert, Alerts};

#[component]
pub fn variants_panel(item_id: Uuid) -> impl IntoView {
    let attributes = Resource::new(|| (), |()| async move { list_attributes().await });
    let chosen = Resource::new(move || item_id, |id| async move { item_selection(id).await });
    let variants = RwSignal::new(Vec::<VariantSummary>::new());

    let reload = Callback::new(move |()| {
        leptos::task::spawn_local(async move {
            if let Ok(rows) = item_variants(item_id).await {
                variants.set(rows);
            }
        });
    });

    // The first load. A resource would do it too, but the panel already needs
    // the callback for every reload after an apply, and one path is fewer.
    Effect::new(move |_| reload.run(()));

    view! {
        <div class="space-y-3">
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let attributes = attributes.await.unwrap_or_default();
                    let chosen = chosen.await.unwrap_or_default();

                    view! {
                        <SelectionEditor
                            item_id=item_id
                            attributes=attributes
                            chosen=chosen
                            reload=reload
                        />
                    }
                })}
            </Transition>

            <VariantList variants=variants reload=reload />
        </div>
    }
}

/// Which values of which attributes this item is offered in.
#[component]
fn selection_editor(
    item_id: Uuid,
    attributes: Vec<Attribute>,
    chosen: Selection,
    reload: Callback<()>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let picked = RwSignal::new(picked_from(&chosen));
    let plan = RwSignal::new(None::<Plan>);
    let busy = RwSignal::new(false);
    let attributes = StoredValue::new(attributes);

    // The selection as the server wants it, built from what is ticked. An
    // attribute with nothing ticked is left out rather than sent empty: it is
    // not something the item varies by.
    let selection = move || Selection {
        lines: attributes
            .get_value()
            .iter()
            .filter_map(|attribute| {
                let value_ids: Vec<Uuid> = attribute
                    .values
                    .iter()
                    .filter(|value| picked.get().contains(&value.id))
                    .map(|value| value.id)
                    .collect();

                (!value_ids.is_empty()).then(|| SelectionLine {
                    attribute_id: attribute.id,
                    attribute_name: attribute.name.clone(),
                    value_ids,
                })
            })
            .collect(),
    };

    let preview = Callback::new(move |()| {
        let wanted = selection();
        busy.set(true);
        leptos::task::spawn_local(async move {
            match plan_variants(item_id, wanted).await {
                Ok(answer) => plan.set(Some(answer)),
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
            busy.set(false);
        });
    });

    let apply = Callback::new(move |()| {
        let wanted = selection();
        busy.set(true);
        leptos::task::spawn_local(async move {
            match set_variants(item_id, wanted).await {
                Ok(answer) => {
                    plan.set(Some(answer));
                    let _ = reload.try_run(());
                    alerts.post(Alert::success(crate::i18n::t(&Message::new(
                        "variants.applied",
                    ))));
                }
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
            busy.set(false);
        });
    });

    view! {
        <Panel title=l!("variants.title") description=l!("variants.help")>
            <Show
                when=move || !attributes.get_value().is_empty()
                fallback=|| {
                    view! {
                        <p class="text-sm text-content-subtle">{l!("variants.no_attributes")}</p>
                    }
                }
            >
                <div class="space-y-3">
                    {attributes
                        .get_value()
                        .into_iter()
                        .map(|attribute| {
                            let name = attribute.name.clone();

                            view! {
                                <div class="space-y-1.5">
                                    <p class="text-xs font-medium text-content-muted">{name}</p>
                                    <div class="flex flex-wrap gap-1.5">
                                        {attribute
                                            .values
                                            .into_iter()
                                            .map(|value| {
                                                let id = value.id;
                                                let label = value.name.clone();
                                                let swatch = value.swatch.clone();

                                                view! {
                                                    <button
                                                        type="button"
                                                        class=move || {
                                                            let on = picked.get().contains(&id);
                                                            format!(
                                                                "inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs {}",
                                                                if on {
                                                                    "border-brand bg-brand-subtle text-brand"
                                                                } else {
                                                                    "border-edge text-content-muted hover:bg-surface-hover"
                                                                },
                                                            )
                                                        }
                                                        aria-pressed=move || picked.get().contains(&id).to_string()
                                                        on:click=move |_| {
                                                            picked
                                                                .update(|chosen| {
                                                                    if let Some(at) = chosen.iter().position(|held| *held == id) {
                                                                        chosen.remove(at);
                                                                    } else {
                                                                        chosen.push(id);
                                                                    }
                                                                });
                                                            plan.set(None);
                                                        }
                                                    >
                                                        {swatch
                                                            .map(|colour| {
                                                                view! {
                                                                    <span
                                                                        class="size-3 rounded-full border border-edge"
                                                                        style=format!("background-color: {colour}")
                                                                        aria-hidden="true"
                                                                    ></span>
                                                                }
                                                            })}
                                                        {label}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}

                    <div class="flex flex-wrap items-center gap-2 border-t border-edge pt-3">
                        <GhostButton
                            label=l!("variants.preview")
                            icon=Icon::Eye
                            disabled=busy
                            on_click=preview
                        />
                        <PrimaryButton
                            label=l!("variants.apply")
                            icon=Icon::Check
                            pending=busy
                            disabled=Signal::derive(move || {
                                plan.get().is_none_or(|plan| plan.changes_nothing())
                            })
                            on_click=apply
                        />
                        <span class="text-xs text-content-subtle">
                            {move || plan.get().map(|plan| describe(&plan))}
                        </span>
                    </div>
                </div>
            </Show>
        </Panel>
    }
}

/// The plan as a sentence somebody reads before clicking apply.
fn describe(plan: &Plan) -> String {
    if plan.changes_nothing() {
        return crate::i18n::t(&Message::new("variants.plan.nothing"));
    }

    let mut parts = Vec::new();

    if !plan.to_create.is_empty() {
        parts.push(crate::i18n::t(&msg!(
            "variants.plan.create",
            count = plan.to_create.len()
        )));
    }
    if !plan.to_retire.is_empty() {
        parts.push(crate::i18n::t(&msg!(
            "variants.plan.retire",
            count = plan.to_retire.len()
        )));
    }
    if !plan.to_revive.is_empty() {
        parts.push(crate::i18n::t(&msg!(
            "variants.plan.revive",
            count = plan.to_revive.len()
        )));
    }

    parts.join(" \u{b7} ")
}

/// Which value ids are ticked, flattened out of the stored selection.
fn picked_from(chosen: &Selection) -> Vec<Uuid> {
    chosen
        .lines
        .iter()
        .flat_map(|line| line.value_ids.iter().copied())
        .collect()
}

/// The variants that exist, and what differs between them.
#[component]
fn variant_list(variants: RwSignal<Vec<VariantSummary>>, reload: Callback<()>) -> impl IntoView {
    // `None` means nothing is being edited. Only ever one row at a time, for
    // the reason the address panel gives: N drafts on screen is N chances to
    // save the wrong one.
    let editing = RwSignal::new(None::<Uuid>);

    view! {
        <Panel title=l!("variants.existing")>
            <Show
                when=move || !variants.get().is_empty()
                fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("variants.none")}</p> }
                }
            >
                <ul class="space-y-2">
                    {move || {
                        variants
                            .get()
                            .into_iter()
                            .map(|variant| {
                                let id = variant.id;
                                let code = variant.code.clone();
                                let combination = variant.combination.clone();
                                let barcode = variant.barcode.clone().unwrap_or_default();
                                let is_default = variant.is_default;
                                let is_active = variant.is_active;

                                view! {
                                    <li class="rounded-card border border-edge p-3">
                                        <div class="flex flex-wrap items-start justify-between gap-3">
                                            <div class="min-w-0 space-y-1">
                                                <div class="flex flex-wrap items-center gap-1.5">
                                                    <span class="font-mono text-xs text-content">
                                                        {code}
                                                    </span>
                                                    {is_default
                                                        .then(|| {
                                                            view! { <Badge label=l!("variants.default") /> }
                                                        })}
                                                    {(!is_active)
                                                        .then(|| {
                                                            view! {
                                                                <Badge
                                                                    label=l!("variants.retired")
                                                                    tone=Tone::Warning
                                                                />
                                                            }
                                                        })}
                                                </div>
                                                {combination
                                                    .map(|combination| {
                                                        view! {
                                                            <p class="text-sm text-content">{combination}</p>
                                                        }
                                                    })}
                                            </div>
                                            <GhostButton
                                                label=l!("common.edit")
                                                icon=Icon::Pencil
                                                on_click=Callback::new(move |()| {
                                                    editing
                                                        .update(|open| {
                                                            *open = (*open != Some(id)).then_some(id);
                                                        });
                                                })
                                            />
                                        </div>

                                        <Show when=move || editing.get() == Some(id) fallback=|| ()>
                                            <VariantEditor
                                                variant_id=id
                                                barcode=barcode.clone()
                                                reload=reload
                                                close=Callback::new(move |()| editing.set(None))
                                            />
                                        </Show>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
            </Show>
        </Panel>
    }
}

/// The barcode and the price difference for one combination.
///
/// The three things that actually differ between a red medium and a blue large.
/// Everything else about them belongs to the item.
#[component]
fn variant_editor(
    variant_id: Uuid,
    barcode: String,
    reload: Callback<()>,
    close: Callback<()>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let barcode = RwSignal::new(barcode);
    let price_extra = RwSignal::new("0".to_owned());
    let cost_extra = RwSignal::new("0".to_owned());
    let busy = RwSignal::new(false);

    let save = Callback::new(move |()| {
        let typed = barcode.get();
        let price = price_extra.get();
        let cost = cost_extra.get();
        busy.set(true);

        leptos::task::spawn_local(async move {
            let typed = typed.trim().to_owned();
            let saved = save_variant(
                variant_id,
                (!typed.is_empty()).then_some(typed),
                price,
                cost,
            )
            .await;

            match saved {
                Ok(_) => {
                    let _ = reload.try_run(());
                    let _ = close.try_run(());
                }
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
            busy.set(false);
        });
    });

    view! {
        <div class="mt-3 grid gap-2 border-t border-edge pt-3 sm:grid-cols-3">
            <label class="space-y-1">
                <span class="text-xs text-content-muted">{l!("items.barcode")}</span>
                <input
                    type="text"
                    class="h-8 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    prop:value=move || barcode.get()
                    on:input=move |ev| barcode.set(event_target_value(&ev))
                />
            </label>
            <label class="space-y-1">
                <span class="text-xs text-content-muted">{l!("variants.price_extra")}</span>
                <input
                    type="text"
                    class="h-8 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    prop:value=move || price_extra.get()
                    on:input=move |ev| price_extra.set(event_target_value(&ev))
                />
            </label>
            <label class="space-y-1">
                <span class="text-xs text-content-muted">{l!("variants.cost_extra")}</span>
                <input
                    type="text"
                    class="h-8 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    prop:value=move || cost_extra.get()
                    on:input=move |ev| cost_extra.set(event_target_value(&ev))
                />
            </label>

            <div class="flex items-center gap-2 sm:col-span-3">
                <PrimaryButton
                    label=l!("common.save")
                    icon=Icon::Save
                    pending=busy
                    on_click=save
                />
                <GhostButton
                    label=l!("common.cancel")
                    on_click=Callback::new(move |()| close.run(()))
                />
            </div>
        </div>
    }
}
