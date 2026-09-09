//! Stock transfers: the list, the editor, the document, and the arrival.
//!
//! # A transfer in transit is not a document you read, it is one you act on
//!
//! So the arrival form is the first thing on the page while anything is still
//! on the road, pre-filled with what left and has not turned up. The common
//! case is that the whole load arrived, and that is one button.
//!
//! # The two acts are two buttons, and they are not on the same screen at once
//!
//! Despatch belongs to the origin and arrival to the destination, days apart
//! and usually two different people. The state decides which one is drawn, and
//! the permissions decide whether the person looking may press it.

use app_inventory::transfer::{
    ArrivalInput, Transfer, TransferInput, TransferLineInput, TransferState,
};
use app_inventory::location::Location;
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    blank_transfer, cancel_transfer, delete_transfer, despatch_transfer, edit_transfer,
    pickable_variants, receive_transfer, save_transfer, selectable_locations, transfer_arrival,
    transfer_detail,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;
use crate::ui::table::DataGrid;
use crate::ui::table::config::transfers::transfers_grid;

const BACK: &str = "/inventory/transfers";

#[component]
pub fn transfers_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("transfers.title")) />

        <PageHeader
            title=l!("transfers.title")
            subtitle=l!("transfers.subtitle")
            icon=Icon::Truck
        />

        <DataGrid config=transfers_grid() />
    }
}

