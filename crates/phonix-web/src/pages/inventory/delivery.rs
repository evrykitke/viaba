//! One delivery: the editor while it is a draft, evidence once it has gone.
//!
//! The mirror of [`super::receipt`], with two differences that are about
//! despatching rather than about screens.
//!
//! # The batch is chosen, not typed
//!
//! A receipt types a lot number, because the number is the supplier's and is
//! new to this workspace. A delivery picks one, because it can only send stock
//! it already holds - so the field is a list of the batches on hand, newest
//! expiry last, with what is on each of them beside it.
//!
//! # There is no cost on the screen until it has gone
//!
//! A receipt knows what a line costs while it is being keyed: the supplier said
//! so. A delivery does not - under average or FIFO the cost of the units
//! leaving is decided by the layers, and only the move knows which it consumed.
//! So the draft shows no figure and the document shows the one the posting
//! worked out, which is the honest order of events rather than a preview that
//! would have to be corrected.

use app_inventory::delivery::{Delivery, DeliveryInput, DeliveryLineInput, DeliveryState};
use app_inventory::lot::LotSummary;
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_master::party::{PartySummary, roles};
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::pages::inventory::item_lookup::ItemLookup;
use crate::server_fns::inventory_fns::{
    blank_delivery, delete_delivery, delivery_against_order, delivery_detail, post_delivery,
    save_delivery, selectable_warehouses, variant_lots,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const BACK: &str = "/inventory/deliveries";

/// Booking goods out.
///
/// `?order=<id>` opens it against a sales order, prefilled with everything that
/// order still owes - the screen somebody wants when the van is at the door.
/// Without it the delivery stands alone, which is ordinary: a sample and a
/// replacement go out against no order at all.
#[component]
pub fn delivery_new_page() -> impl IntoView {
    let query = leptos_router::hooks::use_query_map();
    let against =
        move || query.with(|query| query.get("order").and_then(|raw| raw.parse::<Uuid>().ok()));

    let prefilled = Resource::new(against, move |order_id| async move {
        match order_id {
            None => match blank_delivery().await {
                Ok(draft) => Ok(draft),
                Err(err) => Err(err.to_string()),
            },
            Some(id) => match delivery_against_order(id).await {
                Ok(Submission::Saved(draft)) => Ok(draft),
                Ok(Submission::Rejected(errors)) => Err(errors
                    .first()
                    .map(|error| crate::i18n::t(&error.message))
                    .unwrap_or_else(|| l!("deliveries.gone"))),
                Err(err) => Err(err.to_string()),
            },
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("deliveries.new")) />

        <PageHeader
            title=l!("deliveries.new")
            subtitle=l!("deliveries.new.subtitle")
            icon=Icon::Truck
            back=(BACK, l!("deliveries.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match prefilled.await {
                    Ok(draft) => view! { <DeliveryEditor draft=draft /> }.into_any(),
                    Err(message) => {
                        view! {
                            <Notice
                                message=Signal::derive(move || Some(message.clone()))
                                tone=Tone::Danger
                            />
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
    }
}

/// One delivery: editable while it is a draft, evidence after.
#[component]
pub fn delivery_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let delivery_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let delivery = Resource::new(delivery_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => delivery_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a delivery id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.delivery.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match delivery.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let customer = stored.customer.name.clone();
                        let state = stored.state;
                        let opened_on = DeliveryInput::from_delivery(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=customer
                                    icon=Icon::Truck
                                    back=(BACK, l!("deliveries.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! { <DeliveryEditor draft=opened_on /> }.into_any()
                                } else {
                                    view! { <DeliveryDocument delivery=stored /> }.into_any()
                                }}
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.delivery.singular")
                                    icon=Icon::Truck
                                    back=(BACK, l!("deliveries.title"))
                                />
                                <Notice
                                    message=Signal::derive(move || Some(err.to_string()))
                                    tone=Tone::Danger
                                />
                            </>
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
    }
}

#[component]
fn state_badge(state: DeliveryState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        DeliveryState::Draft => Tone::Neutral,
        DeliveryState::Done => Tone::Success,
        DeliveryState::Cancelled => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

#[component]
fn delivery_editor(draft: DeliveryInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let customers = Resource::new(
        || (),
        |()| async move { list_parties(Some(roles::CUSTOMER.to_owned())).await },
    );
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let customers = customers.await.unwrap_or_default();
                    let warehouses = warehouses.await.unwrap_or_default();

                    let warehouse_options = warehouses
                        .into_iter()
                        .filter(|warehouse| warehouse.is_active)
                        .map(|warehouse| {
                            Choice::new(warehouse.id.to_string(), warehouse.name)
                                .detail(warehouse.code)
                        })
                        .collect::<Vec<_>>();

                    view! {
                        <EditorBody
                            draft=draft
                            customers=customers
                            warehouses=warehouse_options
                            saving=saving
                            rejected=rejected
                        />
                    }
                })}
            </Transition>
        </div>
    }
}

#[component]
fn editor_body(
    draft: RwSignal<DeliveryInput>,
    customers: Vec<PartySummary>,
    warehouses: Vec<Choice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_delivery(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("deliveries.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/deliveries/{id}"),
                                leptos_router::NavigateOptions {
                                    replace: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                    Ok(Submission::Rejected(errors)) => {
                        rejected.set(errors.first().map(|error| crate::i18n::t(&error.message)));
                    }
                    Err(err) => rejected.set(Some(err.to_string())),
                }
            });
        }
    };

    let despatch = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("deliveries.post.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = post_delivery(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(delivery)) => {
                                alerts.post(
                                    Alert::success(l!(
                                        "deliveries.posted",
                                        number = delivery.number
                                    ))
                                    .titled(l!("deliveries.post")),
                                );
                                // Reload the route: it is evidence now, and
                                // this screen draws a different thing for one.
                                navigate(
                                    &format!("/inventory/deliveries/{id}"),
                                    leptos_router::NavigateOptions {
                                        replace: true,
                                        ..Default::default()
                                    },
                                );
                            }
                            Ok(Submission::Rejected(errors)) => {
                                if let Some(error) = errors.first() {
                                    alerts.post(Alert::warning(crate::i18n::t(&error.message)));
                                }
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(l!("deliveries.post"))
                .confirm_label(l!("deliveries.post")),
            );
        }
    };

    let delete = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("deliveries.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_delivery(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("deliveries.deleted")));
                                navigate(BACK, leptos_router::NavigateOptions::default());
                            }
                            Err(err) => alerts.post(Alert::failure(err.to_string())),
                        }
                    });
                })
                .titled(l!("common.delete"))
                .confirm_label(l!("common.delete")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());

    view! {
        <Panel>
            <Section title=l!("deliveries.header")>
                <HeaderFields draft=draft customers=customers warehouses=warehouses />
            </Section>

            <Section title=l!("deliveries.lines") description=l!("deliveries.lines.help")>
                <LineTable draft=draft />
            </Section>

            <Section title=l!("deliveries.note") flush=true>
                <textarea
                    class="w-full"
                    rows="3"
                    prop:value=move || draft.with(|d| d.note.clone())
                    on:input=move |ev| {
                        let text = event_target_value(&ev);
                        draft.update(|d| d.note = text);
                    }
                />
            </Section>

            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("common.delete")
                        icon=Icon::Trash2
                        on_click=Callback::new({
                            let delete = delete.clone();
                            move |()| delete()
                        })
                    />
                </Show>

                <GhostButton
                    label=l!("common.save")
                    icon=Icon::Save
                    disabled=Signal::derive(move || saving.get())
                    on_click=Callback::new({
                        let save = save.clone();
                        move |()| save()
                    })
                />

                // Offered only once there is something saved to despatch.
                <Show when=saved fallback=|| ()>
                    <PrimaryButton
                        label=l!("deliveries.post")
                        icon=Icon::Truck
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let despatch = despatch.clone();
                            move |()| despatch()
                        })
                    />
                </Show>
            </div>
        </Panel>
    }
}

