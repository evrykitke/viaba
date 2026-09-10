//! Correcting a stock figure by hand.
//!
//! # Why this is a screen rather than a document
//!
//! An adjustment is one movement: one shelf, one item, one reason. There is no
//! draft to keep, nothing to send to anybody and nothing to approve later - so
//! a header over one line would be a document whose only field is the reason,
//! and the reason has a table of its own.
//!
//! # The reason is chosen first, and it changes what the rest of the form
//! offers
//!
//! Direction comes off the type: a reason that only takes stock off a shelf
//! does not offer to put stock on one. Refusing afterwards would be correct and
//! useless - the person has already counted, typed and pressed.
//!
//! # Nothing here decides an account
//!
//! The type carries it, the service reads it, and `stock::apply` posts it. See
//! ADR 0006 section 7.

use app_inventory::adjustment::{AdjustmentInput, AdjustmentType, Direction};
use app_inventory::location::Location;
use app_inventory::lot::LotSummary;
use app_inventory::variant::VariantChoice;
use chrono::{NaiveDate, Utc};
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::form::Submission;
use phonix_core::permissions;
use uuid::Uuid;

use crate::components::page::{Notice, PageHeader, Panel, PrimaryButton, Section, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    record_adjustment, selectable_adjustment_types, selectable_locations, pickable_variants,
    variant_lots,
};
use crate::ui::alert::{Alert, Alerts};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;
use crate::ui::viewer::Viewer;

const BACK: &str = "/inventory/stock";

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

#[component]
pub fn adjust_stock_page() -> impl IntoView {
    let types = Resource::new(|| (), |()| async move { selectable_adjustment_types().await });
    let locations = Resource::new(|| (), |()| async move { selectable_locations().await });
    let variants = Resource::new(|| (), |()| async move { pickable_variants().await });

    view! {
        <Title text=format!("{} | Phonix", l!("adjustments.title")) />

        <PageHeader
            title=l!("adjustments.title")
            subtitle=l!("adjustments.subtitle")
            icon=Icon::SlidersHorizontal
            back=(BACK, l!("stock.title"))
        />

        <div class="max-w-3xl">
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let types = types.await.unwrap_or_default();
                    let locations = locations.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();

                    // Said rather than shown empty: a workspace whose reasons
                    // have all been retired has a fixable problem, and a form
                    // with an empty picker does not look like one.
                    if types.is_empty() {
                        return view! {
                            <Notice
                                message=Signal::derive(|| Some(l!("adjustments.no_types")))
                                tone=Tone::Warning
                            />
                        }
                            .into_any();
                    }

                    view! {
                        <AdjustForm types=types locations=locations variants=variants />
                    }
                        .into_any()
                })}
            </Transition>
        </div>
    }
}