#[component]
pub fn transfer_new_page() -> impl IntoView {
    let prefilled = Resource::new(|| (), |()| async move { blank_transfer().await });

    view! {
        <Title text=format!("{} | Phonix", l!("transfers.new")) />

        <PageHeader
            title=l!("transfers.new")
            subtitle=l!("transfers.new.subtitle")
            icon=Icon::Truck
            back=(BACK, l!("transfers.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match prefilled.await {
                    Ok(draft) => view! { <TransferEditor draft=draft /> }.into_any(),
                    Err(err) => {
                        view! {
                            <Notice
                                message=Signal::derive(move || Some(err.to_string()))
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

#[component]
pub fn transfer_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let transfer_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let document = Resource::new(transfer_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => transfer_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a transfer id.")),
        }
    });

    let draft = Resource::new(transfer_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => edit_transfer(id).await.ok(),
            Err(_) => None,
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.stock_transfer.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let opened_on = draft.await;

                match document.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let subtitle = format!("{} → {}", stored.from_path, stored.to_path);
                        let state = stored.state;

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=subtitle
                                    icon=Icon::Truck
                                    back=(BACK, l!("transfers.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {match (state.is_editable(), opened_on) {
                                    (true, Some(draft)) => {
                                        view! { <TransferEditor draft=draft /> }.into_any()
                                    }
                                    _ => {
                                        view! { <TransferDocument document=stored /> }.into_any()
                                    }
                                }}
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.stock_transfer.singular")
                                    icon=Icon::Truck
                                    back=(BACK, l!("transfers.title"))
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
fn state_badge(state: TransferState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        TransferState::Draft => Tone::Neutral,
        TransferState::InTransit => Tone::Warning,
        TransferState::Done => Tone::Success,
        TransferState::Cancelled => Tone::Neutral,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

#[component]
fn transfer_editor(draft: TransferInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let locations = Resource::new(|| (), |()| async move { selectable_locations().await });
    let variants = Resource::new(|| (), |()| async move { pickable_variants().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let locations = locations.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();

                    view! {
                        <EditorBody
                            draft=draft
                            locations=locations
                            variants=variants
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
    draft: RwSignal<TransferInput>,
    locations: Vec<Location>,
    variants: Vec<VariantChoice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let variants = StoredValue::new(variants);

    // A grouping holds nothing of its own, so it is not offered as an end.
    // Refusing it at the gate would be finding out too late.
    let location_options = locations
        .iter()
        .filter(|location| location.is_active && location.kind.can_hold_stock())
        .map(|location| {
            Choice::new(location.id.to_string(), location.code.clone())
                .detail(location.name.clone())
        })
        .collect::<Vec<_>>();
    let destination_options = location_options.clone();

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_transfer(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("transfers.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/transfers/{id}"),
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
                Confirm::new(l!("transfers.despatch.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = despatch_transfer(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(document)) => {
                                alerts.post(
                                    Alert::success(
                                            l!("transfers.despatched.done", number = document.number),
                                        )
                                        .titled(l!("transfers.despatch")),
                                );
                                navigate(
                                    &format!("/inventory/transfers/{id}"),
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
                .titled(l!("transfers.despatch"))
                .confirm_label(l!("transfers.despatch")),
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
                Confirm::new(l!("transfers.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_transfer(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("transfers.deleted")));
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

    let cancel = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("transfers.cancel.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match cancel_transfer(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("transfers.cancelled")));
                                navigate(BACK, leptos_router::NavigateOptions::default());
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
                .titled(l!("transfers.cancel"))
                .confirm_label(l!("transfers.cancel")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());

    view! {
        <div class="space-y-3">
            <Panel title=l!("transfers.header") description=l!("transfers.header.help")>
                <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                    <div class="block space-y-1">
                        <label for="transfer-from" class="block text-xs font-medium text-content-muted">
                            {l!("transfers.from")}
                        </label>
                        <SelectField
                            id="transfer-from"
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.from_location_id
                                            .map(|id| id.to_string())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |value: String| {
                                let chosen = value.parse::<Uuid>().ok();
                                draft.update(|d| d.from_location_id = chosen);
                            })
                            options=location_options
                            placeholder=l!("common.not_set")
                            clearable=true
                        />
                    </div>

                    <div class="block space-y-1">
                        <label for="transfer-to" class="block text-xs font-medium text-content-muted">
                            {l!("transfers.to")}
                        </label>
                        <SelectField
                            id="transfer-to"
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.to_location_id
                                            .map(|id| id.to_string())
                                            .unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |value: String| {
                                let chosen = value.parse::<Uuid>().ok();
                                draft.update(|d| d.to_location_id = chosen);
                            })
                            options=destination_options
                            placeholder=l!("common.not_set")
                            clearable=true
                        />
                    </div>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("transfers.planned")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.planned_on.to_string())
                            on:change=move |ev| {
                                if let Ok(date) = event_target_value(&ev).parse() {
                                    draft.update(|d| d.planned_on = date);
                                }
                            }
                        />
                    </label>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("transfers.reference")}
                        </span>
                        <input
                            type="text"
                            class="w-full font-mono text-xs"
                            prop:value=move || draft.with(|d| d.reference.clone())
                            on:input=move |ev| {
                                let value = event_target_value(&ev);
                                draft.update(|d| d.reference = value);
                            }
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("transfers.reference.help")}
                        </span>
                    </label>
                </div>
            </Panel>

            <Panel title=l!("transfers.lines")>
                <LineTable draft=draft variants=variants />
            </Panel>

            <Panel title=l!("transfers.note")>
                <textarea
                    class="w-full"
                    rows="3"
                    prop:value=move || draft.with(|d| d.note.clone())
                    on:input=move |ev| {
                        let text = event_target_value(&ev);
                        draft.update(|d| d.note = text);
                    }
                />
            </Panel>

            <div class="flex flex-wrap items-center justify-end gap-2">
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

                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("transfers.cancel")
                        icon=Icon::Ban
                        on_click=Callback::new({
                            let cancel = cancel.clone();
                            move |()| cancel()
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

                <Show when=saved fallback=|| ()>
                    <PrimaryButton
                        label=l!("transfers.despatch")
                        icon=Icon::Truck
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let despatch = despatch.clone();
                            move |()| despatch()
                        })
                    />
                </Show>
            </div>
        </div>
    }
}

#[component]
fn line_table(
    draft: RwSignal<TransferInput>,
    variants: StoredValue<Vec<VariantChoice>>,
) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[40rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("purchase_orders.item")}</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-28 py-2 text-right font-medium">
                                {l!("field.quantity")}
                            </th>
                            <th class="w-8 py-2"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let count = draft.with(|d| d.lines.len());
                            (0..count)
                                .map(|index| {
                                    view! { <LineRow draft=draft index=index variants=variants /> }
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("transfers.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(TransferLineInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<TransferInput>,
    index: usize,
    variants: StoredValue<Vec<VariantChoice>>,
) -> impl IntoView {
    let field = move |read: fn(&TransferLineInput) -> String| {
        draft.with(|d| d.lines.get(index).map(read).unwrap_or_default())
    };

    let variant_options = variants.with_value(|variants| {
        variants
            .iter()
            .map(|variant| {
                Choice::new(variant.id.to_string(), variant.label()).detail(variant.code.clone())
            })
            .collect::<Vec<_>>()
    });

    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1 text-xs text-content-subtle">{index + 1}</td>
            <td class="py-1 pr-2">
                <SelectField
                    value=Signal::derive(move || {
                        field(|line| line.variant_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        let picked = chosen
                            .and_then(|id| {
                                variants
                                    .with_value(|variants| {
                                        variants.iter().find(|variant| variant.id == id).cloned()
                                    })
                            });
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.variant_id = chosen;
                                    if let Some(picked) = picked {
                                        if line.description.trim().is_empty() {
                                            line.description = picked.label();
                                        }
                                    }
                                }
                            });
                    })
                    options=variant_options
                    placeholder=l!("common.not_set")
                    clearable=true
                    label=l!("purchase_orders.item")
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
                                    d.lines.push(TransferLineInput::blank());
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

// --- the document -------------------------------------------------------

#[component]
fn transfer_document(document: Transfer) -> impl IntoView {
    let id = document.id;
    let from_path = document.from_path.clone();
    let to_path = document.to_path.clone();
    let planned = document.planned_on.to_string();
    let despatched = document.despatched_on.map(|on| on.to_string());
    let arrived = document.arrived_on.map(|on| on.to_string());
    let reference = document.reference.clone();
    let note = document.note.clone();
    let lines = document.lines.clone();
    let carrying = document.is_carrying();
    let on_the_road = document.in_transit().to_display_string();
    let in_transit_state = matches!(document.state, TransferState::InTransit);

    view! {
        <div class="space-y-3">
            // While anything is still on the road this is the thing to act on,
            // so it comes before the document rather than after it.
            {(in_transit_state && carrying)
                .then(|| view! { <ArrivalPanel transfer_id=id /> })}

            <div class="grid gap-3 lg:grid-cols-2">
                <Panel title=l!("transfers.header")>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("transfers.from")}</dt>
                            <dd class="font-mono text-xs text-content">{from_path}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("transfers.to")}</dt>
                            <dd class="font-mono text-xs text-content">{to_path}</dd>
                        </div>
                        {reference
                            .map(|reference| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("transfers.reference")}
                                        </dt>
                                        <dd class="font-mono text-xs text-content">{reference}</dd>
                                    </div>
                                }
                            })}
                    </dl>
                </Panel>

                <Panel title=l!("transfers.journey") description=l!("transfers.journey.help")>
                    <dl class="space-y-1 text-sm tabular-nums">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("transfers.planned")}</dt>
                            <dd class="text-content">{planned}</dd>
                        </div>
                        {despatched
                            .map(|on| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("transfers.despatched")}
                                        </dt>
                                        <dd class="text-content">{on}</dd>
                                    </div>
                                }
                            })}
                        {arrived
                            .map(|on| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("transfers.arrived")}
                                        </dt>
                                        <dd class="text-content">{on}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex justify-between gap-4 border-t border-edge pt-1 font-medium">
                            <dt class="text-content">{l!("transfers.on_the_road")}</dt>
                            <dd class=if carrying { "text-warning" } else { "text-content" }>
                                {on_the_road}
                            </dd>
                        </div>
                    </dl>
                </Panel>
            </div>

            <Panel title=l!("transfers.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[40rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 text-right font-medium">{l!("field.quantity")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("transfers.sent")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("transfers.received")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("transfers.on_the_road")}
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            {lines
                                .into_iter()
                                .map(|line| {
                                    let outstanding = line.in_transit();
                                    let tone = if outstanding.is_positive() {
                                        "py-1.5 text-right tabular-nums text-warning"
                                    } else {
                                        "py-1.5 text-right tabular-nums text-content-subtle"
                                    };

                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-subtle">
                                                {line.line_no}
                                            </td>
                                            <td class="py-1.5 text-content">
                                                {line.description.clone()}
                                                <div class="text-2xs text-content-subtle">
                                                    {line.variant_code.clone()}
                                                    {line
                                                        .lot_number
                                                        .clone()
                                                        .map(|number| format!(" · {number}"))}
                                                </div>
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {line.quantity.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {line.despatched.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {line.received.to_display_string()}
                                            </td>
                                            <td class=tone>{outstanding.to_display_string()}</td>
                                        </tr>
                                    }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </Panel>

            {note
                .map(|note| {
                    view! {
                        <Panel title=l!("transfers.note")>
                            <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                        </Panel>
                    }
                })}

            <Panel title=l!("common.history")>
                <RecordHistory kind=kinds::STOCK_TRANSFER id=Some(id.to_string()) />
            </Panel>
        </div>
    }
}

/// Booking a load in at the far end.
///
/// Pre-filled with everything still on the road, because the common case is
/// that the whole load turned up and that should be one button. A short
/// delivery is keyed by lowering a line, and what is left stays on the road
/// rather than vanishing.
#[component]
fn arrival_panel(transfer_id: Uuid) -> impl IntoView {
    let prefilled = Resource::new(
        move || transfer_id,
        |id| async move {
            match transfer_arrival(id).await {
                Ok(Submission::Saved(arrival)) => Some(arrival),
                _ => None,
            }
        },
    );

    view! {
        <Panel title=l!("transfers.arrival") description=l!("transfers.arrival.help")>
            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    match prefilled.await {
                        None => {
                            view! {
                                <p class="text-sm text-content-subtle">
                                    {l!("transfers.error.nothing_arriving")}
                                </p>
                            }
                                .into_any()
                        }
                        Some(arrival) => view! { <ArrivalForm arrival=arrival /> }.into_any(),
                    }
                })}
            </Transition>
        </Panel>
    }
}

#[component]
fn arrival_form(arrival: ArrivalInput) -> impl IntoView {
    let arrival = RwSignal::new(arrival);
    let saving = RwSignal::new(false);
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let receive = {
        let navigate = navigate.clone();
        move || {
            let submission = arrival.get_untracked();
            let id = submission.transfer_id;
            let navigate = navigate.clone();
            saving.set(true);

            leptos::task::spawn_local(async move {
                let result = receive_transfer(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(_)) => {
                        alerts.post(Alert::success(l!("transfers.received.done")));
                        navigate(
                            &format!("/inventory/transfers/{id}"),
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
        }
    };

    view! {
        <div class="space-y-2">
            <label class="block max-w-xs space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("transfers.arrived")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || arrival.with(|a| a.arrived_on.to_string())
                    on:change=move |ev| {
                        if let Ok(date) = event_target_value(&ev).parse() {
                            arrival.update(|a| a.arrived_on = date);
                        }
                    }
                />
            </label>

            <div class="overflow-x-auto">
                <table class="w-full min-w-[24rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="py-2 font-medium">{l!("transfers.line")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("transfers.arriving")}
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let count = arrival.with(|a| a.lines.len());
                            (0..count)
                                .map(|index| {
                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1 text-xs text-content-muted">
                                                {index + 1}
                                            </td>
                                            <td class="py-1">
                                                <input
                                                    type="text"
                                                    inputmode="decimal"
                                                    class="w-full text-right tabular-nums"
                                                    prop:value=move || {
                                                        arrival
                                                            .with(|a| {
                                                                a.lines
                                                                    .get(index)
                                                                    .map(|line| line.quantity.clone())
                                                                    .unwrap_or_default()
                                                            })
                                                    }
                                                    on:input=move |ev| {
                                                        let value = event_target_value(&ev);
                                                        arrival
                                                            .update(|a| {
                                                                if let Some(line) = a.lines.get_mut(index) {
                                                                    line.quantity = value;
                                                                }
                                                            });
                                                    }
                                                />
                                            </td>
                                        </tr>
                                    }
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <div class="flex justify-end">
                <PrimaryButton
                    label=l!("transfers.receive")
                    icon=Icon::Check
                    pending=Signal::derive(move || saving.get())
                    on_click=Callback::new({
                        let receive = receive.clone();
                        move |()| receive()
                    })
                />
            </div>
        </div>
    }
}
