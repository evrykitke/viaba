//! Landed costs: the list, the editor, and the document.
//!
//! The editor shows the delivery's own lines beside the charges, because the
//! question somebody actually has - "which of these is the freight going to
//! land on, and how much of it is already sold" - is answered by the cartons
//! rather than by the amount. A line whose stock has gone shows as such before
//! posting, not after somebody wonders why margin moved.

use app_inventory::landed_cost::{
    AllocationBasis, ChargeInput, Landable, LandedCost, LandedCostInput, LandedCostState,
};
use app_inventory::receipt::ReceiptSummary;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    blank_landed_cost, cancel_landed_cost, delete_landed_cost, edit_landed_cost,
    landed_cost_against, landed_cost_detail, landed_cost_lines, list_receipts, post_landed_cost,
    save_landed_cost,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;
use crate::ui::table::DataGrid;
use crate::ui::table::config::landed_costs::landed_costs_grid;

const BACK: &str = "/inventory/landed-costs";

#[component]
pub fn landed_costs_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("landed_costs.title")) />

        <PageHeader
            title=l!("landed_costs.title")
            subtitle=l!("landed_costs.subtitle")
            icon=Icon::Truck
        />

        <DataGrid config=landed_costs_grid() />
    }
}

/// Keying one. `?receipt=<id>` opens it against a delivery.
#[component]
pub fn landed_cost_new_page() -> impl IntoView {
    let query = leptos_router::hooks::use_query_map();
    let against = move || {
        query.with(|query| query.get("receipt").and_then(|raw| raw.parse::<Uuid>().ok()))
    };

    let prefilled = Resource::new(against, |receipt_id| async move {
        match receipt_id {
            None => blank_landed_cost().await.map_err(|err| err.to_string()),
            Some(id) => match landed_cost_against(id).await {
                Ok(Submission::Saved(draft)) => Ok(draft),
                Ok(Submission::Rejected(errors)) => Err(errors
                    .first()
                    .map(|error| crate::i18n::t(&error.message))
                    .unwrap_or_else(|| l!("landed_costs.error.receipt_not_posted"))),
                Err(err) => Err(err.to_string()),
            },
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("landed_costs.new")) />

        <PageHeader
            title=l!("landed_costs.new")
            subtitle=l!("landed_costs.new.subtitle")
            icon=Icon::Truck
            back=(BACK, l!("landed_costs.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match prefilled.await {
                    Ok(draft) => view! { <LandedCostEditor draft=draft /> }.into_any(),
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

#[component]
pub fn landed_cost_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let landed_cost_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let document = Resource::new(landed_cost_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => landed_cost_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a landed cost id.")),
        }
    });

    let draft = Resource::new(landed_cost_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => edit_landed_cost(id).await.ok(),
            Err(_) => None,
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.landed_cost.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let opened_on = draft.await;

                match document.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let subtitle = format!(
                            "{} · {}",
                            stored.supplier_name,
                            stored.receipt_number,
                        );
                        let state = stored.state;

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=subtitle
                                    icon=Icon::Truck
                                    back=(BACK, l!("landed_costs.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {match (state.is_editable(), opened_on) {
                                    (true, Some(draft)) => {
                                        view! { <LandedCostEditor draft=draft /> }.into_any()
                                    }
                                    _ => {
                                        view! { <LandedCostDocument document=stored /> }.into_any()
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
                                    title=l!("entity.landed_cost.singular")
                                    icon=Icon::Truck
                                    back=(BACK, l!("landed_costs.title"))
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
fn state_badge(state: LandedCostState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        LandedCostState::Draft => Tone::Neutral,
        LandedCostState::Done => Tone::Success,
        LandedCostState::Cancelled => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

#[component]
fn landed_cost_editor(draft: LandedCostInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let receipts = Resource::new(|| (), |()| async move { list_receipts().await });

    // Re-read whenever the delivery changes: the cartons on the screen have to
    // be the cartons the charge will land on.
    let chosen = Signal::derive(move || draft.with(|d| d.receipt_id));
    let lines = Resource::new(
        move || chosen.get(),
        |receipt_id| async move {
            match receipt_id {
                None => Vec::new(),
                Some(id) => landed_cost_lines(id).await.unwrap_or_default(),
            }
        },
    );

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let receipts = receipts.await.unwrap_or_default();

                    view! {
                        <EditorBody
                            draft=draft
                            receipts=receipts
                            saving=saving
                            rejected=rejected
                        />
                    }
                })}
            </Transition>

            <Panel
                title=l!("landed_costs.landing_on")
                description=l!("landed_costs.landing_on.help")
            >
                <Transition fallback=|| {
                    view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
                }>
                    {move || Suspend::new(async move {
                        view! { <LandableTable lines=lines.await /> }
                    })}
                </Transition>
            </Panel>
        </div>
    }
}

#[component]
fn editor_body(
    draft: RwSignal<LandedCostInput>,
    receipts: Vec<ReceiptSummary>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    // Only a posted delivery has layers to land on, so an unposted one is not
    // offered rather than offered and refused.
    let receipt_options = receipts
        .iter()
        .filter(|receipt| receipt.state.is_posted())
        .map(|receipt| {
            Choice::new(receipt.id.to_string(), receipt.number.clone())
                .detail(format!("{} · {}", receipt.supplier_name, receipt.received_on))
        })
        .collect::<Vec<_>>();

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_landed_cost(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("landed_costs.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/landed-costs/{id}"),
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
                Confirm::new(l!("landed_costs.post.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = post_landed_cost(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(document)) => {
                                alerts.post(
                                    Alert::success(
                                            l!("landed_costs.posted", number = document.number),
                                        )
                                        .titled(l!("landed_costs.post")),
                                );
                                navigate(
                                    &format!("/inventory/landed-costs/{id}"),
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
                .titled(l!("landed_costs.post"))
                .confirm_label(l!("landed_costs.post")),
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
                Confirm::new(l!("landed_costs.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_landed_cost(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("landed_costs.deleted")));
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
                Confirm::new(l!("landed_costs.cancel.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match cancel_landed_cost(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("landed_costs.cancelled")));
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
                .titled(l!("landed_costs.cancel"))
                .confirm_label(l!("landed_costs.cancel")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());

    view! {
        <Panel>
            <Section title=l!("landed_costs.header")>
                <div class="grid gap-3 sm:grid-cols-2">
                    <div class="block space-y-1">
                        <label
                            for="landed-receipt"
                            class="block text-xs font-medium text-content-muted"
                        >
                            {l!("landed_costs.receipt")}
                        </label>
                        <SelectField
                            id="landed-receipt"
                            value=Signal::derive(move || {
                                draft
                                    .with(|d| {
                                        d.receipt_id.map(|id| id.to_string()).unwrap_or_default()
                                    })
                            })
                            on_change=Callback::new(move |value: String| {
                                let chosen = value.parse::<Uuid>().ok();
                                draft.update(|d| d.receipt_id = chosen);
                            })
                            options=receipt_options
                            placeholder=l!("common.not_set")
                            clearable=true
                        />
                        <span class="block text-2xs text-content-subtle">
                            {l!("landed_costs.receipt.help")}
                        </span>
                    </div>

                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("landed_costs.dated")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.cost_date.to_string())
                            on:change=move |ev| {
                                if let Ok(date) = event_target_value(&ev).parse() {
                                    draft.update(|d| d.cost_date = date);
                                }
                            }
                        />
                    </label>
                </div>
            </Section>

            <Section title=l!("landed_costs.charges") description=l!("landed_costs.charges.help")>
                <ChargeTable draft=draft />
            </Section>

            <Section title=l!("landed_costs.note")>
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

                <Show when=saved fallback=|| ()>
                    <GhostButton
                        label=l!("landed_costs.cancel")
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
                        label=l!("landed_costs.post")
                        icon=Icon::Check
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let post = post.clone();
                            move |()| post()
                        })
                    />
                </Show>
            </div>
        </Panel>
    }
}

#[component]
fn charge_table(draft: RwSignal<LandedCostInput>) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[38rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-40 py-2 font-medium">{l!("landed_costs.basis")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("landed_costs.amount")}
                            </th>
                            <th class="w-8 py-2"></th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let count = draft.with(|d| d.charges.len());
                            (0..count)
                                .map(|index| view! { <ChargeRow draft=draft index=index /> })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("landed_costs.charge.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.charges.push(ChargeInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn charge_row(draft: RwSignal<LandedCostInput>, index: usize) -> impl IntoView {
    let field = move |read: fn(&ChargeInput) -> String| {
        draft.with(|d| d.charges.get(index).map(read).unwrap_or_default())
    };

    // Each charge carries its own basis. Freight goes by weight and duty by
    // value, and one basis for the whole document would make one of them wrong.
    let basis_options = AllocationBasis::ALL
        .iter()
        .map(|basis| {
            Choice::new(basis.as_str().to_owned(), crate::i18n::t(&basis.label()))
                .detail(crate::i18n::t(&basis.explain()))
        })
        .collect::<Vec<_>>();

    let chosen_basis = move || {
        draft.with(|d| {
            d.charges
                .get(index)
                .map(|charge| charge.basis.as_str().to_owned())
                .unwrap_or_default()
        })
    };

    view! {
        <tr class="border-b border-edge/60">
            <td class="py-1 text-xs text-content-subtle">{index + 1}</td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    class="w-full"
                    prop:value=move || field(|charge| charge.description.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(charge) = d.charges.get_mut(index) {
                                    charge.description = value;
                                }
                            });
                    }
                />
            </td>
            <td class="py-1 pr-2">
                <SelectField
                    value=Signal::derive(chosen_basis)
                    on_change=Callback::new(move |value: String| {
                        let chosen = AllocationBasis::parse(&value);
                        draft
                            .update(|d| {
                                if let (Some(charge), Some(chosen)) =
                                    (d.charges.get_mut(index), chosen)
                                {
                                    charge.basis = chosen;
                                }
                            });
                    })
                    options=basis_options
                    label=l!("landed_costs.basis")
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=move || field(|charge| charge.amount.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(charge) = d.charges.get_mut(index) {
                                    charge.amount = value;
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
                                if index < d.charges.len() {
                                    d.charges.remove(index);
                                }
                                if d.charges.is_empty() {
                                    d.charges.push(ChargeInput::blank());
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

/// The delivery's lines, with what is left of each. The column that matters is
/// "still here": a line whose stock has gone takes its share of the freight
/// straight to cost of sales.
#[component]
fn landable_table(lines: Vec<Landable>) -> impl IntoView {
    if lines.is_empty() {
        return view! {
            <p class="text-sm text-content-subtle">{l!("landed_costs.landing_on.empty")}</p>
        }
        .into_any();
    }

    view! {
        <div class="overflow-x-auto">
            <table class="w-full min-w-[40rem] text-sm">
                <thead>
                    <tr class="border-b border-edge text-left text-xs text-content-muted">
                        <th class="py-2 font-medium">{l!("field.description")}</th>
                        <th class="py-2 text-right font-medium">{l!("landed_costs.received")}</th>
                        <th class="py-2 text-right font-medium">{l!("landed_costs.still_here")}</th>
                        <th class="py-2 text-right font-medium">{l!("landed_costs.line_value")}</th>
                        <th class="py-2 text-right font-medium">{l!("landed_costs.weight")}</th>
                    </tr>
                </thead>
                <tbody>
                    {lines
                        .into_iter()
                        .map(|line| {
                            let spent = !line.remaining.is_positive();
                            let remaining_class = if spent {
                                "py-1.5 text-right tabular-nums text-warning"
                            } else {
                                "py-1.5 text-right tabular-nums text-content"
                            };
                            let weight = line
                                .weight_grams
                                .map(|grams| format!("{grams} g"))
                                .unwrap_or_else(|| "—".to_owned());

                            view! {
                                <tr class="border-b border-edge/60">
                                    <td class="py-1.5 text-content">
                                        {line.description.clone()}
                                        <div class="text-2xs text-content-subtle">
                                            {line.variant_code.clone()}
                                        </div>
                                    </td>
                                    <td class="py-1.5 text-right tabular-nums text-content-muted">
                                        {line.quantity.to_display_string()}
                                    </td>
                                    <td class=remaining_class>
                                        {line.remaining.to_display_string()}
                                    </td>
                                    <td class="py-1.5 text-right tabular-nums text-content-muted">
                                        {line.value.to_display_string()}
                                    </td>
                                    <td class="py-1.5 text-right tabular-nums text-content-subtle">
                                        {weight}
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

// --- the document -------------------------------------------------------

#[component]
fn landed_cost_document(document: LandedCost) -> impl IntoView {
    let id = document.id;
    let supplier = document.supplier_name.clone();
    let receipt_href = format!("/inventory/receipts/{}", document.receipt_id);
    let receipt_number = document.receipt_number.clone();
    let dated = document.cost_date.to_string();
    let note = document.note.clone();
    let journal = document.journal_number.clone();
    let total = document.total.to_display_string();
    let capitalised = document.capitalised.to_display_string();
    let expensed = document.expensed.to_display_string();
    let has_expensed = !document.expensed.is_zero();
    let charges = document.charges.clone();
    let allocations = document.allocations.clone();

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("landed_costs.receipt") flush=true>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{supplier}</div>
                        <div class="text-xs">
                            <leptos_router::components::A
                                href=receipt_href
                                attr:class="font-mono text-accent hover:underline"
                            >
                                {receipt_number}
                            </leptos_router::components::A>
                        </div>
                        <div class="text-xs text-content-muted">
                            {l!("landed_costs.dated")} ": " {dated}
                        </div>
                        {journal
                            .map(|number| {
                                view! {
                                    <div class="text-xs text-content-muted">
                                        {l!("landed_costs.journal")} ": "
                                        <span class="font-mono">{number}</span>
                                    </div>
                                }
                            })}
                    </div>
                </Section>

                <Section title=l!("landed_costs.what_it_did") flush=true
                    description=l!("landed_costs.what_it_did.help")
                >
                    <dl class="space-y-1 text-sm tabular-nums">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("landed_costs.capitalised")}</dt>
                            <dd class="text-content">{capitalised}</dd>
                        </div>
                        {has_expensed
                            .then(|| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("landed_costs.expensed")}
                                        </dt>
                                        <dd class="text-warning">{expensed}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex justify-between gap-4 border-t border-edge pt-1 font-medium">
                            <dt class="text-content">{l!("landed_costs.total")}</dt>
                            <dd class="text-content">{total}</dd>
                        </div>
                    </dl>
                </Section>
            </div>

            <Section title=l!("landed_costs.charges")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[34rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 font-medium">{l!("landed_costs.basis")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("landed_costs.amount")}
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            {charges
                                .into_iter()
                                .map(|charge| {
                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-subtle">
                                                {charge.line_no}
                                            </td>
                                            <td class="py-1.5 text-content">
                                                {charge.description.clone()}
                                            </td>
                                            <td class="py-1.5 text-xs text-content-muted">
                                                {crate::i18n::t(&charge.basis.label())}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {charge.amount.to_display_string()}
                                            </td>
                                        </tr>
                                    }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </Section>

            <Section
                title=l!("landed_costs.allocations")
                description=l!("landed_costs.allocations.help")
            >
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[44rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="py-2 font-medium">{l!("landed_costs.charge")}</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("landed_costs.basis_amount")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("landed_costs.capitalised")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("landed_costs.expensed")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("landed_costs.amount")}
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            {allocations
                                .into_iter()
                                .map(|row| {
                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-muted">
                                                {row.charge_description.clone()}
                                            </td>
                                            <td class="py-1.5 text-content">
                                                {row.description.clone()}
                                                <div class="text-2xs text-content-subtle">
                                                    {row.variant_code.clone()}
                                                </div>
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-subtle">
                                                {row.basis_amount.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {row.capitalised.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {row.expensed.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {row.amount.to_display_string()}
                                            </td>
                                        </tr>
                                    }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </Section>

            {note
                .map(|note| {
                    view! {
                        <Section title=l!("landed_costs.note")>
                            <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                        </Section>
                    }
                })}

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::LANDED_COST id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}