/// Who it is for, where it leaves from, and when.
#[component]
fn header_fields(
    draft: RwSignal<DeliveryInput>,
    customers: Vec<PartySummary>,
    warehouses: Vec<Choice>,
) -> impl IntoView {
    let customer_options = customers
        .iter()
        .filter(|party| party.is_active)
        .map(|party| {
            Choice::new(party.id.to_string(), party.name.clone()).detail(party.code.clone())
        })
        .collect::<Vec<_>>();

    view! {
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="block space-y-1">
                <label
                    for="delivery-customer"
                    class="block text-xs font-medium text-content-muted"
                >
                    {l!("deliveries.customer")}
                </label>
                <SelectField
                    id="delivery-customer"
                    value=Signal::derive(move || {
                        draft.with(|d| d.customer_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft.update(|d| d.customer_id = chosen);
                    })
                    options=customer_options
                    placeholder=l!("common.not_set")
                    clearable=true
                />
            </div>

            <div class="block space-y-1">
                <label
                    for="delivery-warehouse"
                    class="block text-xs font-medium text-content-muted"
                >
                    {l!("nav.warehouses")}
                </label>
                <SelectField
                    id="delivery-warehouse"
                    value=Signal::derive(move || {
                        draft.with(|d| d.warehouse_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft.update(|d| d.warehouse_id = chosen);
                    })
                    options=warehouses
                    placeholder=l!("common.not_set")
                    clearable=true
                />
            </div>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("deliveries.despatched")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.despatched_on.to_string())
                    on:change=move |ev| {
                        if let Ok(date) = event_target_value(&ev).parse() {
                            draft.update(|d| d.despatched_on = date);
                        }
                    }
                />
            </label>

            <label class="block space-y-1 sm:col-span-2">
                <span class="text-xs font-medium text-content-muted">
                    {l!("deliveries.carrier")}
                </span>
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.carrier_reference.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.carrier_reference = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("deliveries.carrier.help")}
                </span>
            </label>
        </div>
    }
}

