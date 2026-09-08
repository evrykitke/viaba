//! One goods receipt: the tally while the pallet is being walked, the document
//! once it is posted.
//!
//! # Posting is the act with the accounting consequence
//!
//! Saving a draft moves nothing. Posting writes the stock moves, values them,
//! and files the journal - stock debited, goods received not invoiced credited
//! - and none of it can be edited afterwards. So it is the last button, it asks
//! first, and a posted receipt is corrected with an adjustment rather than a
//! rewrite.
//!
//! # There is no running total here
//!
//! What a line is worth is decided at post, from the item's own cost where the
//! receipt does not say otherwise, and the browser cannot see either that cost
//! or the workspace's currency. A total drawn here would be a figure that
//! disagrees with the one that gets filed, so the value appears on the document
//! and not before.

use app_inventory::receipt::{Receipt, ReceiptInput, ReceiptLineInput, ReceiptState};
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_master::party::{PartySummary, roles};
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    cancel_receipt, pickable_variants, post_receipt, receipt_against_order, receipt_detail,
    save_receipt, selectable_warehouses,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const BACK: &str = "/inventory/receipts";

/// Booking goods in.
///
/// `?order=<id>` opens it against a purchase order, prefilled with everything
/// that order still owes - the screen somebody wants when the lorry is at the
/// door. Without it the receipt stands alone, which is ordinary: samples and
/// customer returns arrive against no order at all.
#[component]
pub fn receipt_new_page() -> impl IntoView {
    let query = leptos_router::hooks::use_query_map();
    let against = move || {
        query.with(|query| {
            query
                .get("order")
                .and_then(|raw| raw.parse::<Uuid>().ok())
        })
    };

    // Today from the browser's own clock: a receipt is dated where the person
    // booking it in is standing.
    let today = chrono::Local::now().date_naive();

    let prefilled = Resource::new(against, move |order_id| async move {
        match order_id {
            None => Ok(ReceiptInput::blank(today)),
            Some(id) => match receipt_against_order(id).await {
                Ok(Submission::Saved(draft)) => Ok(draft),
                Ok(Submission::Rejected(errors)) => Err(errors
                    .first()
                    .map(|error| crate::i18n::t(&error.message))
                    .unwrap_or_else(|| l!("receipts.gone"))),
                Err(err) => Err(err.to_string()),
            },
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("receipts.new")) />

        <PageHeader
            title=l!("receipts.new")
            subtitle=l!("receipts.new.subtitle")
            icon=Icon::Package
            back=(BACK, l!("receipts.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match prefilled.await {
                    Ok(draft) => view! { <ReceiptEditor draft=draft /> }.into_any(),
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

/// One receipt: editable while it is a draft, evidence after.
#[component]
pub fn receipt_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let receipt_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let receipt = Resource::new(receipt_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => receipt_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a receipt id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.goods_receipt.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match receipt.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let supplier = stored.supplier.name.clone();
                        let state = stored.state;
                        let opened_on = ReceiptInput::from_receipt(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=supplier
                                    icon=Icon::Package
                                    back=(BACK, l!("receipts.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! { <ReceiptEditor draft=opened_on /> }.into_any()
                                } else {
                                    view! {
                                        <ReceiptDocument receipt=stored />
                                    }
                                        .into_any()
                                }}
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.goods_receipt.singular")
                                    icon=Icon::Package
                                    back=(BACK, l!("receipts.title"))
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
fn state_badge(state: ReceiptState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        ReceiptState::Draft => Tone::Neutral,
        ReceiptState::Done => Tone::Success,
        ReceiptState::Cancelled => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

#[component]
fn receipt_editor(draft: ReceiptInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let suppliers = Resource::new(
        || (),
        |()| async move { list_parties(Some(roles::SUPPLIER.to_owned())).await },
    );
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });
    let variants = Resource::new(|| (), |()| async move { pickable_variants().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let suppliers = suppliers.await.unwrap_or_default();
                    let warehouses = warehouses.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();

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
                            suppliers=suppliers
                            warehouses=warehouse_options
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
    draft: RwSignal<ReceiptInput>,
    suppliers: Vec<PartySummary>,
    warehouses: Vec<Choice>,
    variants: Vec<VariantChoice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let variants = StoredValue::new(variants);

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_receipt(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("receipts.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/receipts/{id}"),
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

    let post = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("receipts.post.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = post_receipt(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(receipt)) => {
                                alerts.post(
                                    Alert::success(l!("receipts.posted", number = receipt.number))
                                        .titled(l!("receipts.post")),
                                );
                                // Reload the route: it is evidence now, and this
                                // screen draws a different thing for that.
                                navigate(
                                    &format!("/inventory/receipts/{id}"),
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
                .titled(l!("receipts.post"))
                .confirm_label(l!("receipts.post")),
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
                Confirm::new(l!("receipts.cancel.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match cancel_receipt(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("receipts.cancelled")));
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
                .titled(l!("receipts.cancel"))
                .confirm_label(l!("receipts.cancel")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());

    view! {
        <div class="space-y-3">
            <Panel title=l!("receipts.header")>
                <HeaderFields draft=draft suppliers=suppliers warehouses=warehouses />
            </Panel>

            <Panel title=l!("receipts.lines") description=l!("receipts.lines.help")>
                <LineTable draft=draft variants=variants />
            </Panel>

            <Panel title=l!("receipts.note")>
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
                        label=l!("receipts.cancel")
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

                // Offered only once the draft is saved: there is nothing to move
                // and nothing to number otherwise.
                <Show when=saved fallback=|| ()>
                    <PrimaryButton
                        label=l!("receipts.post")
                        icon=Icon::Check
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let post = post.clone();
                            move |()| post()
                        })
                    />
                </Show>
            </div>
        </div>
    }
}

/// Who it came from, where it landed, and when.
#[component]
fn header_fields(
    draft: RwSignal<ReceiptInput>,
    suppliers: Vec<PartySummary>,
    warehouses: Vec<Choice>,
) -> impl IntoView {
    let supplier_options = suppliers
        .iter()
        .filter(|party| party.is_active)
        .map(|party| {
            Choice::new(party.id.to_string(), party.name.clone()).detail(party.code.clone())
        })
        .collect::<Vec<_>>();

    view! {
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="block space-y-1">
                <label for="receipt-supplier" class="block text-xs font-medium text-content-muted">
                    {l!("purchase_orders.supplier")}
                </label>
                <SelectField
                    id="receipt-supplier"
                    value=Signal::derive(move || {
                        draft.with(|d| d.supplier_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft.update(|d| d.supplier_id = chosen);
                    })
                    options=supplier_options
                    placeholder=l!("common.not_set")
                    clearable=true
                />
            </div>

            <div class="block space-y-1">
                <label for="receipt-warehouse" class="block text-xs font-medium text-content-muted">
                    {l!("nav.warehouses")}
                </label>
                <SelectField
                    id="receipt-warehouse"
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
                    {l!("receipts.received_on")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.received_on.to_string())
                    on:change=move |ev| {
                        if let Ok(date) = event_target_value(&ev).parse() {
                            draft.update(|d| d.received_on = date);
                        }
                    }
                />
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("receipts.delivery_note")}
                </span>
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.delivery_note.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.delivery_note = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("receipts.delivery_note.help")}
                </span>
            </label>
        </div>
    }
}

#[component]
fn line_table(
    draft: RwSignal<ReceiptInput>,
    variants: StoredValue<Vec<VariantChoice>>,
) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[56rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("purchase_orders.item")}</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">
                                {l!("field.quantity")}
                            </th>
                            <th class="w-32 py-2 font-medium">{l!("receipts.lot")}</th>
                            <th class="w-36 py-2 font-medium">{l!("receipts.expires")}</th>
                            <th class="w-28 py-2 text-right font-medium">
                                {l!("receipts.unit_cost")}
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
                label=l!("receipts.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(ReceiptLineInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<ReceiptInput>,
    index: usize,
    variants: StoredValue<Vec<VariantChoice>>,
) -> impl IntoView {
    let field = move |read: fn(&ReceiptLineInput) -> String| {
        draft.with(|d| d.lines.get(index).map(read).unwrap_or_default())
    };

    // A line that came from an order names an item already, and changing it
    // would break the thread back to what was committed to.
    let from_order = move || {
        draft.with(|d| {
            d.lines
                .get(index)
                .is_some_and(|line| line.order_line_id.is_some())
        })
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
                        field(|line| {
                            line.variant_id.map(|id| id.to_string()).unwrap_or_default()
                        })
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
                    disabled=Signal::derive(from_order)
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
                // Text, not a number input: the parser refuses a seventh decimal
                // place rather than rounding one away.
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
                // Typed, never generated: it is the supplier's batch number and
                // one this system invented would match nothing on the carton.
                <input
                    type="text"
                    class="w-full font-mono text-xs"
                    prop:value=move || field(|line| line.lot_number.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.lot_number = value;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || field(|line| {
                        line.expires_on.map(|on| on.to_string()).unwrap_or_default()
                    })
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.expires_on = value.parse().ok();
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
                    prop:value=move || field(|line| line.unit_cost.clone())
                    placeholder=l!("receipts.unit_cost.default")
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.unit_cost = value;
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
                                    d.lines.push(ReceiptLineInput::blank());
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

/// A posted or cancelled receipt: read-only, and every figure is the one that
/// was stored rather than one looked up now.
#[component]
fn receipt_document(receipt: Receipt) -> impl IntoView {
    let id = receipt.id;
    let supplier_name = receipt.supplier.name.clone();
    let supplier_code = receipt.supplier.code.clone();
    let order = receipt
        .order_id
        .zip(receipt.order_number.clone())
        .map(|(id, number)| (format!("/inventory/orders/{id}"), number));
    let warehouse = receipt.warehouse_name.clone();
    let location = receipt.to_location_path.clone();
    let received = receipt.received_on.to_string();
    let delivery_note = receipt.delivery_note.clone();
    let note = receipt.note.clone();
    let value = receipt.value.to_display_string();
    let currency = receipt.value.currency().code().to_owned();
    let lines = receipt.lines.clone();

    view! {
        <div class="space-y-3">
            <div class="grid gap-3 lg:grid-cols-2">
                <Panel title=l!("purchase_orders.supplier")>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{supplier_name}</div>
                        <code class="text-2xs text-content-subtle">{supplier_code}</code>
                        {order
                            .map(|(href, number)| {
                                view! {
                                    <div class="text-xs">
                                        <leptos_router::components::A
                                            href=href
                                            attr:class="text-accent hover:underline"
                                        >
                                            {l!("receipts.order")} " " {number}
                                        </leptos_router::components::A>
                                    </div>
                                }
                            })}
                    </div>
                </Panel>

                <Panel title=l!("receipts.header")>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("nav.warehouses")}</dt>
                            <dd class="text-content">{warehouse}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("receipts.landed_in")}</dt>
                            <dd class="font-mono text-xs text-content">{location}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("receipts.received_on")}</dt>
                            <dd class="tabular-nums text-content">{received}</dd>
                        </div>
                        {delivery_note
                            .map(|delivery_note| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("receipts.delivery_note")}
                                        </dt>
                                        <dd class="font-mono text-xs text-content">
                                            {delivery_note}
                                        </dd>
                                    </div>
                                }
                            })}
                    </dl>
                </Panel>
            </div>

            <Panel title=l!("receipts.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[48rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 text-right font-medium">{l!("field.quantity")}</th>
                                <th class="py-2 font-medium">{l!("receipts.lot")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("receipts.unit_cost")}
                                </th>
                                <th class="py-2 text-right font-medium">{l!("receipts.value")}</th>
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
                                    let lot = line.lot_number.clone().unwrap_or_default();
                                    let expires = line.expires_on.map(|on| on.to_string());
                                    let cost = line.unit_cost.to_display_string();
                                    let value = line.value.to_display_string();

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
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {quantity}
                                            </td>
                                            <td class="py-1.5 font-mono text-xs text-content-muted">
                                                {lot}
                                                {expires
                                                    .map(|expires| {
                                                        view! {
                                                            <div class="text-2xs text-content-subtle">
                                                                {expires}
                                                            </div>
                                                        }
                                                    })}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {cost}
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
            </Panel>

            <div class="grid gap-3 lg:grid-cols-[1fr_22rem] lg:items-start">
                {note
                    .map(|note| {
                        view! {
                            <Panel title=l!("receipts.note")>
                                <p class="whitespace-pre-wrap text-sm text-content-muted">
                                    {note}
                                </p>
                            </Panel>
                        }
                    })}

                <Panel title=l!("receipts.value")>
                    <div class="flex items-baseline justify-between gap-4 text-sm font-medium tabular-nums">
                        <span class="text-content">
                            {l!("receipts.value")} " "
                            <span class="text-2xs text-content-subtle">{currency}</span>
                        </span>
                        <span class="text-content">{value}</span>
                    </div>
                    <p class="mt-1 text-2xs text-content-subtle">{l!("receipts.value.help")}</p>
                </Panel>
            </div>

            <Panel title=l!("common.history")>
                <RecordHistory kind=kinds::RECEIPT id=Some(id.to_string()) />
            </Panel>
        </div>
    }
}
