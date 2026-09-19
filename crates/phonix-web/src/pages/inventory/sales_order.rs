//! One sales order: the editor while it is a draft or a quotation, the
//! document once it is agreed.
//!
//! The mirror of [`super::purchase_order`], with three differences that are
//! about selling rather than about screens.
//!
//! # There are two buttons at the end, not one
//!
//! A purchase order has one decision: confirm it. A sales order has two, and
//! they happen to different people on different days. *Send* puts the quotation
//! in front of a customer and is where the number is spent - from then on
//! somebody outside can cite it. *Confirm* is the customer saying yes, and from
//! then on the quantities are what every delivery is measured against. An order
//! taken over the counter skips the first, which is why Confirm is offered on a
//! draft too.
//!
//! # A blank price is refused, not defaulted
//!
//! The purchase order fills an empty price from the item's cost. This does not:
//! a price nobody typed is revenue nobody decided. The field opens on the
//! item's standing sale price when a line is picked, which is a suggestion
//! somebody can see and change.
//!
//! # The net is worked out in the browser, with the server's own arithmetic
//!
//! `phonix_core::money` compiles to wasm, so the total moves as somebody types
//! and the figure they approve is the figure the server computes. Unlike the
//! buying side there is no fallback to work around: every line that is priced
//! is counted, and an unpriced one is the thing the save will refuse.

