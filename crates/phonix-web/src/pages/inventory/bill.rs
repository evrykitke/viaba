//! Supplier bills: the list, the editor, and the document.
//!
//! The match is shown before posting, not after refusing it. A grade that does
//! not clear puts a reason box on the screen with the reason for it beside it,
//! so somebody decides with the facts in front of them.

use app_inventory::bill::{
    Bill, BillInput, BillLineInput, BillState, MatchGrade, UnbilledReceipt,
};
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
use crate::server_fns::inventory_fns::{
    bill_against_order, bill_detail, bill_match, blank_bill, cancel_bill, delete_bill,
    pickable_variants, post_bill, save_bill, unbilled_receipts,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;
use crate::ui::table::DataGrid;
use crate::ui::table::config::bills::bills_grid;

const BACK: &str = "/inventory/bills";

#[component]
pub fn bills_page() -> impl IntoView {
    view! {
        <Title text=format!("{} | Phonix", l!("bills.title")) />

        <PageHeader
            title=l!("bills.title")
            subtitle=l!("bills.subtitle")
            icon=Icon::Receipt
        />

        <DataGrid config=bills_grid() />
    }
}

/// Keying one. `?order=<id>` prefills everything that order still owes.
#[component]
pub fn bill_new_page() -> impl IntoView {
    let query = leptos_router::hooks::use_query_map();
    let against = move || {
        query.with(|query| query.get("order").and_then(|raw| raw.parse::<Uuid>().ok()))
    };

    let prefilled = Resource::new(against, |order_id| async move {
        match order_id {
            None => blank_bill().await.map_err(|err| err.to_string()),
            Some(id) => match bill_against_order(id).await {
                Ok(Submission::Saved(draft)) => Ok(draft),
                Ok(Submission::Rejected(errors)) => Err(errors
                    .first()
                    .map(|error| crate::i18n::t(&error.message))
                    .unwrap_or_else(|| l!("bills.error.nothing_to_bill"))),
                Err(err) => Err(err.to_string()),
            },
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("bills.new")) />

        <PageHeader
            title=l!("bills.new")
            subtitle=l!("bills.new.subtitle")
            icon=Icon::Receipt
            back=(BACK, l!("bills.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match prefilled.await {
                    Ok(draft) => view! { <BillEditor draft=draft grade=None /> }.into_any(),
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
pub fn bill_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let bill_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let bill = Resource::new(bill_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => bill_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a bill id.")),
        }
    });

    // Fetched beside the bill rather than inside the editor: the document view
    // shows it too, and a posted bill's grade is part of its record.
    let grade = Resource::new(bill_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => bill_match(id).await.ok(),
            Err(_) => None,
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.bill.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let grade = grade.await;

                match bill.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let supplier = stored.supplier.name.clone();
                        let state = stored.state;
                        let opened_on = BillInput::from_bill(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=supplier
                                    icon=Icon::Receipt
                                    back=(BACK, l!("bills.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! { <BillEditor draft=opened_on grade=grade /> }.into_any()
                                } else {
                                    view! { <BillDocument bill=stored grade=grade /> }.into_any()
                                }}
                            </>
                        }
                            .into_any()
                    }
                    Err(err) => {
                        view! {
                            <>
                                <PageHeader
                                    title=l!("entity.bill.singular")
                                    icon=Icon::Receipt
                                    back=(BACK, l!("bills.title"))
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

/// Goods received and not yet billed, by age. The aged GRNI balance.
#[component]
pub fn unbilled_page() -> impl IntoView {
    let rows = Resource::new(|| (), |()| async move { unbilled_receipts().await });

    view! {
        <Title text=format!("{} | Phonix", l!("bills.unbilled.title")) />

        <PageHeader
            title=l!("bills.unbilled.title")
            subtitle=l!("bills.unbilled.subtitle")
            icon=Icon::Clock
            back=(BACK, l!("bills.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let rows = rows.await.unwrap_or_default();

                if rows.is_empty() {
                    return view! {
                        <Panel>
                            <div class="py-8 text-center">
                                <p class="text-sm font-medium text-content">
                                    {l!("bills.unbilled.empty.title")}
                                </p>
                                <p class="mt-1 text-xs text-content-subtle">
                                    {l!("bills.unbilled.empty.detail")}
                                </p>
                            </div>
                        </Panel>
                    }
                        .into_any();
                }

                view! { <UnbilledTable rows=rows /> }.into_any()
            })}
        </Transition>
    }
}

#[component]
fn unbilled_table(rows: Vec<UnbilledReceipt>) -> impl IntoView {
    view! {
        <Panel title=l!("bills.unbilled.title")>
            <div class="overflow-x-auto">
                <table class="w-full min-w-[44rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="py-2 font-medium">{l!("field.number")}</th>
                            <th class="py-2 font-medium">{l!("purchase_orders.supplier")}</th>
                            <th class="py-2 font-medium">{l!("receipts.order")}</th>
                            <th class="py-2 font-medium">{l!("receipts.received_on")}</th>
                            <th class="py-2 font-medium">{l!("bills.age")}</th>
                            <th class="py-2 text-right font-medium">
                                {l!("bills.unbilled.value")}
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {rows
                            .into_iter()
                            .map(|row| {
                                let bucket = row.bucket();
                                let tone = match bucket {
                                    app_inventory::bill::AgeBucket::Current => Tone::Neutral,
                                    app_inventory::bill::AgeBucket::ThirtyDays => Tone::Warning,
                                    _ => Tone::Danger,
                                };
                                let href = format!("/inventory/receipts/{}", row.receipt_id);
                                let value = row.unbilled.to_display_string();

                                view! {
                                    <tr class="border-b border-edge/60">
                                        <td class="py-1.5">
                                            <leptos_router::components::A
                                                href=href
                                                attr:class="font-mono text-xs text-accent hover:underline"
                                            >
                                                {row.number.clone()}
                                            </leptos_router::components::A>
                                        </td>
                                        <td class="py-1.5 text-content">
                                            {row.supplier_name.clone()}
                                        </td>
                                        <td class="py-1.5 text-xs text-content-muted">
                                            {row.order_number.clone().unwrap_or_default()}
                                        </td>
                                        <td class="py-1.5 tabular-nums text-content-muted">
                                            {row.received_on.to_string()}
                                        </td>
                                        <td class="py-1.5">
                                            <Badge
                                                label=crate::i18n::t(&bucket.label())
                                                tone=tone
                                            />
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
    }
}

#[component]
fn state_badge(state: BillState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        BillState::Draft => Tone::Neutral,
        BillState::Posted => Tone::Success,
        BillState::Cancelled => Tone::Warning,
    };

    view! { <Badge label=label tone=tone /> }
}

#[component]
fn match_panel(grade: MatchGrade) -> impl IntoView {
    let tone = match grade {
        MatchGrade::Clean => Tone::Success,
        MatchGrade::WithinTolerance | MatchGrade::NoOrder => Tone::Neutral,
        MatchGrade::OverReceived | MatchGrade::Circular | MatchGrade::SameHand => Tone::Danger,
        _ => Tone::Warning,
    };

    view! {
        <Panel title=l!("bills.match")>
            <div class="space-y-1">
                <Badge label=crate::i18n::t(&grade.label()) tone=tone />
                <p class="text-xs text-content-muted">{crate::i18n::t(&grade.detail())}</p>
            </div>
        </Panel>
    }
}

// --- the editor ---------------------------------------------------------

#[component]
fn bill_editor(draft: BillInput, grade: Option<MatchGrade>) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let suppliers = Resource::new(
        || (),
        |()| async move { list_parties(Some(roles::SUPPLIER.to_owned())).await },
    );
    let variants = Resource::new(|| (), |()| async move { pickable_variants().await });

    view! {
        <div class="space-y-3">
            <Notice message=Signal::derive(move || rejected.get()) tone=Tone::Danger />

            {grade.map(|grade| view! { <MatchPanel grade=grade /> })}

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    let suppliers = suppliers.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();

                    view! {
                        <EditorBody
                            draft=draft
                            suppliers=suppliers
                            variants=variants
                            grade=grade
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
    draft: RwSignal<BillInput>,
    suppliers: Vec<PartySummary>,
    variants: Vec<VariantChoice>,
    grade: Option<MatchGrade>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let variants = StoredValue::new(variants);
    let match_note = RwSignal::new(String::new());
    let needs_reason = grade.is_some_and(MatchGrade::needs_override);

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_bill(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("bills.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/bills/{id}"),
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
            let reason = match_note.get_untracked();

            alerts.ask(
                Confirm::new(l!("bills.post.confirm"), move || {
                    let navigate = navigate.clone();
                    let reason = reason.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let reason = (!reason.trim().is_empty()).then_some(reason);
                        let result = post_bill(id, reason).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(bill)) => {
                                alerts.post(
                                    Alert::success(l!("bills.posted", number = bill.number))
                                        .titled(l!("bills.post")),
                                );
                                navigate(
                                    &format!("/inventory/bills/{id}"),
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
                .titled(l!("bills.post"))
                .confirm_label(l!("bills.post")),
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
                Confirm::new(l!("bills.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_bill(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("bills.deleted")));
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
                Confirm::new(l!("bills.cancel.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match cancel_bill(id).await {
                            Ok(Submission::Saved(())) => {
                                alerts.post(Alert::success(l!("bills.cancelled")));
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
                .titled(l!("bills.cancel"))
                .confirm_label(l!("bills.cancel")),
            );
        }
    };

    let saved = move || draft.with(|d| d.id.is_some());
    let blocked = move || needs_reason && match_note.with(|note| note.trim().is_empty());

    view! {
        <Panel>
            <Section title=l!("bills.header")>
                <HeaderFields draft=draft suppliers=suppliers />
            </Section>

            <Section title=l!("bills.lines") description=l!("bills.lines.help")>
                <LineTable draft=draft variants=variants />
            </Section>

            <Section title=l!("bills.note")>
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

            // Only where the grade did not clear. A reason box on every bill
            // teaches people to type "ok" into it.
            <Show when=move || needs_reason fallback=|| ()>
                <Section title=l!("bills.match_note")>
                    <textarea
                        class="w-full"
                        rows="2"
                        prop:value=move || match_note.get()
                        on:input=move |ev| match_note.set(event_target_value(&ev))
                    />
                    <p class="mt-1 text-2xs text-content-subtle">
                        {l!("bills.match_note.help")}
                    </p>
                </Section>
            </Show>

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
                        label=l!("bills.cancel")
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
                        label=l!("bills.post")
                        icon=Icon::Check
                        pending=Signal::derive(move || saving.get())
                        disabled=Signal::derive(blocked)
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
fn header_fields(draft: RwSignal<BillInput>, suppliers: Vec<PartySummary>) -> impl IntoView {
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
                <label for="bill-supplier" class="block text-xs font-medium text-content-muted">
                    {l!("purchase_orders.supplier")}
                </label>
                <SelectField
                    id="bill-supplier"
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

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">{l!("bills.reference")}</span>
                <input
                    type="text"
                    class="w-full font-mono text-xs"
                    prop:value=move || draft.with(|d| d.supplier_reference.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.supplier_reference = value);
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("bills.reference.help")}
                </span>
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">{l!("bills.dated")}</span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || draft.with(|d| d.bill_date.to_string())
                    on:change=move |ev| {
                        if let Ok(date) = event_target_value(&ev).parse() {
                            draft.update(|d| d.bill_date = date);
                        }
                    }
                />
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">{l!("bills.due")}</span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || {
                        draft.with(|d| d.due_on.map(|due| due.to_string()).unwrap_or_default())
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.due_on = value.parse().ok());
                    }
                />
            </label>
        </div>
    }
}

#[component]
fn line_table(
    draft: RwSignal<BillInput>,
    variants: StoredValue<Vec<VariantChoice>>,
) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[46rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("purchase_orders.item")}</th>
                            <th class="py-2 font-medium">{l!("field.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">
                                {l!("field.quantity")}
                            </th>
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
                                    view! { <LineRow draft=draft index=index variants=variants /> }
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("bills.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(BillLineInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<BillInput>,
    index: usize,
    variants: StoredValue<Vec<VariantChoice>>,
) -> impl IntoView {
    let field = move |read: fn(&BillLineInput) -> String| {
        draft.with(|d| d.lines.get(index).map(read).unwrap_or_default())
    };

    // A line that clears a receipt names its item already; changing it would
    // break the thread back to the accrual.
    let from_receipt = move || {
        draft.with(|d| {
            d.lines
                .get(index)
                .is_some_and(|line| line.receipt_line_id.is_some())
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
                    // Empty is a charge line - freight, a deposit, a rebate -
                    // and those are real bills, so it stays clearable.
                    placeholder=l!("common.not_set")
                    clearable=true
                    disabled=Signal::derive(from_receipt)
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
                                if d.lines.is_empty() {
                                    d.lines.push(BillLineInput::blank());
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
fn bill_document(bill: Bill, grade: Option<MatchGrade>) -> impl IntoView {
    let id = bill.id;
    let supplier_name = bill.supplier.name.clone();
    let supplier_code = bill.supplier.code.clone();
    let reference = bill.supplier_reference.clone();
    let order = bill
        .order_id
        .zip(bill.order_number.clone())
        .map(|(id, number)| (format!("/inventory/orders/{id}"), number));
    let dated = bill.bill_date.to_string();
    let due = bill.due_on.map(|due| due.to_string());
    let note = bill.note.clone();
    let match_note = bill.match_note.clone();
    let currency = bill.currency.clone();
    let net = bill.net.to_display_string();
    let accrued = bill.accrued.to_display_string();
    let variance = bill.variance.to_display_string();
    let has_variance = !bill.variance.is_zero();
    let lines = bill.lines.clone();

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("purchase_orders.supplier") flush=true>
                    <div class="space-y-1 text-sm">
                        <div class="font-medium text-content">{supplier_name}</div>
                        <code class="text-2xs text-content-subtle">{supplier_code}</code>
                        <div class="text-xs text-content-muted">
                            {l!("bills.reference")} ": "
                            <span class="font-mono">{reference}</span>
                        </div>
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
                </Section>

                {grade.map(|grade| view! { <MatchPanel grade=grade /> })}
            </div>

            <Section title=l!("bills.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[44rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">{l!("field.description")}</th>
                                <th class="py-2 text-right font-medium">{l!("field.quantity")}</th>
                                <th class="py-2 text-right font-medium">
                                    {l!("purchase_orders.unit_price")}
                                </th>
                                <th class="py-2 text-right font-medium">{l!("bills.accrued")}</th>
                                <th class="py-2 text-right font-medium">{l!("bills.net")}</th>
                            </tr>
                        </thead>
                        <tbody>
                            {lines
                                .into_iter()
                                .map(|line| {
                                    let quantity = match &line.unit_code {
                                        Some(code) => {
                                            format!(
                                                "{} {code}",
                                                line.quantity.to_display_string(),
                                            )
                                        }
                                        None => line.quantity.to_display_string(),
                                    };

                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-subtle">
                                                {line.line_no}
                                            </td>
                                            <td class="py-1.5 text-content">
                                                {line.description.clone()}
                                                <div class="text-2xs text-content-subtle">
                                                    {line.variant_code.clone().unwrap_or_default()}
                                                </div>
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {quantity}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {line.unit_price.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {line.accrued.to_display_string()}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {line.net.to_display_string()}
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
                <div class="space-y-3">
                    {note
                        .map(|note| {
                            view! {
                                <Section title=l!("bills.note")>
                                    <p class="whitespace-pre-wrap text-sm text-content-muted">
                                        {note}
                                    </p>
                                </Section>
                            }
                        })}

                    // The override stays on the document, permanently, where the
                    // next person reads it.
                    {match_note
                        .map(|reason| {
                            view! {
                                <Section title=l!("bills.match_note")>
                                    <p class="whitespace-pre-wrap text-sm text-content-muted">
                                        {reason}
                                    </p>
                                </Section>
                            }
                        })}
                </div>

                <Section title=l!("bills.header") flush=true>
                    <dl class="space-y-1 text-sm tabular-nums">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("bills.dated")}</dt>
                            <dd class="text-content">{dated}</dd>
                        </div>
                        {due
                            .map(|due| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">{l!("bills.due")}</dt>
                                        <dd class="text-content">{due}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("bills.accrued")}</dt>
                            <dd class="text-content">{accrued}</dd>
                        </div>
                        {has_variance
                            .then(|| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">{l!("bills.variance")}</dt>
                                        <dd class="text-warning">{variance}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex justify-between gap-4 border-t border-edge pt-1 font-medium">
                            <dt class="text-content">
                                {l!("bills.net")} " "
                                <span class="text-2xs text-content-subtle">{currency}</span>
                            </dt>
                            <dd class="text-content">{net}</dd>
                        </div>
                    </dl>
                </Section>
            </div>

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::BILL id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}