#[component]
fn adjust_form(
    types: Vec<AdjustmentType>,
    locations: Vec<Location>,
    variants: Vec<VariantChoice>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let viewer = Viewer::get();

    let types = StoredValue::new(types);
    let draft = RwSignal::new(AdjustmentInput::blank(today()));
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let type_options: Vec<Choice> = types.with_value(|types| {
        types
            .iter()
            .map(|kind| Choice::new(kind.id.to_string(), kind.name.clone()).detail(kind.code.clone()))
            .collect()
    });

    let location_options: Vec<Choice> = locations
        .iter()
        .map(|place| Choice::new(place.id.to_string(), place.name.clone()).detail(place.code.clone()))
        .collect();

    let variant_options: Vec<Choice> = variants
        .iter()
        .map(|variant| {
            Choice::new(variant.id.to_string(), variant.label()).detail(variant.code.clone())
        })
        .collect();

    // The reason as chosen, which is what decides the rest of the form.
    let chosen = Signal::derive(move || {
        let id = draft.with(|draft| draft.type_id)?;
        types.with_value(|types| types.iter().find(|kind| kind.id == id).cloned())
    });

    // Lots are asked for only where the item keeps them, and only the item's
    // own: a lot number belongs to one variant, and a picker offering every lot
    // in the workspace is a picker that can be used to move the wrong stock.
    let lots = Resource::new(
        move || draft.with(|draft| draft.variant_id),
        |variant_id| async move {
            match variant_id {
                None => Vec::new(),
                Some(id) => variant_lots(id).await.unwrap_or_default(),
            }
        },
    );

    let may_approve = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::STOCK_ADJUST_APPROVE))
        })
    });

    // Said before the button rather than after the refusal. Somebody who cannot
    // approve a write-off should learn that from the reason they picked, not
    // from a rejection after they have counted and typed.
    let blocked = Signal::derive(move || {
        chosen
            .get()
            .is_some_and(|kind| kind.needs_approval && !may_approve.get())
    });

    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();

        if saving.get_untracked() {
            return;
        }

        saving.set(true);
        rejected.set(None);

        let asked = draft.get_untracked();

        leptos::task::spawn_local(async move {
            let outcome = record_adjustment(asked).await;
            saving.set(false);

            match outcome {
                Ok(Submission::Saved(_)) => {
                    alerts.post(Alert::success(l!("adjustments.done")));
                    // Cleared rather than left filled: the next count is a
                    // different shelf far more often than it is the same one,
                    // and a form still holding the last one invites a second
                    // adjustment nobody meant to make.
                    draft.set(AdjustmentInput::blank(today()));
                }
                Ok(Submission::Rejected(errors)) => {
                    rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                }
                Err(err) => rejected.set(Some(err.to_string())),
            }
        });
    };

    view! {
        <form on:submit=submit>
            <Panel>
                <Section title=l!("adjustments.reason") description=l!("adjustments.reason.help")>
                    <div class="grid gap-3 sm:grid-cols-2">
                        <div class="block space-y-1">
                            <label
                                for="adjust-type"
                                class="block text-xs font-medium text-content-muted"
                            >
                                {l!("adjustments.type")}
                            </label>
                            <SelectField
                                id="adjust-type"
                                value=Signal::derive(move || {
                                    draft.with(|d| d.type_id.map(|id| id.to_string()).unwrap_or_default())
                                })
                                on_change=Callback::new(move |raw: String| {
                                    let id = raw.parse::<Uuid>().ok();
                                    draft.update(|d| d.type_id = id);
                                })
                                options=type_options
                                placeholder=l!("common.not_set")
                                label=l!("adjustments.type")
                            />
                        </div>

                        <DirectionField draft=draft chosen=chosen />
                    </div>

                    <Show when=move || blocked.get() fallback=|| ()>
                        <p class="mt-2 text-xs text-warning">
                            {l!("adjustments.needs_approval.note")}
                        </p>
                    </Show>
                </Section>

                <Section title=l!("adjustments.what") description=l!("adjustments.what.help")>
                    <div class="grid gap-3 sm:grid-cols-2">
                        <div class="block space-y-1">
                            <label
                                for="adjust-item"
                                class="block text-xs font-medium text-content-muted"
                            >
                                {l!("adjustments.item")}
                            </label>
                            <SelectField
                                id="adjust-item"
                                value=Signal::derive(move || {
                                    draft
                                        .with(|d| {
                                            d.variant_id.map(|id| id.to_string()).unwrap_or_default()
                                        })
                                })
                                on_change=Callback::new(move |raw: String| {
                                    let id = raw.parse::<Uuid>().ok();
                                    draft
                                        .update(|d| {
                                            d.variant_id = id;
                                            // A lot belongs to one item. Keeping
                                            // the old one would leave the form
                                            // holding a lot of something else.
                                            d.lot_id = None;
                                        });
                                })
                                options=variant_options
                                placeholder=l!("common.not_set")
                                label=l!("adjustments.item")
                            />
                        </div>

                        <div class="block space-y-1">
                            <label
                                for="adjust-location"
                                class="block text-xs font-medium text-content-muted"
                            >
                                {l!("adjustments.location")}
                            </label>
                            <SelectField
                                id="adjust-location"
                                value=Signal::derive(move || {
                                    draft
                                        .with(|d| {
                                            d.location_id.map(|id| id.to_string()).unwrap_or_default()
                                        })
                                })
                                on_change=Callback::new(move |raw: String| {
                                    let id = raw.parse::<Uuid>().ok();
                                    draft.update(|d| d.location_id = id);
                                })
                                options=location_options
                                placeholder=l!("common.not_set")
                                label=l!("adjustments.location")
                            />
                        </div>

                        <LotField draft=draft lots=lots />

                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("adjustments.quantity")}
                            </span>
                            <input
                                type="text"
                                inputmode="decimal"
                                class="w-full"
                                prop:value=move || draft.with(|d| d.quantity.clone())
                                on:input=move |ev| {
                                    let typed = event_target_value(&ev);
                                    draft.update(|d| d.quantity = typed);
                                }
                            />
                        </label>

                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("adjustments.on")}
                            </span>
                            <input
                                type="date"
                                class="w-full"
                                prop:value=move || draft.with(|d| d.moved_on.to_string())
                                on:change=move |ev| {
                                    let typed = event_target_value(&ev);
                                    if let Ok(on) = typed.parse::<NaiveDate>() {
                                        draft.update(|d| d.moved_on = on);
                                    }
                                }
                            />
                            <span class="block text-2xs text-content-subtle">
                                {l!("adjustments.on.help")}
                            </span>
                        </label>

                        <label class="block space-y-1 sm:col-span-2">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("adjustments.note")}
                            </span>
                            <input
                                type="text"
                                class="w-full"
                                prop:value=move || draft.with(|d| d.reason.clone())
                                on:input=move |ev| {
                                    let typed = event_target_value(&ev);
                                    draft.update(|d| d.reason = typed);
                                }
                            />
                            <span class="block text-2xs text-content-subtle">
                                {l!("adjustments.note.help")}
                            </span>
                        </label>
                    </div>
                </Section>

                <Section>
                    <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

                    <div class="flex flex-wrap items-center justify-end gap-2">
                        <PrimaryButton
                            label=l!("adjustments.record")
                            icon=Icon::Save
                            button_type="submit"
                            pending=Signal::derive(move || saving.get())
                            disabled=Signal::derive(move || blocked.get())
                        />
                    </div>
                </Section>
            </Panel>
        </form>
    }
}

