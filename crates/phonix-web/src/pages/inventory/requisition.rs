//! One requisition: the editor while it is a draft, the document once it has
//! been asked.
//!
//! # Submitting is the decision, so it sits where a decision goes
//!
//! Saving a draft costs nothing. Submitting takes a number nobody can hand back
//! and puts the request in somebody else's queue, so it is the last button on
//! the screen, it asks first, and it is offered only once there is something
//! saved to number. That is the shape [`super::purchase_order`] uses for
//! confirming, and the reason is the same.
//!
//! # The answer is a panel, not a dialog
//!
//! Both answers have to say why - see
//! `app_inventory::requisition::DecisionInput`. A yes/no dialog cannot carry a
//! reason, so the decision is a panel on the document with one field and two
//! buttons, and *both* stay disabled until there is something in it. The
//! service checks the same rule, because a disabled button is not a control.
//!
//! # A requisition needs a cost centre, so it needs the HR app
//!
//! The picker is required. Where the port answers an empty list - a workspace
//! with no HR app - the form says so and the save will refuse, which is the
//! honest state of affairs rather than a form that looks fillable and is not.
//!
//! # Ordering from here waits for consolidation
//!
//! An approved requisition offers no "raise an order" button yet, and that is
//! deliberate rather than missing. An order raised from one has to remember
//! *which* requisition lines it satisfied - otherwise `ordered` can never be
//! advanced, and the request stays not-ordered for ever while the goods arrive.
//! That link table is the consolidated requisition's, and it lands with it. A
//! button that opened a blank order form and left the requisition behind would
//! be worse than no button.
//!
//! # There is no running total in the editor
//!
//! The estimate is denominated in the workspace's own currency, and a
//! requisition carries no currency column for the browser to read - it names no
//! supplier, so there is nothing to quote in. Rather than guess at one and show
//! a figure in the wrong units, the editor shows no total and the document shows
//! the stored one. The receipt screen makes the same call for the same kind of
//! reason.

use app_inventory::requisition::{
    DecisionInput, OrderProgress, Requisition, RequisitionInput, RequisitionLineInput,
    RequisitionState,
};
use app_inventory::variant::VariantChoice;
use leptos::prelude::*;
use leptos_meta::Title;
use phonix_core::audit::kinds;
use phonix_core::form::Submission;
use phonix_core::permissions;
use uuid::Uuid;

use crate::components::history::RecordHistory;
use crate::components::page::{
    Badge, GhostButton, Notice, PageHeader, Panel, PrimaryButton, Section, Tone,
};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::inventory_fns::{
    blank_requisition, cancel_requisition, chargeable_cost_centres, decide_requisition,
    delete_requisition, pickable_variants, requisition_detail, save_requisition,
    selectable_units, selectable_warehouses, submit_requisition,
};
use crate::ui::alert::{Alert, Alerts, Confirm};
use crate::ui::form::field::Choice;
use crate::ui::lookup::SelectField;

const BACK: &str = "/inventory/requisitions";

