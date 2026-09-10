//! One consolidation: the editor while it is a draft, the document once the
//! orders have been raised.
//!
//! # The new-consolidation screen is a warehouse picker first
//!
//! A blank consolidation is not much use - a buyer starting one wants what is
//! already waiting, not an empty table. So the new screen asks for a warehouse
//! and then fills itself from `requisition_demand`: one line per item, quantity
//! equal to the outstanding demand. Editing it down, rounding it up to case
//! sizes, and choosing suppliers is the buyer's work; the typing is not.
//!
//! Changing the warehouse redraws the table from that warehouse's demand, so
//! the picker sits on the unsaved screen only. Once the draft is saved the
//! warehouse is fixed for the life of the document - the lines are already
//! sourced against it, and moving them would be an order that cannot be
//! received where it was raised.
//!
//! # Every line says how far it is from the demand
//!
//! `beyond_demand` is what nobody asked for and `short_of_demand` is what is
//! being left outstanding. Neither is an error - rounding up to a case and
//! buying half of what was asked for are both ordinary purchasing decisions -
//! but both are things a buyer should be able to see without arithmetic.
//!
//! A line whose demand has moved since it was drawn is marked too. A draft
//! written last week and confirmed today allocates against what is outstanding
//! *now*, so a requisition withdrawn in between changes what the button will
//! do, and the buyer should see that before pressing it.
//!
//! # Confirming says how many orders it will raise
//!
//! Because that is the thing a single Confirm button otherwise hides. One
//! consolidation becomes one purchase order per supplier, and "this will create
//! three orders" is what a buyer most wants to know before agreeing to it.

use app_inventory::consolidation::{
    Consolidation, ConsolidationInput, ConsolidationLineInput, ConsolidationState,
};
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::permissions;
use phonix_master::party::{PartySummary, roles};
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    cancel_consolidation, confirm_consolidation, consolidation_detail, consolidation_from_demand,
    delete_consolidation, pickable_variants, save_consolidation, selectable_warehouses,
};
use crate::server_fns::master_fns::list_parties;
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const BACK: &str = "/inventory/consolidations";