use app_inventory::quantity::{self, Quantity};
use app_inventory::sales_order::{Progress, SaleInput, SaleLineInput, SaleState, SalesOrder};
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
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::pages::inventory::item_lookup::ItemLookup;
use crate::server_fns::inventory_fns::{
    blank_sales_order, cancel_sales_order, close_sales_order, confirm_sales_order,
    delete_sales_order, order_outstanding, quoted_price, sales_order_detail, save_sales_order,
    selectable_units, selectable_warehouses, send_sales_order,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const BACK: &str = "/selling/orders";

/// Raising one.
#[component]
pub fn sales_order_new_page() -> impl IntoView {
    // The blank comes from the server because its currency is the workspace's,
    // which the browser has no way to know.
    let blank = Resource::new(|| (), |()| async move { blank_sales_order().await });

    view! {
        <Title text=format!("{} | Evrykit", l!("sales_orders.new")) />

        <PageHeader
            title=l!("sales_orders.new")
            subtitle=l!("sales_orders.new.subtitle")
            icon=Icon::ScrollText
            back=(BACK, l!("sales_orders.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match blank.await {
                    Ok(draft) => {
                        view! { <SaleEditor draft=draft state=SaleState::Draft /> }.into_any()
                    }
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
pub fn sales_order_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let order_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let order = Resource::new(order_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => sales_order_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not an order id.")),
        }
    });

    view! {
        <Title text=format!("{} | Evrykit", l!("entity.sales_order.singular")) />

        // Transition rather than Suspense, so moving between two orders replaces
        // the screen when the next one arrives instead of blanking it.
        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match order.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let customer = stored.customer.name.clone();
                        let state = stored.state;
                        let opened_on = SaleInput::from_order(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=customer
                                    icon=Icon::ScrollText
                                    back=(BACK, l!("sales_orders.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! { <SaleEditor draft=opened_on state=state /> }
                                        .into_any()
                                } else {
                                    view! {
                                        <SaleDocument
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
                                    title=l!("entity.sales_order.singular")
                                    icon=Icon::ScrollText
                                    back=(BACK, l!("sales_orders.title"))
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
fn state_badge(state: SaleState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        SaleState::Draft | SaleState::Sent => Tone::Neutral,
        SaleState::Confirmed => Tone::Success,
        SaleState::Done => Tone::Brand,
        SaleState::Cancelled => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

/// How much has gone, or been billed. Over is a warning rather than a failure:
/// it is not wrong, and somebody should look at it.
#[component]
fn progress_badge(progress: Progress, invoiced: bool) -> impl IntoView {
    let label = crate::i18n::t(&if invoiced {
        progress.invoiced_label()
    } else {
        progress.delivered_label()
    });

    let tone = match progress {
        Progress::Nothing => Tone::Neutral,
        Progress::Partly => Tone::Warning,
        Progress::Everything => Tone::Success,
        Progress::Over => Tone::Danger,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

/// What the lines come to.
///
/// A line with no price is skipped rather than counted as zero - not because
/// the server will fill it in, as it does on a purchase order, but because it
/// is the line the save is about to refuse, and a total that quietly included
/// it as nothing would be a figure nobody is going to be charged.
fn net_of(draft: &SaleInput) -> Option<Money> {
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
fn sale_editor(draft: SaleInput, state: SaleState) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let customers = Resource::new(
        || (),
        |()| async move { list_parties(Some(roles::CUSTOMER.to_owned())).await },
    );
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });
    let units = Resource::new(|| (), |()| async move { selectable_units().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    // Empty pickers rather than a failed screen: a workspace
                    // with no customers yet gets a form it cannot submit, which
                    // is the honest state of affairs.
                    let customers = customers.await.unwrap_or_default();
                    let warehouses = warehouses.await.unwrap_or_default();
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
                            state=state
                            customers=customers
                            warehouses=warehouse_options
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
    draft: RwSignal<SaleInput>,
    state: SaleState,
    customers: Vec<PartySummary>,
    warehouses: Vec<Choice>,
    unit_options: Vec<Choice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

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
                let result = save_sales_order(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("sales_orders.saved")));

                        // A new draft has an id now, so its address has changed.
                        // Replacing rather than pushing: back should reach the
                        // list, not a form that no longer exists.
                        if let Some(id) = id {
                            navigate(
                                &format!("/selling/orders/{id}"),
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

    // Sending and confirming are the same shape - ask, call, reload - so they
    // are one closure over which call to make. Two copies of this would be two
    // places for the reload to be forgotten.
    let issue = {
        let navigate = navigate.clone();

        move |confirming: bool| {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            let question = if confirming {
                l!("sales_orders.confirm.confirm")
            } else {
                l!("sales_orders.send.confirm")
            };
            let title = if confirming {
                l!("sales_orders.confirm")
            } else {
                l!("sales_orders.send")
            };

            alerts.ask(
                Confirm::new(question, {
                    let title = title.clone();

                    move || {
                        let navigate = navigate.clone();
                        let title = title.clone();
                        saving.set(true);

                        leptos::task::spawn_local(async move {
                            let result = if confirming {
                                confirm_sales_order(id).await
                            } else {
                                send_sales_order(id).await
                            };
                            saving.set(false);

                            match result {
                                Ok(Submission::Saved(order)) => {
                                    let words = if confirming {
                                        l!("sales_orders.confirmed", number = order.number)
                                    } else {
                                        l!("sales_orders.sent", number = order.number)
                                    };

                                    alerts.post(Alert::success(words).titled(title));

                                    // Reload the route: a confirmed order is a
                                    // document now, and a sent one has a number
                                    // this screen has not seen.
                                    navigate(
                                        &format!("/selling/orders/{id}"),
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
                })
                .titled(title.clone())
                .confirm_label(title),
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
                Confirm::new(l!("sales_orders.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_sales_order(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("sales_orders.deleted")));
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

    // Deleting is offered only on a draft. A quotation that has been sent
    // carries a number somebody outside has been given, so it is cancelled and
    // kept - the service refuses the delete either way, and a button that is
    // always refused is worse than no button.
    let deletable = move || state == SaleState::Draft && saved();

    view! {
        <Panel>
            <Section title=l!("sales_orders.header")>
                <HeaderFields draft=draft customers=customers warehouses=warehouses />
            </Section>

            <Section title=l!("sales_orders.lines") description=l!("sales_orders.lines.help")>
                <LineTable draft=draft unit_options=unit_options />
            </Section>

            <div class="grid gap-3 lg:grid-cols-[1fr_22rem] lg:items-start">
                <Section title=l!("sales_orders.note") flush=true>
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

                <Section title=l!("sales_orders.net") flush=true>
                    {move || match net.get() {
                        None => {
                            view! {
                                <p class="text-sm text-content-subtle">
                                    {l!("sales_orders.no_net")}
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
                                        {l!("sales_orders.net")} " "
                                        <span class="text-2xs text-content-subtle">{code}</span>
                                    </span>
                                    <span class="text-content">{amount}</span>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                </Section>
            </div>

            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=deletable fallback=|| ()>
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

                // Both offered only once there is something saved to number.
                // Send is a ghost beside Confirm on purpose: an order taken
                // over the counter goes straight to the stronger one, and the
                // screen should not make quoting look like the only way
                // forward.
                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("sales_orders.send")
                        icon=Icon::Mail
                        disabled=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let issue = issue.clone();
                            move |()| issue(false)
                        })
                    />
                    <PrimaryButton
                        label=l!("sales_orders.confirm")
                        icon=Icon::Check
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let issue = issue.clone();
                            move |()| issue(true)
                        })
                    />
                </Show>
            </div>
        </Panel>
    }
}

/// Who it is with, where it ships from, and when.
#[component]
fn header_fields(
    draft: RwSignal<SaleInput>,
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
    let currency_options = Currency::all()
        .iter()
        .map(|currency| Choice::new(currency.code(), currency.label()))
        .collect::<Vec<_>>();

    view! {
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="block space-y-1">
                <label for="sale-customer" class="block text-xs font-medium text-content-muted">
                    {l!("sales_orders.customer")}
                </label>
                <SelectField
                    id="sale-customer"
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
                <label for="sale-warehouse" class="block text-xs font-medium text-content-muted">
                    {l!("nav.warehouses")}
                </label>
                <SelectField
                    id="sale-warehouse"
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
                    {l!("sales_orders.ordered")}
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
                    {l!("sales_orders.promised")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || {
                        draft.with(|d| d.promised_on.map(|on| on.to_string()).unwrap_or_default())
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.promised_on = value.parse().ok());
                    }
                />
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("sales_orders.valid_until")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || {
                        draft.with(|d| d.valid_until.map(|on| on.to_string()).unwrap_or_default())
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.valid_until = value.parse().ok());
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("sales_orders.valid_until.help")}
                </span>
            </label>

            <div class="block space-y-1">
                <label for="sale-currency" class="block text-xs font-medium text-content-muted">
                    {l!("field.currency")}
                </label>
                <SelectField
                    id="sale-currency"
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
                    {l!("sales_orders.reference")}
                </span>
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.customer_reference.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.customer_reference = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("sales_orders.reference.help")}
                </span>
            </label>
        </div>
    }
}

#[component]
fn line_table(draft: RwSignal<SaleInput>, unit_options: StoredValue<Vec<Choice>>) -> impl IntoView {
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
                            <th class="w-56 py-2 font-medium">{l!("sales_orders.item")}</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">{l!("field.quantity")}</th>
                            <th class="w-32 py-2 font-medium">{l!("units.title")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("sales_orders.unit_price")}
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
                label=l!("sales_orders.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(SaleLineInput::blank()));
                })
            />
        </div>
    }
}

/// Whether the price in this line's box is one the price list put there.
///
/// True for an empty box and for one still holding exactly what was quoted into
/// it. False the moment somebody edits it - which is what stops a re-price from
/// overwriting a figure a person negotiated and typed.
fn price_is_ours(
    draft: RwSignal<SaleInput>,
    index: usize,
    quoted: RwSignal<Option<String>>,
) -> bool {
    draft.with_untracked(|d| {
        d.lines.get(index).is_some_and(|line| {
            let typed = line.unit_price.trim();

            typed.is_empty()
                || quoted.with_untracked(|last| last.as_deref().is_some_and(|last| last == typed))
        })
    })
}

#[component]
fn line_row(
    draft: RwSignal<SaleInput>,
    index: usize,
    unit_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    // Every read goes through the index rather than a held clone: a row that
    // cached its own values would stop updating the moment another row was
    // removed and the indexes shifted.
    let field = move |read: fn(&SaleLineInput) -> String| {
        draft.with(|d| d.lines.get(index).map(read).unwrap_or_default())
    };

    // The chip the field opens with, built once from what the line already
    // says. No query: a fifty-line document reopened would otherwise be fifty
    // lookups before anything is on screen.
    let initial = draft.with_untracked(|d| {
        let line = d.lines.get(index)?;
        let id = line.variant_id?;
        let label = match line.description.trim() {
            "" => id.to_string(),
            words => words.to_owned(),
        };

        Some(Choice::new(id.to_string(), label))
    });

    // The last price the list put in this row's box. What makes re-pricing
    // safe: a box still holding this is one nobody has touched since, and a box
    // holding anything else was typed by a person and is theirs.
    let quoted: RwSignal<Option<String>> = RwSignal::new(None);

    // What the customer's price list says, asked when the item is chosen and
    // again when the quantity is committed - a line typed as one and changed to
    // a hundred should cross its break. On the quantity's `change` rather than
    // its `input`, so this is one request per edit and not one per keystroke.
    //
    // A blank answer leaves the box blank: no list, or a list that does not
    // carry this item, is not a reason to put a zero on a quotation.
    let quote = move || {
        let Some((party_id, variant_id, quantity, on)) = draft.with_untracked(|d| {
            let line = d.lines.get(index)?;
            Some((
                d.customer_id?,
                line.variant_id?,
                line.quantity.clone(),
                d.order_date,
            ))
        }) else {
            return;
        };

        if !price_is_ours(draft, index, quoted) {
            return;
        }

        leptos::task::spawn_local(async move {
            if let Ok(Some(price)) = quoted_price(party_id, variant_id, quantity, on).await {
                // Asked again on the way back: the request took a moment, and
                // somebody may have typed a price into the box during it.
                if !price_is_ours(draft, index, quoted) {
                    return;
                }

                draft.update(|d| {
                    if let Some(line) = d.lines.get_mut(index) {
                        line.unit_price = price.clone();
                    }
                });

                quoted.set(Some(price));
            }
        });
    };

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
                                    if let Some(picked) = &picked
                                        && line.unit_id.is_none()
                                    {
                                        line.unit_id = Some(picked.unit_id);
                                    }
                                    if let Some(picked) = &picked
                                        && line.description.trim().is_empty()
                                    {
                                        line.description = picked.label();
                                    }
                                }
                            });
                        quote();
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
                    on:change=move |_| quote()
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
                                    d.lines.push(SaleLineInput::blank());
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
fn sale_document(order: SalesOrder, reload: Callback<()>) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();
    let viewer = crate::ui::viewer::Viewer::get();

    let may_cancel = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::SALES_ORDERS_CANCEL))
        })
    });
    let may_close = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::SALES_ORDERS_CONFIRM))
        })
    });
    let may_ship = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::DELIVERIES_CREATE))
        })
    });

    let id = order.id;
    let is_open = order.state == SaleState::Confirmed;
    let can_be_delivered = order.can_be_delivered();
    let delivery_state = order.delivery_state();
    let invoice_state = order.invoice_state();

    let customer_name = order.customer.name.clone();
    let customer_code = order.customer.code.clone();
    let warehouse = order.warehouse_name.clone();
    let ordered = order.order_date.to_string();
    let promised = order.promised_on.map(|on| on.to_string());
    let reference = order.customer_reference.clone();
    let note = order.note.clone();
    let code = order.currency.clone();
    let net = order.net.to_display_string();
    let lines = order.lines.clone();

    let ship = {
        let navigate = navigate.clone();
        move || {
            // The delivery screen opens against this order and prefills what is
            // still owed; the lines do not need carrying across.
            navigate(
                &format!("/selling/deliveries/new?order={id}"),
                leptos_router::NavigateOptions::default(),
            );
        }
    };

    let cancel = move || {
        alerts.ask(
            Confirm::new(l!("sales_orders.cancel.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match cancel_sales_order(id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("sales_orders.cancelled")));
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
            .titled(l!("sales_orders.cancel"))
            .confirm_label(l!("sales_orders.cancel")),
        );
    };

    let close = move || {
        alerts.ask(
            Confirm::new(l!("sales_orders.close.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match close_sales_order(id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("sales_orders.closed")));
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
            .titled(l!("sales_orders.close"))
            .confirm_label(l!("sales_orders.close")),
        );
    };

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("sales_orders.customer") flush=true>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{customer_name}</div>
                        <code class="text-2xs text-content-subtle">{customer_code}</code>
                        {reference
                            .map(|reference| {
                                view! {
                                    <div class="text-xs text-content-muted">
                                        {l!("sales_orders.reference")} ": " {reference}
                                    </div>
                                }
                            })}
                    </div>
                </Section>

                <Section title=l!("sales_orders.header") flush=true>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("nav.warehouses")}</dt>
                            <dd class="text-content">{warehouse}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("sales_orders.ordered")}</dt>
                            <dd class="tabular-nums text-content">{ordered}</dd>
                        </div>
                        {promised
                            .map(|promised| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("sales_orders.promised")}
                                        </dt>
                                        <dd class="tabular-nums text-content">{promised}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex items-center justify-between gap-4">
                            <dt class="text-content-muted">{l!("sales_orders.delivered")}</dt>
                            <dd>
                                <ProgressBadge progress=delivery_state invoiced=false />
                            </dd>
                        </div>
                        <div class="flex items-center justify-between gap-4">
                            <dt class="text-content-muted">{l!("sales_orders.invoiced")}</dt>
                            <dd>
                                <ProgressBadge progress=invoice_state invoiced=true />
                            </dd>
                        </div>
                    </dl>
                </Section>
            </div>

            <Section title=l!("sales_orders.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[48rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 text-right font-medium">{l!("field.quantity")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("sales_orders.delivered")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("sales_orders.invoiced")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("sales_orders.unit_price")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("sales_orders.net")}
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
                                    let delivered = line.delivered.to_display_string();
                                    let invoiced = line.invoiced.to_display_string();
                                    let price = line.unit_price.to_display_string();
                                    let net = line.net.to_display_string();
                                    // A cancelled line stays on the document -
                                    // it was agreed - and is struck through
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
                                                {delivered}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {invoiced}
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
            </Section>

            <div class="grid gap-3 lg:grid-cols-[1fr_22rem] lg:items-start">
                {note
                    .map(|note| {
                        view! {
                            <Section title=l!("sales_orders.note")>
                                <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                            </Section>
                        }
                    })}

                <Section title=l!("sales_orders.net") flush=true>
                    <div class="flex items-baseline justify-between gap-4 text-sm font-medium tabular-nums">
                        <span class="text-content">
                            {l!("sales_orders.net")} " "
                            <span class="text-2xs text-content-subtle">{code}</span>
                        </span>
                        <span class="text-content">{net}</span>
                    </div>
                </Section>
            </div>

            // Two conditions, and they are different questions. The state decides
            // whether the act means anything; the permission decides whether this
            // reader may do it - and the service checks it again.
            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=move || is_open && may_cancel.get() fallback=|| ()>
                    <GhostButton
                        label=l!("sales_orders.cancel")
                        icon=Icon::Ban
                        on_click=Callback::new(move |()| cancel())
                    />
                </Show>

                <Show when=move || is_open && may_close.get() fallback=|| ()>
                    <GhostButton
                        label=l!("sales_orders.close")
                        icon=Icon::Check
                        on_click=Callback::new(move |()| close())
                    />
                </Show>

                <Show when=move || can_be_delivered && may_ship.get() fallback=|| ()>
                    <PrimaryButton
                        label=l!("sales_orders.ship")
                        icon=Icon::Truck
                        on_click=Callback::new({
                            let ship = ship.clone();
                            move |()| ship()
                        })
                    />
                </Show>
            </div>

            <OutstandingPanel order_id=id />

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::SALES_ORDER id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}

/// What this order still owes, and what has already gone.
///
/// Only drawn where something is outstanding. An order everything has left on
/// is complete, and a panel saying "nothing" on every finished order is
/// furniture - the same rule the purchase order's allocation panel follows.
#[component]
fn outstanding_panel(order_id: Uuid) -> impl IntoView {
    let outstanding = Resource::new(
        move || order_id,
        |order_id| async move { order_outstanding(order_id).await.ok().flatten() },
    );

    view! {
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                let Some(outstanding) = outstanding.await else {
                    return ().into_any();
                };

                view! {
                    <Section title=l!("deliveries.outstanding")>
                        <table class="w-full text-sm">
                            <tbody>
                                {outstanding
                                    .lines
                                    .into_iter()
                                    .map(|line| {
                                        let quantity = line.outstanding.to_display_string();

                                        view! {
                                            <tr class="border-b border-edge/60">
                                                <td class="py-1.5 text-content">
                                                    {line.description}
                                                    <div class="text-2xs text-content-subtle">
                                                        {line.variant_code}
                                                    </div>
                                                </td>
                                                <td class="py-1.5 pl-3 text-right tabular-nums text-content">
                                                    {quantity}
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    </Section>
                }
                    .into_any()
            })}
        </Transition>
    }
}
