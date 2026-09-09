//! One purchase order: the editor while it is a draft, the document once it is
//! a commitment.
//!
//! # Confirming is the decision, so it sits where a decision goes
//!
//! Saving a draft costs nothing and can be undone. Confirming takes a number
//! nobody can hand back, freezes the supplier onto the record, and makes the
//! quantities the thing every receipt and bill is measured against. So it is
//! the last button on the screen, it asks first, and it is offered only once
//! there is something saved to number.
//!
//! # The net is worked out in the browser, with the server's own arithmetic
//!
//! `phonix_core::money` compiles to wasm, so the total moves as somebody types
//! and the figure they approve is the figure the server computes. The one thing
//! this cannot know is the fallback price - a line with no price takes the
//! item's own cost, which lives in the database - so a blank line is left out
//! of the preview rather than counted as nothing.

use app_inventory::purchase::{
    OrderInput, OrderLineInput, OrderState, PurchaseOrder, ReceiptState,
};
use app_inventory::quantity::{self, Quantity};
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::locale::Currency;
use phonix_core::money::{Money, Rounding};
use phonix_core::permissions;
use phonix_master::party::{PartySummary, roles};
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Tone};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    blank_purchase_order, cancel_purchase_order, confirm_purchase_order, delete_purchase_order,
    pickable_variants, purchase_order_detail, save_purchase_order, selectable_units,
    selectable_warehouses,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const BACK: &str = "/inventory/orders";