#[component]
pub fn consolidation_new_page() -> impl IntoView {
    // Nothing is drawn until a warehouse is chosen: demand is per warehouse,
    // and a table gathered across two buildings is one that cannot be received
    // in either.
    let chosen = RwSignal::new(None::<Uuid>);
    let warehouses = Resource::new(|| (), |()| async move { selectable_warehouses().await });
    let drawn = Resource::new(
        move || chosen.get(),
        |warehouse_id| async move { consolidation_from_demand(warehouse_id).await },
    );

    view! {
        <Title text=format!("{} | Phonix", l!("consolidations.new")) />

        <PageHeader
            title=l!("consolidations.new")
            subtitle=l!("consolidations.new.subtitle")
            icon=Icon::Boxes
            back=(BACK, l!("consolidations.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                let warehouses = warehouses.await.unwrap_or_default();
                let options = warehouses
                    .into_iter()
                    .filter(|warehouse| warehouse.is_active)
                    .map(|warehouse| {
                        Choice::new(warehouse.id.to_string(), warehouse.name).detail(warehouse.code)
                    })
                    .collect::<Vec<_>>();

                match drawn.await {
                    Ok(draft) => {
                        let empty = draft.lines.is_empty() && chosen.get_untracked().is_some();

                        view! {
                            <div class="space-y-3">
                                <Panel
                                    title=l!("consolidations.warehouse")
                                    description=l!("consolidations.warehouse.help")
                                >
                                    <SelectField
                                        id="con-warehouse"
                                        value=Signal::derive(move || {
                                            chosen
                                                .get()
                                                .map(|id| id.to_string())
                                                .unwrap_or_default()
                                        })
                                        on_change=Callback::new(move |value: String| {
                                            chosen.set(value.parse::<Uuid>().ok());
                                        })
                                        options=options
                                        placeholder=l!("common.not_set")
                                        clearable=true
                                        label=l!("consolidations.warehouse")
                                    />
                                </Panel>

                                {if empty {
                                    view! {
                                        <Notice
                                            message=Signal::derive(move || {
                                                Some(l!("consolidations.demand.empty"))
                                            })
                                            tone=Tone::Neutral
                                        />
                                    }
                                        .into_any()
                                } else if draft.warehouse_id.is_some() {
                                    view! { <ConsolidationEditor draft=draft /> }.into_any()
                                } else {
                                    ().into_any()
                                }}
                            </div>
                        }
                            .into_any()
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

/// One consolidation: editable while it is a draft, a document after.
#[component]
pub fn consolidation_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let consolidation_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let consolidation = Resource::new(consolidation_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => consolidation_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a consolidation id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.consolidation.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match consolidation.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let subtitle = stored.warehouse_name.clone();
                        let state = stored.state;
                        let opened_on = ConsolidationInput::from_consolidation(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=subtitle
                                    icon=Icon::Boxes
                                    back=(BACK, l!("consolidations.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! {
                                        <ConsolidationEditor draft=opened_on />
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <ConsolidationDocument consolidation=stored />
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
                                    title=l!("entity.consolidation.singular")
                                    icon=Icon::Boxes
                                    back=(BACK, l!("consolidations.title"))
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
fn state_badge(state: ConsolidationState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    let tone = match state {
        // A draft is the one state that is somebody's outstanding work.
        ConsolidationState::Draft => Tone::Warning,
        ConsolidationState::Confirmed => Tone::Success,
        ConsolidationState::Cancelled => Tone::Neutral,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

#[component]
fn consolidation_editor(draft: ConsolidationInput) -> impl IntoView {
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

            <Transition fallback=|| {
                view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
            }>
                {move || Suspend::new(async move {
                    // Empty pickers rather than a failed screen. A workspace
                    // with no suppliers gets a form it cannot confirm, which is
                    // the honest state of affairs.
                    let suppliers = suppliers.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();

                    view! {
                        <EditorBody
                            draft=draft
                            suppliers=suppliers
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
    draft: RwSignal<ConsolidationInput>,
    suppliers: Vec<PartySummary>,
    variants: Vec<VariantChoice>,
    saving: RwSignal<bool>,
    rejected: RwSignal<Option<String>>,
) -> impl IntoView {
    let alerts = Alerts::get();
    let navigate = leptos_router::hooks::use_navigate();

    let variants = StoredValue::new(variants);
    let supplier_options = StoredValue::new(
        suppliers
            .iter()
            .filter(|party| party.is_active)
            .map(|party| {
                Choice::new(party.id.to_string(), party.name.clone()).detail(party.code.clone())
            })
            .collect::<Vec<_>>(),
    );

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_consolidation(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("consolidations.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/consolidations/{id}"),
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
                Confirm::new(l!("consolidations.confirm.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = confirm_consolidation(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(stored)) => {
                                alerts.post(
                                    Alert::success(
                                            l!(
                                                "consolidations.confirmed_as", number = stored
                                                .number
                                            ),
                                        )
                                        .titled(l!("consolidations.confirm")),
                                );
                                // Reload the route: it is a document now, and
                                // this screen draws a different thing for one.
                                navigate(
                                    &format!("/inventory/consolidations/{id}"),
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
                .titled(l!("consolidations.confirm"))
                .confirm_label(l!("consolidations.confirm")),
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
                Confirm::new(l!("consolidations.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_consolidation(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("consolidations.deleted")));
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

    // How many orders confirming would raise, worked out from what is on the
    // form rather than from the stored document, so it follows the buyer's
    // typing. Empty is a form with nothing sourced yet.
    let order_count = move || {
        draft.with(|d| {
            let mut seen: Vec<Uuid> = Vec::new();

            for line in &d.lines {
                if let Some(supplier) = line.supplier_id {
                    if !seen.contains(&supplier) {
                        seen.push(supplier);
                    }
                }
            }

            seen.len()
        })
    };

    let unsourced = move || {
        draft.with(|d| {
            d.lines
                .iter()
                .filter(|line| line.variant_id.is_some() && line.supplier_id.is_none())
                .count()
        })
    };

    view! {
        <Panel>
            <Section title=l!("consolidations.lines") description=l!("consolidations.lines.help")>
                <LineTable draft=draft variants=variants supplier_options=supplier_options />
            </Section>

            <div class="grid gap-3 lg:grid-cols-2 lg:items-start">
                <Section title=l!("consolidations.header") flush=true>
                    <label class="block space-y-1">
                        <span class="text-xs font-medium text-content-muted">
                            {l!("consolidations.raised_on")}
                        </span>
                        <input
                            type="date"
                            class="w-full"
                            prop:value=move || draft.with(|d| d.raised_on.to_string())
                            on:change=move |ev| {
                                if let Ok(date) = event_target_value(&ev).parse() {
                                    draft.update(|d| d.raised_on = date);
                                }
                            }
                        />
                    </label>
                </Section>

                <Section title=l!("consolidations.note") flush=true>
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
            </div>

            <div class="flex flex-wrap items-center justify-between gap-2">
                // What the button is about to do, said before it is pressed.
                <p class="text-xs text-content-subtle">
                    {move || {
                        let unsourced = unsourced();
                        if unsourced > 0 {
                            l!("consolidations.error.supplier_required")
                        } else {
                            let count = order_count();
                            if count == 1 {
                                l!("consolidations.will_raise.one")
                            } else {
                                l!("consolidations.will_raise.other", count = count)
                            }
                        }
                    }}
                </p>

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

                    // Offered only once there is something saved to number, and
                    // refused while any line has nobody to buy it from - the
                    // service checks the same rule, because a disabled button is
                    // not a control.
                    <Show when=saved fallback=|| ()>
                        <PrimaryButton
                            label=l!("consolidations.confirm")
                            icon=Icon::ArrowRight
                            disabled=Signal::derive(move || unsourced() > 0)
                            pending=Signal::derive(move || saving.get())
                            on_click=Callback::new({
                                let confirm = confirm.clone();
                                move |()| confirm()
                            })
                        />
                    </Show>
                </div>
            </div>
        </Panel>
    }
}

#[component]
fn line_table(
    draft: RwSignal<ConsolidationInput>,
    variants: StoredValue<Vec<VariantChoice>>,
    supplier_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[58rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("consolidations.item")}</th>
                            <th class="py-2 font-medium">{l!("consolidations.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">
                                {l!("consolidations.demand")}
                            </th>
                            <th class="w-24 py-2 text-right font-medium">
                                {l!("consolidations.quantity")}
                            </th>
                            <th class="w-48 py-2 font-medium">{l!("consolidations.supplier")}</th>
                            <th class="w-28 py-2 text-right font-medium">
                                {l!("consolidations.unit_price")}
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
                                            supplier_options=supplier_options
                                        />
                                    }
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </div>

            <GhostButton
                label=l!("common.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft
                        .update(|d| {
                            d.lines
                                .push(ConsolidationLineInput {
                                    id: None,
                                    variant_id: None,
                                    description: String::new(),
                                    quantity: String::new(),
                                    // Nobody asked for a line the buyer added
                                    // by hand, so all of it is beyond demand.
                                    demand: String::new(),
                                    supplier_id: None,
                                    unit_price: String::new(),
                                    note: String::new(),
                                });
                        });
                })
            />
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<ConsolidationInput>,
    index: usize,
    variants: StoredValue<Vec<VariantChoice>>,
    supplier_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    // Every read goes through the index rather than a held clone, on the same
    // terms as a requisition line: a row that cached its values would stop
    // updating the moment another row was removed and the indexes shifted.
    let field = move |read: fn(&ConsolidationLineInput) -> String| {
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
                    label=l!("consolidations.item")
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
            // What was asked for, read-only: it is a snapshot of the demand when
            // the line was drawn, not something the buyer decides.
            <td class="py-1 pr-2 text-right text-xs tabular-nums text-content-muted">
                {move || field(|line| line.demand.clone())}
            </td>
            <td class="py-1 pr-2">
                // Text rather than a number input, for the reason a requisition
                // line's is: `Quantity::parse` refuses a seventh decimal place
                // rather than rounding it away.
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
                        field(|line| line.supplier_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.supplier_id = chosen;
                                }
                            });
                    })
                    options=supplier_options.get_value()
                    placeholder=l!("common.not_set")
                    clearable=true
                    label=l!("consolidations.supplier")
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    placeholder=l!("consolidations.unit_price.default")
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

/// A confirmed or cancelled consolidation: what was bought, from whom, and
/// which orders it became.
#[component]
fn consolidation_document(consolidation: Consolidation) -> impl IntoView {
    let alerts = Alerts::get();
    let viewer = crate::ui::viewer::Viewer::get();

    let may_manage = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::CONSOLIDATIONS_MANAGE))
        })
    });

    let id = consolidation.id;
    let editable = consolidation.state.is_editable();
    let warehouse = consolidation.warehouse_name.clone();
    let raised_on = consolidation.raised_on.to_string();
    let raised_by = consolidation.raised_by_name.clone();
    let note = consolidation.note.clone();
    let lines = consolidation.lines.clone();
    let orders = consolidation.orders.clone();

    let cancel = move || {
        alerts.ask(
            Confirm::new(l!("consolidations.cancel.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match cancel_consolidation(id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("consolidations.cancelled_note")));
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
            .titled(l!("consolidations.cancel"))
            .confirm_label(l!("consolidations.cancel")),
        );
    };

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("consolidations.header") flush=true>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("consolidations.warehouse")}</dt>
                            <dd class="text-content">{warehouse}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("consolidations.raised_on")}</dt>
                            <dd class="tabular-nums text-content">{raised_on}</dd>
                        </div>
                        {raised_by
                            .map(|who| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("consolidations.raised_by")}
                                        </dt>
                                        <dd class="text-content">{who}</dd>
                                    </div>
                                }
                            })}
                    </dl>
                </Section>

                <Section title=l!("consolidations.orders") flush=true>
                    {if orders.is_empty() {
                        view! {
                            <p class="text-sm text-content-subtle">
                                {l!("consolidations.orders.none")}
                            </p>
                        }
                            .into_any()
                    } else {
                        view! {
                            <ul class="space-y-1 text-sm">
                                {orders
                                    .into_iter()
                                    .map(|order| {
                                        let href = format!("/inventory/orders/{}", order.id);
                                        let net = order.net.to_display_string();

                                        view! {
                                            <li class="flex items-center justify-between gap-4">
                                                <a
                                                    href=href
                                                    class="font-mono text-brand hover:underline"
                                                >
                                                    {order.number}
                                                </a>
                                                <span class="flex-1 truncate text-content-muted">
                                                    {order.supplier_name}
                                                </span>
                                                <span class="tabular-nums text-content">{net}</span>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                            .into_any()
                    }}
                </Section>
            </div>

            <Section title=l!("consolidations.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[48rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">
                                    {l!("consolidations.description")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("consolidations.demand")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("consolidations.quantity")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("consolidations.beyond_demand")}
                                </th>
                                <th class="py-2 font-medium">{l!("consolidations.supplier")}</th>
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
                                    let demand = line.demand.to_display_string();
                                    let beyond = line.beyond_demand();
                                    let extra = beyond
                                        .is_positive()
                                        .then(|| beyond.to_display_string())
                                        .unwrap_or_default();
                                    let supplier = line
                                        .supplier
                                        .as_ref()
                                        .map(|supplier| supplier.name.clone())
                                        .unwrap_or_default();
                                    let code = line.variant_code.clone();

                                    view! {
                                        <tr class="border-b border-edge/60">
                                            <td class="py-1.5 text-xs text-content-subtle">
                                                {line.line_no}
                                            </td>
                                            <td class="py-1.5 text-content">
                                                {line.description.clone()}
                                                <div class="text-2xs text-content-subtle">
                                                    {code}
                                                </div>
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {demand}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {quantity}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {extra}
                                            </td>
                                            <td class="py-1.5 text-content-muted">{supplier}</td>
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
                        <Section title=l!("consolidations.note")>
                            <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                        </Section>
                    }
                })}

            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=move || editable && may_manage.get() fallback=|| ()>
                    <GhostButton
                        label=l!("consolidations.cancel")
                        icon=Icon::Ban
                        on_click=Callback::new(move |()| cancel())
                    />
                </Show>
            </div>

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::CONSOLIDATION id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}