#[component]
pub fn requisition_new_page() -> impl IntoView {
    // From the server, unlike an order's blank, only because the permission is
    // checked there. There is no currency to fetch.
    let blank = Resource::new(|| (), |()| async move { blank_requisition().await });

    view! {
        <Title text=format!("{} | Phonix", l!("requisitions.new")) />

        <PageHeader
            title=l!("requisitions.new")
            subtitle=l!("requisitions.new.subtitle")
            icon=Icon::ClipboardList
            back=(BACK, l!("requisitions.title"))
        />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match blank.await {
                    Ok(draft) => view! { <RequisitionEditor draft=draft /> }.into_any(),
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

/// One requisition: editable while it is a draft, a document after.
#[component]
pub fn requisition_page() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let requisition_id = move || params.with(|params| params.get("id").unwrap_or_default());

    let requisition = Resource::new(requisition_id, |raw| async move {
        match raw.parse::<Uuid>() {
            Ok(id) => requisition_detail(id).await,
            Err(_) => Err(ServerFnError::new("That is not a requisition id.")),
        }
    });

    view! {
        <Title text=format!("{} | Phonix", l!("entity.requisition.singular")) />

        <Transition fallback=|| {
            view! { <p class="text-sm text-content-subtle">{l!("common.loading")}</p> }
        }>
            {move || Suspend::new(async move {
                match requisition.await {
                    Ok(stored) => {
                        let heading = stored.label();
                        let subtitle = stored.cost_centre.label();
                        let state = stored.state;
                        let opened_on = RequisitionInput::from_requisition(&stored);

                        view! {
                            <>
                                <PageHeader
                                    title=heading
                                    subtitle=subtitle
                                    icon=Icon::ClipboardList
                                    back=(BACK, l!("requisitions.title"))
                                >
                                    <StateBadge state=state />
                                </PageHeader>

                                {if state.is_editable() {
                                    view! { <RequisitionEditor draft=opened_on /> }.into_any()
                                } else {
                                    view! {
                                        <RequisitionDocument
                                            requisition=stored
                                            reload=Callback::new(move |()| requisition.refetch())
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
                                    title=l!("entity.requisition.singular")
                                    icon=Icon::ClipboardList
                                    back=(BACK, l!("requisitions.title"))
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
fn state_badge(state: RequisitionState) -> impl IntoView {
    let label = crate::i18n::t(&state.label());
    // Waiting is a warning: it is the one state that needs somebody to act.
    let tone = match state {
        RequisitionState::Draft | RequisitionState::Cancelled => Tone::Neutral,
        RequisitionState::Submitted => Tone::Warning,
        RequisitionState::Approved => Tone::Success,
        RequisitionState::Rejected => Tone::Danger,
    };

    view! { <Badge label=label tone=tone /> }
}

#[component]
fn progress_badge(progress: OrderProgress) -> impl IntoView {
    let label = crate::i18n::t(&progress.label());
    let tone = match progress {
        OrderProgress::Nothing => Tone::Neutral,
        OrderProgress::Partly => Tone::Brand,
        OrderProgress::Everything => Tone::Success,
    };

    view! { <Badge label=label tone=tone /> }
}

// --- the editor ---------------------------------------------------------

#[component]
fn requisition_editor(draft: RequisitionInput) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let saving = RwSignal::new(false);
    let rejected = RwSignal::new(None::<String>);

    let centres = Resource::new(|| (), |()| async move { chargeable_cost_centres().await });
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
                    // Empty pickers rather than a failed screen. An empty cost
                    // centre list means no HR app, and the form then cannot be
                    // saved at all - `HeaderFields` says so rather than letting
                    // somebody fill the rest in first.
                    let centres = centres.await.unwrap_or_default();
                    let warehouses = warehouses.await.unwrap_or_default();
                    let variants = variants.await.unwrap_or_default();
                    let units = units.await.unwrap_or_default();

                    let centre_options = centres
                        .iter()
                        .map(|centre| {
                            Choice::new(centre.id.to_string(), centre.name.clone())
                                .detail(centre.code.clone())
                        })
                        .collect::<Vec<_>>();
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
                            centres=centre_options
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
    draft: RwSignal<RequisitionInput>,
    centres: Vec<Choice>,
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
    let no_centres = centres.is_empty();

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            rejected.set(None);
            let submission = draft.get_untracked();
            let navigate = navigate.clone();

            leptos::task::spawn_local(async move {
                let result = save_requisition(submission).await;
                saving.set(false);

                match result {
                    Ok(Submission::Saved(stored)) => {
                        let id = stored.id;
                        draft.set(stored);
                        alerts.post(Alert::success(l!("requisitions.saved")));

                        if let Some(id) = id {
                            navigate(
                                &format!("/inventory/requisitions/{id}"),
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

    let submit = {
        let navigate = navigate.clone();
        move || {
            let Some(id) = draft.with_untracked(|d| d.id) else {
                return;
            };
            let navigate = navigate.clone();

            alerts.ask(
                Confirm::new(l!("requisitions.submit.confirm"), move || {
                    let navigate = navigate.clone();
                    saving.set(true);

                    leptos::task::spawn_local(async move {
                        let result = submit_requisition(id).await;
                        saving.set(false);

                        match result {
                            Ok(Submission::Saved(stored)) => {
                                alerts.post(
                                    Alert::success(
                                            l!("requisitions.submitted_as", number = stored.number),
                                        )
                                        .titled(l!("requisitions.submit")),
                                );
                                // Reload the route: it is a document now, and
                                // this screen draws a different thing for one.
                                navigate(
                                    &format!("/inventory/requisitions/{id}"),
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
                .titled(l!("requisitions.submit"))
                .confirm_label(l!("requisitions.submit")),
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
                Confirm::new(l!("requisitions.delete.confirm"), move || {
                    let navigate = navigate.clone();

                    leptos::task::spawn_local(async move {
                        match delete_requisition(id).await {
                            Ok(_) => {
                                alerts.post(Alert::success(l!("requisitions.deleted")));
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
            <Section title=l!("requisitions.header")>
                <HeaderFields draft=draft centres=centres warehouses=warehouses no_centres=no_centres />
            </Section>

            <Section title=l!("requisitions.lines") description=l!("requisitions.lines.help")>
                <LineTable draft=draft variants=variants unit_options=unit_options />
            </Section>

            <div class="grid gap-3 lg:grid-cols-2 lg:items-start">
                <Section flush=true
                    title=l!("requisitions.justification")
                    description=l!("requisitions.justification.help")
                >
                    <textarea
                        class="w-full"
                        rows="3"
                        prop:value=move || draft.with(|d| d.justification.clone())
                        on:input=move |ev| {
                            let text = event_target_value(&ev);
                            draft.update(|d| d.justification = text);
                        }
                    />
                </Section>

                <Section title=l!("requisitions.note") flush=true>
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

                // Offered only once there is something saved to number.
                <Show when=saved fallback=|| ()>
                    <PrimaryButton
                        label=l!("requisitions.submit")
                        icon=Icon::ArrowRight
                        pending=Signal::derive(move || saving.get())
                        on_click=Callback::new({
                            let submit = submit.clone();
                            move |()| submit()
                        })
                    />
                </Show>
            </div>
        </Panel>
    }
}

/// Who is paying, where it goes, and when it is wanted.
#[component]
fn header_fields(
    draft: RwSignal<RequisitionInput>,
    centres: Vec<Choice>,
    warehouses: Vec<Choice>,
    /// Whether this workspace has any cost centre at all, which is a different
    /// thing from the requester not having chosen one.
    no_centres: bool,
) -> impl IntoView {
    view! {
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <div class="block space-y-1">
                <label for="req-centre" class="block text-xs font-medium text-content-muted">
                    {l!("requisitions.cost_centre")}
                </label>
                <SelectField
                    id="req-centre"
                    value=Signal::derive(move || {
                        draft.with(|d| d.cost_centre_id.map(|id| id.to_string()).unwrap_or_default())
                    })
                    on_change=Callback::new(move |value: String| {
                        let chosen = value.parse::<Uuid>().ok();
                        draft.update(|d| d.cost_centre_id = chosen);
                    })
                    options=centres
                    placeholder=l!("common.not_set")
                    clearable=true
                />
                <span class=if no_centres {
                    "block text-2xs text-danger"
                } else {
                    "block text-2xs text-content-subtle"
                }>
                    {if no_centres {
                        l!("requisitions.cost_centre.unavailable")
                    } else {
                        l!("requisitions.cost_centre.help")
                    }}
                </span>
            </div>

            <div class="block space-y-1">
                <label for="req-warehouse" class="block text-xs font-medium text-content-muted">
                    {l!("requisitions.warehouse")}
                </label>
                <SelectField
                    id="req-warehouse"
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
                <span class="block text-2xs text-content-subtle">
                    {l!("requisitions.warehouse.help")}
                </span>
            </div>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("requisitions.raised_on")}
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

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("requisitions.needed_by")}
                </span>
                <input
                    type="date"
                    class="w-full"
                    prop:value=move || {
                        draft.with(|d| d.needed_by.map(|on| on.to_string()).unwrap_or_default())
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        draft.update(|d| d.needed_by = value.parse().ok());
                    }
                />
                <span class="block text-2xs text-content-subtle">
                    {l!("requisitions.needed_by.help")}
                </span>
            </label>
        </div>
    }
}

#[component]
fn line_table(
    draft: RwSignal<RequisitionInput>,
    variants: StoredValue<Vec<VariantChoice>>,
    unit_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <div class="overflow-x-auto">
                <table class="w-full min-w-[52rem] text-sm">
                    <thead>
                        <tr class="border-b border-edge text-left text-xs text-content-muted">
                            <th class="w-8 py-2 font-medium">"#"</th>
                            <th class="w-56 py-2 font-medium">{l!("requisitions.item")}</th>
                            <th class="py-2 font-medium">{l!("requisitions.description")}</th>
                            <th class="w-24 py-2 text-right font-medium">
                                {l!("requisitions.quantity")}
                            </th>
                            <th class="w-32 py-2 font-medium">{l!("requisitions.unit")}</th>
                            <th class="w-32 py-2 text-right font-medium">
                                {l!("requisitions.estimate")}
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
                label=l!("requisitions.line.add")
                icon=Icon::Plus
                on_click=Callback::new(move |()| {
                    draft.update(|d| d.lines.push(RequisitionLineInput::blank()));
                })
            />
        </div>
    }
}

#[component]
fn line_row(
    draft: RwSignal<RequisitionInput>,
    index: usize,
    variants: StoredValue<Vec<VariantChoice>>,
    unit_options: StoredValue<Vec<Choice>>,
) -> impl IntoView {
    // Every read goes through the index rather than a held clone, for the reason
    // an order line does: a row that cached its values would stop updating the
    // moment another row was removed and the indexes shifted.
    let field = move |read: fn(&RequisitionLineInput) -> String| {
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
                    label=l!("requisitions.item")
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
                // seventh decimal place rather than rounding it away, and a
                // browser number input would have rounded before this saw it.
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
                    label=l!("requisitions.unit")
                />
            </td>
            <td class="py-1 pr-2">
                <input
                    type="text"
                    inputmode="decimal"
                    class="w-full text-right tabular-nums"
                    prop:value=move || field(|line| line.estimate.clone())
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        draft
                            .update(|d| {
                                if let Some(line) = d.lines.get_mut(index) {
                                    line.estimate = value;
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
                                    d.lines.push(RequisitionLineInput::blank());
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

/// A submitted, decided or withdrawn requisition: read-only, plus the decision
/// panel for whoever may answer it.
#[component]
fn requisition_document(requisition: Requisition, reload: Callback<()>) -> impl IntoView {
    let alerts = Alerts::get();
    let viewer = crate::ui::viewer::Viewer::get();

    let may_decide = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::REQUISITIONS_DECIDE))
        })
    });
    let may_order = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::PURCHASE_ORDERS_CREATE))
        })
    });
    let may_withdraw = Signal::derive(move || {
        viewer.with(|user| {
            user.as_ref()
                .is_some_and(|user| user.can(permissions::REQUISITIONS_CREATE))
        })
    });

    let id = requisition.id;
    let state = requisition.state;
    let awaits = state.awaits_decision();
    let progress = requisition.order_progress();
    let can_raise_an_order = requisition.can_raise_an_order();

    let centre = requisition.cost_centre.label();
    let warehouse = requisition.warehouse_name.clone();
    let raised_on = requisition.raised_on.to_string();
    let raised_by = requisition.raised_by_name.clone();
    let needed_by = requisition.needed_by.map(|on| on.to_string());
    let justification = requisition.justification.clone();
    let note = requisition.note.clone();
    let decision = requisition.decision.clone();
    let estimate = requisition.estimate().map(|total| total.to_display_string());
    let lines = requisition.lines.clone();

    let reason = RwSignal::new(String::new());
    let deciding = RwSignal::new(false);

    let decide = move |approving: bool| {
        deciding.set(true);
        let decision = DecisionInput {
            note: reason.get_untracked(),
        };

        leptos::task::spawn_local(async move {
            let result = decide_requisition(id, approving, decision).await;
            deciding.set(false);

            match result {
                Ok(Submission::Saved(_)) => {
                    alerts.post(Alert::success(if approving {
                        l!("requisitions.approved_note")
                    } else {
                        l!("requisitions.rejected_note")
                    }));
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
    };

    let withdraw = move || {
        alerts.ask(
            Confirm::new(l!("requisitions.cancel.confirm"), move || {
                leptos::task::spawn_local(async move {
                    match cancel_requisition(id).await {
                        Ok(Submission::Saved(())) => {
                            alerts.post(Alert::success(l!("requisitions.cancelled_note")));
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
            .titled(l!("requisitions.cancel"))
            .confirm_label(l!("requisitions.cancel")),
        );
    };

    view! {
        <Panel>
            <div class="grid gap-3 lg:grid-cols-2">
                <Section title=l!("requisitions.header") flush=true>
                    <dl class="space-y-1 text-sm">
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("requisitions.cost_centre")}</dt>
                            <dd class="text-content">{centre}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("requisitions.warehouse")}</dt>
                            <dd class="text-content">{warehouse}</dd>
                        </div>
                        <div class="flex justify-between gap-4">
                            <dt class="text-content-muted">{l!("requisitions.raised_on")}</dt>
                            <dd class="tabular-nums text-content">{raised_on}</dd>
                        </div>
                        {raised_by
                            .map(|who| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("requisitions.raised_by")}
                                        </dt>
                                        <dd class="text-content">{who}</dd>
                                    </div>
                                }
                            })}
                        {needed_by
                            .map(|by| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("requisitions.needed_by")}
                                        </dt>
                                        <dd class="tabular-nums text-content">{by}</dd>
                                    </div>
                                }
                            })}
                        <div class="flex items-center justify-between gap-4">
                            <dt class="text-content-muted">{l!("requisitions.ordered")}</dt>
                            <dd>
                                <ProgressBadge progress=progress />
                            </dd>
                        </div>
                        {estimate
                            .map(|total| {
                                view! {
                                    <div class="flex justify-between gap-4">
                                        <dt class="text-content-muted">
                                            {l!("requisitions.estimate.total")}
                                        </dt>
                                        <dd class="tabular-nums text-content">{total}</dd>
                                    </div>
                                }
                            })}
                    </dl>
                </Section>

                {justification
                    .map(|why| {
                        view! {
                            <Section title=l!("requisitions.justification")>
                                <p class="whitespace-pre-wrap text-sm text-content-muted">{why}</p>
                            </Section>
                        }
                    })}
            </div>

            <Section title=l!("requisitions.lines")>
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[44rem] text-sm">
                        <thead>
                            <tr class="border-b border-edge text-left text-xs text-content-muted">
                                <th class="w-8 py-2 font-medium">"#"</th>
                                <th class="py-2 font-medium">
                                    {l!("requisitions.description")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("requisitions.quantity")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("requisitions.ordered_quantity")}
                                </th>
                                <th class="py-2 text-right font-medium">
                                    {l!("requisitions.estimate")}
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
                                    let ordered = line.ordered.to_display_string();
                                    let estimate = line
                                        .estimate
                                        .map(|each| each.to_display_string())
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
                                            <td class="py-1.5 text-right tabular-nums text-content">
                                                {quantity}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {ordered}
                                            </td>
                                            <td class="py-1.5 text-right tabular-nums text-content-muted">
                                                {estimate}
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
                        <Section title=l!("requisitions.note")>
                            <p class="whitespace-pre-wrap text-sm text-content-muted">{note}</p>
                        </Section>
                    }
                })}

            // The answer already given, where there is one.
            {decision
                .map(|decision| {
                    let at = decision.at.to_rfc3339();
                    let by = decision.by_name.clone();
                    let why = decision.note.clone();


                    view! {
                        <Section title=l!("requisitions.decision")>
                            <dl class="space-y-1 text-sm">
                                {by
                                    .map(|by| {
                                        view! {
                                            <div class="flex justify-between gap-4">
                                                <dt class="text-content-muted">
                                                    {l!("requisitions.decision.by")}
                                                </dt>
                                                <dd class="text-content">{by}</dd>
                                            </div>
                                        }
                                    })}
                                <div class="flex justify-between gap-4">
                                    <dt class="text-content-muted">
                                        {l!("requisitions.decision.at")}
                                    </dt>
                                    <dd class="tabular-nums text-content">{at}</dd>
                                </div>
                            </dl>
                            <p class="mt-2 whitespace-pre-wrap text-sm text-content-muted">
                                {why}
                            </p>
                        </Section>
                    }
                })}

            // The decision itself. A panel rather than a dialog, because a
            // rejection carries a reason and an approval does not - see the
            // module documentation.
            <Show when=move || awaits && may_decide.get() fallback=|| ()>
                <Section title=l!("requisitions.decision")>
                    <div class="space-y-2">
                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("requisitions.decision.note")}
                            </span>
                            <textarea
                                class="w-full"
                                rows="2"
                                prop:value=move || reason.get()
                                on:input=move |ev| reason.set(event_target_value(&ev))
                            />
                            <span class="block text-2xs text-content-subtle">
                                {l!("requisitions.decision.note.help")}
                            </span>
                        </label>

                        <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                            // Both disabled until there is a reason, not
                            // just the rejection. The service refuses either
                            // without one - a disabled button is not a control.
                            <GhostButton
                                label=l!("requisitions.reject")
                                icon=Icon::X
                                disabled=Signal::derive(move || {
                                    deciding.get() || reason.with(|why| why.trim().is_empty())
                                })
                                on_click=Callback::new(move |()| decide(false))
                            />
                            <PrimaryButton
                                label=l!("requisitions.approve")
                                icon=Icon::Check
                                disabled=Signal::derive(move || {
                                    reason.with(|why| why.trim().is_empty())
                                })
                                pending=Signal::derive(move || deciding.get())
                                on_click=Callback::new(move |()| decide(true))
                            />
                        </div>
                    </div>
                </Section>
            </Show>

            // Two conditions, and they are different questions. The state decides
            // whether the act means anything; the permission decides whether this
            // reader may do it - and the service checks it again.
            <div class="mt-4 flex flex-wrap items-center justify-end gap-2 border-t border-edge pt-4">
                <Show when=move || awaits && may_withdraw.get() fallback=|| ()>
                    <GhostButton
                        label=l!("requisitions.cancel")
                        icon=Icon::Ban
                        on_click=Callback::new(move |()| withdraw())
                    />
                </Show>

                // Where the order button will go. See the module docs for why
                // it is not here yet: it needs the link table that lets an order
                // advance the lines it satisfied.
                <Show when=move || can_raise_an_order && may_order.get() fallback=|| ()>
                    <p class="text-xs text-content-subtle">
                        {l!("requisitions.order_pending")}
                    </p>
                </Show>
            </div>

            <Section title=l!("common.history")>
                <RecordHistory kind=kinds::REQUISITION id=Some(id.to_string()) />
            </Section>
        </Panel>
    }
}