/// Raising one.
#[component]
pub fn purchase_order_new_page() -> impl IntoView {
    // The blank comes from the server because its currency is the workspace's,
    // which the browser has no way to know.
    let blank = Resource::new(|| (), |()| async move { blank_purchase_order().await });

    view! {
        <Title text=format!("{} | Phonix", l!("purchase_orders.new")) />

        <PageHeader
            title=l!("purchase_orders.new")
            subtitle=l!("purchase_orders.new.subtitle")
            icon=Icon::ScrollText
            back=(BACK, l!("purchase_orders.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match blank.await {
                    Ok(draft) => view! { <OrderEditor draft=draft /> }.into_any(),
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

/// One order: editable while it is a draft or a quotation, a document after.
#[component]
pub fn purchase_order_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let order_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let order = Resource::new(order_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => purchase_order_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not an order id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.purchase_order.singular")) />

        // Transition rather than Suspense, so moving between two orders replaces
        // the screen when the next one arrives instead of blanking it.
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match order.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let supplier = stored.supplier.name.clone();
                        let state = stored.state;
                        let opened_on = OrderInput::from_order(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=supplier
                                    icon=Icon::ScrollText
                                    back=(BACK, l!("purchase_orders.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! { <OrderEditor draft=opened_on /> }.into_any()
                                } else {
                                    view! {
                                        <OrderDocument
                                            order=stored
                                            reload=Callback::new(move |()| order.refetch())
                                        />
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
                                    title=l!("entity.purchase_order.singular")
                                    icon=Icon::ScrollText
                                    back=(BACK, l!("purchase_orders.title"))
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
fn state_badge(state: OrderState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        OrderState::Draft | OrderState::Sent => Tone::Neutral,
        OrderState::Confirmed => Tone::Success,
        OrderState::Done => Tone::Brand,
        OrderState::Cancelled => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

#[component]
fn receipt_badge(state: ReceiptState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        ReceiptState::Nothing => Tone::Neutral,
        ReceiptState::Partly => Tone::Warning,
        ReceiptState::Everything => Tone::Success,
        ReceiptState::Over => Tone::Danger,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

/// What the lines come to, priced the way the server prices them.
///
/// A line with no price is skipped rather than treated as zero: the server
/// fills it from the item's cost, and a preview that read it as nothing would
/// show a total nobody is going to be charged.
fn net_of(draft: &OrderInput) -> Option<Money> {
    let currency = Currency::parse(&draft.currency).ok()?;
    let mut lines = Vec::with_capacity(draft.lines.len());

    for line in &draft.lines {
        let Ok(quantity) = Quantity::parse(&line.quantity) else {
            continue;
        };
        let Ok(price) = Money::parse(currency, line.unit_price.trim()) else {
            continue;
        };

        lines.push(
            price
                .scale_by(quantity.scaled(), quantity::SCALE_FACTOR, Rounding::HalfUp)
                .ok()?,
        );
    }

    Money::total(currency, lines).ok()
}

#[component]
fn order_editor(draft: OrderInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let suppliers = Resource::new(
        || (),
        |()| async move { list_parties(Some(roles::SUPPLIER.to_owned())).await },
    );
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });
    let variants = Resource::new(|| (), |()| async move { pickable_variants().await });
    let units = Resource::new(|| (), |()| async move { selectable_units().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    // Empty pickers rather than a failed screen: a workspace with
                    // no suppliers yet gets a form it cannot submit, which is the
                    // honest state of affairs.
                    let suppliers = suppliers.await.unwrap_or_default();
                    let warehouses = warehouses.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();
                    let units = units.await.unwrap_or_default();

                    let unit_options = units
                        .iter()
                        .map(|unit| {
                            Choice::new(unit.id.to_string(), unit.name.clone())
                                .detail(unit.code.clone())
                        })
                        .collect::<Vec<_>>();
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
                            unit_options=unit_options
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
    draft: RwSignal<OrderInput>,
    suppliers: Vec<PartySummary>,
    warehouses: Vec<Choice>,
    variants: Vec<VariantChoice>,
    unit_options: Vec<Choice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let variants = StoredValue::new(variants);
    let unit_options = StoredValue::new(unit_options);

    let net = Memo::new(move |_| draft.with(net_of));

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_purchase_order(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("purchase_orders.saved")));

                        // A new draft has an id now, so its address has changed.
                        // Replacing rather than pushing: back should reach the
                        // list, not a form that no longer exists.
                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/orders/{id}"),
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

    let confirm = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("purchase_orders.confirm.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = confirm_purchase_order(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(order)) => {
                                alerts.post(
                                    Alert::success(
                                            l!("purchase_orders.confirmed", number = order.number),
                                        )
                                        .titled(l!("purchase_orders.confirm")),
                                );
                                // Reload the route: it is a document now, and
                                // this screen draws a different thing for one.
                                navigate(
                                    &format!("/inventory/orders/{id}"),
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
                .titled(l!("purchase_orders.confirm"))
                .confirm_label(l!("purchase_orders.confirm")),
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
                Confirm::new(l!("purchase_orders.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_purchase_order(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("purchase_orders.deleted")));
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
        <div class="space-y-3">
            <Panel title=l!("purchase_orders.header")>
                <HeaderFields draft=draft suppliers=suppliers warehouses=warehouses />
            </Panel>

            <Panel title=l!("purchase_orders.lines") description=l!("purchase_orders.lines.help")>
                <LineTable draft=draft variants=variants unit_options=unit_options />
            </Panel>

            <div class="grid gap-3 lg:grid-cols-[1fr_22rem] lg:items-start">
                <Panel title=l!("purchase_orders.note")>
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

                <Panel title=l!("purchase_orders.net")>
                    {move || match net.get() {
                        None => {
                            view! {
                                <p class="text-sm text-content-subtle">
                                    {l!("purchase_orders.no_net")}
                                </p>
                            }
                                .into_any()
                        }
                        Some(net) => {
                            let code = net.currency().code().to_owned();
                            let amount = net.to_display_string();
                            view! {
                                <div class="flex items-baseline justify-between gap-4 text-sm font-medium tabular-nums">
                                    <span class="text-content">
                                        {l!("purchase_orders.net")} " "
                                        <span class="text-2xs text-content-subtle">{code}</span>
                                    </span>
                                    <span class="text-content">{amount}</span>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                </Panel>
            </div>

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

                <GhostButton
                    label=l!("common.save")
                    icon=Icon::Save
                    disabled=Signal::derive(move || saving.get())
                    on_click=Callback::new({
                        let save = save.clone();
                        move |()| save()
                    })
                />

                // Offered only once there is something saved to number.
                <Show when=saved fallback=|| ()>
                    <PrimaryButton
                        label=l!("purchase_orders.confirm")
                        icon=Icon::Check
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let confirm = confirm.clone();
                            move |()| confirm()
                        })
                    />
                </Show>
            </div>
        </div>
    }
}

/// Who it is with, where it is going, and when.
#[component]
fn header_fields(
    draft: RwSignal<OrderInput>,
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
    let currency_options = Currency::all()
        .iter()
        .map(|currency| Choice::new(currency.code(), currency.label()))
        .collect::<Vec<_>>();

    view! {
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="block space-y-1">
                <label for="order-supplier" class="block text-xs font-medium text-content-muted">
                    {l!("purchase_orders.supplier")}
                </label>
                <SelectField
                    id="order-supplier"
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
                <label for="order-warehouse" class="block text-xs font-medium text-content-muted">
                    {l!("nav.warehouses")}
                </label>
                <SelectField
                    id="order-warehouse"
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
                    {l!("purchase_orders.ordered")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.order_date.to_string())
                    on:change=move |ev| {
                        if let Ok(date) = event_target_value(&ev).parse() {
                            draft.update(|d| d.order_date = date);
                        }
                    }
                />
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("purchase_orders.expected")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || {
                        draft.with(|d| d.expected_on.map(|on| on.to_string()).unwrap_or_default())
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.expected_on = value.parse().ok());
                    }
                />
            </label>

            <div class="block space-y-1">
                <label for="order-currency" class="block text-xs font-medium text-content-muted">
                    {l!("field.currency")}
                </label>
                <SelectField
                    id="order-currency"
                    value=Signal::derive(move || draft.with(|d| d.currency.clone()))
                    on_change=Callback::new(move |value: String| {
                        // Unrecognised keeps what was there: a currency silently
                        // becoming dollars changes what every price on the order
                        // means.
                        if Currency::parse(&value).is_ok() {
                            draft.update(|d| d.currency = value);
                        }
                    })
                    options=currency_options
                />
            </div>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("purchase_orders.reference")}
                </span>
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.supplier_reference.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.supplier_reference = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("purchase_orders.reference.help")}
                </span>
            </label>
        </div>
    }
}

#[component]
fn line_table(
    draft: RwSignal<OrderInput>,
    variants: StoredValue<Vec<VariantChoice>>,
    unit_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    view! {
        <div class="space-y-2">
            // `overflow-x-auto` on the table's own container, never on the page:
            // anything wider than the phone inflates the viewport and throws
            // every fixed overlay off screen.
            <div class="overflow-x-auto">
                <table class="w-full min-w-[52rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("purchase_orders.item")}</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">
                                {l!("field.quantity")}
                            </th>
                            <th class="w-32 py-2 font-medium">{l!("units.title")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("purchase_orders.unit_price")}
                            </th>
                            <th class="w-8 py-2"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let count = draft.with(|d| d.lines.len());
                            (0..count)
                                .map(|index| {
                                    view! {
                                        <LineRow
                                            draft=draft
                                            index=index
                                            variants=variants
                                            unit_options=unit_options
                                        />
                                    }
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("purchase_orders.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(OrderLineInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<OrderInput>,
    index: usize,
    variants: StoredValue<Vec<VariantChoice>>,
    unit_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    // Every read goes through the index rather than a held clone: a row that
    // cached its own values would stop updating the moment another row was
    // removed and the indexes shifted.
    let field = move |read: fn(&OrderLineInput) -> String| {
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
                        field(|line| {
                            line.variant_id.map(|id| id.to_string()).unwrap_or_default()
                        })
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        // Choosing an item fills the unit it is bought in, and
                        // its name where nothing has been typed. Both stay
                        // editable: a supplier who sells in cases is exactly the
                        // case this has to allow.
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
                                        if line.unit_id.is_none() {
                                            line.unit_id = Some(picked.purchase_unit_id);
                                        }
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
                // Text rather than a number input: `Quantity::parse` refuses a
                // seventh decimal place rather than rounding it, and a browser
                // number input would have rounded before this ever saw it.
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
                <SelectField
                    value=Signal::derive(move || {
                        field(|line| line.unit_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.unit_id = chosen;
                                }
                            });
                    })
                    options=unit_options.get_value()
                    placeholder=l!("common.not_set")
                    clearable=true
                    label=l!("units.title")
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=move || field(|line| line.unit_price.clone())
                    placeholder=l!("purchase_orders.unit_price.default")
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.unit_price = value;
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
                                // Never leave the table empty: a form with no
                                // rows has nothing to type into.
                                if d.lines.is_empty() {
                                    d.lines.push(OrderLineInput::blank());
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

/// A confirmed, closed or cancelled order: read-only, and every figure on it is
/// what was stored rather than what could be looked up now.
#[component]
fn order_document(order: PurchaseOrder, reload: Callback<()>) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();
    let viewer = crate::ui::viewer::Viewer::get();

    let may_cancel = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::PURCHASE_ORDERS_CANCEL))
        })
    });
    let may_receive = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::RECEIPTS_CREATE))
        })
    });

    let id = order.id;
    let can_be_received = order.can_be_received();
    let is_cancellable = order.state == OrderState::Confirmed;
    let receipt_state = order.receipt_state();

    let supplier_name = order.supplier.name.clone();
    let supplier_code = order.supplier.code.clone();
    let warehouse = order.warehouse_name.clone();
    let ordered = order.order_date.to_string();
    let expected = order.expected_on.map(|on| on.to_string());
    let reference = order.supplier_reference.clone();
    let note = order.note.clone();
    let code = order.currency.clone();
    let net = order.net.to_display_string();
    let lines = order.lines.clone();

    let receive = {
        let navigate = navigate.clone();
        move || {
            // The receipt screen opens against this order and prefills what is
            // still owed; the lines do not need carrying across.
            navigate(
                &format!("/inventory/receipts/new?order={id}"),
                leptos_router::NavigateOptions::default(),
            );
        }
    };

    let cancel = move || {
        alerts.ask(
            Confirm::new(l!("purchase_orders.cancel.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match cancel_purchase_order(id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("purchase_orders.cancelled")));
                            let _ = reload.try_run(());
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
            .titled(l!("purchase_orders.cancel"))
            .confirm_label(l!("purchase_orders.cancel")),
        );
    };

    view! {
        <div class="space-y-3">
            <div class="grid gap-3 lg:grid-cols-2">
                <Panel title=l!("purchase_orders.supplier")>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{supplier_name}</div>
                        <code class="text-2xs text-content-subtle">{supplier_code}</code>
                        {reference
                            .map(|reference| {
                                view! {
                                    <div class="text-xs text-content-muted">
                                        {l!("purchase_orders.reference")} ": " {reference}
                                    </div>
                                }
                            })}
                    </div>
                </Panel>

                <Panel title=l!("purchase_orders.header")>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("nav.warehouses")}</dt>
                            <dd class="text-content">{warehouse}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("purchase_orders.ordered")}</dt>
                            <dd class="tabular-nums text-content">{ordered}</dd>
                        </div>
                        {expected
                            .map(|expected| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("purchase_orders.expected")}
                                        </dt>
                                        <dd class="tabular-nums text-content">{expected}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex items-center justify-between gap-4">
                            <dt class="text-content-muted">{l!("purchase_orders.received")}</dt>
                            <dd>
                                <ReceiptBadge state=receipt_state />
                            </dd>
                        </div>
                    </dl>
                </Panel>
            </div>

            <Panel title=l!("purchase_orders.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[44rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 text-right font-medium">{l!("field.quantity")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("purchase_orders.received")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("purchase_orders.unit_price")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("purchase_orders.net")}
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
                                    let received = line.received.to_display_string();
                                    let price = line.unit_price.to_display_string();
                                    let net = line.net.to_display_string();
                                    // A cancelled line stays on the document -
                                    // it was ordered - and is struck through
                                    // rather than removed.
                                    let tone = if line.is_cancelled {
                                        "line-through text-content-subtle"
                                    } else {
                                        "text-content"
                                    };

                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-subtle">
                                                {line.line_no}
                                            </td>
                                            <td class=format!("py-1.5 {tone}")>
                                                {line.description.clone()}
                                                <div class="text-2xs text-content-subtle">
                                                    {line.variant_code.clone()}
                                                </div>
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {quantity}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {received}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {price}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {net}
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
                            <Panel title=l!("purchase_orders.note")>
                                <p class="whitespace-pre-wrap text-sm text-content-muted">
                                    {note}
                                </p>
                            </Panel>
                        }
                    })}

                <Panel title=l!("purchase_orders.net")>
                    <div class="flex items-baseline justify-between gap-4 text-sm font-medium tabular-nums">
                        <span class="text-content">
                            {l!("purchase_orders.net")} " "
                            <span class="text-2xs text-content-subtle">{code}</span>
                        </span>
                        <span class="text-content">{net}</span>
                    </div>
                </Panel>
            </div>

            // Two conditions, and they are different questions. The state decides
            // whether the act means anything; the permission decides whether this
            // reader may do it - and the service checks it again.
            <div class="flex flex-wrap items-center justify-end gap-2">
                <Show when=move || is_cancellable && may_cancel.get() fallback=|| ()>
                    <GhostButton
                        label=l!("purchase_orders.cancel")
                        icon=Icon::Ban
                        on_click=Callback::new(move |()| cancel())
                    />
                </Show>

                <Show when=move || can_be_received && may_receive.get() fallback=|| ()>
                    <PrimaryButton
                        label=l!("purchase_orders.receive")
                        icon=Icon::Package
                        on_click=Callback::new({
                            let receive = receive.clone();
                            move |()| receive()
                        })
                    />
                </Show>
            </div>

            <Panel title=l!("common.history")>
                <RecordHistory kind=kinds::PURCHASE_ORDER id=Some(id.to_string()) />
            </Panel>
        </div>
    }
}