#[component]
fn line_table(draft: RwSignal<DeliveryInput>) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[48rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("deliveries.item")}</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">{l!("field.quantity")}</th>
                            <th class="w-48 py-2 font-medium">{l!("deliveries.lot")}</th>
                            <th class="w-8 py-2"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let count = draft.with(|d| d.lines.len());
                            (0..count)
                                .map(|index| view! { <LineRow draft=draft index=index /> })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("deliveries.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(DeliveryLineInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn line_row(draft: RwSignal<DeliveryInput>, index: usize) -> impl IntoView {
    let field = move |read: fn(&DeliveryLineInput) -> String| {
        draft.with(|d| d.lines.get(index).map(read).unwrap_or_default())
    };

    let variant =
        Signal::derive(move || draft.with(|d| d.lines.get(index).and_then(|line| line.variant_id)));

    let initial = draft.with_untracked(|d| {
        let line = d.lines.get(index)?;
        let id = line.variant_id?;
        let label = match line.description.trim() {
            "" => id.to_string(),
            words => words.to_owned(),
        };

        Some(Choice::new(id.to_string(), label))
    });

    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1 text-xs text-content-subtle">{index + 1}</td>
            <td class="py-1 pr-2">
                <ItemLookup
                    initial=initial
                    sellable=true
                    on_pick=Callback::new(move |picked: Option<VariantChoice>| {
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.variant_id = picked.as_ref().map(|variant| variant.id);
                                    // A different item is a different set of
                                    // batches, so the one chosen for the last
                                    // one is cleared rather than left pointing
                                    // at stock of something else.
                                    line.lot_id = None;
                                    if let Some(picked) = &picked
                                        && line.description.trim().is_empty()
                                    {
                                        line.description = picked.label();
                                    }
                                }
                            });
                    })
                    placeholder=Some(l!("common.not_set"))
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || field(|line| line.description.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.description = value;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=move || field(|line| line.quantity.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.quantity = value;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <LotPicker draft=draft index=index variant=variant />
            </td>
            <td class="py-1 text-right">
                <button
                    type="button"
                    class="rounded-control p-1 text-content-subtle hover:bg-surface-hover hover:text-danger"
                    aria-label=l!("common.remove")
                    on:click=move |_| {
                        draft
                            .update(|d| {
                                if index < d.lines.len() {
                                    d.lines.remove(index);
                                }
                                if d.lines.is_empty() {
                                    d.lines.push(DeliveryLineInput::blank());
                                }
                            });
                    }
                >
                    <Icon icon=Icon::Trash2 size=crate::icons::IconSize::Xs />
                </button>
            </td>
        </tr>
    }
}

/// Which batch is going, out of the ones this workspace holds.
///
/// Nothing is drawn for an item that keeps no batches, which is most of them: a
/// field offered where there is nothing to choose is a field somebody stops
/// reading. The list is what the pick would reach for anyway, in the same
/// order, with what is on each batch beside it - because "which one" is a
/// decision somebody makes by looking at the quantities.
#[component]
fn lot_picker(
    draft: RwSignal<DeliveryInput>,
    index: usize,
    variant: Signal<Option<Uuid>>,
) -> impl IntoView {
    let lots = Resource::new(
        move || variant.get(),
        |variant_id| async move {
            match variant_id {
                Some(id) => variant_lots(id).await.unwrap_or_default(),
                None => Vec::new(),
            }
        },
    );

    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                let lots: Vec<LotSummary> = lots.await;

                if lots.is_empty() {
                    return ().into_any();
                }

                let options = lots
                    .into_iter()
                    .map(|lot| {
                        let detail = lot.on_hand.to_display_string();
                        let label = match lot.expires_on {
                            Some(on) => format!("{} \u{b7} {on}", lot.number),
                            None => lot.number.clone(),
                        };

                        Choice::new(lot.id.to_string(), label).detail(detail)
                    })
                    .collect::<Vec<_>>();

                view! {
                    <SelectField
                        value=Signal::derive(move || {
                            draft
                                .with(|d| {
                                    d.lines
                                        .get(index)
                                        .and_then(|line| line.lot_id)
                                        .map(|id| id.to_string())
                                        .unwrap_or_default()
                                })
                        })
                        on_change=Callback::new(move |value: String| {
                            let chosen = value.parse::<Uuid>().ok();
                            draft
                                .update(|d| {
                                    if let Some(line) = d.lines.get_mut(index) {
                                        line.lot_id = chosen;
                                    }
                                });
                        })
                        options=options
                        placeholder=l!("common.not_set")
                        clearable=true
                        label=l!("deliveries.lot")
                    />
                }
                    .into_any()
            })}
        </Transition>
    }
}