/// Which way the stock moves, offering only the ways the reason allows.
#[component]
fn direction_field(
    draft: RwSignal<AdjustmentInput>,
    chosen: Signal<Option<AdjustmentType>>,
) -> impl IntoView {
    // A one-way reason has nothing to choose, so it states what it does instead
    // of offering a select with one entry in it.
    let fixed = Signal::derive(move || {
        chosen
            .get()
            .map(|kind| kind.direction)
            .filter(|direction| *direction != Direction::Both)
    });

    // The form starts on "out", which is what nearly every adjustment is; a
    // reason that only goes the other way corrects it the moment it is picked.
    //
    // Guarded rather than written every time: `update` notifies whether or not
    // the value changed, and this effect reads the draft through `fixed` - so
    // an unconditional write is not a wasted call but a loop that starves the
    // event loop and takes the tab with it.
    Effect::new(move |_| {
        let Some(direction) = fixed.get() else {
            return;
        };

        let found = direction == Direction::In;

        if draft.with_untracked(|d| d.found) != found {
            draft.update(|d| d.found = found);
        }
    });

    view! {
        <div class="block space-y-1">
            <label for="adjust-way" class="block text-xs font-medium text-content-muted">
                {l!("adjustments.way")}
            </label>

            <Show
                when=move || fixed.get().is_none()
                fallback=move || {
                    view! {
                        <p class="flex h-8 items-center text-sm text-content">
                            {move || {
                                fixed
                                    .get()
                                    .map(|direction| crate::i18n::t(&direction.label()))
                                    .unwrap_or_default()
                            }}
                        </p>
                    }
                }
            >
                <select
                    id="adjust-way"
                    class="h-8 w-full rounded-control border border-edge bg-surface px-2 text-sm text-content"
                    prop:value=move || {
                        if draft.with(|d| d.found) { "in" } else { "out" }
                    }
                    on:change=move |ev| {
                        let found = event_target_value(&ev) == "in";
                        draft.update(|d| d.found = found);
                    }
                >
                    <option value="out">{l!("adjustments.way.out")}</option>
                    <option value="in">{l!("adjustments.way.in")}</option>
                </select>
            </Show>
        </div>
    }
}

/// The lot, where the item keeps them.
#[component]
fn lot_field(
    draft: RwSignal<AdjustmentInput>,
    lots: Resource<Vec<LotSummary>>,
) -> impl IntoView {
    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                let lots = lots.await;

                // Absent, not empty: an item that keeps no lots has no question
                // here, and a disabled picker would be one more thing on the
                // screen to read past.
                if lots.is_empty() {
                    return ().into_any();
                }

                let options: Vec<Choice> = lots
                    .iter()
                    .map(|lot| {
                        Choice::new(lot.id.to_string(), lot.number.clone())
                            .detail(lot.on_hand.to_display_string())
                    })
                    .collect();

                view! {
                    <div class="block space-y-1">
                        <label
                            for="adjust-lot"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("adjustments.lot")}
                        </label>
                        <SelectField
                            id="adjust-lot"
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| d.lot_id.map(|id| id.to_string()).unwrap_or_default())
                            })
                            on_change=Callback::new(move |raw: String| {
                                let id = raw.parse::<Uuid>().ok();
                                draft.update(|d| d.lot_id = id);
                            })
                            options=options
                            placeholder=l!("common.not_set")
                            clearable=true
                            label=l!("adjustments.lot")
                        />
                    </div>
                }
                    .into_any()
            })}
        </Transition>
    }
}