// --- the document -------------------------------------------------------

/// A despatched or cancelled delivery: read-only, and every figure on it is
/// what the posting worked out rather than what could be looked up now.
#[component]
fn delivery_document(delivery: Delivery) -> impl IntoView {
    let id = delivery.id;
    let navigate = leptos_router::hooks::use_navigate();

    let customer_name = delivery.customer.name.clone();
    let customer_code = delivery.customer.code.clone();
    let order_number = delivery.order_number.clone();
    let order_id = delivery.order_id;
    let warehouse = delivery.warehouse_name.clone();
    let from = delivery.from_location_path.clone();
    let despatched = delivery.despatched_on.to_string();
    let carrier = delivery.carrier_reference.clone();
    let note = delivery.note.clone();
    let value = delivery.value.to_display_string();
    let lines = delivery.lines.clone();

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("deliveries.customer") flush=true>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{customer_name}</div>
                        <code class="text-2xs text-content-subtle">{customer_code}</code>
                        {carrier
                            .map(|carrier| {
                                view! {
                                    <div class="text-xs text-content-muted">
                                        {l!("deliveries.carrier")} ": " {carrier}
                                    </div>
                                }
                            })}
                    </div>
                </Section>

                <Section title=l!("deliveries.header") flush=true>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("nav.warehouses")}</dt>
                            <dd class="text-content">{warehouse}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("deliveries.from")}</dt>
                            <dd class="font-mono text-xs text-content-muted">{from}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("deliveries.despatched")}</dt>
                            <dd class="tabular-nums text-content">{despatched}</dd>
                        </div>
                        {order_id
                            .zip(order_number)
                            .map(|(order_id, number)| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("deliveries.order")}
                                        </dt>
                                        <dd>
                                            <a
                                                class="font-mono text-xs text-brand hover:underline"
                                                href=format!("/inventory/sales-orders/{order_id}")
                                            >
                                                {number}
                                            </a>
                                        </dd>
                                    </div>
                                }
                            })}
                    </dl>
                </Section>
            </div>

            <Section title=l!("deliveries.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[44rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 font-medium">{l!("deliveries.lot")}</th>
                                <th class="py-2 text-right font-medium">{l!("field.quantity")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("deliveries.value")}
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            {lines
                                .into_iter()
                                .map(|line| {
                                    let quantity = format!(
                                        "{} {}",
                                        line.quantity.to_display_string(),
                                        line.unit_code,
                                    );
                                    let value = line.value.to_display_string();
                                    let lot = line.lot_number.clone().unwrap_or_default();

                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-subtle">
                                                {line.line_no}
                                            </td>
                                            <td class="py-1.5 text-content">
                                                {line.description.clone()}
                                                <div class="text-2xs text-content-subtle">
                                                    {line.variant_code.clone()}
                                                </div>
                                            </td>
                                            <td class="py-1.5 font-mono text-xs text-content-muted">
                                                {lot}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {quantity}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {value}
                                            </td>
                                        </tr>
                                    }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </Section>

            <div class="grid gap-3 lg:grid-cols-[1fr_22rem] lg:items-start">
                {note
                    .map(|note| {
                        view! {
                            <Section title=l!("deliveries.note")>
                                <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                            </Section>
                        }
                    })}

                <Section title=l!("deliveries.value") description=l!("deliveries.value.help") flush=true>
                    <div class="flex items-baseline justify-between gap-4 text-sm font-medium tabular-nums">
                        <span class="text-content">{l!("deliveries.value")}</span>
                        <span class="text-content">{value}</span>
                    </div>
                </Section>
            </div>

            // The invoice screen opens against this despatch and prefills what
            // has not been charged for; the lines do not need carrying across.
            // Offered whatever is left on it - the screen says when that is
            // nothing, which is a better answer than a button that is not there.
            <div class="mt-4 flex justify-end border-t border-edge pt-4">
                <GhostButton
                    label=l!("deliveries.invoice")
                    icon=Icon::FileText
                    on_click=Callback::new(move |()| {
                        navigate(
                            &format!("/sales/invoices/new?delivery={id}"),
                            leptos_router::NavigateOptions::default(),
                        );
                    })
                />
            </div>

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::DELIVERY id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}
